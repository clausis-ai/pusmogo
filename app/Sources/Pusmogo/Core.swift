import CPusmogoCore
import Foundation

/// Swift wrapper over the Rust C ABI. Loading and every engine call run on one
/// serial queue (the handle is not thread-safe); completions return on the main actor.
final class Core: @unchecked Sendable {
    /// Test hook only: `PUSMOGO_MODELS_DIR=<dir>` looks for (and would save) the model
    /// there instead, so the no-model path can be checked without moving the real file.
    static var modelsDirectory: URL {
        if let dir = ProcessInfo.processInfo.environment["PUSMOGO_MODELS_DIR"] {
            return URL(fileURLWithPath: dir, isDirectory: true)
        }
        return FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Pusmogo/models", isDirectory: true)
    }

    static var defaultModelPath: String { SpeechModel.turbo.path }

    /// The speech models the app can load. Which one a language uses is decided
    /// in the core (`models.rs`): large-v3 for Hungarian above 8 GB of memory,
    /// turbo otherwise, an explicit Settings choice first.
    enum SpeechModel: Int32 {
        case turbo = 0, largeV3 = 1, pusmogoHu = 2

        var fileName: String {
            switch self {
            case .turbo: return "ggml-large-v3-turbo-q5_0.bin"
            case .largeV3: return "ggml-large-v3-q5_0.bin"
            case .pusmogoHu: return "ggml-pusmogo-hu-q5_0.bin"
            }
        }
        /// Exact size of the pinned file; any other size is a cut-short copy.
        var bytes: Int64 {
            switch self {
            case .turbo: return 574_041_195
            case .largeV3: return 1_081_140_203
            case .pusmogoHu: return 574_041_195
            }
        }
        var path: String { Core.modelsDirectory.appendingPathComponent(fileName).path }
        var isInstalled: Bool {
            let size = (try? FileManager.default.attributesOfItem(atPath: path)[.size] as? Int64) ?? nil
            return size == bytes
        }
        /// Seconds between streaming steps: large-v3's background passes are slower
        /// than real time at 1 s, which roughly doubled the wait after release
        /// (`docs/bench/stream-02.md`).
        var streamStep: TimeInterval { self == .largeV3 ? 2 : 1 }

        /// Settings choice per language (UserDefaults `model.hu` / `model.en`):
        /// 0 automatic, 1 fast, 2 most accurate, 3 Pusmogo HU.
        static func preference(_ language: String) -> Int32 {
            Int32(UserDefaults.standard.integer(forKey: "model.\(language)"))
        }

        /// Test hook: `PUSMOGO_FAKE_RAM_GB=8` acts as a Mac with that much memory.
        static var physicalMemory: UInt64 {
            if let gb = ProcessInfo.processInfo.environment["PUSMOGO_FAKE_RAM_GB"].flatMap(Double.init) {
                return UInt64(gb * 1_073_741_824)
            }
            return ProcessInfo.processInfo.physicalMemory
        }

        /// The model to load for `language` now, or nil when none is installed.
        static func pick(_ language: String) -> SpeechModel? {
            let raw = language.withCString {
                pusmogo_model_pick($0, preference(language), physicalMemory,
                                   turbo.isInstalled ? 1 : 0, largeV3.isInstalled ? 1 : 0,
                                   pusmogoHu.isInstalled ? 1 : 0)
            }
            return SpeechModel(rawValue: raw)
        }

        /// The model `language` should use once every model is downloaded.
        static func wanted(_ language: String) -> SpeechModel {
            let raw = language.withCString { pusmogo_model_wanted($0, preference(language), physicalMemory) }
            return SpeechModel(rawValue: raw) ?? .turbo
        }
    }

    /// False in a stub-only build (`./scripts/build.sh` without `--whisper`).
    static let hasSpeechEngine = pusmogo_has_speech_engine() == 1

    private let queue = DispatchQueue(label: "pusmogo.core", qos: .userInitiated)
    private var handle: OpaquePointer?
    private var modelPath: String?
    /// Called on the main actor whenever the model enters or leaves memory.
    var onLoadedChange: (@MainActor (Bool) -> Void)?
    /// Called on the main actor with the cleaned text so far each time a
    /// streaming chunk is transcribed.
    var onLiveText: (@MainActor (String) -> Void)?
    /// Called on the main actor after the dictionary is applied to a loaded
    /// engine, with how many of the oldest spellings no longer fit Whisper's
    /// prompt window (nil when the engine cannot count tokens).
    var onDictionaryLeftOut: (@MainActor (Int?) -> Void)?

    /// Runs on `queue`.
    private func reportLoaded() {
        let loaded = handle != nil, report = onLoadedChange
        Task { @MainActor in report?(loaded) }
    }

    /// Loads in the background so the menu bar appears instantly, then runs the
    /// engine's warm-up so the first real dictation doesn't pay for GPU setup.
    func load(modelPath: String, completion: @escaping @MainActor (String) -> Void) {
        queue.async { [self] in
            self.modelPath = modelPath
            if let old = handle { pusmogo_engine_free(old) }
            handle = pusmogo_engine_new(modelPath)
            applyDictionary()
            applyDebugDir()
            applySelfCorrections()
            _ = pusmogo_engine_warm_up(handle)
            let name = Core.take(pusmogo_engine_name(handle)) ?? "?"
            reportLoaded()
            Task { @MainActor in completion(name) }
        }
    }

