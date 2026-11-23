import SwiftUI
import AVFoundation

/// The test-set recording mode: a window that shows one line of the reading
/// script at a time and records it into the speaker's folder under `testset/`
/// (core `testset.rs`). The speaker chooses a consent level first; nothing is
/// uploaded. Recording uses its own `AudioRecorder`, so it never types into
/// another app and never touches History.
///
/// `PUSMOGO_TESTSET=<speaker>` opens it at launch (no key tap, quits when the
/// window closes, may run next to an installed copy). `PUSMOGO_TESTSET_WAV=<wav>`
/// records that 16 kHz mono clip instead of the microphone, so the whole flow
/// can be checked unattended with `PUSMOGO_TESTSET_DIR=<scratch dir>`.
@MainActor
final class TestSetSession: ObservableObject {
    enum Level: String, CaseIterable, Identifiable {
        case local = "private", release
        var id: String { rawValue }
    }

    @Published var speaker: String
    @Published private(set) var status: Core.TestSetStatus?
    @Published private(set) var recording = false
    @Published private(set) var level: Float = 0
    @Published private(set) var notice: String?
    @Published var chosenLevel: Level = .local

    private let recorder = AudioRecorder()
    private let fakeClip: [Float]?
    private(set) var dir: URL?

    init(speaker: String, micUID: String?) {
        self.speaker = speaker
        recorder.preferredDeviceUID = micUID
        fakeClip = ProcessInfo.processInfo.environment["PUSMOGO_TESTSET_WAV"].flatMap(Self.readWav)
        recorder.onLevel = { [weak self] l in
            DispatchQueue.main.async { self?.level = l }
        }
        if !speaker.isEmpty { openSpeaker() }
    }

    var current: Core.TestTake? {
        guard let s = status, s.next < s.takes.count else { return nil }
        return s.takes[s.next]
    }

    var doneCount: Int { status?.takes.filter(\.done).count ?? 0 }

    /// Resolves the speaker's folder and reads their progress.
    func openSpeaker() {
        dir = Core.testsetSpeakerDir(name: speaker)
        refresh()
        Timings.note("testset: speaker folder \(dir?.lastPathComponent ?? "-"), consent \(status?.consent ?? "none"), next \(status?.next ?? -1)")
    }

    func refresh() {
        status = dir.flatMap { Core.testsetStatus(dir: $0) }
    }

    func giveConsent(who: String) {
        guard let dir else { return }
        let date = ISO8601DateFormatter.string(from: Date(), timeZone: .current, formatOptions: [.withFullDate])
        if Core.writeTestsetConsent(dir: dir, level: chosenLevel.rawValue, date: date, who: who) {
            Timings.note("testset: consent \(chosenLevel.rawValue) saved")
        } else {
            notice = String(localized: "A beleegyezést nem sikerült menteni.")
        }
        refresh()
    }

    func toggleRecording() {
        recording ? stopAndSave() : startRecording()
    }

    /// The dictation key while this window is in front: holding it records the
    /// current take, letting go saves it, Esc throws it away. Returns whether the
    /// key was used here (it is not, outside the recording step).
    func keyDown() -> Bool {
        guard status?.consent != nil, current != nil else { return false }
        if !recording { startRecording() }
        return true
    }

    func keyUp() -> Bool {
        guard recording else { return false }
        stopAndSave()
        return true
    }

    func discardTake() -> Bool {
        guard recording else { return false }
        recording = false
        level = 0
        if fakeClip == nil { _ = recorder.stop(); recorder.release() }
        Timings.note("testset: take discarded")
        return true
    }

    private func startRecording() {
        guard current != nil else { return }
        notice = nil
        if fakeClip == nil {
            do { try recorder.start() } catch {
                notice = String(localized: "A mikrofont nem sikerült elindítani.")
                return
            }
        }
        recording = true
    }

    private func stopAndSave() {
        recording = false
        level = 0
        let samples = fakeClip ?? recorder.stop()
        if fakeClip == nil { recorder.release() }
        guard let dir, let take = current else { return }
        let result = Core.saveTestTake(dir: dir, id: take.id, whispered: take.whispered, samples: samples)
        Timings.note(String(format: "testset: take %@ %.2f s -> %@", take.key, Double(samples.count) / 16_000,
                            String(describing: result)))
        switch result {
        case .saved: notice = nil
        case .tooShort: notice = String(localized: "Túl rövid lett, mondd el újra.")
        case .writeFailed, .badArgument: notice = String(localized: "A felvételt nem sikerült menteni.")
        }
        refresh()
    }

    func openFolder() {
        guard let dir else { return }
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        NSWorkspace.shared.open(dir)
    }

    private static func readWav(_ path: String) -> [Float]? {
        guard let file = try? AVAudioFile(forReading: URL(fileURLWithPath: path)),
              file.processingFormat.sampleRate == 16_000, file.processingFormat.channelCount == 1,
              let buf = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: AVAudioFrameCount(file.length)),
              (try? file.read(into: buf)) != nil, let ch = buf.floatChannelData?[0]
        else { return nil }
        return Array(UnsafeBufferPointer(start: ch, count: Int(buf.frameLength)))
    }
}

