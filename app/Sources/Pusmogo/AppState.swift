import AppKit
import AVFoundation
import Combine

/// v1: Hungarian or English, chosen by the user (fn + ⇧ swaps). No auto-detect.
enum DictationLanguage: String, CaseIterable, Identifiable {
    case hungarian = "hu", english = "en"
    var id: String { rawValue }
    var code: String { rawValue }
    var tag: String { rawValue.uppercased() }
    var label: String { self == .hungarian ? "Magyar" : "English" }
    var flipped: DictationLanguage { self == .hungarian ? .english : .hungarian }
}

enum Phase: Equatable {
    case idle
    case loading
    case listening(locked: Bool)
    case thinking
    case done
    case message(String)
    case info(String)
}

struct HistoryEntry: Identifiable {
    let id = UUID()
    /// Row id in the profile; nil until the write lands (or if it failed).
    var storedId: Int64?
    let date: Date
    let text: String
    let app: String
    /// What was said before spoken self-corrections were dropped from `text`;
    /// nil when nothing was dropped.
    var spoken: String? = nil
    var timings: Timings
}

/// The dictation loop and everything the UI shows.
/// Fn down → listen; Fn up → stop → transcribe → insert.
@MainActor
final class AppState: ObservableObject {
    @Published var phase: Phase = .idle {
        didSet {
            switch phase {
            case .listening:
                if case .listening = oldValue {} else { liveText = "" }
                editSuggestion = nil  // a new dictation replaces the offer
            case .thinking: break
            default: liveText = ""
            }
            if phase != .thinking { redoLanguage = nil }
            flowBar?.update()
            if case .listening = oldValue, phase == .idle { scheduleIdleUnload() }
            if oldValue == .thinking { scheduleIdleUnload() }
        }
    }
    @Published var levels: [Float] = []
    /// Text of the streaming chunks transcribed so far, shown above the bar
    /// while the user is still speaking (long dictations only: the first chunk
    /// is cut after about 8 s).
    @Published private(set) var liveText = ""
    /// Set while a clip is transcribed again because fn + ⇧ changed the
    /// language mid-dictation (the whole clip is redone, so the wait is longer):
    /// the bar shows the new language beside its dots instead of a caption in
    /// the old one.
    @Published private(set) var redoLanguage: DictationLanguage?
    @Published var engineName = "betöltés…"
    /// Whether the speech model is in memory (it is freed after idle minutes).
    @Published private(set) var engineLoaded = false
    @Published var lastTimings: Timings?
    /// A word the user corrected in a dictation just typed (C10), waiting for
    /// an "Add to dictionary?" answer.
    @Published var editSuggestion: EditWatcher.Suggestion? {
        didSet { flowBar?.update() }
    }
    @Published var history: [HistoryEntry] = []
    @Published var dictionary: [Core.DictionaryEntry] = []
    /// Where a damaged profile was moved when a fresh one replaced it at
    /// launch; Home says so until the user dismisses it.
    @Published var profileMovedAside: String?
    /// The profile was written by a newer Pusmogo and left untouched: History,
    /// Dictionary and Snippets are unavailable until the app is updated.
    @Published var profileFromNewerVersion = false
    /// Saved snippets, oldest first (core `snippets.rs`).
    @Published var snippets: [Core.Snippet] = []
    /// How many of the oldest dictionary entries no longer reach the engine's
    /// prompt (their replacements still apply). Known only once the model has
    /// been loaded with the current dictionary.
    @Published private(set) var dictionaryLeftOut: Int?
    /// How long dictations stay in the history, in days; 0 keeps them all.
    @Published private(set) var historyDays = 0
    /// Profile settings read by the local MCP server: AI apps may search the
    /// history / read the dictionary and snippets. Both off by default.
    @Published private(set) var shareHistory = false
    @Published private(set) var shareDictionary = false
    /// `PUSMOGO_PREVIEW_DENIED=1` shows both permissions as missing (`mic` or
    /// `ax` only that one), so the denied state of the UI can be screenshotted
    /// on a Mac that granted them. It never prompts and never changes the real
    /// permissions.
    private static let previewDeniedValue = ProcessInfo.processInfo.environment["PUSMOGO_PREVIEW_DENIED"]
    /// A test run that does its job and quits by itself.
    static let oneShotTest = ["PUSMOGO_SELFTEST_WAV", "PUSMOGO_INSERT_TEST", "PUSMOGO_MIC_TEST", "PUSMOGO_TESTSET"]
        .contains { ProcessInfo.processInfo.environment[$0] != nil }
    static let previewDenied = ["1", "mic", "ax"].contains(previewDeniedValue ?? "")
    @Published var micGranted = previewDenied
        ? previewDeniedValue == "ax" : AudioRecorder.micGranted
    @Published var accessibilityGranted = previewDenied
        ? previewDeniedValue == "mic" : Permissions.accessibilityGranted(prompt: false)
    @Published var fnPressed = false
    @Published var dictationCount = 0
    let models = ModelStore()
    /// large-v3, Hungarian's model above 8 GB of memory. Fetched in the background
    /// once turbo is in place (`fetchAccurateModelIfWanted`); until it lands,
    /// Hungarian dictations use turbo.
    let accurateModel = ModelStore(model: .largeV3)
    /// Pusmogo HU, our Hungarian-only fine-tune: Hungarian's default on every Mac.
    /// Fetched in the background like large-v3; until it lands, Hungarian uses turbo.
    let huModel = ModelStore(model: .pusmogoHu)

    /// The store that downloads `model`.
    func store(for model: Core.SpeechModel) -> ModelStore {
        switch model {
        case .turbo: return models
        case .largeV3: return accurateModel
        case .pusmogoHu: return huModel
        }
    }

    @Published var language: DictationLanguage {
        didSet { UserDefaults.standard.set(language.rawValue, forKey: "language") }
    }
    @Published var soundsOn: Bool {
        didSet { UserDefaults.standard.set(soundsOn, forKey: "sounds"); sounds.enabled = soundsOn }
    }
    /// Pause playing music or video while dictating (PLAN: on by default).
    @Published var pauseMedia: Bool {
        didSet { UserDefaults.standard.set(pauseMedia, forKey: "pauseMedia") }
    }
    /// Caption previews from about 4 s into a dictation instead of after the
    /// first chunk (8 s or more). On by default; see `Core.captionPreviews`.
    @Published var captionPreviews: Bool {
        didSet { UserDefaults.standard.set(captionPreviews, forKey: "captionPreviews") }
    }
    /// Reads the focused field's text before the caret as spelling hints for
    /// the coming dictation (local only, never stored). On by default; see
    /// `Core.contextHints(setting:)`.
    @Published var contextHints: Bool {
        didSet { UserDefaults.standard.set(contextHints, forKey: "contextHints") }
    }
    /// Drops a spoken self-correction ("háromkor, nem, négykor" → "négykor")
    /// from the typed text. On by default; see `Core.selfCorrections`.
    @Published var selfCorrections: Bool {
        didSet { UserDefaults.standard.set(selfCorrections, forKey: "selfCorrections"); core.refreshSelfCorrections() }
    }
    @Published var showIdleBar: Bool {
        didSet { UserDefaults.standard.set(showIdleBar, forKey: "showIdleBar"); flowBar?.update() }
    }
    /// The mouse is over the idle bar, which grows into a pill with buttons
    /// (`FlowBarController` tracks the mouse; only while idle).
    @Published var barHovered = false
    /// macOS parked the menu bar icon off screen or under the notch: the
    /// hovered bar then offers the panel, and Home says how to make room.
    /// `PUSMOGO_PREVIEW_HIDDEN_ICON=1` starts it hidden (screenshots).
    @Published var menuIconHidden = ProcessInfo.processInfo.environment["PUSMOGO_PREVIEW_HIDDEN_ICON"] == "1"
    /// Keep the newest 20 dictations' audio and text on this Mac (Settings),
    /// so a real failure can be replayed. Off by default; see `Core.keepDebugAudio`.
    @Published var keepDebugAudio: Bool {
        didSet { UserDefaults.standard.set(keepDebugAudio, forKey: "keepDebugAudio"); core.refreshDebugDir() }
    }
    /// Second push-to-talk key next to fn (keyboards without fn).
    @Published var altKey: KeyboardTap.AltKey {
        didSet { UserDefaults.standard.set(altKey.rawValue, forKey: "altKey"); keyboard?.altKey = altKey }
    }