    /// Frees the model (~700 MB with Whisper) while dictation is idle. The next
    /// `prepare()` or `transcribe()` loads it again from the page cache.
    func unload() {
        queue.async { [self] in
            if let old = handle { pusmogo_engine_free(old) }
            handle = nil
            reportLoaded()
            Timings.note("engine unloaded")
        }
    }

    /// Starts loading an unloaded model now, e.g. when the user starts speaking,
    /// so it is ready by the time they finish.
    func prepare() {
        queue.async { [self] in ensureLoaded() }
    }

    // MARK: - Context hints (see core/crates/pusmogo-core/src/context.rs)

    /// Text before the caret in the field being dictated into, held only for
    /// one dictation so a reload in the middle of it keeps the hints. Never
    /// logged or stored. Only touched on `queue`.
    private var context: String?
    /// The context comes from a code editor or terminal (`captureContext(codeTitle:)`).
    private var codeContext = false

    /// Context hints follow the Settings switch (on by default);
    /// `PUSMOGO_CONTEXT_HINTS=0|1` overrides it for tests.
    static func contextHints(setting: Bool) -> Bool {
        switch ProcessInfo.processInfo.environment["PUSMOGO_CONTEXT_HINTS"] {
        case "0": return false
        case "1": return true
        default: return setting
        }
    }

    /// Reads the focused field's text before the caret and hands it to the
    /// engine as spelling hints for the coming dictation. Queued ahead of the
    /// session and the transcription, so both see it.
    ///
    /// `codeTitle` is the focused window's title when the app in front is a
    /// code editor or terminal (style category code): it usually names the
    /// open file, and the core then also takes identifiers and file names
    /// as hints. Never logged or stored, like the field text.
    func captureContext(codeTitle: String? = nil) {
        queue.async { [self] in
            let field = FocusedText.beforeCaret()
            if let codeTitle {
                codeContext = true
                // The title first: hints nearest the end (the cursor) win.
                let text = [codeTitle, field ?? ""].filter { !$0.isEmpty }.joined(separator: "\n")
                context = text.isEmpty ? nil : text
            } else {
                codeContext = false
                context = field
            }
            applyContext()
        }
    }

    /// Drops the hints once a dictation is over (done, cancelled or empty).
    func clearContext() {
        queue.async { [self] in
            guard context != nil else { return }
            context = nil
            applyContext()
        }
    }

    /// Opt-in record of what the engine received, for replaying real failures:
    /// the Settings switch (UserDefaults `keepDebugAudio`). `PUSMOGO_KEEP_AUDIO=1`
    /// forces it on and `PUSMOGO_KEEP_AUDIO=0` forces it off. A self-test
    /// (`PUSMOGO_SELFTEST_WAV`) never records unless forced on, so test clips
    /// cannot push real dictations out of the record. The newest 20 dictations
    /// (16 kHz WAV + raw/cleaned text, prompt, hint words) stay in
    /// `debugDirectory`; nothing is uploaded. Off by default.
    static var keepDebugAudio: Bool {
        let env = ProcessInfo.processInfo.environment
        switch env["PUSMOGO_KEEP_AUDIO"] {
        case "1": return true
        case "0": return false
        default:
            if env["PUSMOGO_SELFTEST_WAV"] != nil { return false }
            return UserDefaults.standard.bool(forKey: "keepDebugAudio")
        }
    }

    /// `PUSMOGO_DEBUG_DIR=<dir>` puts the record somewhere else (tests).
    static var debugDirectory: URL {
        if let dir = ProcessInfo.processInfo.environment["PUSMOGO_DEBUG_DIR"], !dir.isEmpty {
            return URL(fileURLWithPath: dir, isDirectory: true)
        }
        return FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Pusmogo/debug", isDirectory: true)
    }

    /// Re-reads the switch, for a change made in Settings while the model is loaded.
    func refreshDebugDir() {
        queue.async { [self] in applyDebugDir() }
    }

    /// Runs on `queue`, after every engine load and when the switch changes.
    private func applyDebugDir() {
        guard let handle else { return }
        guard Core.keepDebugAudio else {
            _ = pusmogo_set_debug_dir(handle, nil)
            return
        }
        _ = Core.debugDirectory.path.withCString { pusmogo_set_debug_dir(handle, $0) }
        Timings.note("keeping dictations in \(Core.debugDirectory.path)")
    }

    /// Drops spoken self-corrections ("háromkor, nem, négykor" → "négykor",
    /// core `backtrack.rs`): the Settings switch (UserDefaults
    /// `selfCorrections`, on by default). `PUSMOGO_SELF_CORRECTIONS=0|1`
    /// overrides it for tests.
    static var selfCorrections: Bool {
        switch ProcessInfo.processInfo.environment["PUSMOGO_SELF_CORRECTIONS"] {
        case "0": return false
        case "1": return true
        default: return UserDefaults.standard.object(forKey: "selfCorrections") as? Bool ?? true
        }
    }

