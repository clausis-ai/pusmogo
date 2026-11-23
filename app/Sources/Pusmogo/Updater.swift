import AppKit
import Sparkle

/// App updates through Sparkle: it reads the update list (Info.plist `SUFeedURL`,
/// GitHub Releases), and installs only an update signed with the key whose public
/// half is in Info.plist (`SUPublicEDKey`). No system profile is sent.
///
/// Off in test runs and in builds without a feed. `PUSMOGO_FEED_URL=<url>` (tests)
/// reads another list; the signature check still applies.
@MainActor
final class Updater: NSObject, SPUUpdaterDelegate {
    static let shared = Updater()
    private var controller: SPUStandardUpdaterController?

    static var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"
    }

    /// Any other test hook means no update check.
    private static var isTestRun: Bool {
        ProcessInfo.processInfo.environment.keys.contains { $0.hasPrefix("PUSMOGO_") && $0 != "PUSMOGO_FEED_URL" }
    }

    func start() {
        guard controller == nil, !Self.isTestRun, Bundle.main.bundleIdentifier != nil,
              Bundle.main.object(forInfoDictionaryKey: "SUPublicEDKey") != nil else { return }
        controller = SPUStandardUpdaterController(startingUpdater: true, updaterDelegate: self, userDriverDelegate: nil)
        NSLog("updates: version \(Self.version), feed \(controller?.updater.feedURL?.absoluteString ?? "none")")
    }

    var isOn: Bool { controller != nil }

    var automatic: Bool {
        get { controller?.updater.automaticallyChecksForUpdates ?? false }
        set { controller?.updater.automaticallyChecksForUpdates = newValue }
    }

    func check() {
        // A menu bar app is not in front: without this the update window opens behind.
        if #available(macOS 14, *) { NSApp.activate() } else { NSApp.activate(ignoringOtherApps: true) }
        controller?.checkForUpdates(nil)
    }

    nonisolated func feedURLString(for updater: SPUUpdater) -> String? {
        ProcessInfo.processInfo.environment["PUSMOGO_FEED_URL"]
    }
}
