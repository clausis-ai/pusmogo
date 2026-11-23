import Foundation
import PusmogoKeys

/// Per-dictation stage timings (chunk A0). Appended to
/// ~/Library/Logs/Pusmogo/timings.jsonl (or `PUSMOGO_TIMINGS_LOG`, for test runs) and
/// shown in the panel. The file is cut back to its newest lines past `LogTrim.maxBytes`.
struct Timings: Codable {
    var date = Date()
    var engine = ""
    var audioSeconds: Double = 0
    var micStartMs: Double = 0    // key down → first audio from the microphone
    var stopMs: Double = 0        // key release → audio closed
    var transcribeMs: Double = 0  // engine + cleanup (Rust)
    var insertMs: Double = 0      // text insertion
    var totalMs: Double = 0       // key release → text inserted
    var chars = 0
    var insert = ""               // direct | pasted | pasted:unverified | copied:<reason>

    var summary: String {
        String(format: NSLocalizedString("%.0f ms  (hang %.0f · átírás %.0f · beillesztés %.0f)", comment: "Home tab: timings of the last dictation"), totalMs, stopMs, transcribeMs, insertMs)
    }

    /// One-line engine lifecycle event in the system log (`log stream --predicate 'process == "Pusmogo"'`).
    static func note(_ message: String) {
        NSLog("pusmogo engine: %@", message)
    }

    func log() {
        let line = String(format: "pusmogo timing: total %.1f ms | mic start %.1f | stop %.1f | transcribe %.1f | insert %.1f (%@) | audio %.2f s | %@",
                          totalMs, micStartMs, stopMs, transcribeMs, insertMs, insert, audioSeconds, engine)
        NSLog("%@", line)
        let url = Self.logURL
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        guard var data = try? encoder.encode(self) else { return }
        data.append(0x0A)
        if let handle = try? FileHandle(forWritingTo: url) {
            handle.seekToEndOfFile()
            handle.write(data)
            try? handle.close()
        } else {
            try? data.write(to: url)
        }
        Self.capLog(at: url)
    }

    static var logURL: URL {
        if let path = ProcessInfo.processInfo.environment["PUSMOGO_TIMINGS_LOG"], !path.isEmpty {
            return URL(fileURLWithPath: (path as NSString).expandingTildeInPath)
        }
        return FileManager.default.urls(for: .libraryDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Logs/Pusmogo/timings.jsonl")
    }

    /// Keeps the log from growing without bound: past the cap only the newest lines stay.
    private static func capLog(at url: URL) {
        guard let size = (try? FileManager.default.attributesOfItem(atPath: url.path))?[.size] as? Int,
              size > LogTrim.maxBytes,
              let data = try? Data(contentsOf: url),
              let kept = LogTrim.trimmed(data) else { return }
        do {
            try kept.write(to: url, options: .atomic)
            note("timings log cut from \(data.count) to \(kept.count) bytes")
        } catch {
            note("timings log not cut: \(error.localizedDescription)")
        }
    }
}

@inline(__always) func nowMs() -> Double { Double(DispatchTime.now().uptimeNanoseconds) / 1_000_000 }