    /// UID of the chosen microphone; empty means the system default input.
    @Published var micUID: String {
        didSet {
            UserDefaults.standard.set(micUID, forKey: "micUID")
            recorder.preferredDeviceUID = micUID.isEmpty ? nil : micUID
            updateMicReady()
        }
    }

    /// When the microphone stays running between dictations (see
    /// `AudioRecorder.warm`): automatically for an input that measured slow
    /// to start, always, or never. macOS shows the microphone indicator
    /// while it runs; the audio stays in memory and is overwritten.
    enum MicReady: String, CaseIterable { case auto, always, never }
    @Published var micReady: MicReady = .auto {
        didSet { UserDefaults.standard.set(micReady.rawValue, forKey: "micReady"); updateMicReady() }
    }
    /// An input that needed longer than this to deliver its first audio is
    /// kept running in the automatic mode. Built-in microphones measured
    /// well under it, a USB display microphone about 450 ms.
    static let slowMicStartMs = 150.0

    /// Measured start-up time per input UID, from cold starts only.
    private var micStartupByUID: [String: Double] {
        get { UserDefaults.standard.dictionary(forKey: "micStartupMs") as? [String: Double] ?? [:] }
        set { UserDefaults.standard.set(newValue, forKey: "micStartupMs") }
    }

    /// True while setup runs: the try-it step must not lose its first word
    /// before the input was ever measured.
    var micReadyForSetup = false {
        didSet { updateMicReady() }
    }

    func updateMicReady() {
        guard micGranted, !Self.previewDenied else { recorder.setWarm(false); return }
        // Never hold a Bluetooth microphone, not even to measure it.
        guard !recorder.usesBluetooth else { recorder.setWarm(false); return }
        let uid = recorder.currentDeviceUID
        let known = uid.flatMap { micStartupByUID[$0] }
        if micReady == .auto, known == nil, !recorder.warm {
            // An input not measured yet: start it once without recording;
            // `onWarmStartup` stores the time and decides again.
            recorder.setWarm(true)
            Timings.note("mic ready: measuring \(uid ?? "?")")
            return
        }
        let measured = known ?? 0
        let on = micReady == .always || micReadyForSetup || (micReady == .auto && measured > Self.slowMicStartMs)
        if on != recorder.warm {
            recorder.setWarm(on)
            Timings.note("mic ready \(on ? "on" : "off") (\(micReady.rawValue), start \(Int(measured)) ms)")
        }
    }

    private func noteWarmStartup(_ ms: Double) {
        guard ms > 0, let uid = recorder.currentDeviceUID, micStartupByUID[uid] == nil else { return }
        micStartupByUID[uid] = ms
        Timings.note("mic ready: \(uid) starts in \(Int(ms)) ms")
        updateMicReady()
    }

    private func noteMicStartup(_ ms: Double?) {
        guard let ms, ms > 0, !recorder.warm, let uid = recorder.currentDeviceUID else { return }
        micStartupByUID[uid] = ms
        updateMicReady()
    }
    /// Inputs shown in Settings; refreshed when the tab appears.
    @Published private(set) var micDevices: [MicDevice] = []

    func refreshMicDevices() {
        micDevices = MicDevice.all()
        // `PUSMOGO_PREVIEW_MIC_NAME=<name>` adds a fake input (uid "preview"), so
        // Settings can be checked with a long device name; launch with
        // `-micUID preview` to show it selected without saving the choice.
        if let name = ProcessInfo.processInfo.environment["PUSMOGO_PREVIEW_MIC_NAME"] {
            micDevices.append(MicDevice(id: 0, uid: "preview", name: name, bluetooth: false))
        }
    }

    /// Shows the kept dictations in Finder (the folder is created if empty).
    func openDebugFolder() {
        let dir = Core.debugDirectory
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        NSWorkspace.shared.open(dir)
    }

    /// `PUSMOGO_PREVIEW_BLUETOOTH=1` treats the microphone in use as a
    /// Bluetooth one, so the Settings hint can be checked without a headset.
    static let previewBluetooth = ProcessInfo.processInfo.environment["PUSMOGO_PREVIEW_BLUETOOTH"] == "1"

    /// The input dictation records from (the chosen one, else the system
    /// default) is a Bluetooth device.
    var micIsBluetooth: Bool {
        if Self.previewBluetooth { return true }
        let device = micUID.isEmpty ? MicDevice.automatic() : micDevices.first { $0.uid == micUID }
        return device?.bluetooth ?? false
    }

    /// Name of the wired microphone used in place of a Bluetooth system default.
    var micInsteadOfBluetooth: String? {
        guard micUID.isEmpty, let used = MicDevice.automatic(), !used.bluetooth,
              used.id != MicDevice.defaultInputID() else { return nil }
        return used.name
    }

    var lastText: String? { history.first?.text }

    private let recorder = AudioRecorder()
    private let core = Core()
    private let sounds = Sounds()
    private var keyboard: KeyboardTap?
    private var flowBar: FlowBarController?
    private var permissionPoll: Timer?
    private var lastLevelAt: Double = 0
    private var targetApp = ""
    /// Writing style for the app in front at key-down: category and style.
    private var targetStyle: (category: Int32, style: Int32) = (0, 0)
    /// The focused window's title when the app in front is a code editor or
    /// terminal, for context hints only; cleared at the next key down.
    private var targetCodeTitle: String?
    /// The last app other than Pusmogo that was in front: where a History
    /// entry goes back to when the panel takes focus.
    private var previousApp: NSRunningApplication?
    /// Name and bundle id of the app in front before the panel opened (the
    /// Style tab offers to map it to a category).
    var previousAppIdentity: (name: String, bundleID: String)? {
        guard let app = previousApp, !app.isTerminated, let id = app.bundleIdentifier else { return nil }
        return (app.localizedName ?? id, id)
    }
    private var activationObserver: NSObjectProtocol?
    private var idleUnload: Timer?
    private var streamTimer: Timer?
    private var streamSource: (() -> [Float])?
    private var streamLanguage: DictationLanguage?

    /// Long dictations are transcribed in chunks while the user speaks, so key
    /// up only waits for the tail. `PUSMOGO_STREAM=0` turns it off (A/B tests).
    private let streaming = ProcessInfo.processInfo.environment["PUSMOGO_STREAM"] != "0"

    /// Free the model after this long without dictation. `PUSMOGO_IDLE_UNLOAD`
    /// (seconds) overrides it for testing.
    private let idleUnloadAfter: TimeInterval =
        ProcessInfo.processInfo.environment["PUSMOGO_IDLE_UNLOAD"].flatMap(TimeInterval.init) ?? 10 * 60

