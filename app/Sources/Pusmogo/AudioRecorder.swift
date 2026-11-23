import AVFoundation
import CoreAudio
import IOKit
import PusmogoKeys

/// What a finished dictation hands to the transcriber.
struct Recording {
    /// The audio still kept: all of it when `complete`, else only the part
    /// not yet handed to the streaming session.
    let samples: [Float]
    /// Every sample recorded, kept or not (for the duration and the tap check).
    let total: Int
    /// False once a long streamed dictation dropped audio the session already
    /// has; then only the session can produce the text.
    let complete: Bool

    var seconds: Double { Double(total) / 16_000 }

    init(samples: [Float], total: Int? = nil, complete: Bool = true) {
        self.samples = samples
        self.total = total ?? samples.count
        self.complete = complete
    }
}

/// The samples of one dictation, appended from the capture thread and handed
/// to the streaming session by `takeNew()`. Everything is kept for the first
/// `keepAllSamples` (5 min, 19 MB), so a language swap can still transcribe
/// the whole clip again; past that, audio the session already took is
/// dropped, so a long hands-free dictation stays flat in memory instead of
/// growing by ~230 MB an hour. Only `takeNew()` drops audio: a recording that
/// is never streamed (`PUSMOGO_STREAM=0`, the test-set window) keeps all of it.
final class SampleBuffer {
    /// `PUSMOGO_RECORDER_KEEP_SECS=<s>` changes the limit in tests (a huge
    /// value keeps everything, for an A/B run).
    static let keepAllSamples = (ProcessInfo.processInfo.environment["PUSMOGO_RECORDER_KEEP_SECS"]
        .flatMap(Int.init) ?? 5 * 60) * 16_000
    private let lock = NSLock()
    private var buffer: [Float] = []
    /// Samples dropped from the front of `buffer`.
    private var dropped = 0
    /// Samples handed out by `takeNew()`, counted from the start.
    private var drained = 0
    private let limit: Int

    init(keepAllSamples: Int = SampleBuffer.keepAllSamples) { limit = keepAllSamples }

    func reset() { lock.withLock { buffer.removeAll(keepingCapacity: false); dropped = 0; drained = 0 } }

    func append<C: Collection>(_ samples: C) where C.Element == Float {
        lock.withLock { buffer.append(contentsOf: samples) }
    }

    /// Samples recorded since the previous call (or since `reset()`).
    func takeNew() -> [Float] {
        lock.withLock {
            let start = drained - dropped
            let new = Array(buffer[start...])
            drained = dropped + buffer.count
            if dropped + buffer.count > limit {
                // The session has everything up to here; keep nothing of it.
                if dropped == 0 { Timings.note("recorder: past \(limit / 16_000) s, keeping only audio not yet streamed") }
                dropped = drained
                buffer.removeAll(keepingCapacity: false)
            }
            return new
        }
    }

    var recording: Recording {
        lock.withLock {
            dropped == 0 ? Recording(samples: buffer)
                : Recording(samples: Array(buffer[(drained - dropped)...]), total: dropped + buffer.count, complete: false)
        }
    }

    /// Samples kept in memory right now (for logs and tests).
    var kept: Int { lock.withLock { buffer.count } }
}