    /// Re-reads the switch, for a change made in Settings while the model is loaded.
    func refreshSelfCorrections() {
        queue.async { [self] in applySelfCorrections() }
    }

    /// Runs on `queue`, after every engine load and when the switch changes.
    private func applySelfCorrections() {
        guard let handle else { return }
        let on = Core.selfCorrections
        _ = pusmogo_set_self_corrections(handle, on ? 1 : 0)
        if !on { Timings.note("self-corrections off") }
    }

    /// Runs on `queue`.
    private func applyContext() {
        guard let handle else { return }
        let n = context.map { c in
            c.withCString { codeContext ? pusmogo_set_code_context(handle, $0) : pusmogo_set_context(handle, $0) }
        } ?? pusmogo_set_context(handle, nil)
        if context != nil { Timings.note("context: \(n) hint words") }
    }

    /// `completion` gets the text to type and, when spoken self-corrections were
    /// dropped from it, the text before that (`pusmogo_last_spoken`, for History).
    func transcribe(samples: [Float], language: String?, completion: @escaping @MainActor (String?, String?) -> Void) {
        queue.async { [self] in
            ensureLoaded()
            let raw = samples.withUnsafeBufferPointer { buf in
                if let language {
                    return language.withCString { pusmogo_transcribe(handle, buf.baseAddress, buf.count, $0) }
                }
                return pusmogo_transcribe(handle, buf.baseAddress, buf.count, nil)
            }
            let text = Core.take(raw)
            let spoken = text == nil ? nil : Core.take(pusmogo_last_spoken(handle))
            Task { @MainActor in completion(text, spoken) }
        }
    }

    // MARK: - Streaming (see core/crates/pusmogo-core/src/session.rs)

    private var session: OpaquePointer?
    /// Caption previews show the caption from about 4 s in instead of after
    /// the first chunk (8 s or more), for the same typed text and about the
    /// same release wait (docs/bench/caption-01.md, caption-03.md). The
    /// Settings switch decides; `PUSMOGO_CAPTION_PREVIEW=0|1` overrides it.
    static func captionPreviews(setting: Bool) -> Bool {
        switch ProcessInfo.processInfo.environment["PUSMOGO_CAPTION_PREVIEW"] {
        case "0": return false
        case "1": return true
        default: return setting
        }
    }
    private let stepLock = NSLock()
    private var stepPending = false

    /// Starts a streaming dictation; replaces any session left open.
    func beginSession(language: String, previews: Bool) {
        queue.async { [self] in
            if let session { pusmogo_session_free(session) }
            session = language.withCString { pusmogo_session_new($0) }
            if previews { pusmogo_session_set_previews(session, 1) }
        }
    }

    /// Hands new audio to the session and transcribes a finished chunk if one
    /// is ready, else previews the waiting audio for the live caption. Returns
    /// false without taking the audio while the previous step is still
    /// running, so ticks never pile up behind a slow pass; the caller keeps
    /// the audio for the next tick.
    func feed(_ samples: @escaping () -> [Float]) -> Bool {
        let busy = stepLock.withLock { () -> Bool in
            if stepPending { return true }
            stepPending = true
            return false
        }
        if busy { return false }
        let chunk = samples()
        queue.async { [self] in
            defer { stepLock.withLock { stepPending = false } }
            guard let session else { return }
            Core.push(session, chunk)
            ensureLoaded()
            let t0 = DispatchTime.now().uptimeNanoseconds
            if pusmogo_session_step(handle, session) == 1 {
                let ms = Double(DispatchTime.now().uptimeNanoseconds - t0) / 1e6
                Timings.note(String(format: "stream: caption updated in %.0f ms", ms))
                if let text = Core.take(pusmogo_session_text(handle, session)), let report = onLiveText {
                    Task { @MainActor in report(text) }
                }
            }
        }
        return true
    }

    /// Pushes the last audio, transcribes what is left and returns the cleaned
    /// text of the whole dictation. Frees the session.
    func finishSession(rest: [Float], completion: @escaping @MainActor (String?, String?) -> Void) {
        // A caption preview still running gives up instead of delaying the result.
        pusmogo_session_interrupt()
        queue.async { [self] in
            guard let session else { Task { @MainActor in completion(nil, nil) }; return }
            self.session = nil
            Core.push(session, rest)
            ensureLoaded()
            let text = Core.take(pusmogo_session_finish(handle, session))
            let spoken = text == nil ? nil : Core.take(pusmogo_last_spoken(handle))
            pusmogo_session_free(session)
            Task { @MainActor in completion(text, spoken) }
        }
    }

    /// Drops the session without transcribing (cancel, tap, language swap).
    func cancelSession() {
        pusmogo_session_interrupt()
        queue.async { [self] in
            if let session { pusmogo_session_free(session) }
            session = nil
        }
    }

    private static func push(_ session: OpaquePointer, _ samples: [Float]) {
        samples.withUnsafeBufferPointer { pusmogo_session_push(session, $0.baseAddress, $0.count) }
    }

