import AppKit
import Darwin

/// Offers once, at launch, to move Pusmogo into the Applications folder when it
/// runs from anywhere else (Downloads, a disk image, a build folder). From a
/// stable place it can start at login, and it does not vanish when Downloads is
/// cleaned up. The copy goes to /Applications (or ~/Applications when that is
/// where the user keeps apps and /Applications is not writable), then the new
/// copy is opened once this process has quit. The old copy is removed only
/// when it was in Downloads; a disk image or a build folder is left alone.
///
/// Test hooks (never point them at the real /Applications):
/// - `PUSMOGO_APPLICATIONS_DIR=<dir>`: the only Applications folder, and the
///   target. The run is one-shot: no key tap, no other copy check, it quits
///   after the offer, and the relaunched copy logs where it runs and quits.
///   "Don't ask again" is logged, not saved.
/// - `PUSMOGO_DOWNLOADS_DIR=<dir>`: counts as Downloads (old copy removed).
/// - `PUSMOGO_MOVE_ANSWER=move|later`: answers the offer without showing it;
///   `preview` shows it without taking focus for a screenshot, then quits.
enum AppMover {
    private static let env = ProcessInfo.processInfo.environment
    static let testDir = env["PUSMOGO_APPLICATIONS_DIR"].map { URL(fileURLWithPath: $0, isDirectory: true) }
    private static let declinedKey = "moveToApplicationsDeclined"

    /// Call at launch, after the other-copy check. Returns only when Pusmogo
    /// keeps running from where it is.
    static func offerIfNeeded() {
        let here = Bundle.main.bundleURL.standardizedFileURL
        let original = originalURL(of: here)
        if let testDir, env["PUSMOGO_MOVE_RELAUNCHED"] == "1" {
            NSLog("move: relaunched from \(here.path) (inside \(testDir.path): \(isInside(here, testDir)))")
            exit(0)
        }
        guard here.pathExtension == "app", !applicationsDirs.contains(where: { isInside(original, $0) }) else {
            if testDir != nil {
                NSLog("move: already in an Applications folder, nothing to offer")
                exit(0)
            }
            return
        }
        if testDir == nil && UserDefaults.standard.bool(forKey: declinedKey) { return }
        // A test hook that is not about moving: keep the dev build where it is.
        if testDir == nil && env.keys.contains(where: { $0.hasPrefix("PUSMOGO_") }) { return }

        NSLog("move: running from \(original.path), offering to move it")
        let (move, dontAsk) = ask(from: original.deletingLastPathComponent())
        guard move else {
            if dontAsk {
                if testDir == nil { UserDefaults.standard.set(true, forKey: declinedKey) }
                NSLog("move: declined, not asking again")
            } else {
                NSLog("move: not now")
            }
            if testDir != nil { exit(0) }
            return
        }
        do {
            let target = try install(here)
            if isInside(original, downloadsDir) {
                try? remove(original)
                NSLog("move: removed the old copy from Downloads")
            }
            NSLog("move: copied to \(target.path), relaunching")
            relaunch(target)
            exit(0)
        } catch {
            NSLog("move: failed: \(error.localizedDescription)")
            let alert = NSAlert()
            alert.messageText = String(localized: "Nem sikerült áthelyezni")
            alert.informativeText = String(format: String(localized: "Húzd át kézzel az Alkalmazások mappába. (%@)"), error.localizedDescription)
            if env["PUSMOGO_MOVE_ANSWER"] == nil { alert.runModal() }
            if testDir != nil { exit(1) }
        }
    }

    // MARK: - Where things are

