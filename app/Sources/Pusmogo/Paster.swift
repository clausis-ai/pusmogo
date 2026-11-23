import AppKit
import Carbon.HIToolbox

/// Puts text into the focused field of the frontmost app.
///
/// 1. **Accessibility direct insert**: replace the selection of the focused
///    element and read the field back to prove the text landed. Skipped inside
///    web content (browsers, Electron), where a page may not notice an AX edit.
/// 2. **Pasteboard + ⌘V**: the text is offered lazily, so we learn whether
///    anyone read it. A read alone proves little (clipboard managers read every
///    new clipboard at once), so the focused field is read back too. The user's
///    clipboard (every item and type) is restored only when the field changed;
///    when the field cannot be read, the dictated text stays on the clipboard
///    so it is never lost.
/// 3. **Copied**: no permission, secure input, or nobody read the paste. The
///    text stays on the clipboard and the caller tells the user to press ⌘V.
@MainActor
enum TextInserter {
    enum Outcome: Equatable {
        case direct
        /// The focused field changed after ⌘V; the old clipboard is restored.
        case pasted
        /// The paste was read but the field cannot be read back; the text is
        /// left on the clipboard in case it did not land.
        case pastedUnverified
        case copied(Reason)
    }

    enum Reason: String {
        case noPermission = "no-permission"
        case secureInput = "secure-input"
        case notRead = "not-read"
    }

    /// How long a target app gets to read the pasteboard after ⌘V.
    static let pasteReadTimeout: TimeInterval = 1.0
    /// How long the focused field gets to show the pasted text after the read.
    static let pasteLandTimeout: TimeInterval = 0.6
    /// `PUSMOGO_INSERT_MODE=paste` skips the Accessibility path (for tests).
    static var allowDirect = ProcessInfo.processInfo.environment["PUSMOGO_INSERT_MODE"] != "paste"

    static func insert(_ text: String, completion: @escaping @MainActor (Outcome) -> Void = { _ in }) {
        EditWatcher.cancel()
        guard Permissions.accessibilityGranted(prompt: false) else {
            putOnClipboard(text)
            return completion(.copied(.noPermission))
        }
        guard !IsSecureEventInputEnabled() else {
            putOnClipboard(text)
            return completion(.copied(.secureInput))
        }
        let text = fittedToFocusedField(text)
        if allowDirect, insertDirect(text) { return completion(.direct) }
        paste(text, completion: completion)
    }

    /// Single-line fields (web `<input>`, search boxes) drop line breaks, which
    /// glues "jövök.\nHozol" into "jövök.Hozol" and defeats the paste read-back.
    /// For them, spoken line breaks become spaces.
    private static func fittedToFocusedField(_ text: String) -> String {
        guard text.contains(where: \.isNewline) else { return text }
        let system = AXUIElementCreateSystemWide()
        AXUIElementSetMessagingTimeout(system, 0.25)
        guard let el = copyElement(system, kAXFocusedUIElementAttribute) else { return text }
        AXUIElementSetMessagingTimeout(el, 0.25)
        let role = copyString(el, kAXRoleAttribute)
        guard role == kAXTextFieldRole as String || role == kAXComboBoxRole as String else { return text }
        Timings.note("insert: single-line field, line breaks become spaces")
        return Core.singleLine(text)
    }

    // MARK: Accessibility

    /// Fields longer than this are not read back (cost); they get the paste path.
    private static let maxDirectValueLength = 100_000

    private static func insertDirect(_ text: String) -> Bool {
        let system = AXUIElementCreateSystemWide()
        AXUIElementSetMessagingTimeout(system, 0.25)
        guard let el = copyElement(system, kAXFocusedUIElementAttribute) else { return false }
        AXUIElementSetMessagingTimeout(el, 0.25)
        if copyString(el, kAXSubroleAttribute) == kAXSecureTextFieldSubrole as String { return false }
        if isInsideWebArea(el) { return false }

        var settable: DarwinBoolean = false
        guard AXUIElementIsAttributeSettable(el, kAXSelectedTextAttribute as CFString, &settable) == .success,
              settable.boolValue,
              let before = copyString(el, kAXValueAttribute),
              (before as NSString).length <= maxDirectValueLength,
              let range = copyRange(el)
        else { return false }

        guard AXUIElementSetAttributeValue(el, kAXSelectedTextAttribute as CFString, text as CFString) == .success,
              let after = copyString(el, kAXValueAttribute)
        else { return false }

        // Proof: the field now holds `before` with the selection replaced by `text`.
        let ns = before as NSString
        let expected = ns.replacingCharacters(in: NSRange(location: range.location, length: range.length), with: text)
        guard after == expected else {
            Timings.note(after == before
                ? "insert: AX edit did not land, pasting instead"
                : "insert: AX edit landed but reads back differently, not pasting again")
            // Unchanged: nothing landed, paste is safe. Changed differently:
            // something landed; pasting again would type it twice.
            return after != before
        }
        // Caret after the inserted text, as typing would leave it.
        var caret = CFRange(location: range.location + (text as NSString).length, length: 0)
        if let value = AXValueCreate(.cfRange, &caret) {
            AXUIElementSetAttributeValue(el, kAXSelectedTextRangeAttribute as CFString, value)
        }
        EditWatcher.watch(el, dictated: text, before: after)
        return true
    }