    /// Runs on `queue`. No warm-up here: a reload sits on the dictation path, and
    /// the GPU pipelines stay cached by the system after the first load.
    private func ensureLoaded() {
        guard handle == nil, let modelPath else { return }
        let t0 = DispatchTime.now().uptimeNanoseconds
        handle = pusmogo_engine_new(modelPath)
        applyDictionary()
        applyDebugDir()
        applySelfCorrections()
        applyContext()
        reportLoaded()
        let ms = Double(DispatchTime.now().uptimeNanoseconds - t0) / 1e6
        Timings.note(String(format: "engine reloaded in %.0f ms", ms))
    }

    // MARK: - Profile (history, dictionary, settings; see core/crates/pusmogo-core/src/profile.rs)

    private var profile: OpaquePointer?

    static var defaultProfilePath: String {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Pusmogo/profile.sqlite").path
    }

    /// A dictation as stored in the profile. `createdAt` is Unix seconds.
    /// What opening the profile needs to tell the user once.
    enum ProfileNotice {
        /// A damaged file was moved here and a fresh profile started.
        case movedAside(String)
        /// The file was written by a newer Pusmogo and left untouched.
        case newerVersion
    }

    struct StoredDictation: Decodable {
        let id: Int64
        let createdAt: Int64
        let lang: String
        let text: String
        let latencyMs: Int64
        let app: String?
        /// The text before spoken self-corrections were dropped; absent when none were.
        let spoken: String?
        enum CodingKeys: String, CodingKey {
            case id, lang, text, app, spoken
            case createdAt = "created_at", latencyMs = "latency_ms"
        }
    }

    /// Opens (or creates) the profile, prunes history older than the
    /// `history_days` setting and returns the newest `limit` dictations.
    /// Opening first, before any engine load is queued, means the first load
    /// already gets the user's dictionary.
    /// A file that is not a database or is damaged is moved aside and a fresh
    /// profile started; the second completion value is then the old file's
    /// new path, so the user can be told once. A profile written by a newer
    /// Pusmogo is refused untouched and reported as `.newerVersion`.
    func openProfile(path: String, limit: Int,
                     completion: @escaping @MainActor ([StoredDictation], ProfileNotice?) -> Void) {
        queue.async { [self] in
            try? FileManager.default.createDirectory(
                at: URL(fileURLWithPath: path).deletingLastPathComponent(), withIntermediateDirectories: true)
            if let profile { pusmogo_profile_free(profile) }
            var movedTo: UnsafeMutablePointer<CChar>?
            profile = pusmogo_profile_open_recovering(path, &movedTo)
            let moved = Core.take(movedTo)
            if let moved { Timings.note("profile: damaged file moved aside to \(moved), fresh profile started") }
            guard let profile else {
                // A file from a newer build is left untouched; the user is told
                // to update instead of seeing an empty History.
                let newer = pusmogo_profile_is_newer(path) == 1
                Timings.note("profile: cannot open \(path)\(newer ? " (written by a newer version)" : "")")
                Task { @MainActor in completion([], newer ? .newerVersion : nil) }
                return
            }
            let days = Core.take(pusmogo_profile_setting(profile, "history_days")).flatMap(Int64.init) ?? 0
            let pruned = pusmogo_profile_prune_history(profile, days)
            applyDictionary()
            let json = Core.take(pusmogo_profile_search_history(profile, nil, limit)) ?? "[]"
            let entries = (try? JSONDecoder().decode([StoredDictation].self, from: Data(json.utf8))) ?? []
            Timings.note("profile: \(entries.count) dictations loaded, \(pruned) pruned")
            Task { @MainActor in completion(entries, moved.map(ProfileNotice.movedAside)) }
        }
    }

    /// Stores a finished dictation; the id comes back for later edits (delete).
    /// Retention is applied here too, not only at launch: a menu bar app runs
    /// for weeks, and "keep 7 days" must not mean "until the next relaunch".
    /// The second value is how many old dictations that removed.
    /// `audioMs` is the recording length, kept for the words-per-minute insight;
    /// `spoken` the text before self-corrections were dropped, if any were.
    func addHistory(lang: String, text: String, latencyMs: Int64, audioMs: Int64, app: String, spoken: String? = nil,
                    completion: @escaping @MainActor (Int64?, Int) -> Void = { _, _ in }) {
        queue.async { [self] in
            guard let profile else { Task { @MainActor in completion(nil, 0) }; return }
            let id = pusmogo_profile_add_history(profile, lang, text, latencyMs, app.isEmpty ? nil : app)
            if id >= 0, audioMs > 0, pusmogo_profile_set_audio_ms(profile, id, audioMs) != 0 {
                Timings.note("profile: recording length not saved")
            }
            if id >= 0, let spoken, !spoken.isEmpty, pusmogo_profile_set_spoken(profile, id, spoken) != 0 {
                Timings.note("profile: spoken text not saved")
            }
            let days = Core.take(pusmogo_profile_setting(profile, "history_days")).flatMap(Int64.init) ?? 0
            let pruned = max(0, Int(pusmogo_profile_prune_history(profile, days)))
            if pruned > 0 { Timings.note("profile: \(pruned) dictations past retention pruned") }
            Task { @MainActor in completion(id >= 0 ? id : nil, pruned) }
        }
    }