struct TestSetView: View {
    @ObservedObject var session: TestSetSession
    @State private var who = ""

    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.l) {
            Text("Tesztfelvétel").font(DS.Font.title)
            if session.dir == nil {
                speakerStep
            } else if session.status?.consent == nil {
                consentStep
            } else if let take = session.current {
                recordStep(take)
            } else {
                finishedStep
            }
            if let notice = session.notice {
                Text(notice).font(DS.Font.meta).foregroundStyle(DS.Color.warning)
            }
            Spacer(minLength: 0)
        }
        .padding(DS.Space.xl)
        .frame(width: 560, height: 420, alignment: .topLeading)
    }

    private var speakerStep: some View {
        VStack(alignment: .leading, spacing: DS.Space.m) {
            Text("Adj meg egy álnevet vagy monogramot. Ez lesz a felvételeid mappájának neve.")
                .font(DS.Font.body)
            TextField("Álnév", text: $session.speaker).frame(width: 240)
            Button("Tovább") { session.openSpeaker() }
                .keyboardShortcut(.defaultAction)
                .disabled(session.speaker.trimmingCharacters(in: .whitespaces).isEmpty)
        }
    }

    private var consentStep: some View {
        VStack(alignment: .leading, spacing: DS.Space.m) {
            Text("Kb. 130 rövid mondatot olvasol fel, 20-at másodszor suttogva is. A felvételek ezen a Macen maradnak, a Pusmogo semmit nem tölt fel.")
                .font(DS.Font.body)
                .fixedSize(horizontal: false, vertical: true)
            Picker("", selection: $session.chosenLevel) {
                Text("Csak helyben: csak mérésre, ezen a gépen").tag(TestSetSession.Level.local)
                Text("Megosztás: tanításra és nyilvános tesztkészletbe, álnévvel").tag(TestSetSession.Level.release)
            }
            .pickerStyle(.radioGroup)
            .labelsHidden()
            Text("Bármikor meggondolhatod magad: töröld a mappát. Ami már megosztott adatkészletbe került, azt utólag nem lehet teljesen visszavonni.")
                .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                .fixedSize(horizontal: false, vertical: true)
            HStack {
                TextField("Monogram vagy álnév", text: $who).frame(width: 200)
                Button("Elfogadom") { session.giveConsent(who: who) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(who.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
    }

    private func recordStep(_ take: Core.TestTake) -> some View {
        VStack(alignment: .leading, spacing: DS.Space.m) {
            HStack(spacing: DS.Space.s) {
                Text(String(format: String(localized: "%d / %d"), session.doneCount + 1, session.status?.takes.count ?? 0))
                    .font(DS.Font.mono).foregroundStyle(DS.Color.quiet)
                if take.whispered {
                    Text("Suttogva")
                        .font(DS.Font.label)
                        .padding(.horizontal, DS.Space.s).padding(.vertical, 2)
                        .background(RoundedRectangle(cornerRadius: DS.Radius.control).fill(DS.Color.accent.opacity(0.14)))
                }
            }
            Text(take.text)
                .font(.system(size: 22, weight: .medium))
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, minHeight: 120, alignment: .topLeading)
            HStack(spacing: DS.Space.m) {
                Button(session.recording ? "Kész" : "Felvétel") { session.toggleRecording() }
                    .keyboardShortcut(.space, modifiers: [])
                    .controlSize(.large)
                if session.recording {
                    ProgressView(value: Double(min(1, session.level * 1.4)))
                        .frame(width: 140)
                }
                Spacer()
                Button("Mappa") { session.openFolder() }
            }
            Text(session.recording
                 ? "Olvasd fel a mondatot, aztán nyomd meg a szóközt (vagy engedd el az fn-t)."
                 : "Nyomd meg a szóközt vagy tartsd lenyomva az fn-t, és olvasd fel a mondatot úgy, ahogy írva van.")
                .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
        }
    }

    private var finishedStep: some View {
        VStack(alignment: .leading, spacing: DS.Space.m) {
            Text("Kész vagy, köszönjük. Minden mondat fel van véve.").font(DS.Font.body)
            Button("Mappa megnyitása") { session.openFolder() }
        }
    }
}

@MainActor
enum TestSetWindow {
    private static var window: NSWindow?
    private static var session: TestSetSession?
    /// Set when the window was opened by `PUSMOGO_TESTSET`: closing it quits.
    nonisolated static let launchSpeaker = ProcessInfo.processInfo.environment["PUSMOGO_TESTSET"]

    /// Routes a dictation-key intent to the take in progress while the window
    /// is in front (or open in an unattended clip check), so fn records a line
    /// instead of dictating into another app. Returns false to let it dictate.
    static func route(_ intent: KeyboardTap.Intent, idle: Bool) -> Bool {
        guard let w = window, w.isVisible, let s = session else { return false }
        let unattended = ProcessInfo.processInfo.environment["PUSMOGO_TESTSET_WAV"] != nil
        guard w.isKeyWindow || unattended else { return false }
        switch intent {
        case .start, .startLocked: return idle && s.keyDown()
        case .stop: return s.keyUp()
        case .lock: return s.recording
        case .cancel, .discard: return s.discardTake()
        default: return false
        }
    }

    static func show(speaker: String = "", micUID: String?) {
        let s = session ?? TestSetSession(speaker: speaker, micUID: micUID)
        session = s
        let w = window ?? {
            let w = EscClosingWindow(contentRect: NSRect(x: 0, y: 0, width: 560, height: 420),
                                     styleMask: [.titled, .closable], backing: .buffered, defer: false)
            w.title = String(localized: "Tesztfelvétel")
            w.isReleasedWhenClosed = false
            w.contentView = NSHostingView(rootView: TestSetView(session: s))
            w.center()
            window = w
            if launchSpeaker != nil {
                NotificationCenter.default.addObserver(forName: NSWindow.willCloseNotification, object: w, queue: .main) { _ in
                    exit(0)
                }
            }
            return w
        }()
        // An unattended check (a clip instead of the microphone) is driven through
        // Accessibility: never take focus from the app the user is typing in.
        if ProcessInfo.processInfo.environment["PUSMOGO_TESTSET_WAV"] != nil { w.orderFrontRegardless(); return }
        NSApp.activate(ignoringOtherApps: true)
        w.makeKeyAndOrderFront(nil)
    }
}