    private static func isInsideWebArea(_ start: AXUIElement) -> Bool {
        var el: AXUIElement? = start
        for _ in 0..<40 {
            guard let cur = el else { return false }
            if copyString(cur, kAXRoleAttribute) == "AXWebArea" { return true }
            el = copyElement(cur, kAXParentAttribute)
        }
        return false
    }

    private static func copyElement(_ el: AXUIElement, _ attr: String) -> AXUIElement? {
        var v: CFTypeRef?
        guard AXUIElementCopyAttributeValue(el, attr as CFString, &v) == .success, let v,
              CFGetTypeID(v) == AXUIElementGetTypeID() else { return nil }
        return (v as! AXUIElement)
    }

    private static func copyString(_ el: AXUIElement, _ attr: String) -> String? {
        var v: CFTypeRef?
        guard AXUIElementCopyAttributeValue(el, attr as CFString, &v) == .success else { return nil }
        return v as? String
    }

    private static func copyRange(_ el: AXUIElement) -> CFRange? {
        var v: CFTypeRef?
        guard AXUIElementCopyAttributeValue(el, kAXSelectedTextRangeAttribute as CFString, &v) == .success, let v,
              CFGetTypeID(v) == AXValueGetTypeID() else { return nil }
        var r = CFRange()
        return AXValueGetValue(v as! AXValue, .cfRange, &r) ? r : nil
    }

    // MARK: Pasteboard

    /// Hands out the text on demand and records that someone asked for it.
    private final class LazyText: NSObject, NSPasteboardItemDataProvider {
        let text: String
        var read = false
        init(_ text: String) { self.text = text }
        func pasteboard(_ pasteboard: NSPasteboard?, item: NSPasteboardItem, provideDataForType type: NSPasteboard.PasteboardType) {
            read = true
            item.setString(text, forType: type)
        }
    }

    /// Kept alive until the paste is resolved; the pasteboard holds it weakly.
    private static var pending: LazyText?

    private static func paste(_ text: String, completion: @escaping @MainActor (Outcome) -> Void) {
        let pb = NSPasteboard.general
        let saved = snapshot(pb)
        // Before: the field's text, when readable. Asking also wakes up apps
        // that build their accessibility tree lazily (WebKit, Chromium).
        let app = NSWorkspace.shared.frontmostApplication.map { AXUIElementCreateApplication($0.processIdentifier) }
        let before = app.flatMap { focusedValue(in: $0) }
        let provider = LazyText(text)
        pending = provider
        let item = NSPasteboardItem()
        item.setDataProvider(provider, forTypes: [.string])
        pb.clearContents()
        pb.writeObjects([item])
        let ourChange = pb.changeCount

        let src = CGEventSource(stateID: .combinedSessionState)
        let down = CGEvent(keyboardEventSource: src, virtualKey: CGKeyCode(kVK_ANSI_V), keyDown: true)
        let up = CGEvent(keyboardEventSource: src, virtualKey: CGKeyCode(kVK_ANSI_V), keyDown: false)
        down?.flags = .maskCommand
        up?.flags = .maskCommand
        down?.post(tap: .cghidEventTap)
        up?.post(tap: .cghidEventTap)

        func release() { if pending === provider { pending = nil } }

        func notLanded() {
            release()
            if pb.changeCount == ourChange { putOnClipboard(text) }
            completion(.copied(.notRead))
        }

        /// After the read: did the focused field change? Without a "before",
        /// the field holding the text counts as proof.
        var landDeadline = Date.distantFuture
        var sawField = false
        func verify() {
            if landDeadline == .distantFuture { landDeadline = Date().addingTimeInterval(pasteLandTimeout) }
            let field = app.flatMap { focusedField(in: $0) }
            let now = field?.value
            if now != nil { sawField = true }
            let landed: Bool
            if let now, let before { landed = now != before } else if let now { landed = now.contains(text) } else { landed = false }
            if landed {
                if let field, let now { EditWatcher.watch(field.element, dictated: text, before: now) }
                completion(.pasted)
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                    release()
                    if pb.changeCount == ourChange, !saved.isEmpty {
                        pb.clearContents()
                        pb.writeObjects(saved)
                    }
                }
            } else if Date() < landDeadline {
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.03) { verify() }
            } else if sawField {
                Timings.note("insert: paste was read but the field did not change")
                notLanded()
            } else {
                // No readable field: keep the text on the clipboard, just in case.
                release()
                completion(.pastedUnverified)
            }
        }

        let readDeadline = Date().addingTimeInterval(pasteReadTimeout)
        func check() {
            if provider.read {
                verify()
            } else if pb.changeCount != ourChange {
                // Someone else replaced the clipboard; leave it alone.
                release()
                completion(.copied(.notRead))
            } else if Date() >= readDeadline {
                // Nobody pasted: leave the text on the clipboard for ⌘V.
                notLanded()
            } else {
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.02) { check() }
            }
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.02) { check() }
    }

    /// Text of the app's focused element, if it has readable text.
    private static func focusedValue(in app: AXUIElement) -> String? {
        focusedField(in: app)?.value
    }

    /// The app's focused element and its text, if it has readable text.
    private static func focusedField(in app: AXUIElement) -> (element: AXUIElement, value: String)? {
        AXUIElementSetMessagingTimeout(app, 0.25)
        guard let el = copyElement(app, kAXFocusedUIElementAttribute) else { return nil }
        AXUIElementSetMessagingTimeout(el, 0.25)
        guard let value = copyString(el, kAXValueAttribute),
              (value as NSString).length <= maxDirectValueLength else { return nil }
        return (el, value)
    }

    private static func snapshot(_ pb: NSPasteboard) -> [NSPasteboardItem] {
        pb.pasteboardItems?.map { item -> NSPasteboardItem in
            let copy = NSPasteboardItem()
            for type in item.types {
                if let data = item.data(forType: type) { copy.setData(data, forType: type) }
            }
            return copy
        } ?? []
    }

    private static func putOnClipboard(_ text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }
}