    /// Newest first; an empty query returns everything (up to `limit`).
    /// The match is a literal, case-insensitive substring search in the core.
    func searchHistory(_ query: String, limit: Int, completion: @escaping @MainActor ([StoredDictation]) -> Void) {
        queue.async { [self] in
            guard let profile, let json = Core.take(pusmogo_profile_search_history(profile, query, limit)) else {
                Task { @MainActor in completion([]) }
                return
            }
            let entries = (try? JSONDecoder().decode([StoredDictation].self, from: Data(json.utf8))) ?? []
            Task { @MainActor in completion(entries) }
        }
    }

    func deleteHistory(id: Int64, completion: @escaping @MainActor (Bool) -> Void = { _ in }) {
        queue.async { [self] in
            let ok = profile.map { pusmogo_profile_delete_history($0, id) == 0 } ?? false
            Task { @MainActor in completion(ok) }
        }
    }

    func clearHistory(completion: @escaping @MainActor (Bool) -> Void = { _ in }) {
        queue.async { [self] in
            let ok = profile.map { pusmogo_profile_clear_history($0) == 0 } ?? false
            Task { @MainActor in completion(ok) }
        }
    }

    /// History retention in days (0 keeps everything), from the profile.
    func historyDays(completion: @escaping @MainActor (Int) -> Void) {
        queue.async { [self] in
            let days = profile.flatMap { Core.take(pusmogo_profile_setting($0, "history_days")) }.flatMap(Int.init) ?? 0
            Task { @MainActor in completion(days) }
        }
    }

    /// Stores the retention and prunes at once, so a shorter period takes
    /// effect without a restart. Returns how many dictations were removed,
    /// or nil when the setting could not be saved.
    func setHistoryDays(_ days: Int, completion: @escaping @MainActor (Int?) -> Void) {
        queue.async { [self] in
            guard let profile, pusmogo_profile_set_setting(profile, "history_days", String(days)) == 0 else {
                Task { @MainActor in completion(nil) }
                return
            }
            let pruned = Int(pusmogo_profile_prune_history(profile, Int64(days)))
            Task { @MainActor in completion(pruned) }
        }
    }

    /// Profile settings that let the local MCP server (`pusmogo-mcp`) read the
    /// profile. Off unless the value is "1" (core `Profile::SHARE_*_KEY`).
    static let shareHistoryKey = "mcp.shareHistory"
    static let shareDictionaryKey = "mcp.shareDictionary"

    /// Whether a share setting is on in the profile.
    func shareSetting(_ key: String, completion: @escaping @MainActor (Bool) -> Void) {
        queue.async { [self] in
            let on = profile.flatMap { Core.take(pusmogo_profile_setting($0, key)) } == "1"
            Task { @MainActor in completion(on) }
        }
    }

    /// Writes a share setting; completion gets false when it was not saved.
    func setShareSetting(_ key: String, on: Bool, completion: @escaping @MainActor (Bool) -> Void) {
        queue.async { [self] in
            let ok = profile.map { pusmogo_profile_set_setting($0, key, on ? "1" : "0") == 0 } ?? false
            Task { @MainActor in completion(ok) }
        }
    }

    /// A dictionary entry: what the engine tends to hear → how to write it.
    struct DictionaryEntry: Decodable, Identifiable, Equatable {
        let from: String
        let to: String
        var id: String { from }
    }

    /// The profile's dictionary, in the order the entries were added.
    func dictionaryEntries(completion: @escaping @MainActor ([DictionaryEntry]) -> Void) {
        queue.async { [self] in
            let entries = readDictionary()
            Task { @MainActor in completion(entries) }
        }
    }

    /// Adds or updates an entry (`from` is matched case-insensitively), applies
    /// the new dictionary to the loaded engine at once and returns the list,
    /// or nil when the profile rejected the entry.
    func setDictionaryEntry(from: String, to: String,
                            completion: @escaping @MainActor ([DictionaryEntry]?) -> Void) {
        queue.async { [self] in
            guard let profile, pusmogo_profile_set_dictionary(profile, from, to) == 0 else {
                Task { @MainActor in completion(nil) }
                return
            }
            applyDictionary()
            let entries = readDictionary()
            Task { @MainActor in completion(entries) }
        }
    }

    /// Records an accepted correction for the "most corrected word" insight.
    func addCorrection(wrong: String, right: String) {
        queue.async { [self] in
            guard let profile else { return }
            if pusmogo_profile_add_correction(profile, wrong, right) != 0 { Timings.note("profile: correction not saved") }
        }
    }

    /// Records a fix the user made after a dictation. When the same fix was
    /// seen before, the core has just added it to the dictionary: the engine
    /// gets the new dictionary and `completion` the list; otherwise nil.
    func noteCorrection(wrong: String, right: String,
                        completion: @escaping @MainActor ([DictionaryEntry]?) -> Void) {
        queue.async { [self] in
            let status = profile.map { pusmogo_profile_note_correction($0, wrong, right) } ?? -1
            if status < 0 { Timings.note("profile: correction not saved") }
            guard status == 1 else {
                Task { @MainActor in completion(nil) }
                return
            }
            applyDictionary()
            let entries = readDictionary()
            Task { @MainActor in completion(entries) }
        }
    }

