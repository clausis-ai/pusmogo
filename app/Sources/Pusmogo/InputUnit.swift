import AudioToolbox
import AVFoundation
import CoreAudio

/// One running microphone. It opens exactly the device it is given: an
/// `AVAudioEngine` also opens the system's default input and output, so with a
/// Bluetooth headset connected it put the headset into call mode (16 kHz mono
/// music) and delivered no audio, whatever input was selected.
final class InputUnit {
    let device: AudioDeviceID
    /// What the buffers hold: the device's rate, at most two channels.
    let format: AVAudioFormat
    private let unit: AudioUnit
    private let onBuffer: (AVAudioPCMBuffer, AVAudioTime) -> Void
    /// Buffers go from the real-time thread to this queue, in order.
    private let queue = DispatchQueue(label: "pusmogo.input")
    private var pool: [AVAudioPCMBuffer] = []
    private var next = 0
    private var closed = false

    /// Prepares the device without starting it. `onBuffer` runs on a private
    /// queue; the buffer is reused later, so it must not be kept.
    init(device: AudioDeviceID, onBuffer: @escaping (AVAudioPCMBuffer, AVAudioTime) -> Void) throws {
        self.device = device
        self.onBuffer = onBuffer
        var description = AudioComponentDescription(
            componentType: kAudioUnitType_Output, componentSubType: kAudioUnitSubType_HALOutput,
            componentManufacturer: kAudioUnitManufacturer_Apple, componentFlags: 0, componentFlagsMask: 0)
        guard let component = AudioComponentFindNext(nil, &description) else { throw Self.error(kAudioUnitErr_InvalidElement) }
        var made: AudioUnit?
        try Self.check(AudioComponentInstanceNew(component, &made))
        guard let unit = made else { throw Self.error(kAudioUnitErr_InvalidElement) }
        self.unit = unit
        do {
            try Self.set(unit, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Input, 1, UInt32(1))
            try Self.set(unit, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Output, 0, UInt32(0))
            try Self.set(unit, kAudioOutputUnitProperty_CurrentDevice, kAudioUnitScope_Global, 0, device)
            var native = AudioStreamBasicDescription()
            var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
            try Self.check(AudioUnitGetProperty(unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Input, 1, &native, &size))
            guard native.mSampleRate > 0, native.mChannelsPerFrame > 0,
                  let format = AVAudioFormat(standardFormatWithSampleRate: native.mSampleRate, channels: min(native.mChannelsPerFrame, 2))
            else { throw Self.error(kAudioUnitErr_FormatNotSupported) }
            self.format = format
            try Self.set(unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Output, 1, format.streamDescription.pointee)
            var frames = UInt32(0)
            size = UInt32(MemoryLayout<UInt32>.size)
            try Self.check(AudioUnitGetProperty(unit, kAudioUnitProperty_MaximumFramesPerSlice, kAudioUnitScope_Global, 0, &frames, &size))
            pool = (0..<32).compactMap { _ in AVAudioPCMBuffer(pcmFormat: format, frameCapacity: max(frames, 4096)) }
            guard !pool.isEmpty else { throw Self.error(kAudioUnitErr_FormatNotSupported) }
            let callback = AURenderCallbackStruct(inputProc: { context, flags, time, bus, frames, _ in
                Unmanaged<InputUnit>.fromOpaque(context).takeUnretainedValue().render(flags, time, bus, frames)
            }, inputProcRefCon: Unmanaged.passUnretained(self).toOpaque())
            try Self.set(unit, kAudioOutputUnitProperty_SetInputCallback, kAudioUnitScope_Global, 0, callback)
            try Self.check(AudioUnitInitialize(unit))
        } catch {
            AudioComponentInstanceDispose(unit)
            closed = true
            throw error
        }
    }

    deinit { close() }

    func start() throws { try Self.check(AudioOutputUnitStart(unit)) }

    /// Stops, waits for the buffers already captured, and lets go of the device.
    func close() {
        guard !closed else { return }
        closed = true
        AudioOutputUnitStop(unit)
        AudioUnitUninitialize(unit)
        AudioComponentInstanceDispose(unit)
        queue.sync {}
    }

    /// The device's rate right now; a Bluetooth headset changes it when it opens.
    static func rate(of device: AudioDeviceID) -> Double {
        var address = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyNominalSampleRate,
                                                 mScope: kAudioObjectPropertyScopeGlobal,
                                                 mElement: kAudioObjectPropertyElementMain)
        var rate = 0.0
        var size = UInt32(MemoryLayout<Double>.size)
        return AudioObjectGetPropertyData(device, &address, 0, nil, &size, &rate) == noErr ? rate : 0
    }

    private func render(_ flags: UnsafeMutablePointer<AudioUnitRenderActionFlags>, _ time: UnsafePointer<AudioTimeStamp>,
                        _ bus: UInt32, _ frames: UInt32) -> OSStatus {
        let buffer = pool[next]
        guard frames <= buffer.frameCapacity else { return kAudioUnitErr_TooManyFramesToProcess }
        buffer.frameLength = frames
        let status = AudioUnitRender(unit, flags, time, bus, frames, buffer.mutableAudioBufferList)
        guard status == noErr else { return status }
        next = (next + 1) % pool.count
        let when = AVAudioTime(hostTime: time.pointee.mHostTime)
        queue.async { [onBuffer] in onBuffer(buffer, when) }
        return noErr
    }

    private static func set<T>(_ unit: AudioUnit, _ property: AudioUnitPropertyID, _ scope: AudioUnitScope,
                               _ element: AudioUnitElement, _ value: T) throws {
        try withUnsafePointer(to: value) {
            try check(AudioUnitSetProperty(unit, property, scope, element, $0, UInt32(MemoryLayout<T>.size)))
        }
    }

    private static func check(_ status: OSStatus) throws {
        guard status == noErr else { throw error(status) }
    }

    private static func error(_ status: OSStatus) -> NSError {
        NSError(domain: NSOSStatusErrorDomain, code: Int(status),
                userInfo: [NSLocalizedDescriptionKey: "Nincs használható mikrofon (\(status))"])
    }
}
