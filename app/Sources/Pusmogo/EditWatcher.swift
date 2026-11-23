import AppKit

/// Learning from edits, the watching half. After a dictation
/// lands in a readable field, the same field is read again once a second for a
/// while. When its text has changed and then held still for two reads in a row
/// (so a half-typed word does not count), the core decides whether the user
/// replaced a word of the dictation (`Core.learnedCorrection`). A match is
/// handed to `onSuggestion`; nothing is stored or logged with its words.
///
/// Only one field is watched at a time: a new insertion replaces the old watch.
@MainActor
enum EditWatcher {
    struct Suggestion: Equatable {
        let wrong: String
        let right: String
        /// Already put into the dictionary, because the same fix was seen
        /// before; the bar then offers Undo instead of asking.
        var learned = false
    }

    /// Called once per watch when the user corrected part of the dictation.
    static var onSuggestion: ((Suggestion) -> Void)?

    /// How long a field is watched after an insertion. `PUSMOGO_EDIT_WATCH_SECS`
    /// overrides it (0 turns watching off).
    static let watchSeconds = ProcessInfo.processInfo.environment["PUSMOGO_EDIT_WATCH_SECS"]
        .flatMap(Double.init) ?? 30
    static let interval: TimeInterval = 1.0
    /// Fields longer than this are not watched (cost of each read).
    private static let maxLength = 100_000

    private static var generation = 0

    /// Starts watching `element`, which now holds `dictated` (the text exactly as
    /// inserted) and reads `before`.
    static func watch(_ element: AXUIElement, dictated: String, before: String) {
        generation += 1
        guard watchSeconds > 0, (before as NSString).length <= maxLength,
              !dictated.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        let gen = generation
        let deadline = Date().addingTimeInterval(watchSeconds)
        var last = before
        var asked = before
        func tick() {
            guard gen == generation else { return }  // replaced by a newer insertion
            guard let now = read(element) else { return stop("field gone") }
            if now == last, now != asked {
                asked = now
                if let s = Core.learnedCorrection(dictated: dictated, before: before, after: now) {
                    stop("suggestion")
                    onSuggestion?(s)
                    return
                }
            }
            last = now
            if Date() < deadline {
                DispatchQueue.main.asyncAfter(deadline: .now() + interval) { tick() }
            } else {
                stop("time up")
            }
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + interval) { tick() }
    }

    /// Stops the current watch, if any (e.g. a new dictation started).
    static func cancel() { generation += 1 }

    private static func stop(_ why: String) {
        generation += 1
        Timings.note("edit-watch: stopped (\(why))")
    }

    private static func read(_ el: AXUIElement) -> String? {
        AXUIElementSetMessagingTimeout(el, 0.25)
        var v: CFTypeRef?
        guard AXUIElementCopyAttributeValue(el, kAXValueAttribute as CFString, &v) == .success,
              let s = v as? String, (s as NSString).length <= maxLength else { return nil }
        return s
    }
}