    private static var applicationsDirs: [URL] {
        if let testDir { return [testDir] }
        return [URL(fileURLWithPath: "/Applications", isDirectory: true),
                FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Applications", isDirectory: true)]
    }

    private static var downloadsDir: URL {
        if let dir = env["PUSMOGO_DOWNLOADS_DIR"] { return URL(fileURLWithPath: dir, isDirectory: true) }
        return FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first
            ?? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Downloads", isDirectory: true)
    }

    /// The first Applications folder that can be written to.
    private static var targetDir: URL {
        if let testDir { return testDir }
        let dirs = applicationsDirs
        return dirs.first(where: { FileManager.default.isWritableFile(atPath: $0.path) }) ?? dirs[0]
    }

    private static func isInside(_ url: URL, _ dir: URL) -> Bool {
        let a = url.resolvingSymlinksInPath().standardizedFileURL.pathComponents
        let b = dir.resolvingSymlinksInPath().standardizedFileURL.pathComponents
        return a.count > b.count && Array(a.prefix(b.count)) == b
    }

    /// An app opened from Downloads with the quarantine flag runs from a
    /// random read-only copy (App Translocation). This asks the Security
    /// framework where the real one is; it is looked up at run time.
    private static func originalURL(of url: URL) -> URL {
        guard url.path.contains("/AppTranslocation/"),
              let lib = dlopen("/System/Library/Frameworks/Security.framework/Security", RTLD_LAZY),
              let sym = dlsym(lib, "SecTranslocateCreateOriginalPathForURL") else { return url }
        typealias Fn = @convention(c) (CFURL, UnsafeMutablePointer<Unmanaged<CFError>?>?) -> Unmanaged<CFURL>?
        let fn = unsafeBitCast(sym, to: Fn.self)
        return (fn(url as CFURL, nil)?.takeRetainedValue() as URL?) ?? url
    }

    // MARK: - Asking

    private static func ask(from folder: URL) -> (move: Bool, dontAsk: Bool) {
        let answer = env["PUSMOGO_MOVE_ANSWER"]
        if let answer, answer != "preview" { return (answer == "move", false) }
        let alert = NSAlert()
        alert.messageText = String(localized: "Áthelyezed az Alkalmazások mappába?")
        alert.informativeText = String(
            format: String(localized: "A Pusmogo most innen fut: %@. Az Alkalmazások mappából bejelentkezéskor is elindulhat, és nem tűnik el, ha kiüríted a Letöltéseket."),
            FileManager.default.displayName(atPath: folder.path))
        alert.addButton(withTitle: String(localized: "Áthelyezés"))
        alert.addButton(withTitle: String(localized: "Nem most"))
        alert.showsSuppressionButton = true
        alert.suppressionButton?.title = String(localized: "Ne kérdezd újra")
        if answer == "preview" {
            // Screenshot only: shown without activating, so it never takes the
            // keyboard from the app in front; quits after a few seconds.
            alert.layout()
            alert.window.center()
            alert.window.orderFrontRegardless()
            NSLog("move: previewing the offer (window \(alert.window.windowNumber))")
            RunLoop.main.run(until: Date().addingTimeInterval(Double(env["PUSMOGO_PREVIEW_SECS"] ?? "") ?? 8))
            exit(0)
        }
        NSApplication.shared.activate(ignoringOtherApps: true)
        let move = alert.runModal() == .alertFirstButtonReturn
        return (move, alert.suppressionButton?.state == .on)
    }

    // MARK: - Moving

    /// Copies the bundle into the target folder, replacing an older copy (to
    /// the Trash), and clears the quarantine flag so the copy is not
    /// translocated again: the user already chose to open this app.
    private static func install(_ bundle: URL) throws -> URL {
        let fm = FileManager.default
        let dir = targetDir
        try fm.createDirectory(at: dir, withIntermediateDirectories: true)
        let target = dir.appendingPathComponent(bundle.lastPathComponent)
        let staging = dir.appendingPathComponent(".\(bundle.lastPathComponent).moving")
        try? fm.removeItem(at: staging)
        try fm.copyItem(at: bundle, to: staging)
        if fm.fileExists(atPath: target.path) {
            try remove(target)
        }
        try fm.moveItem(at: staging, to: target)
        clearQuarantine(target)
        return target
    }

    /// To the Trash, so a mistake can be undone; scratch copies in tests are deleted.
    private static func remove(_ url: URL) throws {
        if testDir != nil { try FileManager.default.removeItem(at: url) } else { try FileManager.default.trashItem(at: url, resultingItemURL: nil) }
    }

    private static func clearQuarantine(_ root: URL) {
        let paths = [root.path] + (FileManager.default.enumerator(atPath: root.path)?.compactMap { $0 as? String }
            .map { root.appendingPathComponent($0).path } ?? [])
        for path in paths { removexattr(path, "com.apple.quarantine", XATTR_NOFOLLOW) }
    }

    /// Opens the new copy once this process is gone, so it does not find this
    /// one running and hand over to it.
    private static func relaunch(_ app: URL) {
        let task = Process()
        task.executableURL = URL(fileURLWithPath: "/bin/sh")
        let open = testDir == nil
            ? "/usr/bin/open \"$2\""
            : "PUSMOGO_MOVE_RELAUNCHED=1 \"$2/Contents/MacOS/Pusmogo\""
        task.arguments = ["-c", "while kill -0 \"$1\" 2>/dev/null; do sleep 0.1; done; \(open)", "sh",
                          String(getpid()), app.path]
        do { try task.run() } catch { NSLog("move: relaunch failed: \(error.localizedDescription)") }
    }
}