    func removeDictionaryEntry(from: String, completion: @escaping @MainActor ([DictionaryEntry]) -> Void) {
        queue.async { [self] in
            if let profile, pusmogo_profile_remove_dictionary(profile, from) == 0 { applyDictionary() }
            let entries = readDictionary()
            Task { @MainActor in completion(entries) }
        }
    }

    // MARK: Snippets (core snippets.rs): a spoken trigger types a saved text verbatim.

    struct Snippet: Decodable, Identifiable, Equatable {
        let id: Int64
        let trigger: String
        let text: String
        let createdAt: Int64
        enum CodingKeys: String, CodingKey {
            case id, trigger, text
            case createdAt = "created_at"
        }
    }

    /// The saved text when the whole cleaned dictation is a snippet trigger
    /// (case, accents and punctuation ignored), else nil.
    func snippet(for utterance: String, completion: @escaping @MainActor (String?) -> Void) {
        queue.async { [self] in
            let text = profile.flatMap { Core.take(pusmogo_profile_snippet_for($0, utterance)) }
            Task { @MainActor in completion(text) }
        }
    }

    func snippets(completion: @escaping @MainActor ([Snippet]) -> Void) {
        queue.async { [self] in
            let list = readSnippets()
            Task { @MainActor in completion(list) }
        }
    }

    /// Adds a snippet, or replaces the text of the one with the same trigger.
    /// Nil when the profile refused it (empty trigger or text).
    func setSnippet(trigger: String, text: String, completion: @escaping @MainActor ([Snippet]?) -> Void) {
        queue.async { [self] in
            guard let profile, pusmogo_profile_set_snippet(profile, trigger, text) >= 0 else {
                Task { @MainActor in completion(nil) }
                return
            }
            let list = readSnippets()
            Task { @MainActor in completion(list) }
        }
    }

    /// Nil when the new trigger is empty or belongs to another snippet.
    func updateSnippet(id: Int64, trigger: String, text: String,
                       completion: @escaping @MainActor ([Snippet]?) -> Void) {
        queue.async { [self] in
            guard let profile, pusmogo_profile_update_snippet(profile, id, trigger, text) == 0 else {
                Task { @MainActor in completion(nil) }
                return
            }
            let list = readSnippets()
            Task { @MainActor in completion(list) }
        }
    }

    func deleteSnippet(id: Int64, completion: @escaping @MainActor ([Snippet]) -> Void) {
        queue.async { [self] in
            if let profile { _ = pusmogo_profile_delete_snippet(profile, id) }
            let list = readSnippets()
            Task { @MainActor in completion(list) }
        }
    }

    /// The portable form of a snippet in an exported file: `[{"trigger", "text"}]`.
    struct SnippetFile: Codable { let trigger: String; let text: String }

    /// Writes every snippet to `path` as pretty-printed JSON (oldest first).
    func exportSnippets(to path: String, completion: @escaping @MainActor (Bool) -> Void) {
        queue.async { [self] in
            let list = readSnippets().map { SnippetFile(trigger: $0.trigger, text: $0.text) }
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
            let ok = (try? encoder.encode(list).write(to: URL(fileURLWithPath: path), options: .atomic)) != nil
            Timings.note("snippets: export \(ok ? "\(list.count) done" : "failed")")
            Task { @MainActor in completion(ok) }
        }
    }

    /// Adds the snippets in a JSON file to the profile (a trigger already
    /// saved gets the file's text). Nil, with nothing changed, when the file is
    /// not a snippet list; otherwise the new list and how many entries were
    /// skipped (empty trigger or text).
    func importSnippets(from path: String,
                        completion: @escaping @MainActor ((list: [Snippet], added: Int, skipped: Int)?) -> Void) {
        queue.async { [self] in
            guard let profile,
                  let data = FileManager.default.contents(atPath: path),
                  let entries = try? JSONDecoder().decode([SnippetFile].self, from: data) else {
                Timings.note("snippets: import failed")
                Task { @MainActor in completion(nil) }
                return
            }
            var added = 0
            for entry in entries where pusmogo_profile_set_snippet(profile, entry.trigger, entry.text) >= 0 {
                added += 1
            }
            let list = readSnippets()
            Timings.note("snippets: import \(added) of \(entries.count)")
            Task { @MainActor in completion((list, added, entries.count - added)) }
        }
    }

    /// Usage numbers computed by the core from the local profile; nothing leaves the Mac.
    struct Insights: Decodable {
        struct Count: Decodable { let text: String; let count: Int }
        let dictations: Int
        let words: Int
        let wordsPerMinute: Int?
        let currentStreak: Int
        let longestStreak: Int
        let topWord: Count?
        let topCorrection: Count?
        let peakHour: Int?
        let topApp: Count?
        let title: String
        enum CodingKeys: String, CodingKey {
            case dictations, words, title
            case wordsPerMinute = "words_per_minute"
            case currentStreak = "current_streak"
            case longestStreak = "longest_streak"
            case topWord = "top_word"
            case topCorrection = "top_correction"
            case peakHour = "peak_hour"
            case topApp = "top_app"
        }
    }

