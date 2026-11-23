import AppKit
import CryptoKit

/// Downloads a speech model (none is bundled, so the app stays tiny). One store per
/// model: turbo is the first download and drives the onboarding, Home and Settings
/// states; large-v3 (Hungarian's default above 8 GB) follows quietly in the background.
final class ModelStore: NSObject, ObservableObject, URLSessionDownloadDelegate, @unchecked Sendable {
    enum Status: Equatable {
        case missing
        case downloading(Double)
        case ready
        case failed(String)
    }

    /// Pinned to one commit of the model repository and checked against its SHA-256,
    /// so a changed or corrupted file on the server is never installed.
    /// Test hooks only: `PUSMOGO_MODEL_URL=<url>` (turbo), `PUSMOGO_LARGE_MODEL_URL=<url>`
    /// (large-v3) and `PUSMOGO_HU_MODEL_URL=<url>` (Pusmogo HU) download from elsewhere;
    /// the hash still applies.
    static func url(for model: Core.SpeechModel) -> URL {
        if let override = ProcessInfo.processInfo.environment[urlHook(model)], let url = URL(string: override) {
            return url
        }
        if model == .pusmogoHu {
            // Our Hungarian fine-tune (docs/bench/train-01.md, model card per docs/data-card.md).
            return URL(string: "https://huggingface.co/vatsonai/pusmogo-hu-001/resolve/main/\(model.fileName)")!
        }
        return URL(string: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/\(model.fileName)")!
    }
    static func urlHook(_ model: Core.SpeechModel) -> String {
        switch model {
        case .turbo: return "PUSMOGO_MODEL_URL"
        case .largeV3: return "PUSMOGO_LARGE_MODEL_URL"
        case .pusmogoHu: return "PUSMOGO_HU_MODEL_URL"
        }
    }
    static func sha256(for model: Core.SpeechModel) -> String {
        switch model {
        case .turbo: return "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2"
        case .largeV3: return "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1"
        case .pusmogoHu: return "29a3c2a308a663f8d432d62225f41dfd6e10f79ed7856728eea18f7cdaf0b953"
        }
    }
    static let modelSHA256 = sha256(for: .turbo)

    let model: Core.SpeechModel
    var url: URL { Self.url(for: model) }
    /// Exact size of the pinned file. A model on disk of any other size was cut short
    /// (a full disk, a copy interrupted by hand) and only loads partly: every dictation
    /// would end with no text. Checked by size, not hash, so launch stays instant.
    var modelBytes: Int64 { model.bytes }
    /// A download much smaller than the model is an error page.
    var minimumBytes: Int64 { model.bytes * 7 / 8 }
    var path: String { model.path }
    /// Turbo's size, for the first-download labels.
    static let modelBytes: Int64 = Core.SpeechModel.turbo.bytes
    /// Every size label (Download buttons, onboarding, notices, failure lines) comes
    /// from `modelBytes`, so the app never shows two different sizes for one file.
    static let displaySize = "\(modelBytes / 1_000_000) MB"

    /// Silero speech detector (MIT, ggml-org/whisper-vad): the core runs it when the
    /// file sits next to the Whisper model and skips Whisper on noise-only audio.
    /// Optional, so it is fetched quietly; pinned by hash because the name is versioned.
    static let detectorURL = URL(string: "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v5.1.2.bin")!
    static let detectorSHA256 = "29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf"
    static var detectorPath: String {
        Core.modelsDirectory.appendingPathComponent(detectorURL.lastPathComponent).path
    }

    @Published private(set) var status: Status
    var onInstalled: (@MainActor () -> Void)?
    /// Called on the main actor once the speech detector file is in place.
    var onDetectorInstalled: (@MainActor () -> Void)?
    private var fetchingDetector = false
    private var session: URLSession?
    /// What URLSession hands back when a model download drops or the app quits
    /// mid-download: the next attempt continues from there instead of fetching all
    /// ~574 MB again. Also written to `resumePath`, so it survives a quit.
    private var resumeData: Data?
    private var task: URLSessionDownloadTask?
    /// The running task started from resume data saved by an earlier launch and has
    /// not resumed yet: if it fails, that data was stale and a fresh download starts.
    private var resumingFromDisk = false
    private var quitting = false

    /// Next to the model, removed once the model is installed or found damaged.
    var resumePath: URL {
        URL(fileURLWithPath: path).appendingPathExtension("resume")
    }

    /// What the model file on disk is: absent, the pinned file, or something else.
    enum OnDisk { case absent, installed, damaged(Int64) }
    static func onDisk(_ model: Core.SpeechModel) -> OnDisk {
        guard let attrs = try? FileManager.default.attributesOfItem(atPath: model.path) else {
            return .absent
        }
        let size = (attrs[.size] as? Int64) ?? 0
        return size == model.bytes ? .installed : .damaged(size)
    }
    var onDisk: OnDisk { Self.onDisk(model) }
    /// Turbo, the model every dictation can fall back to.
    static var onDisk: OnDisk { onDisk(.turbo) }
    static var isInstalled: Bool {
        if case .installed = onDisk { return true }
        return false
    }

    /// Log lines name the model only for the second store, so turbo's stay as they were.
    private var tag: String { model == .turbo ? "model" : "model \(model.fileName)" }

    init(model: Core.SpeechModel = .turbo) {
        self.model = model
        switch Self.onDisk(model) {
        case .installed: status = .ready
        case .absent: status = .missing
        case .damaged(let size):
            NSLog("\(model == .turbo ? "model" : "model \(model.fileName)"): file on disk is damaged (\(size) bytes, expected \(model.bytes))")
            status = .failed(Self.damagedOnDisk)
        }
        if let preview = Self.previewStatus(for: model) { status = preview }
        super.init()
        if !Self.inert {
            NotificationCenter.default.addObserver(
                self, selector: #selector(saveResumeDataBeforeQuit),
                name: NSApplication.willTerminateNotification, object: nil)
        }
    }

    /// Failure lines are complete sentences in the app's UI language: the system's
    /// `localizedDescription` follows the macOS language, which may differ.
    static func tooSmall(_ bytes: Int64, of expected: Int64 = modelBytes) -> String {
        String(localized: "A letöltés hiányos (\(bytes / 1_000_000) / \(expected / 1_000_000) MB). Próbáld újra.")
    }
    static var networkFailed: String {
        String(localized: "A letöltés megszakadt. Nézd meg az internetkapcsolatot, és próbáld újra.")
    }
    static var checksumFailed: String {
        String(localized: "A letöltött modell sérült. Próbáld újra.")
    }
    static var damagedOnDisk: String {
        String(localized: "A modellfájl sérült vagy hiányos. Töltsd le újra.")
    }
    static var saveFailed: String {
        String(localized: "A modellt nem tudtam elmenteni a Pusmogo mappájába.")
    }
    /// `neededBytes` is the room the model needs (`neededBytes(for:)`), shown rounded
    /// to 100 MB: about 600 MB for turbo, 1100 MB for large-v3.
    static func diskFull(freeBytes: Int64?, neededBytes: Int64 = neededBytes(for: .turbo)) -> String {
        let need = (neededBytes + 50_000_000) / 100_000_000 * 100
        if let freeBytes {
            let free = max(0, freeBytes) / 1_000_000
            return String(localized: "Nincs elég hely a lemezen: a modellhez kb. \(need)\u{00A0}MB kell, most \(free)\u{00A0}MB szabad. Szabadíts fel helyet, és próbáld újra.")
        }
        return String(localized: "Nincs elég hely a lemezen: a modellhez kb. \(need)\u{00A0}MB kell. Szabadíts fel helyet, és próbáld újra.")
    }

    /// Room a model needs on a volume: the file plus a margin for the staged copy's
    /// metadata and whatever else writes meanwhile.
    static func neededBytes(for model: Core.SpeechModel) -> Int64 { model.bytes + 26_000_000 }
    var neededBytes: Int64 { Self.neededBytes(for: model) }

    /// The first volume (models folder, then the download's temporary folder) that has
    /// less than `neededBytes` free, with its free bytes; nil when both have room.
    /// URLSession writes to the temporary folder, and the model is moved from there,
    /// so each needs room; on the same volume the move is a rename and one check covers
    /// both. Checking up front saves a 574 MB download that could never be saved.
    static func shortOfSpace(needed neededBytes: Int64) -> Int64? {
        let fm = FileManager.default
        try? fm.createDirectory(at: Core.modelsDirectory, withIntermediateDirectories: true)
        var seen = Set<String>()
        for dir in [Core.modelsDirectory, fm.temporaryDirectory] {
            guard let values = try? dir.resourceValues(forKeys: [
                .volumeURLKey, .volumeAvailableCapacityForImportantUsageKey, .volumeAvailableCapacityKey,
            ]) else { continue }
            // The important-usage figure counts purgeable space on the startup disk but
            // reads 0 on other volumes (an external drive, a disk image): take the larger.
            let important = values.volumeAvailableCapacityForImportantUsage ?? 0
            let plain = Int64(values.volumeAvailableCapacity ?? 0)
            let free = max(important, plain)
            if free == 0 && values.volumeAvailableCapacity == nil { continue }
            let volume = values.volume?.path ?? dir.path
            if !seen.insert(volume).inserted { continue }
            if free < neededBytes {
                NSLog("model: not enough disk space on \(volume): \(free / 1_000_000) MB free")
                return free
            }
        }
        return nil
    }

    /// A full disk shows up as Cocoa's out-of-space error when the file is moved and as
    /// URLSession's cannot-write error while it downloads.
    static func isOutOfSpace(_ error: Error) -> Bool {
        let e = error as NSError
        if e.domain == NSCocoaErrorDomain && e.code == NSFileWriteOutOfSpaceError { return true }
        if e.domain == NSURLErrorDomain && e.code == NSURLErrorCannotWriteToFile { return true }
        if e.domain == NSPOSIXErrorDomain && e.code == Int(ENOSPC) { return true }
        if let underlying = e.userInfo[NSUnderlyingErrorKey] as? Error { return isOutOfSpace(underlying) }
        return false
    }

    /// Test hooks only: `PUSMOGO_PREVIEW_MODEL=missing|downloading|failed|offline|damaged|diskfull`
    /// shows that download state for turbo, `PUSMOGO_PREVIEW_LARGE_MODEL=…` (same values)
    /// for large-v3. `download()` does nothing then, so neither the network nor the
    /// model file is touched. Either hook keeps both stores inert.
    private static func previewStatus(for model: Core.SpeechModel) -> Status? {
        let env = ProcessInfo.processInfo.environment
        let value = env[model == .turbo ? "PUSMOGO_PREVIEW_MODEL" : model == .largeV3 ? "PUSMOGO_PREVIEW_LARGE_MODEL" : "PUSMOGO_PREVIEW_HU_MODEL"]
        switch value {
        case "missing": return .missing
        case "downloading": return .downloading(0.42)
        case "failed": return .failed(tooSmall(29_000_000, of: model.bytes))
        case "offline": return .failed(networkFailed)
        case "damaged": return .failed(damagedOnDisk)
        case "diskfull": return .failed(diskFull(freeBytes: 102_000_000, neededBytes: neededBytes(for: model)))
        default: return nil
        }
    }
    private static var inert: Bool {
        let env = ProcessInfo.processInfo.environment
        return env["PUSMOGO_PREVIEW_MODEL"] != nil || env["PUSMOGO_PREVIEW_LARGE_MODEL"] != nil
    }

    var progress: Double {
        switch status {
        case .ready: return 1
        case .downloading(let p): return p
        default: return 0
        }
    }

    /// Seconds without a byte before a model download counts as stopped.
    /// `PUSMOGO_DOWNLOAD_STALL_SECS` overrides it for tests.
    static let stallTimeout: TimeInterval = {
        if let s = ProcessInfo.processInfo.environment["PUSMOGO_DOWNLOAD_STALL_SECS"],
           let v = TimeInterval(s), v > 0 { return v }
        return 20
    }()

    /// `status` is read from disk at launch only; a model removed or replaced while
    /// the app runs would otherwise stay "ready", and `download()` would do nothing.
    /// Called before a dictation is refused and whenever Home or Settings appears.
    func recheck() {
        guard !Self.inert, status == .ready else { return }
        switch onDisk {
        case .installed: return
        case .absent:
            NSLog("\(tag): file gone since launch")
            status = .missing
        case .damaged(let size):
            NSLog("\(tag): file on disk is damaged (\(size) bytes, expected \(modelBytes))")
            status = .failed(Self.damagedOnDisk)
        }
    }

    func download() {
        if Self.inert { return }
        recheck()
        switch status {
        case .ready, .downloading: return
        default: break
        }
        if case .damaged = onDisk {
            // Never leave a broken file where the engine would load it.
            try? FileManager.default.removeItem(atPath: path)
            NSLog("\(tag): removed the damaged file before downloading")
        }
        if let free = Self.shortOfSpace(needed: neededBytes) {
            status = .failed(Self.diskFull(freeBytes: free, neededBytes: neededBytes))
            return
        }
        status = .downloading(0)
        if model == .turbo { fetchSpeechDetector() }  // small, so it lands before the model and loads with it
        // A server that accepts and then stops sending would otherwise hold the bar at
        // a frozen percentage for URLSession's default 60 s. This limit is idle time
        // between packets, not the whole transfer, so a slow but live link still finishes.
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = Self.stallTimeout
        let session = URLSession(configuration: config, delegate: self, delegateQueue: .main)
        self.session = session
        resumingFromDisk = false
        if resumeData == nil, let saved = try? Data(contentsOf: resumePath) {
            resumeData = saved
            resumingFromDisk = true
        }
        let task: URLSessionDownloadTask
        if let data = resumeData {
            resumeData = nil
            NSLog("\(tag): resuming download (\(data.count) bytes of resume data\(resumingFromDisk ? ", saved" : ""))")
            task = session.downloadTask(withResumeData: data)
        } else {
            NSLog("\(tag): downloading from \(url.host ?? url.absoluteString)")
            task = session.downloadTask(with: url)
        }
        self.task = task
        task.resume()
    }

    /// On quit mid-download, asks URLSession for resume data and writes it next to the
    /// model. Runs on the main thread, so it spins the run loop (at most 3 s) while
    /// the session's main-queue callback delivers the data.
    @objc private func saveResumeDataBeforeQuit() {
        guard case .downloading = status, let task else { return }
        quitting = true
        var done = false
        task.cancel { [weak self] data in
            OperationQueue.main.addOperation {
                if let data, let self { Self.writeResumeData(data, to: self.resumePath) }
                self?.resumeData = nil
                done = true
            }
        }
        let deadline = Date().addingTimeInterval(3)
        while !done && Date() < deadline {
            RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05))
        }
        NSLog("\(tag): quit mid-download, resume data \(done ? "saved" : "not saved")")
    }

    private static func writeResumeData(_ data: Data, to resumePath: URL) {
        do {
            try FileManager.default.createDirectory(at: Core.modelsDirectory, withIntermediateDirectories: true)
            try data.write(to: resumePath, options: .atomic)
        } catch {
            NSLog("model: could not save resume data: \(error)")
        }
    }

    private func removeResumeData() {
        try? FileManager.default.removeItem(at: resumePath)
    }

    /// Fetches the speech detector if it is missing. Best effort: a failure only
    /// means Whisper also runs on noise, so it is logged and retried next launch.
    func fetchSpeechDetector() {
        if Self.inert || fetchingDetector { return }
        if FileManager.default.fileExists(atPath: Self.detectorPath) { return }
        fetchingDetector = true
        URLSession.shared.downloadTask(with: Self.detectorURL) { [weak self] location, _, error in
            let installed = Self.installDetector(from: location, error: error)
            Task { @MainActor in
                guard let self else { return }
                self.fetchingDetector = false
                if installed { self.onDetectorInstalled?() }
            }
        }.resume()
    }

    /// Runs on the download's queue, before the temporary file is deleted.
    private static func installDetector(from location: URL?, error: Error?) -> Bool {
        guard let location, error == nil else {
            NSLog("model: speech detector download failed: \(error.map { "\($0)" } ?? "no file")")
            return false
        }
        let fm = FileManager.default
        guard let data = try? Data(contentsOf: location),
              SHA256.hash(data: data).map({ String(format: "%02x", $0) }).joined() == detectorSHA256 else {
            NSLog("model: speech detector download did not match its checksum")
            return false
        }
        do {
            try fm.createDirectory(at: Core.modelsDirectory, withIntermediateDirectories: true)
            let dest = URL(fileURLWithPath: detectorPath)
            try? fm.removeItem(at: dest)
            try fm.moveItem(at: location, to: dest)
            NSLog("model: speech detector installed")
            return true
        } catch {
            NSLog("model: speech detector save failed: \(error)")
            return false
        }
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData _: Int64,
                    totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        let expected = totalBytesExpectedToWrite > 0 ? totalBytesExpectedToWrite : modelBytes
        status = .downloading(min(0.999, Double(totalBytesWritten) / Double(expected)))
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didResumeAtOffset fileOffset: Int64,
                    expectedTotalBytes: Int64) {
        resumingFromDisk = false
        NSLog("\(tag): download resumed at \(fileOffset / 1_000_000) MB")
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        removeResumeData()
        let fm = FileManager.default
        let size = (try? fm.attributesOfItem(atPath: location.path)[.size] as? Int64) ?? 0
        guard size >= minimumBytes else {
            NSLog("\(tag): download too small (\(size) bytes)")
            status = .failed(Self.tooSmall(size, of: modelBytes))
            return
        }
        // The temporary file is deleted when this returns: move it next to its final
        // place first, then hash it off the main thread (~1 s for 574 MB).
        let dest = URL(fileURLWithPath: path)
        let staged = dest.appendingPathExtension("part")
        do {
            try fm.createDirectory(at: Core.modelsDirectory, withIntermediateDirectories: true)
            try? fm.removeItem(at: staged)
            try fm.moveItem(at: location, to: staged)
        } catch {
            NSLog("\(tag): save failed: \(error)")
            status = .failed(Self.isOutOfSpace(error) ? Self.diskFull(freeBytes: nil, neededBytes: neededBytes) : Self.saveFailed)
            return
        }
        status = .downloading(0.999)
        let sha = Self.sha256(for: model), tag = tag, needed = neededBytes
        Task.detached(priority: .utility) { [self] in
            let result = Self.install(staged: staged, to: dest, sha256: sha, needed: needed, tag: tag)
            await MainActor.run {
                self.status = result ?? .ready
                if result == nil { self.onInstalled?() }
            }
        }
    }

    /// Checks the staged file's SHA-256 and moves it into place.
    /// Returns nil on success, or the failure to show.
    private static func install(staged: URL, to dest: URL, sha256 expected: String, needed: Int64, tag: String) -> Status? {
        let fm = FileManager.default
        guard let digest = sha256(of: staged), digest == expected else {
            NSLog("\(tag): download did not match its checksum")
            try? fm.removeItem(at: staged)
            return .failed(checksumFailed)
        }
        do {
            try? fm.removeItem(at: dest)
            try fm.moveItem(at: staged, to: dest)
            NSLog("\(tag): installed, checksum ok")
            return nil
        } catch {
            NSLog("\(tag): save failed: \(error)")
            try? fm.removeItem(at: staged)
            return .failed(isOutOfSpace(error) ? diskFull(freeBytes: nil, neededBytes: needed) : saveFailed)
        }
    }

    /// Streams the file through SHA-256 in 4 MB reads, so the model never sits in memory.
    private static func sha256(of url: URL) -> String? {
        guard let handle = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? handle.close() }
        var hasher = SHA256()
        while true {
            let chunk: Data? = autoreleasepool { try? handle.read(upToCount: 4 << 20) }
            guard let chunk, !chunk.isEmpty else { break }
            hasher.update(data: chunk)
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        session.finishTasksAndInvalidate()
        self.session = nil
        self.task = nil
        if quitting { return }
        if let error, case .downloading = status {
            if resumingFromDisk {
                // Saved by an earlier launch and no longer usable (its partial file
                // was cleaned up, or the server changed): start again from zero.
                NSLog("\(tag): saved resume data unusable (\(error)), starting over")
                removeResumeData()
                resumingFromDisk = false
                status = .missing
                download()
                return
            }
            // Present when the server supports ranges and the file did not change;
            // the URL is pinned to one commit, so a resumed file is still the same file.
            resumeData = (error as NSError).userInfo[NSURLSessionDownloadTaskResumeData] as? Data
            if let resumeData { Self.writeResumeData(resumeData, to: resumePath) } else { removeResumeData() }
            NSLog("\(tag): download failed (resumable: \(resumeData != nil)): \(error)")
            status = .failed(Self.isOutOfSpace(error) ? Self.diskFull(freeBytes: nil) : Self.networkFailed)
        }
    }
}
