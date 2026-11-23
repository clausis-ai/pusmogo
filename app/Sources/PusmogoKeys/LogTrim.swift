import Foundation

/// Size cap for an append-only line log (the per-dictation timings file).
public enum LogTrim {
    /// Above this many bytes the log is cut back to its newest lines.
    public static let maxBytes = 256 * 1024
    /// Lines kept after a cut (about 230 bytes each, so well under `maxBytes`).
    public static let keepLines = 1000

    /// The newest `keepLines` complete lines of `data` when it is larger than
    /// `maxBytes`, or nil when it needs no cut. A torn last line (no newline)
    /// is kept as it is so the next append still starts on a fresh line.
    public static func trimmed(_ data: Data, maxBytes: Int = maxBytes, keepLines: Int = keepLines) -> Data? {
        guard data.count > maxBytes, keepLines > 0 else { return nil }
        let newline = UInt8(ascii: "\n")
        var seen = 0
        var i = data.endIndex
        // A trailing newline ends the last line; it is not a line of its own.
        if i > data.startIndex, data[i - 1] == newline { i -= 1 }
        while i > data.startIndex {
            i -= 1
            if data[i] == newline {
                seen += 1
                if seen == keepLines { return Data(data[(i + 1)...]) }
            }
        }
        // Fewer lines than `keepLines` but still too big (very long lines): keep the newest one.
        guard let last = data.dropLast().lastIndex(of: newline) else { return nil }
        return Data(data[(last + 1)...])
    }
}