    /// Insights for the profile, with days and hours in the Mac's time zone.
    func insights(completion: @escaping @MainActor (Insights?) -> Void) {
        queue.async { [self] in
            let offset = Int64(TimeZone.current.secondsFromGMT())
            let json = profile.flatMap { Core.take(pusmogo_profile_insights($0, offset)) }
            let value = json.flatMap { try? JSONDecoder().decode(Insights.self, from: Data($0.utf8)) }
            Task { @MainActor in completion(value) }
        }
    }

    private func readSnippets() -> [Snippet] {
        guard let profile, let json = Core.take(pusmogo_profile_snippets(profile)) else { return [] }
        return (try? JSONDecoder().decode([Snippet].self, from: Data(json.utf8))) ?? []
    }

    /// Writes a copy of the whole profile (history, dictionary, settings) to
    /// `path`. An existing file there is replaced: the save panel has already
    /// asked, and SQLite refuses to export over a file.
    func exportProfile(to path: String, completion: @escaping @MainActor (Bool) -> Void) {
        queue.async { [self] in
            guard let profile else { Task { @MainActor in completion(false) }; return }
            try? FileManager.default.removeItem(atPath: path)
            let ok = pusmogo_profile_export(profile, path) == 0
            Timings.note("profile: export \(ok ? "done" : "failed")")
            Task { @MainActor in completion(ok) }
        }
    }

    /// Replaces the profile's contents with an exported file and applies the
    /// imported dictionary to the engine. A file that is not a Pusmogo profile
    /// (or is from a newer version) changes nothing.
    func importProfile(from path: String, completion: @escaping @MainActor (Bool) -> Void) {
        queue.async { [self] in
            guard let profile else { Task { @MainActor in completion(false) }; return }
            let ok = pusmogo_profile_import(profile, path) == 0
            if ok { applyDictionary() }
            Timings.note("profile: import \(ok ? "done" : "failed")")
            Task { @MainActor in completion(ok) }
        }
    }

    /// Runs on `queue`.
    private func readDictionary() -> [DictionaryEntry] {
        guard let profile, let json = Core.take(pusmogo_profile_dictionary(profile)) else { return [] }
        return (try? JSONDecoder().decode([DictionaryEntry].self, from: Data(json.utf8))) ?? []
    }

    /// Runs on `queue`. The engine keeps its own copy of the dictionary, so it
    /// must be refreshed after every load or idle reload.
    ///
    /// Only the loaded model can count prompt tokens. After an idle unload, an
    /// edit loads a short-lived engine just to count (~0.2 s from the page
    /// cache) and frees it at once, so the Dictionary tab's "list too long"
    /// hint stays right without keeping ~700 MB in memory.
    private func applyDictionary() {
        guard let profile else { return }
        let engine: OpaquePointer
        if let handle {
            engine = handle
        } else if let modelPath, FileManager.default.fileExists(atPath: modelPath),
                  let counter = pusmogo_engine_new(modelPath) {
            engine = counter
        } else {
            return
        }
        defer { if engine != handle { pusmogo_engine_free(engine) } }
        if pusmogo_profile_apply_dictionary(profile, engine) != 0 {
            Timings.note("profile: dictionary not applied")
        }
        let n = Int(pusmogo_dictionary_prompt_left_out(engine)), report = onDictionaryLeftOut
        if engine != handle { Timings.note("profile: dictionary counted with a temporary engine") }
        Task { @MainActor in report?(n < 0 ? nil : n) }
    }

    deinit {
        if let profile { pusmogo_profile_free(profile) }
        if let session { pusmogo_session_free(session) }
        if let handle { pusmogo_engine_free(handle) }
    }

    /// The text with its lines joined by single spaces (for single-line fields).
    static func singleLine(_ text: String) -> String {
        take(pusmogo_single_line(text)) ?? text
    }

    /// Writing-style category of the app in front (0 other, 1 personal messages,
    /// 2 work messages, 3 email, 4 code). The window title only matters for browsers.
    static func styleCategory(bundleID: String, windowTitle: String, overrides: String) -> Int32 {
        pusmogo_style_category(bundleID, windowTitle, overrides)
    }

    /// Cleaned text in a writing style (0 formal, 1 casual, 2 very casual).
    static func applyStyle(_ text: String, category: Int32, style: Int32, language: String) -> String {
        take(pusmogo_style_apply(text, category, style, language)) ?? text
    }

    /// The word(s) the user replaced in dictated text between two reads of the
    /// field, or nil (writing on, deletions, punctuation, long rewrites).
    static func learnedCorrection(dictated: String, before: String, after: String) -> EditWatcher.Suggestion? {
        guard let pair = take(pusmogo_learned_correction(dictated, before, after)) else { return nil }
        let parts = pair.split(separator: "\t", maxSplits: 1, omittingEmptySubsequences: false)
        guard parts.count == 2 else { return nil }
        return EditWatcher.Suggestion(wrong: String(parts[0]), right: String(parts[1]))
    }

    // MARK: Test-set recording (testset.rs). Stateless file calls; nothing is uploaded.