    init() {
        let d = UserDefaults.standard
        d.register(defaults: ["language": DictationLanguage.hungarian.rawValue, "sounds": true, "showIdleBar": true, "pauseMedia": true,
                                    "captionPreviews": true, "contextHints": true, "selfCorrections": true])
        language = DictationLanguage(rawValue: d.string(forKey: "language") ?? "") ?? .hungarian
        soundsOn = d.bool(forKey: "sounds")
        showIdleBar = d.bool(forKey: "showIdleBar")
        pauseMedia = d.bool(forKey: "pauseMedia")
        captionPreviews = d.bool(forKey: "captionPreviews")
        contextHints = d.bool(forKey: "contextHints")
        selfCorrections = d.bool(forKey: "selfCorrections")
        keepDebugAudio = Core.keepDebugAudio
        // Earlier builds stored an on/off switch for Right ⌥ only.
        let legacy: KeyboardTap.AltKey = d.bool(forKey: "rightOptionPTT") ? .rightOption : .none
        altKey = d.string(forKey: "altKey").flatMap(KeyboardTap.AltKey.init) ?? legacy
        micUID = d.string(forKey: "micUID") ?? ""
        micReady = d.string(forKey: "micReady").flatMap(MicReady.init) ?? .auto
        recorder.onWarmStartup = { [weak self] ms in self?.noteWarmStartup(ms) }
        recorder.onInputChange = { [weak self] in self?.updateMicReady() }
        sounds.enabled = soundsOn
        recorder.preferredDeviceUID = micUID.isEmpty ? nil : micUID

        flowBar = FlowBarController(state: self)
        // The hover preview runs next to a real copy: no key tap, and it quits by itself.
        let env = ProcessInfo.processInfo.environment
        if let hover = env["PUSMOGO_PREVIEW_HOVER"] {
            let secs = env["PUSMOGO_PREVIEW_SECS"].flatMap(Double.init) ?? 12
            NSLog("hover preview (\(hover)): quitting in \(secs) s")
            // `offer` / `learned` show the dictionary offer instead (screenshots).
            if hover == "offer" || hover == "learned" {
                editSuggestion = EditWatcher.Suggestion(wrong: "Kubernetis", right: "Kubernetes", learned: hover == "learned")
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + secs) { exit(0) }
        } else if Self.oneShotTest, env["PUSMOGO_SELFTEST_KEYS"] != "1" {
            // One-shot tests often run next to the user's own copy: a key tap
            // here would also record and paste the user's real dictations.
            NSLog("one-shot test: no key tap (PUSMOGO_SELFTEST_KEYS=1 adds it)")
        } else {
            keyboard = KeyboardTap { [weak self] intent in
                MainActor.assumeIsolated { self?.handle(intent) }
            }
        }
        keyboard?.altKey = altKey
        keyboard?.onFnChange = { [weak self] down in
            MainActor.assumeIsolated { self?.fnPressed = down }
        }

        trackPreviousApp()
        SingleInstance.observeHandOver { [weak self] in
            guard let self else { return }
            PanelWindow.show(state: self)
        }
        EditWatcher.onSuggestion = { [weak self] s in
            // The words stay out of the log unless asked for (tests).
            let words = ProcessInfo.processInfo.environment["PUSMOGO_DEBUG_EDITS"] == "1" ? ": \(s.wrong) -> \(s.right)" : ""
            Timings.note("edit-watch: correction found\(words)")
            self?.noticeCorrection(s)
        }

        recorder.onLevel = { [weak self] level in
            DispatchQueue.main.async { self?.pushLevel(level) }
        }

        if let secs = ProcessInfo.processInfo.environment["PUSMOGO_MIC_TEST"].flatMap(Double.init) {
            runMicTest(seconds: secs)
            return
        }
        // No engine for the insert test: exiting while the model loads hangs.
        if let text = ProcessInfo.processInfo.environment["PUSMOGO_INSERT_TEST"] {
            // A scratch profile lets the "Add to dictionary?" offer be accepted.
            if ProcessInfo.processInfo.environment["PUSMOGO_PROFILE"] != nil { openProfile() }
            runInsertTest(text)
            return
        }
        // Before the engine load, so the first load already gets the dictionary.
        openProfile()
        core.onLoadedChange = { [weak self] loaded in
            self?.engineLoaded = loaded
            NSLog("engine: \(loaded ? "loaded" : "unloaded")")
        }
        core.onDictionaryLeftOut = { [weak self] n in
            self?.dictionaryLeftOut = n
            if let n, n > 0 { NSLog("dictionary: \(n) entries left out of the prompt") }
        }
        core.onLiveText = { [weak self] text in
            guard let self, case .listening = self.phase else { return }
            self.liveText = text
            if self.selfTest { Timings.note("live text: \(text)") }
        }
        loadEngine()
        models.onInstalled = { [weak self] in
            self?.loadEngine()
            self?.fetchAccurateModelIfWanted()
        }
        models.onDetectorInstalled = { [weak self] in self?.reloadForDetector() }
        models.fetchSpeechDetector()
        accurateModel.onInstalled = { [weak self] in self?.accurateModelInstalled(.largeV3) }
        huModel.onInstalled = { [weak self] in self?.accurateModelInstalled(.pusmogoHu) }
        fetchAccurateModelIfWanted()

        if ProcessInfo.processInfo.environment["PUSMOGO_DEMO"] != nil {
            runDemo()
            return
        }
        if let speaker = TestSetWindow.launchSpeaker {
            TestSetWindow.show(speaker: speaker, micUID: micUID.isEmpty ? nil : micUID)
            return
        }
        if let wav = ProcessInfo.processInfo.environment["PUSMOGO_SELFTEST_WAV"] {
            // With PUSMOGO_ONBOARDING_STEP=3 the clip is dictated into the try-it field.
            if Onboarding.startStep != nil { Onboarding.show(state: self) }
            // With PUSMOGO_PANEL a Settings change can be made (through AX) before the clip plays.
            // Not inside this init: SwiftUI is still making the state here and aborts.
            else if PanelView.startTab != nil { DispatchQueue.main.async { [weak self] in self.map { PanelWindow.show(state: $0) } } }
            runSelfTest(wavPath: wav)
            return
        }
        if PanelView.startTab != nil {
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                PanelWindow.show(state: self)
            }
        } else if Onboarding.isNeeded {
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                Onboarding.show(state: self)
            }
        } else if !Self.previewDenied {
            accessibilityGranted = Permissions.accessibilityGranted(prompt: true)
            Task { self.micGranted = await AudioRecorder.requestMicAccess() }
        }
        DispatchQueue.main.async { [weak self] in self?.updateMicReady() }
        permissionPoll = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.refreshPermissions() }
        }
        // The status item is laid out a moment after launch. Other apps add
        // their icons later and displays come and go, so look again on a
        // display change and every few minutes until the notice was shown.
        DispatchQueue.main.asyncAfter(deadline: .now() + 3) { [weak self] in
            self?.noticeHiddenIcon()
        }
        iconCheck = Timer.scheduledTimer(withTimeInterval: Self.iconCheckInterval, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.noticeHiddenIcon() }
        }
        NotificationCenter.default.addObserver(
            forName: NSApplication.didChangeScreenParametersNotification, object: nil, queue: .main
        ) { [weak self] _ in
            // The menu bar is laid out again a moment after the change.
            DispatchQueue.main.asyncAfter(deadline: .now() + 2) { self?.noticeHiddenIcon() }
        }
    }

    private var iconCheck: Timer?
    private var iconNoticeShown = false
    /// `PUSMOGO_ICON_CHECK_SECONDS` shortens it for tests.
    private static let iconCheckInterval: TimeInterval =
        Double(ProcessInfo.processInfo.environment["PUSMOGO_ICON_CHECK_SECONDS"] ?? "") ?? 180

    /// On a full menu bar macOS hides status items behind the notch, leaving
    /// History and Settings out of reach. Every check updates `menuIconHidden`
    /// (the hover bar's panel button, Home's help card); the bar notice naming
    /// ⌃⌥P shows once per launch, at the next check if a dictation is running.
    /// `PUSMOGO_PREVIEW_HIDDEN_ICON=1` forces it.
    private func noticeHiddenIcon() {
        let forced = ProcessInfo.processInfo.environment["PUSMOGO_PREVIEW_HIDDEN_ICON"] == "1"
        let hidden = forced || StatusItemVisibility.isHidden()
        if menuIconHidden != hidden { menuIconHidden = hidden }
        guard hidden, !iconNoticeShown else { return }
        guard phase == .idle else {
            NSLog("menu bar: icon hidden, notice postponed (busy)")
            return
        }
        NSLog("menu bar: icon hidden, showing the ⌃⌥P notice")
        iconNoticeShown = true
        show("Rejtett menüikon. Panel: ⌃⌥P", seconds: 6)
    }

    // MARK: - Hotkey intents

    private func handle(_ intent: KeyboardTap.Intent) {
        if PusmogoDebug.keys { NSLog("keys: intent \(intent)") }
        if TestSetWindow.route(intent, idle: phase == .idle) {
            Timings.note("testset: key \(intent)")
            return
        }
        switch intent {
        case .start: start()
        case .startLocked: start(locked: true)
        case .stop: stop()
        case .lock: lock()
        case .cancel: cancel()
        case .discard: discard()
        case .toggleLanguage: toggleLanguage()
        case .openPanel: PanelWindow.show(state: self)
        case .pasteLast: if let lastText { TextInserter.insert(lastText) }
        case .copyLast:
            if let lastText {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(lastText, forType: .string)
            }
        }
    }

    /// History survives restarts in the profile. `PUSMOGO_PROFILE=<path>`
    /// points tests at a scratch file instead of the user's.
    private func openProfile() {
        let path = Self.profilePath(ProcessInfo.processInfo.environment)
        core.openProfile(path: path, limit: Self.historyLimit) { [weak self] stored, notice in
            guard let self else { return }
            switch notice {
            case .movedAside(let movedAside): self.profileMovedAside = movedAside
            case .newerVersion: self.profileFromNewerVersion = true
            case nil: break
            }
            let loaded = stored.map(Self.entry)
            // Dictations finished while the profile was opening stay on top.
            self.history = Array((self.history + loaded).prefix(Self.historyLimit))
        }
        core.dictionaryEntries { [weak self] in self?.dictionary = $0 }
        // Self-test only: `PUSMOGO_SELFTEST_SNIPPET=<trigger><TAB><text>` saves a
        // snippet into the test's own profile before the clip plays.
        let env = ProcessInfo.processInfo.environment
        if env["PUSMOGO_SELFTEST_WAV"] != nil, let seed = env["PUSMOGO_SELFTEST_SNIPPET"] {
            let parts = seed.split(separator: "\t", maxSplits: 1).map(String.init)
            if parts.count == 2 {
                core.setSnippet(trigger: parts[0], text: parts[1].replacingOccurrences(of: "\\n", with: "\n")) {
                    Timings.note("self-test: snippet saved: \($0 != nil)")
                }
            }
        }
        // Self-test only: `PUSMOGO_SELFTEST_SNIPPETS_IMPORT=<json>` imports a
        // snippet file, then `PUSMOGO_SELFTEST_SNIPPETS_EXPORT=<json>` writes
        // the list back out (both through the panel's own code path).
        if env["PUSMOGO_SELFTEST_WAV"] != nil {
            if let path = env["PUSMOGO_SELFTEST_SNIPPETS_IMPORT"] {
                core.importSnippets(from: path) { _ in }
            }
            if let path = env["PUSMOGO_SELFTEST_SNIPPETS_EXPORT"] {
                core.exportSnippets(to: path) { _ in }
            }
        }
        // Self-test only: `PUSMOGO_SELFTEST_SHARE=history,dictionary` turns those
        // share settings on through the Settings code path.
        if env["PUSMOGO_SELFTEST_WAV"] != nil, let share = env["PUSMOGO_SELFTEST_SHARE"] {
            let parts = share.split(separator: ",").map(String.init)
            if parts.contains("history") { setShareHistory(true) }
            if parts.contains("dictionary") { setShareDictionary(true) }
        }
        core.snippets { [weak self] in self?.snippets = $0 }
        core.historyDays { [weak self] in self?.historyDays = $0 }
        loadShareSettings()
    }

    private func loadShareSettings() {
        core.shareSetting(Core.shareHistoryKey) { [weak self] in self?.shareHistory = $0 }
        core.shareSetting(Core.shareDictionaryKey) { [weak self] in self?.shareDictionary = $0 }
    }

    func setShareHistory(_ on: Bool) {
        shareHistory = on
        core.setShareSetting(Core.shareHistoryKey, on: on) { [weak self] ok in
            Timings.note("profile: share history \(on ? "on" : "off"), saved \(ok)")
            if !ok { self?.shareHistory = !on }
        }
    }

    func setShareDictionary(_ on: Bool) {
        shareDictionary = on
        core.setShareSetting(Core.shareDictionaryKey, on: on) { [weak self] ok in
            Timings.note("profile: share dictionary \(on ? "on" : "off"), saved \(ok)")
            if !ok { self?.shareDictionary = !on }
        }
    }

    /// The self-test never writes the user's History: without
    /// `PUSMOGO_PROFILE` it gets an empty profile of its own, removed on exit.
    private static func profilePath(_ env: [String: String]) -> String {
        if let path = env["PUSMOGO_PROFILE"] { return path }
        guard env["PUSMOGO_SELFTEST_WAV"] != nil else { return Core.defaultProfilePath }
        let path = selfTestProfilePath()
        removeSelfTestProfile()
        atexit { AppState.removeSelfTestProfile() }
        return path
    }

    private nonisolated static func selfTestProfilePath() -> String {
        (NSTemporaryDirectory() as NSString).appendingPathComponent("pusmogo-selftest-\(getpid()).sqlite")
    }

    private nonisolated static func removeSelfTestProfile() {
        let path = selfTestProfilePath()
        for suffix in ["", "-wal", "-shm", "-journal"] { try? FileManager.default.removeItem(atPath: path + suffix) }
    }

    private static func entry(_ d: Core.StoredDictation) -> HistoryEntry {
        var t = Timings()
        t.date = Date(timeIntervalSince1970: TimeInterval(d.createdAt))
        t.totalMs = Double(d.latencyMs)
        t.chars = d.text.count
        return HistoryEntry(storedId: d.id, date: t.date, text: d.text, app: d.app ?? "", spoken: d.spoken, timings: t)
    }

    // MARK: - History

    /// Searches the whole stored history, not only the entries in memory.
    func searchHistory(_ query: String, done: @escaping @MainActor ([HistoryEntry]) -> Void) {
        core.searchHistory(query, limit: Self.historyLimit) { done($0.map(Self.entry)) }
    }

    /// Removes one dictation from the list and from the profile. An entry
    /// whose write has not landed yet only leaves the list.
    /// When the list was full, the next older dictation moves up from the
    /// profile, so deleting never shrinks the list while older ones exist.
    func deleteHistoryEntry(_ entry: HistoryEntry) {
        let wasFull = history.count >= Self.historyLimit
        history.removeAll { $0.id == entry.id || (entry.storedId != nil && $0.storedId == entry.storedId) }
        guard let id = entry.storedId else { return }
        core.deleteHistory(id: id) { [weak self] _ in
            if wasFull { self?.reloadHistory() }
        }
    }

    /// Saves the retention and drops what it no longer keeps, then reloads
    /// the list from the profile so the panel shows what is really stored.
    func setHistoryDays(_ days: Int) {
        let previous = historyDays
        historyDays = days
        core.setHistoryDays(days) { [weak self] pruned in
            guard let self else { return }
            guard let pruned else {
                Timings.note("profile: history retention not saved")
                self.historyDays = previous
                return
            }
            Timings.note("profile: history kept \(days) days, \(pruned) pruned")
            guard pruned > 0 else { return }
            self.reloadHistory()
        }
    }

    /// Replaces the list with what the profile really stores (newest first).
    /// Dictations whose write has not landed yet stay on top.
    private func reloadHistory() {
        core.searchHistory("", limit: Self.historyLimit) { [weak self] stored in
            guard let self else { return }
            let pending = self.history.filter { $0.storedId == nil }
            self.history = Array((pending + stored.map(Self.entry)).prefix(Self.historyLimit))
        }
    }

    func clearHistory() {
        history.removeAll()
        core.clearHistory { ok in if !ok { Timings.note("profile: history not cleared") } }
    }

    // MARK: - Dictionary

    /// Saves an entry and applies it to the engine before the next dictation.
    /// Returns false (through `done`) when the entry was rejected.
    func addDictionaryEntry(from: String, to: String, done: @escaping @MainActor (Bool) -> Void = { _ in }) {
        core.setDictionaryEntry(from: from, to: to) { [weak self] entries in
            if let entries { self?.dictionary = entries }
            done(entries != nil)
        }
    }

    /// Fresh usage numbers for the Insights tab.
    func loadInsights(_ done: @escaping @MainActor (Core.Insights?) -> Void) {
        core.insights(completion: done)
    }

    /// How long the "Add to dictionary?" offer stays on the bar.
    private static let offerSeconds: TimeInterval = 12

    /// Every fix the edit watch sees is recorded in the profile. The second
    /// time the same word is fixed the same way, it goes into the dictionary
    /// without asking and the bar shows it with Undo; otherwise the bar asks.
    private func noticeCorrection(_ s: EditWatcher.Suggestion) {
        core.noteCorrection(wrong: s.wrong, right: s.right) { [weak self] entries in
            guard let self else { return }
            guard let entries else { return self.offer(s) }
            Timings.note("edit-watch: fixed twice, added to the dictionary")
            self.dictionary = entries
            var learned = s
            learned.learned = true
            self.show(learned)
        }
    }

    /// Shows the offer for a corrected word, unless the dictionary already
    /// turns that word into the same correction.
    private func offer(_ s: EditWatcher.Suggestion) {
        let known = dictionary.contains {
            $0.from.lowercased() == s.wrong.lowercased() && $0.to == s.right
        }
        guard !known else { return Timings.note("edit-watch: already in the dictionary") }
        show(s)
    }

    private func show(_ s: EditWatcher.Suggestion) {
        editSuggestion = s
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.offerSeconds) { [weak self] in
            if self?.editSuggestion == s { self?.editSuggestion = nil }
        }
    }

    /// The user accepted the offer: the corrected word goes into the
    /// dictionary, so the next dictation writes it the corrected way.
    func acceptEditSuggestion() {
        guard let s = editSuggestion else { return }
        editSuggestion = nil
        addDictionaryEntry(from: s.wrong, to: s.right) { [weak self] ok in
            Timings.note("edit-watch: offer accepted, \(ok ? "saved" : "not saved")")
            guard ok, let self else { return }
            // Counted for the insights only once the user said yes.
            self.core.addCorrection(wrong: s.wrong, right: s.right)
            self.info(String(localized: "Szótárba felvéve"))
        }
    }

    /// Undo for a word learned without asking: it leaves the dictionary again.
    func undoLearnedSuggestion() {
        guard let s = editSuggestion, s.learned else { return }
        editSuggestion = nil
        removeDictionaryEntry(from: s.wrong)
        Timings.note("edit-watch: learned word undone")
    }

    func dismissEditSuggestion() {
        editSuggestion = nil
        Timings.note("edit-watch: offer dismissed")
    }

    func removeDictionaryEntry(from: String) {
        core.removeDictionaryEntry(from: from) { [weak self] in self?.dictionary = $0 }
    }

    // MARK: - Snippets

    /// Adds a snippet (`id` nil) or changes the one with that id. Returns
    /// false (through `done`) when the trigger or text is empty, or when the
    /// trigger already belongs to another snippet.
    func saveSnippet(id: Int64?, trigger: String, text: String, done: @escaping @MainActor (Bool) -> Void) {
        let finish: @MainActor ([Core.Snippet]?) -> Void = { [weak self] list in
            if let list { self?.snippets = list }
            done(list != nil)
        }
        if let id {
            core.updateSnippet(id: id, trigger: trigger, text: text, completion: finish)
        } else {
            core.setSnippet(trigger: trigger, text: text, completion: finish)
        }
    }

    func deleteSnippet(id: Int64) {
        core.deleteSnippet(id: id) { [weak self] in self?.snippets = $0 }
    }

    func exportSnippets(to url: URL, done: @escaping @MainActor (Bool) -> Void) {
        core.exportSnippets(to: url.path, completion: done)
    }

    /// Adds the snippets of a JSON file; `done` gets (added, skipped), or nil
    /// when the file is not a snippet list.
    func importSnippets(from url: URL, done: @escaping @MainActor ((added: Int, skipped: Int)?) -> Void) {
        core.importSnippets(from: url.path) { [weak self] result in
            if let result { self?.snippets = result.list }
            done(result.map { ($0.added, $0.skipped) })
        }
    }

    // MARK: - Export and import

    /// Saves history, dictionary and settings to one file the user picked.
    func exportProfile(to url: URL, done: @escaping @MainActor (Bool) -> Void) {
        core.exportProfile(to: url.path, completion: done)
    }

    /// Replaces history, dictionary and settings with an exported file, then
    /// reloads everything the panel shows from the profile.
    func importProfile(from url: URL, done: @escaping @MainActor (Bool) -> Void) {
        core.importProfile(from: url.path) { [weak self] ok in
            if ok, let self {
                self.core.searchHistory("", limit: Self.historyLimit) { [weak self] stored in
                    self?.history = stored.map(Self.entry)
                }
                self.core.dictionaryEntries { [weak self] in self?.dictionary = $0 }
                self.core.snippets { [weak self] in self?.snippets = $0 }
                self.core.historyDays { [weak self] in self?.historyDays = $0 }
                self.loadShareSettings()
            }
            done(ok)
        }
    }

    /// How many dictations the list and a search show; older ones stay
    /// stored and searchable.
    static let historyLimit = 200

    func loadEngine() {
        // A cut-short file loads only partly; the model card offers a fresh download.
        if case .damaged = ModelStore.onDisk { return }
        phase = .loading
        let model = Core.SpeechModel.pick(language.code) ?? .turbo
        loadedModel = model
        Timings.note("engine: loading \(model.fileName) for \(language.code)")
        core.load(modelPath: model.path) { [weak self] name in
            guard let self else { return }
            self.engineName = name
            if self.phase == .loading { self.phase = .idle }
            self.scheduleIdleUnload()
            if self.detectorWaiting {
                self.detectorWaiting = false
                self.reloadForDetector()
            }
        }
    }

    private var detectorWaiting = false

    /// The model in the engine (or the one it reloads after an idle unload).
    @Published private(set) var loadedModel: Core.SpeechModel?

    /// Hungarian and English may use different models. After a language swap,
    /// load the other one: in the background when idle, or queued ahead of the
    /// dictation on key down (the core runs calls in order, so the clip is
    /// transcribed by the right model either way). Only one model is in memory.
    private func switchModelIfNeeded() {
        guard let model = Core.SpeechModel.pick(language.code), model != loadedModel,
              loadedModel != nil else { return }
        loadedModel = model
        let t0 = nowMs(), lang = language.code
        Timings.note("engine: switching to \(model.fileName) for \(lang)")
        core.load(modelPath: model.path) { [weak self] name in
            self?.engineName = name
            Timings.note(String(format: "engine: switched to %@ in %.0f ms", model.fileName, nowMs() - t0))
        }
    }

    /// Starts the download of the model the current language wants (Pusmogo HU for
    /// Hungarian, large-v3 after a "most accurate" choice in Settings) once turbo is
    /// in place: turbo comes first because English and every fallback need it. A
    /// failure is only logged and retried at the next launch or language swap; the
    /// dictation keeps working on turbo meanwhile. Test runs fetch only from a test
    /// server (`ModelStore.urlHook`), never from the network.
    func fetchAccurateModelIfWanted(for other: DictationLanguage? = nil) {
        guard Core.hasSpeechEngine, models.status == .ready,
              case let wanted = Core.SpeechModel.wanted((other ?? language).code), wanted != .turbo else { return }
        let env = ProcessInfo.processInfo.environment
        let testRun = ["PUSMOGO_SELFTEST_WAV", "PUSMOGO_DEMO", "PUSMOGO_PREVIEW_HOVER", "PUSMOGO_INSERT_TEST",
                       "PUSMOGO_MIC_TEST", "PUSMOGO_PREVIEW_MODEL", "PUSMOGO_PREVIEW_LARGE_MODEL", "PUSMOGO_PREVIEW_HU_MODEL",
                       "PUSMOGO_TESTSET"].contains { env[$0] != nil }
        if testRun && env[ModelStore.urlHook(wanted)] == nil { return }
        let store = store(for: wanted)
        store.recheck()
        switch store.status {
        case .ready, .downloading: return
        case .missing, .failed: break
        }
        Timings.note("engine: fetching \(wanted.fileName) in the background")
        store.download()
    }

    /// The new model takes over at once when idle, else at the next key down
    /// (`switchModelIfNeeded` runs there too).
    private func accurateModelInstalled(_ model: Core.SpeechModel) {
        Timings.note("engine: \(model.fileName) installed")
        switch phase {
        case .idle, .info: switchModelIfNeeded()
        default: break
        }
    }

    /// The engine reads the speech detector file when it loads. Reload now if idle,
    /// after a load in flight, or else at the next idle reload. Without the model,
    /// `onInstalled` loads both.
    private func reloadForDetector() {
        guard models.status == .ready else { return }
        switch phase {
        case .idle: loadEngine()
        case .loading: detectorWaiting = true
        default: break
        }
    }

    /// Reloading takes ~0.2 s from the page cache and overlaps the user's speech
    /// (`core.prepare()` on key down), so holding ~700 MB while idle buys nothing.
    private func scheduleIdleUnload() {
        idleUnload?.invalidate()
        idleUnload = Timer.scheduledTimer(withTimeInterval: idleUnloadAfter, repeats: false) { [weak self] _ in
            MainActor.assumeIsolated { self?.core.unload() }
        }
    }

    /// Settings "Legpontosabb / Gyors" for one language. An explicit choice
    /// always wins over the automatic default (`models.rs`). It takes effect at
    /// once: the large-v3 download starts if needed, and when idle the engine
    /// reloads with the current language's model.
    func setModel(_ model: Core.SpeechModel, for language: DictationLanguage) {
        let pref: Int = switch model { case .turbo: 1; case .largeV3: 2; case .pusmogoHu: 3 }
        UserDefaults.standard.set(pref, forKey: "model.\(language.code)")
        objectWillChange.send()
        Timings.note("engine: settings chose \(model.fileName) for \(language.code)")
        fetchAccurateModelIfWanted(for: language)
        switch phase {
        case .idle, .info: switchModelIfNeeded()
        default: break
        }
    }

    func toggleLanguage() {
        language = language.flipped
        sounds.play(.swap)
        fetchAccurateModelIfWanted()
        if case .idle = phase { switchModelIfNeeded() }
        if case .info = phase { switchModelIfNeeded() }
        switch phase {
        case .listening: return  // the tag on the bar updates
        // The text on its way keeps its language and the bar keeps its dots:
        // the swap applies to the next dictation, and the cue confirms it.
        case .thinking, .loading: return
        default: info(language.label)
        }
    }

    func start(locked: Bool = false) {
        switch phase {
        case .idle, .done, .message, .info: break
        default: return
        }
        guard micGranted else { show("Engedélyezd a mikrofont a Beállításokban"); keyboard?.reset(); return }
        guard !needsModel() else { keyboard?.reset(); return }
        captureTarget()
        levels = []
        do {
            try recorder.start()
            idleUnload?.invalidate()
            switchModelIfNeeded()
            core.prepare()
            if Core.contextHints(setting: contextHints) { core.captureContext(codeTitle: targetCodeTitle) } else { Timings.note("context hints off") }
            beginStream { [recorder] in recorder.takeNew() }
            phase = .listening(locked: locked)
            // Like the start sound, not for taps and fn + ⇧ (they end sooner).
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.18) { [weak self] in
                guard let self, self.pauseMedia, case .listening = self.phase else { return }
                MediaPause.begin()
            }
            if locked {
                sounds.play(.lock)
            } else {
                // Sound only for real dictation, not for taps and fn + ⇧.
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.18) { [weak self] in
                    if self?.phase == .listening(locked: false) { self?.sounds.play(.start) }
                }
            }
        } catch {
            keyboard?.reset()
            show("A mikrofon nem indult el")
        }
    }

    /// With the speech engine compiled in but no model file, the core falls back
    /// to the stub, whose placeholder text must never reach the user's app.
    /// A file of the wrong size counts as missing: it loads only partly and gives no text.
    /// Says so on the bar and starts the download (it was missing, damaged or failed).
    private func needsModel() -> Bool {
        guard Core.hasSpeechEngine, !ModelStore.isInstalled else { return false }
        models.recheck()
        if case .downloading(let p) = models.status {
            show("A beszédmodell még töltődik: \(Int(p * 100))%")
        } else {
            models.download()
            show("Előbb letöltöm a beszédmodellt (\(ModelStore.displaySize))")
        }
        Timings.note("dictation refused: no model at \(Core.defaultModelPath), status \(models.status)")
        return true
    }

    /// The microphone button on the hovered idle bar: a hands-free dictation
    /// into the app in front (the bar never takes focus); stop or Esc ends it.
    func startFromBar() {
        barHovered = false
        start(locked: true)
    }

    /// The panel button on the hovered idle bar, shown while macOS hides the
    /// menu bar icon: opens the panel window, as ⌃⌥P does.
    func openPanelFromBar() {
        barHovered = false
        PanelWindow.show(state: self)
    }

    func lock() {
        guard case .listening = phase else { return }
        sounds.play(.lock)
        phase = .listening(locked: true)
    }

    /// Silent stop: a tap or a language swap, nothing to transcribe.
    func discard() {
        guard case .listening = phase else { return }
        _ = recorder.stop()
        dropStream()
        recorder.release()
        MediaPause.end()
        core.clearContext()
        keyboard?.reset()
        phase = .idle
    }

    func cancel() {
        guard case .listening = phase else { return }
        _ = recorder.stop()
        dropStream()
        recorder.release()
        MediaPause.end()
        core.clearContext()
        keyboard?.reset()
        sounds.play(.cancel)
        phase = .idle
    }

    func stop() {
        guard case .listening = phase else { return }
        keyboard?.reset()
        var t = Timings()
        let t0 = nowMs()
        let wasWarm = recorder.warm
        let recording = recorder.stopRecording()
        t.stopMs = nowMs() - t0
        t.micStartMs = recorder.startupMs ?? 0
        if !wasWarm { noteMicStartup(recorder.startupMs) }
        MediaPause.end()
        finish(recording, timings: t, startedAt: t0)
    }

    // MARK: - Streaming

    /// Starts a core session and feeds it about once a second from `source`.
    private func beginStream(_ source: @escaping () -> [Float]) {
        guard streaming else { return }
        streamSource = source
        streamLanguage = language
        core.beginSession(language: language.code, previews: Core.captionPreviews(setting: captionPreviews))
        streamTimer?.invalidate()
        let step = (loadedModel ?? .turbo).streamStep
        streamTimer = Timer.scheduledTimer(withTimeInterval: step, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, let source = self.streamSource else { return }
                _ = self.core.feed(source)
            }
        }
    }

    /// Stops feeding; returns the audio not yet handed to the session.
    private func endStream() -> [Float]? {
        streamTimer?.invalidate()
        streamTimer = nil
        defer { streamSource = nil }
        return streamSource?()
    }

    private func dropStream() {
        guard endStream() != nil else { return }
        core.cancelSession()
    }

    /// Key down: remember the app in front (for History) and its writing style.
    /// `PUSMOGO_STYLE_BUNDLE=<bundle id>` and `PUSMOGO_STYLE_TITLE=<title>` stand in
    /// for the app and its window in tests. The window title is read only for
    /// the core's browser check and, in a code editor, for context hints;
    /// never stored.
    private func captureTarget() {
        let front = NSWorkspace.shared.frontmostApplication
        targetApp = front?.localizedName ?? ""
        let d = UserDefaults.standard
        let bundle = ProcessInfo.processInfo.environment["PUSMOGO_STYLE_BUNDLE"] ?? front?.bundleIdentifier ?? ""
        let title = ProcessInfo.processInfo.environment["PUSMOGO_STYLE_TITLE"]
            ?? front.map { Self.focusedWindowTitle(pid: $0.processIdentifier) } ?? ""
        let category = Core.styleCategory(bundleID: bundle, windowTitle: title,
                                          overrides: d.string(forKey: "styleOverrides") ?? "")
        // UserDefaults `style.<category>`: 0 formal, 1 casual, 2 very casual.
        // Defaults as in the core (`style::Category::default_style`): personal casual.
        let style = d.object(forKey: "style.\(category)") == nil
            ? (category == 1 ? 1 : 0) : Int32(min(2, max(0, d.integer(forKey: "style.\(category)"))))
        targetStyle = (category, style)
        // Category 4 (code): the window title names the open file for context hints.
        targetCodeTitle = category == 4 ? title : nil
        Timings.note("style: category \(category), style \(style)")
    }

    private static func focusedWindowTitle(pid: pid_t) -> String {
        let app = AXUIElementCreateApplication(pid)
        var window: CFTypeRef?
        guard AXUIElementCopyAttributeValue(app, kAXFocusedWindowAttribute as CFString, &window) == .success,
              let window, CFGetTypeID(window) == AXUIElementGetTypeID() else { return "" }
        var title: CFTypeRef?
        guard AXUIElementCopyAttributeValue(window as! AXUIElement, kAXTitleAttribute as CFString, &title) == .success
        else { return "" }
        return title as? String ?? ""
    }

    /// Transcribe and insert. Shared by the key release and the self-test.
    private func finish(_ recording: Recording, timings: Timings, startedAt t0: Double, insert: Bool = true) {
        var t = timings
        t.audioSeconds = recording.seconds
        t.engine = engineName
        let rest = endStream()
        recorder.release()

        // Accidental tap: under 0.3 s is not dictation.
        guard recording.total > 4_800 else {
            if rest != nil { core.cancelSession() }
            core.clearContext()
            phase = .idle
            return
        }
        sounds.play(.stop)
        phase = .thinking

        let t1 = nowMs()
        let lang = language.code  // the language this clip is transcribed in
        let style = targetStyle
        let completion: @MainActor (String?, String?) -> Void = { [weak self] cleaned, spoken in
            guard let self else { return }
            self.core.clearContext()
            t.transcribeMs = nowMs() - t1
            guard let cleaned, !cleaned.isEmpty else {
                self.sounds.play(.error)
                self.show("Nem hallottam semmit")
                return
            }
            // A dictation that is a whole snippet trigger types the saved text
            // as written: no style pass, History keeps what was typed.
            self.core.snippet(for: cleaned) { [weak self] snippet in
                guard let self else { return }
                if snippet != nil { Timings.note("snippet: trigger matched") }
                let text = snippet ?? Core.applyStyle(cleaned, category: style.category, style: style.style, language: lang)
                // A snippet types its saved text: what was said is not a dropped correction.
                let spoken = snippet == nil ? spoken : nil
                if spoken != nil { Timings.note("self-correction: words dropped, spoken text kept for History") }
                let t2 = nowMs()
                if self.selfTest { Timings.note("self-test text: \(text)") }
                self.history.insert(HistoryEntry(date: Date(), text: text, app: self.targetApp, spoken: spoken, timings: t), at: 0)
                self.dictationCount += 1
                if self.history.count > Self.historyLimit { self.history.removeLast() }
                let app = self.targetApp
                let done: @MainActor (TextInserter.Outcome) -> Void = { [weak self] outcome in
                    guard let self else { return }
                    t.insertMs = nowMs() - t2
                    t.totalMs = nowMs() - t0
                    t.chars = text.count
                    t.insert = Self.label(outcome)
                    t.log()
                    self.lastTimings = t
                    if !self.history.isEmpty, self.history[0].text == text { self.history[0].timings = t }
                    self.core.addHistory(lang: lang, text: text, latencyMs: Int64(t.totalMs.rounded()),
                                         audioMs: Int64((t.audioSeconds * 1000).rounded()), app: app,
                                         spoken: spoken) { [weak self] id, pruned in
                        guard let self else { return }
                        if pruned > 0 { self.reloadHistory() }
                        guard let id, let i = self.history.firstIndex(where: { $0.text == text && $0.storedId == nil })
                        else { return }
                        self.history[i].storedId = id
                    }
                    self.report(outcome)
                }
                if insert { TextInserter.insert(text, completion: done) } else { done(.direct) }
            }
        }
        // A language swap while speaking invalidates the session's language:
        // transcribe the whole clip in the new one instead. Past the first
        // 5 minutes the recorder no longer has the whole clip, so the
        // session finishes in the language it started with.
        if let rest, streamLanguage == language || !recording.complete {
            if streamLanguage != language { Timings.note("swap after the kept audio: finishing in \(streamLanguage?.code ?? "?")") }
            core.finishSession(rest: rest, completion: completion)
        } else {
            if rest != nil {
                core.cancelSession()
                redoLanguage = language
                liveText = ""
                flowBar?.update()
            }
            core.transcribe(samples: recording.samples, language: language.code, completion: completion)
        }
    }

    /// Tells the user how the text arrived: a brief "done", or where it went
    /// when it could not be typed.
    private func report(_ outcome: TextInserter.Outcome) {
        switch outcome {
        case .direct, .pasted, .pastedUnverified:
            phase = .done
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.7) { [weak self] in
                if self?.phase == .done { self?.phase = .idle }
            }
        case .copied(.secureInput):
            show("Jelszómezőbe nem írok, a vágólapon van")
        case .copied:
            show("Vágólapra másolva, nyomd meg: ⌘V")
        }
    }

    private static func label(_ o: TextInserter.Outcome) -> String {
        switch o {
        case .direct: return "direct"
        case .pasted: return "pasted"
        case .pastedUnverified: return "pasted:unverified"
        case .copied(let r): return "copied:" + r.rawValue
        }
    }

    // MARK: - Insert again from History

    private func trackPreviousApp() {
        let me = ProcessInfo.processInfo.processIdentifier
        if let front = NSWorkspace.shared.frontmostApplication, front.processIdentifier != me {
            previousApp = front
        }
        activationObserver = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main
        ) { [weak self] note in
            guard let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
                  app.processIdentifier != me else { return }
            MainActor.assumeIsolated { self?.previousApp = app }
        }
    }

    /// Types a History entry into the app that was in front before the panel
    /// opened: closes the panel, brings that app back and inserts the text the
    /// same way as a dictation. If that app is gone or does not come to the
    /// front, nothing is typed and the text goes to the clipboard instead, so
    /// a keystroke never lands in a window the user did not pick.
    func insertAgain(_ text: String) {
        NSApp.keyWindow?.close()
        guard let app = previousApp, !app.isTerminated else { return copyInstead(text, reason: "no previous app") }
        app.activate()
        var tries = 0
        func attempt() {
            if NSWorkspace.shared.frontmostApplication?.processIdentifier == app.processIdentifier {
                // Give the app a moment to restore its focused field.
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) {
                    let t0 = nowMs()
                    TextInserter.insert(text) { outcome in
                        Timings.note(String(format: "insert-again: %@ into %@ in %.0f ms",
                                            Self.label(outcome), app.localizedName ?? "?", nowMs() - t0))
                        self.report(outcome)
                    }
                }
            } else if tries < 20 {
                tries += 1
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { attempt() }
            } else {
                copyInstead(text, reason: "\(app.localizedName ?? "?") did not come to the front")
            }
        }
        attempt()
    }

    private func copyInstead(_ text: String, reason: String) {
        Timings.note("insert-again: copied (\(reason))")
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        report(.copied(.notRead))
    }

    // MARK: - Insert test (PUSMOGO_INSERT_TEST=<text>)

    /// Inserts the text into whatever is focused after `PUSMOGO_INSERT_DELAY`
    /// seconds (default 2), logs the outcome and quits. Put a known window in
    /// front first. `PUSMOGO_INSERT_MODE=paste` forces the pasteboard path.
    private func runInsertTest(_ text: String) {
        let delay = ProcessInfo.processInfo.environment["PUSMOGO_INSERT_DELAY"].flatMap(Double.init) ?? 2
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) {
            let t0 = nowMs()
            let app = NSWorkspace.shared.frontmostApplication?.localizedName ?? "?"
            TextInserter.insert(text) { outcome in
                Timings.note(String(format: "insert-test: %@ into %@ in %.0f ms", Self.label(outcome), app, nowMs() - t0))
                // Same feedback as a real dictation, left up long enough to screenshot.
                self.report(outcome)
                // PUSMOGO_INSERT_LINGER keeps the app up so the edit watcher can be tested.
                let usual: Double = if case .copied = outcome { 3 } else { 0.3 }
                let linger = ProcessInfo.processInfo.environment["PUSMOGO_INSERT_LINGER"].flatMap(Double.init) ?? usual
                DispatchQueue.main.asyncAfter(deadline: .now() + linger) { exit(0) }
            }
        }
    }

    // MARK: - Microphone test (PUSMOGO_MIC_TEST=<seconds>)

    /// Records from the chosen input for a few seconds and logs which device
    /// was used, how much audio arrived and how loud it was, then exits. Lists
    /// the inputs first so the device choice can be checked unattended.
    private func runMicTest(seconds: Double) {
        for m in MicDevice.all() { Timings.note("mic-test: input \(m.name) [\(m.uid)]") }
        Timings.note("mic-test: preferred \(micUID.isEmpty ? "system default" : micUID)")
        do { try recorder.start() } catch {
            Timings.note("mic-test: start failed: \(error.localizedDescription)")
            exit(3)
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { [self] in
            let samples = recorder.stop()
            let rms = samples.isEmpty ? 0 : (samples.reduce(0) { $0 + $1 * $1 } / Float(samples.count)).squareRoot()
            // Digital silence at the start: some inputs deliver zeros while they warm up.
            let silent = samples.firstIndex { abs($0) > 1e-6 } ?? samples.count
            Timings.note(String(format: "mic-test: device %@, %.2f s of audio, rms %.5f, first audio after %.0f ms, %.0f ms of digital silence first",
                                recorder.activeDeviceName, Double(samples.count) / 16_000, rms,
                                recorder.startupMs ?? -1, Double(silent) / 16))
            // `PUSMOGO_MIC_TEST_HOLD=<s>` stays open after the recording, to see what the app still holds.
            let hold = ProcessInfo.processInfo.environment["PUSMOGO_MIC_TEST_HOLD"].flatMap(Double.init) ?? 0
            DispatchQueue.main.asyncAfter(deadline: .now() + hold) { exit(0) }
        }
    }

    // MARK: - Self-test (PUSMOGO_SELFTEST_WAV=<16 kHz mono WAV>)

    private var selfTest = false

    /// Plays a WAV through the real dictation path (prepare on "key down",
    /// transcribe, timings) without a key press or a microphone, so the engine
    /// lifecycle can be checked unattended. Runs `PUSMOGO_SELFTEST_RUNS` times
    /// (default 1), `PUSMOGO_SELFTEST_GAP` seconds apart (default 3), after a
    /// first wait of `PUSMOGO_SELFTEST_DELAY` (default 3). Text is only typed
    /// into the focused app with `PUSMOGO_SELFTEST_INSERT=1`; otherwise it is
    /// logged. Quits when done.
    private func runSelfTest(wavPath: String) {
        selfTest = true
        let env = ProcessInfo.processInfo.environment
        let delay = env["PUSMOGO_SELFTEST_DELAY"].flatMap(Double.init) ?? 3
        let gap = env["PUSMOGO_SELFTEST_GAP"].flatMap(Double.init) ?? 3
        let runs = max(1, env["PUSMOGO_SELFTEST_RUNS"].flatMap(Int.init) ?? 1)
        let insert = env["PUSMOGO_SELFTEST_INSERT"] == "1"
        guard let samples = Self.readWav(wavPath) else {
            Timings.note("self-test: cannot read \(wavPath) (needs 16 kHz mono WAV)")
            exit(2)
        }
        Timings.note(String(format: "self-test: %.2f s of audio, %d runs", Double(samples.count) / 16_000, runs))
        let speaking = Double(samples.count) / 16_000

        func run(_ i: Int) {
            if needsModel() {
                DispatchQueue.main.asyncAfter(deadline: .now() + 3) { exit(3) }
                return
            }
            // "Key down": the same calls `start()` makes, minus the microphone.
            idleUnload?.invalidate()
            switchModelIfNeeded()
            core.prepare()
            captureTarget()
            if env["PUSMOGO_SELFTEST_CONTEXT"] != "0" {
                if Core.contextHints(setting: contextHints) { core.captureContext(codeTitle: targetCodeTitle) } else { Timings.note("context hints off") }
            }
            // Hand the clip over as if it were arriving from the microphone.
            // Through the recorder's own buffer, so a long clip drops audio
            // the session has taken exactly as a long dictation does.
            let began = nowMs()
            var fed = 0
            let buffer = SampleBuffer()
            func arrive() {
                let upTo = min(samples.count, Int((nowMs() - began) / 1000 * 16_000))
                if upTo > fed { buffer.append(samples[fed..<upTo]) }
                fed = max(fed, upTo)
            }
            beginStream { arrive(); return buffer.takeNew() }
            phase = .listening(locked: false)
            if pauseMedia { MediaPause.begin() }
            // fn + ⇧ while speaking, `PUSMOGO_SELFTEST_SWAP` seconds in. Like the
            // real swap it saves the new language, so restore it after the test.
            if let swapAt = env["PUSMOGO_SELFTEST_SWAP"].flatMap(Double.init), swapAt < speaking {
                DispatchQueue.main.asyncAfter(deadline: .now() + swapAt) { [self] in
                    toggleLanguage()
                    Timings.note("self-test: language swapped to \(language.code)")
                }
            }
            // "Key up" after the clip's own duration, as if spoken live.
            DispatchQueue.main.asyncAfter(deadline: .now() + speaking) { [self] in
                MediaPause.end()
                arrive()
                if fed < samples.count { buffer.append(samples[fed...]) }
                finish(streaming ? buffer.recording : Recording(samples: samples),
                       timings: Timings(), startedAt: nowMs(), insert: insert)
                waitForIdle {
                    if i + 1 < runs {
                        DispatchQueue.main.asyncAfter(deadline: .now() + gap) { run(i + 1) }
                    } else {
                        Timings.note("self-test: done")
                        // Leave a visible window up long enough to screenshot.
                        let linger = Onboarding.startStep == nil ? 0.5 : 4
                        DispatchQueue.main.asyncAfter(deadline: .now() + linger) { exit(0) }
                    }
                }
            }
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { run(0) }
    }

    private func waitForIdle(_ then: @escaping @MainActor () -> Void) {
        if phase == .thinking || phase == .loading {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [self] in waitForIdle(then) }
        } else {
            then()
        }
    }

    private static func readWav(_ path: String) -> [Float]? {
        guard let file = try? AVAudioFile(forReading: URL(fileURLWithPath: path)),
              file.processingFormat.sampleRate == 16_000, file.processingFormat.channelCount == 1,
              let buf = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: AVAudioFrameCount(file.length)),
              (try? file.read(into: buf)) != nil, let ch = buf.floatChannelData?[0]
        else { return nil }
        return Array(UnsafeBufferPointer(start: ch, count: Int(buf.frameLength)))
    }

    // MARK: - Demo (PUSMOGO_DEMO=1): cycles every flow bar state for design review

    private func runDemo() {
        var t = 0.0
        func at(_ dt: Double, _ f: @escaping @MainActor () -> Void) {
            t += dt
            DispatchQueue.main.asyncAfter(deadline: .now() + t) { f() }
        }
        at(2.0) { self.phase = .listening(locked: false) }
        for i in 0..<120 {
            at(0.033) {
                let speech = 0.55 + 0.4 * sin(Double(i) * 0.37) * sin(Double(i) * 0.11)
                self.levels.append(Float(max(0.05, speech + Double.random(in: -0.15...0.15))))
                if self.levels.count > 32 { self.levels.removeFirst() }
                if i == 40 { self.liveText = "Szia, holnap reggel kilenckor találkozunk az irodában." }
                if i == 70 { self.phase = .listening(locked: true) }
                if i == 90 {
                    self.liveText = "Szia, holnap reggel kilenckor találkozunk az irodában, utána átnézzük a negyedéves számokat, "
                        + "és ha marad idő, megbeszéljük a következő hét feladatait is a csapattal."
                }
            }
        }
        at(0.5) { self.phase = .thinking }
        at(2.0) { self.phase = .done }
        at(1.5) { self.phase = .message(String(localized: "Vágólapra másolva, nyomd meg: ⌘V")) }
        at(3.0) { self.phase = .info("English") }
        at(3.0) { self.phase = .idle }
    }

    // MARK: - Helpers

    /// Shows a notice on the bar. The Hungarian text is the key; other
    /// languages come from `app/Resources/<lang>.lproj/Localizable.strings`.
    private func show(_ key: String.LocalizationValue, seconds: Double = 2.6) {
        let message = String(localized: key)
        phase = .message(message)
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { [weak self] in
            if self?.phase == .message(message) { self?.phase = .idle }
        }
    }

    /// A short notice for an idle bar. It never replaces an active dictation:
    /// a completion that lands after a new key press (the dictionary save
    /// behind "Add to dictionary?") would otherwise hide the recording.
    private func info(_ text: String) {
        switch phase {
        case .listening, .thinking, .loading: return
        default: break
        }
        phase = .info(text)
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.1) { [weak self] in
            if self?.phase == .info(text) { self?.phase = .idle }
        }
    }

    func requestPermissions() {
        guard !Self.previewDenied else { return }
        accessibilityGranted = Permissions.accessibilityGranted(prompt: true)
        Task { self.micGranted = await AudioRecorder.requestMicAccess() }
    }

    private func pushLevel(_ level: Float) {
        guard case .listening = phase else { return }
        let now = nowMs()
        guard now - lastLevelAt > 33 else { return }  // ~30 fps is plenty
        lastLevelAt = now
        levels.append(level)
        if levels.count > 32 { levels.removeFirst(levels.count - 32) }
    }

    private func refreshPermissions() {
        guard !Self.previewDenied else { return }
        let hadMic = micGranted
        micGranted = AudioRecorder.micGranted
        if micGranted, !hadMic { updateMicReady() }
        accessibilityGranted = Permissions.accessibilityGranted(prompt: false)
    }
}