/// Captures the default input and converts it to 16 kHz mono Float32,
/// the contract of pusmogo-core. Reports a 0…1 level for the menu bar icon.
final class AudioRecorder {
    private var unit: InputUnit?
    private let target = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false)!
    private let lock = NSLock()
    private let buffer = SampleBuffer()
    private var running: Bool { unit != nil }
    /// True between `start()` and the stop of one dictation. A warm engine
    /// runs outside it too, and then only fills `preroll`.
    private var capturing = false
    /// Keeps the engine running between dictations, so a press records at
    /// once: a USB or display microphone needs about 450 ms to start, and a
    /// short first word ("Mi a lopás?") was lost in that gap.
    private(set) var warm = false
    /// The last `prerollSamples` heard while warm and not capturing; they go
    /// in front of the next recording. Kept only in memory and overwritten.
    private var preroll: [Float] = []
    static let prerollSamples = 8_000
    /// Takes back the listeners of the running input and of the system default.
    private var unlisten: [() -> Void] = []
    /// Called once with the start-up time (ms) of an engine started warm,
    /// so an input can be measured without recording anything.
    var onWarmStartup: ((Double) -> Void)?
    private var warmStartedAt: Double?
    /// Called on the main queue, about 1 s after the input of a warm engine,
    /// or the system's default input, changed.
    var onInputChange: (() -> Void)?
    private var inputChange: DispatchWorkItem?
    /// The current recording's converter, kept so `stop()` can drain the
    /// last few milliseconds it holds back for its resampling filter.
    private var converter: AVAudioConverter?
    var onLevel: ((Float) -> Void)?
    /// UID of the input to record from; nil or a device that is gone means
    /// the system default input. Only this app's capture changes, never the
    /// system setting.
    var preferredDeviceUID: String?
    /// Name of the input the last `start()` used, for logs.
    private(set) var activeDeviceName = ""
    private var startedAt = 0.0
    private var firstAudioAt: Double?
    /// Milliseconds from `start()` to the first audio buffer of this
    /// recording; whatever the user said in that gap is not in the recording.
    var startupMs: Double? { lock.withLock { firstAudioAt.map { $0 - startedAt } } }

    static func requestMicAccess() async -> Bool {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: return true
        case .notDetermined: return await AVCaptureDevice.requestAccess(for: .audio)
        default: return false
        }
    }

    static var micGranted: Bool { AVCaptureDevice.authorizationStatus(for: .audio) == .authorized }

    func start() throws {
        buffer.reset()
        startedAt = nowMs()
        // Warm may be left on from another input.
        if warm, usesBluetooth { setWarm(false) }
        if warm, running, unit?.device == wantedDeviceID() {
            let earlier: [Float] = lock.withLock {
                let p = preroll
                preroll.removeAll(keepingCapacity: true)
                capturing = true
                firstAudioAt = startedAt
                return p
            }
            buffer.append(earlier)
            return
        }
        if running { stopEngine() }
        lock.withLock { firstAudioAt = nil; capturing = true }
        try startEngine()
    }

    /// Turns the warm mode on or off. Off stops an idle engine; a running
    /// dictation keeps its engine until it stops.
    /// A Bluetooth microphone is never kept running: see `MicDevice.bluetooth`.
    func setWarm(_ on: Bool) {
        let on = on && !usesBluetooth
        warm = on
        if on, !running {
            lock.withLock { capturing = false }
            warmStartedAt = nowMs()
            do { try startEngine() } catch { warm = false; Timings.note("recorder: warm start failed: \(error.localizedDescription)") }
        } else if !on, running, !lock.withLock({ capturing }) {
            stopEngine()
        }
    }

    /// UID of the input recording uses now (the chosen one, else the default).
    var currentDeviceUID: String? {
        guard let id = unit?.device ?? wantedDeviceID() else { return nil }
        return MicDevice.all().first { $0.id == id }?.uid
    }

    /// True when the next recording uses a Bluetooth microphone.
    var usesBluetooth: Bool {
        guard let id = wantedDeviceID() else { return false }
        return MicDevice.all().first { $0.id == id }?.bluetooth ?? false
    }

    private func wantedDeviceID() -> AudioDeviceID? {
        let chosen = preferredDeviceUID.flatMap { uid in MicDevice.all().first { $0.uid == uid } }
        return (chosen ?? MicDevice.automatic())?.id
    }

    private func stopEngine() {
        closeUnit()
        converter = nil
        lock.withLock { preroll.removeAll() }
    }

    private func closeUnit() {
        unlisten.forEach { $0() }
        unlisten = []
        unit?.close()
        unit = nil
    }

    private func startEngine() throws {
        guard let id = wantedDeviceID() else {
            throw NSError(domain: "pusmogo", code: 1, userInfo: [NSLocalizedDescriptionKey: "Nincs használható mikrofon"])
        }
        var made: AVAudioConverter?
        let unit = try InputUnit(device: id) { [weak self] pcm, when in
            guard let self, let converter = made else { return }
            self.append(pcm, at: when, converter: converter)
        }
        made = AVAudioConverter(from: unit.format, to: target)
        guard made != nil else {
            throw NSError(domain: "pusmogo", code: 1, userInfo: [NSLocalizedDescriptionKey: "Nincs használható mikrofon (\(unit.format))"])
        }
        converter = made
        let name = MicDevice.all().first { $0.id == id }?.name ?? "?"
        activeDeviceName = "\(name) @ \(Int(unit.format.sampleRate)) Hz × \(unit.format.channelCount)"
        try unit.start()
        self.unit = unit
        listen(id, kAudioDevicePropertyNominalSampleRate) { [weak self] in self?.inputChanged() }
        listen(id, kAudioDevicePropertyDeviceIsAlive) { [weak self] in self?.inputChanged() }
        listen(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultInputDevice) { [weak self] in
            self?.inputChanged()
        }
    }

    private func listen(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector, _ changed: @escaping () -> Void) {
        var address = AudioObjectPropertyAddress(mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal,
                                                 mElement: kAudioObjectPropertyElementMain)
        let block: AudioObjectPropertyListenerBlock = { _, _ in changed() }
        AudioObjectAddPropertyListenerBlock(object, &address, .main, block)
        unlisten.append { AudioObjectRemovePropertyListenerBlock(object, &address, .main, block) }
    }

    /// The running input, or the system default, changed. A dictation goes on
    /// with the input that is there now when its own one is gone or changed its
    /// rate; a warm one is closed, and the owner decides about the new input.
    private func inputChanged() {
        guard let unit else { return }
        let gone = !MicDevice.all().contains { $0.id == unit.device }
        if lock.withLock({ capturing }) {
            guard gone || InputUnit.rate(of: unit.device) != unit.format.sampleRate else { return }
            closeUnit()
            if let converter { drainTail(converter) }
            converter = nil
            do { try startEngine() } catch { Timings.note("recorder: input lost: \(error.localizedDescription)") }
            return
        }
        guard gone || unit.device != wantedDeviceID() || InputUnit.rate(of: unit.device) != unit.format.sampleRate else { return }
        stopEngine()
        warm = false
        // Several changes follow each other when a device connects.
        inputChange?.cancel()
        let work = DispatchWorkItem { [weak self] in self?.onInputChange?() }
        inputChange = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 1, execute: work)
    }

    /// Stops capture and returns everything recorded.
    func stop() -> [Float] { stopRecording().samples }

    /// Stops capture and returns the recording, which after a long streamed
    /// dictation holds only the audio not yet taken by `takeNew()`.
    func stopRecording() -> Recording {
        lock.withLock { capturing = false }
        if warm, running {
            // The engine keeps running for the next press; the converter's last
            // ~10 ms go to the pre-roll instead of this recording.
            return buffer.recording
        }
        closeUnit()
        if let converter {
            self.converter = nil
            drainTail(converter)
        }
        return buffer.recording
    }

    /// The resampler keeps up to ~10 ms of input back (its filter's look-ahead)
    /// until it sees more audio; signalling end of stream once the tap is gone
    /// turns that into output, so the last sound of the dictation is not cut.
    private func drainTail(_ converter: AVAudioConverter) {
        guard let out = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: 4096) else { return }
        var error: NSError?
        converter.convert(to: out, error: &error) { _, status in
            status.pointee = .endOfStream
            return nil
        }
        guard error == nil, let ch = out.floatChannelData?[0], out.frameLength > 0 else { return }
        let samples = UnsafeBufferPointer(start: ch, count: Int(out.frameLength))
        buffer.append(samples)
    }

    /// Drops the recording once the dictation has taken what it needs, so an
    /// idle app does not keep the user's last dictation in memory (about
    /// 3.8 MB a minute) until the next one starts. Call it after the last
    /// `takeNew()`, which still reads the buffer after `stop()`.
    func release() {
        buffer.reset()
    }

    /// Samples recorded since the previous call (or since `start()`), for
    /// streaming them to the core while the user is still speaking.
    func takeNew() -> [Float] { buffer.takeNew() }

    private func append(_ pcm: AVAudioPCMBuffer, at when: AVAudioTime, converter: AVAudioConverter) {
        let ratio = target.sampleRate / pcm.format.sampleRate
        let capacity = AVAudioFrameCount(Double(pcm.frameLength) * ratio) + 32
        guard let out = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity) else { return }
        var fed = false
        var error: NSError?
        converter.convert(to: out, error: &error) { _, status in
            if fed { status.pointee = .noDataNow; return nil }
            fed = true
            status.pointee = .haveData
            return pcm
        }
        guard error == nil, let ch = out.floatChannelData?[0] else { return }
        let samples = UnsafeBufferPointer(start: ch, count: Int(out.frameLength))
        let isCapturing: Bool = lock.withLock {
            guard !capturing else { return true }
            if let started = warmStartedAt {
                warmStartedAt = nil
                let ms = (when.isHostTimeValid ? AVAudioTime.seconds(forHostTime: when.hostTime) * 1000 : nowMs()) - started
                DispatchQueue.main.async { [weak self] in self?.onWarmStartup?(ms) }
            }
            preroll.append(contentsOf: samples)
            if preroll.count > Self.prerollSamples { preroll.removeFirst(preroll.count - Self.prerollSamples) }
            return false
        }
        guard isCapturing else { return }
        lock.withLock {
            if firstAudioAt == nil {
                // The buffer's host time is when its first sample was captured.
                firstAudioAt = when.isHostTimeValid ? AVAudioTime.seconds(forHostTime: when.hostTime) * 1000 : nowMs()
            }
        }
        buffer.append(samples)

        if let onLevel, !samples.isEmpty {
            var sum: Float = 0
            for s in samples { sum += s * s }
            let rms = (sum / Float(samples.count)).squareRoot()
            // Speech RMS is roughly 0.01–0.3; map to 0…1 on a log-ish curve.
            onLevel(min(1, max(0, (20 * log10(max(rms, 1e-5)) + 50) / 40)))
        }
    }
}