    /// Where test-set recordings live: `testset/` beside the models folder.
    /// `PUSMOGO_TESTSET_DIR` overrides it so self-tests never touch real takes.
    static var testsetDirectory: URL {
        if let dir = ProcessInfo.processInfo.environment["PUSMOGO_TESTSET_DIR"] {
            return URL(fileURLWithPath: dir, isDirectory: true)
        }
        return modelsDirectory.deletingLastPathComponent()
            .appendingPathComponent("testset", isDirectory: true)
    }

    /// One line of the reading script to record, normally or whispered.
    struct TestTake: Decodable, Identifiable, Equatable {
        let id: String
        let lang: String
        let category: String
        let whispered: Bool
        let text: String
        let done: Bool
        /// Unique per take: a whispered take reuses its line's id.
        var key: String { whispered ? id + "-whisper" : id }
    }

    /// A speaker's progress: consent level (nil, "private" or "release"),
    /// the index of the next take to record, and every take in order.
    struct TestSetStatus: Decodable, Equatable {
        let consent: String?
        let next: Int
        let takes: [TestTake]
    }

    /// Outcome of saving one take, as the core reports it.
    enum TakeSaveResult: Equatable {
        case saved, badArgument, tooShort, writeFailed

        init(code: Int32) {
            switch code {
            case 0: self = .saved
            case -2: self = .tooShort
            case -3: self = .writeFailed
            default: self = .badArgument
            }
        }
    }

    /// The speaker's folder under `root` (the name is reduced to safe
    /// characters by the core). Not created until a take or consent is saved.
    static func testsetSpeakerDir(root: URL = testsetDirectory, name: String) -> URL? {
        take(pusmogo_testset_speaker_dir(root.path, name)).map { URL(fileURLWithPath: $0, isDirectory: true) }
    }

    static func testsetStatus(dir: URL) -> TestSetStatus? {
        guard let json = take(pusmogo_testset_status(dir.path)) else { return nil }
        return try? JSONDecoder().decode(TestSetStatus.self, from: Data(json.utf8))
    }

    /// Saves 16 kHz mono samples as the take's WAV plus its reference text.
    static func saveTestTake(dir: URL, id: String, whispered: Bool, samples: [Float]) -> TakeSaveResult {
        let code = samples.withUnsafeBufferPointer {
            pusmogo_testset_save(dir.path, id, whispered ? 1 : 0, $0.baseAddress, $0.count)
        }
        return TakeSaveResult(code: code)
    }

    /// Writes `consent.txt` ("private" or "release", an ISO date, initials or a pseudonym).
    static func writeTestsetConsent(dir: URL, level: String, date: String, who: String) -> Bool {
        pusmogo_testset_consent(dir.path, level, date, who) == 0
    }

    // MARK: MCP config for local AI apps (core `mcp_config.rs`)

    /// An AI app whose MCP config the core knows how to edit.
    enum McpClient: String, CaseIterable, Identifiable {
        case claudeDesktop = "claude-desktop"
        case claudeCode = "claude-code"
        case cursor
        var id: String { rawValue }
        var name: String {
            switch self {
            case .claudeDesktop: return "Claude Desktop"
            case .claudeCode: return "Claude Code"
            case .cursor: return "Cursor"
            }
        }
    }

    /// The home folder whose AI app configs are read and written.
    /// `PUSMOGO_MCP_HOME` points tests at a scratch folder.
    static var mcpHome: String {
        ProcessInfo.processInfo.environment["PUSMOGO_MCP_HOME"]
            ?? FileManager.default.homeDirectoryForCurrentUser.path
    }

    /// The bundled server the configs point at.
    static var mcpCommand: String {
        Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/pusmogo-mcp").path
    }

    static func mcpConfigPath(_ client: McpClient) -> String? {
        take(pusmogo_mcp_config_path(mcpHome, client.rawValue))
    }

    /// 1 installed for this copy, 2 installed for another path (a moved or
    /// deleted copy), 0 not, -1 bad argument, -2 unreadable JSON, -3 read failed.
    static func mcpInstalled(_ client: McpClient) -> Int32 {
        pusmogo_mcp_installed(mcpHome, client.rawValue, mcpCommand)
    }

    /// The diff installing would apply, "" when already there, nil on error.
    static func mcpPreview(_ client: McpClient) -> String? {
        take(pusmogo_mcp_preview(mcpHome, client.rawValue, mcpCommand))
    }

    /// 1 changed, 0 nothing to do, -1 bad argument, -2 unreadable file left as is, -3 write failed.
    static func mcpInstall(_ client: McpClient) -> Int32 {
        pusmogo_mcp_install(mcpHome, client.rawValue, mcpCommand)
    }

    /// Undo: removes only the Pusmogo entry. Codes as `mcpInstall`.
    static func mcpUninstall(_ client: McpClient) -> Int32 {
        pusmogo_mcp_uninstall(mcpHome, client.rawValue)
    }

    private static func take(_ p: UnsafeMutablePointer<CChar>?) -> String? {
        guard let p else { return nil }
        defer { pusmogo_string_free(p) }
        return String(cString: p)
    }
}
