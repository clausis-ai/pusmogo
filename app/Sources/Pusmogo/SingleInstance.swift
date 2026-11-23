import AppKit

/// Keeps one copy of Pusmogo running. Two copies (an installed app and a copy
/// left in Downloads, or a dev build) would each install a key tap and type
/// every dictation twice. A copy that finds an older one running asks it to
/// open its panel, so the launch still visibly does something, and quits.
enum SingleInstance {
    static let showPanelNote = Notification.Name("ai.vatson.pusmogo.showPanel")

    /// One-shot test hooks quit by themselves and never install a key tap
    /// that matters, so they may run next to a real copy.
    private static let oneShotHooks = ["PUSMOGO_SELFTEST_WAV", "PUSMOGO_INSERT_TEST", "PUSMOGO_MIC_TEST", "PUSMOGO_PREVIEW_HOVER", "PUSMOGO_TESTSET",
                                       "PUSMOGO_APPLICATIONS_DIR"]

    /// Call first thing at launch. Quits this process if an older copy runs.
    static func quitIfAnotherCopyRuns() {
        let env = ProcessInfo.processInfo.environment
        guard !oneShotHooks.contains(where: { env[$0] != nil }),
              let id = Bundle.main.bundleIdentifier else { return }
        let me = NSRunningApplication.current
        let others = NSRunningApplication.runningApplications(withBundleIdentifier: id)
            .filter { $0.processIdentifier != me.processIdentifier && !$0.isTerminated }
        // The older copy stays; two copies started at the same moment keep the lower pid.
        guard let older = others.first(where: { isOlder($0, than: me) }) else { return }
        NSLog("single instance: another copy runs (pid \(older.processIdentifier), \(older.bundleURL?.path ?? "?")), handing over and quitting")
        DistributedNotificationCenter.default().postNotificationName(
            showPanelNote, object: nil, userInfo: nil, deliverImmediately: true)
        exit(0)
    }

    /// In the copy that stays: open the panel when a newer copy hands over.
    @MainActor
    static func observeHandOver(_ handler: @escaping @MainActor () -> Void) {
        DistributedNotificationCenter.default().addObserver(
            forName: showPanelNote, object: nil, queue: .main
        ) { _ in
            NSLog("single instance: a second copy was launched, opening the panel")
            MainActor.assumeIsolated { handler() }
        }
    }

    private static func isOlder(_ a: NSRunningApplication, than b: NSRunningApplication) -> Bool {
        if let da = a.launchDate, let db = b.launchDate, da != db { return da < db }
        return a.processIdentifier < b.processIdentifier
    }
}