/// An audio input the user can pick in Settings.
struct MicDevice: Identifiable, Hashable {
    let id: AudioDeviceID
    let uid: String
    let name: String
    /// Bluetooth inputs (AirPods, headsets) switch to a narrow-band call
    /// codec while the mic is open, which costs recognition accuracy and
    /// turns the music they play into a phone call.
    let bluetooth: Bool
    /// Preference when the system default is Bluetooth: 0 built-in, 1 wired,
    /// nil for inputs never picked automatically (virtual, phone, Bluetooth).
    var wiredRank: Int?

    /// The microphone used when the user picked none: the system default, but
    /// a built-in or wired one instead of a Bluetooth default when there is one.
    static func automatic() -> MicDevice? {
        let devices = all()
        let id = automaticMic(devices.map { MicOption(id: $0.id, bluetooth: $0.bluetooth, wiredRank: $0.wiredRank) },
                              systemDefault: defaultInputID(), lidClosed: lidClosed())
        return devices.first { $0.id == id }
    }

    /// A closed MacBook's own microphone is switched off in hardware.
    private static func lidClosed() -> Bool {
        let root = IOServiceGetMatchingService(kIOMainPortDefault, IOServiceMatching("IOPMrootDomain"))
        guard root != 0 else { return false }
        defer { IOObjectRelease(root) }
        let state = IORegistryEntryCreateCFProperty(root, "AppleClamshellState" as CFString, kCFAllocatorDefault, 0)
        return state?.takeRetainedValue() as? Bool ?? false
    }

