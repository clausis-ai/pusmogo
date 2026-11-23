// Test target for text insertion. Build and run:
//   swiftc -O scripts/insert-probe.swift -o /tmp/insert-probe
//   /tmp/insert-probe native|nomenu|single|web|webline|blank <seconds> [prefill]
// native: text view; nomenu: text view without an Edit menu, so ⌘V does
// nothing; single: one-line NSTextField; web: WKWebView textarea;
// webline: WKWebView <input type=text>; blank: no text field at all.
// prefill (native, nomenu, single): text already in the field, caret at its
// end, e.g. names for the context hint check.
// Activates itself, waits, prints FIELD=[…] with what landed, then exits.
import AppKit
import WebKit
let mode = CommandLine.arguments[1]
let secs = Double(CommandLine.arguments[2]) ?? 6
let prefill = CommandLine.arguments.count > 3 ? CommandLine.arguments[3] : ""
let app = NSApplication.shared
app.setActivationPolicy(.regular)
if mode != "nomenu" {
    let main = NSMenu(); let editItem = NSMenuItem(); main.addItem(editItem)
    let edit = NSMenu(title: "Edit"); edit.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
    editItem.submenu = edit; app.mainMenu = main
}
let win = NSWindow(contentRect: NSRect(x: 300, y: 300, width: 480, height: 200), styleMask: [.titled], backing: .buffered, defer: false)
win.title = "probe-\(mode)"
var tv: NSTextView?
var wv: WKWebView?
var tf: NSTextField?
switch mode {
case "native", "nomenu":
    let t = NSTextView(frame: win.contentView!.bounds); win.contentView!.addSubview(t); tv = t
    t.string = prefill; t.setSelectedRange(NSRange(location: (prefill as NSString).length, length: 0))
case "single":
    let f = NSTextField(frame: NSRect(x: 20, y: 80, width: 440, height: 24)); win.contentView!.addSubview(f); tf = f
    f.stringValue = prefill
case "web", "webline":
    let w = WKWebView(frame: win.contentView!.bounds); win.contentView!.addSubview(w); wv = w
    let field = mode == "web" ? "<textarea id=t style='width:90%;height:150px'></textarea>" : "<input id=t type=text style='width:90%'>"
    w.loadHTMLString(field + "<script>document.getElementById('t').focus()</script>", baseURL: nil)
default: break
}
win.makeKeyAndOrderFront(nil)
if let t = tv { win.makeFirstResponder(t) }
if let f = tf { win.makeFirstResponder(f) }
if let w = wv { DispatchQueue.main.asyncAfter(deadline: .now() + 1) { win.makeFirstResponder(w); w.evaluateJavaScript("document.getElementById('t').focus()") } }
app.activate(ignoringOtherApps: true)
DispatchQueue.main.asyncAfter(deadline: .now() + secs) {
    if let t = tv { print("FIELD=[\(t.string)]"); exit(0) }
    if let f = tf { print("FIELD=[\(f.stringValue)]"); exit(0) }
    if let w = wv {
        w.evaluateJavaScript("document.getElementById('t').value") { v, _ in print("FIELD=[\(v ?? "nil")]"); exit(0) }
        return
    }
    print("FIELD=(none)"); exit(0)
}
app.run()