enum Permissions {
    static func accessibilityGranted(prompt: Bool) -> Bool {
        let key = kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String
        return AXIsProcessTrustedWithOptions([key: prompt] as CFDictionary)
    }

    static func openAccessibilitySettings() {
        open("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
    }

    static func openMicrophoneSettings() {
        open("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
    }

    static func openKeyboardSettings() {
        open("x-apple.systempreferences:com.apple.Keyboard-Settings.extension")
    }

    private static func open(_ s: String) {
        if let url = URL(string: s) { NSWorkspace.shared.open(url) }
    }
}

/// Reads the text just before the caret in the focused field, so names already
/// on screen can steer the engine's spelling (`Core.setContext`). Returns nil
/// for password fields, under secure input, without Accessibility, or when the
/// field cannot be read. Safe off the main thread; the caller never logs or
/// stores the text.
enum FocusedText {
    /// The core looks at the last 2000 characters only.
    static let maxLength = 2000

    static func beforeCaret() -> String? {
        guard AXIsProcessTrusted(), !IsSecureEventInputEnabled() else { return nil }
        let system = AXUIElementCreateSystemWide()
        AXUIElementSetMessagingTimeout(system, 0.2)
        var v: CFTypeRef?
        guard AXUIElementCopyAttributeValue(system, kAXFocusedUIElementAttribute as CFString, &v) == .success,
              let v, CFGetTypeID(v) == AXUIElementGetTypeID() else { return nil }
        let el = v as! AXUIElement
        AXUIElementSetMessagingTimeout(el, 0.2)
        if string(el, kAXSubroleAttribute) == kAXSecureTextFieldSubrole as String { return nil }
        guard let caret = selectionStart(el) else {
            // No caret to cut at: only a short value is worth taking whole.
            guard let value = string(el, kAXValueAttribute) else { return nil }
            return String(value.suffix(maxLength))
        }
        let from = max(0, caret - maxLength)
        var range = CFRange(location: from, length: caret - from)
        if range.length == 0 { return nil }
        if let param = AXValueCreate(.cfRange, &range) {
            var out: CFTypeRef?
            if AXUIElementCopyParameterizedAttributeValue(
                el, kAXStringForRangeParameterizedAttribute as CFString, param, &out) == .success,
               let s = out as? String {
                return s
            }
        }
        guard let value = string(el, kAXValueAttribute) else { return nil }
        let ns = value as NSString
        guard caret <= ns.length else { return String(value.suffix(maxLength)) }
        return ns.substring(with: NSRange(location: from, length: caret - from))
    }

    private static func string(_ el: AXUIElement, _ attr: String) -> String? {
        var v: CFTypeRef?
        guard AXUIElementCopyAttributeValue(el, attr as CFString, &v) == .success else { return nil }
        return v as? String
    }

    private static func selectionStart(_ el: AXUIElement) -> Int? {
        var v: CFTypeRef?
        guard AXUIElementCopyAttributeValue(el, kAXSelectedTextRangeAttribute as CFString, &v) == .success, let v,
              CFGetTypeID(v) == AXValueGetTypeID() else { return nil }
        var r = CFRange()
        return AXValueGetValue(v as! AXValue, .cfRange, &r) ? r.location : nil
    }
}