    private static func wiredRank(_ transport: UInt32?) -> Int? {
        switch transport {
        case kAudioDeviceTransportTypeBuiltIn: 0
        case kAudioDeviceTransportTypeUSB, kAudioDeviceTransportTypeThunderbolt, kAudioDeviceTransportTypeDisplayPort,
             kAudioDeviceTransportTypeHDMI, kAudioDeviceTransportTypePCI, kAudioDeviceTransportTypeFireWire: 1
        default: nil
        }
    }

    /// Every device with at least one input channel, in system order.
    static func all() -> [MicDevice] {
        var addr = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyDevices,
                                              mScope: kAudioObjectPropertyScopeGlobal,
                                              mElement: kAudioObjectPropertyElementMain)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(AudioObjectID(kAudioObjectSystemObject), &addr, 0, nil, &size) == noErr else { return [] }
        var ids = [AudioDeviceID](repeating: 0, count: Int(size) / MemoryLayout<AudioDeviceID>.size)
        guard AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &addr, 0, nil, &size, &ids) == noErr else { return [] }
        return ids.compactMap { id in
            guard inputChannels(id) > 0, let uid = string(id, kAudioDevicePropertyDeviceUID),
                  let name = string(id, kAudioObjectPropertyName) else { return nil }
            let transport = uint32(id, kAudioDevicePropertyTransportType)
            let bluetooth = transport == kAudioDeviceTransportTypeBluetooth
                || transport == kAudioDeviceTransportTypeBluetoothLE
            return MicDevice(id: id, uid: uid, name: name, bluetooth: bluetooth, wiredRank: wiredRank(transport))
        }
    }

    static func defaultInputID() -> AudioDeviceID? {
        var addr = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyDefaultInputDevice,
                                              mScope: kAudioObjectPropertyScopeGlobal,
                                              mElement: kAudioObjectPropertyElementMain)
        var id = AudioDeviceID(0)
        var size = UInt32(MemoryLayout<AudioDeviceID>.size)
        let status = AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &addr, 0, nil, &size, &id)
        return status == noErr && id != 0 ? id : nil
    }

    private static func inputChannels(_ id: AudioDeviceID) -> Int {
        var addr = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyStreamConfiguration,
                                              mScope: kAudioDevicePropertyScopeInput,
                                              mElement: kAudioObjectPropertyElementMain)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(id, &addr, 0, nil, &size) == noErr, size > 0 else { return 0 }
        let raw = UnsafeMutableRawPointer.allocate(byteCount: Int(size), alignment: MemoryLayout<AudioBufferList>.alignment)
        defer { raw.deallocate() }
        guard AudioObjectGetPropertyData(id, &addr, 0, nil, &size, raw) == noErr else { return 0 }
        let list = UnsafeMutableAudioBufferListPointer(raw.assumingMemoryBound(to: AudioBufferList.self))
        return list.reduce(0) { $0 + Int($1.mNumberChannels) }
    }

    private static func uint32(_ id: AudioDeviceID, _ selector: AudioObjectPropertySelector) -> UInt32? {
        var addr = AudioObjectPropertyAddress(mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal,
                                              mElement: kAudioObjectPropertyElementMain)
        var value: UInt32 = 0
        var size = UInt32(MemoryLayout<UInt32>.size)
        return AudioObjectGetPropertyData(id, &addr, 0, nil, &size, &value) == noErr ? value : nil
    }

    private static func string(_ id: AudioDeviceID, _ selector: AudioObjectPropertySelector) -> String? {
        var addr = AudioObjectPropertyAddress(mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal,
                                              mElement: kAudioObjectPropertyElementMain)
        var value: Unmanaged<CFString>?
        var size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
        guard AudioObjectGetPropertyData(id, &addr, 0, nil, &size, &value) == noErr, let value else { return nil }
        return value.takeRetainedValue() as String
    }
}
