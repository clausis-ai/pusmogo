import ServiceManagement

/// Launch at login through `SMAppService.mainApp` (macOS 13+). Only the user's
/// toggle in Settings registers or unregisters; everything else just reads.
enum LoginItem {
    enum State: Equatable { case on, off, needsApproval, unavailable }

    static var state: State {
        switch SMAppService.mainApp.status {
        case .enabled: return .on
        case .requiresApproval: return .needsApproval
        case .notRegistered: return .off
        case .notFound: return .off
        @unknown default: return .unavailable
        }
    }

    /// Returns the new state, or the error text macOS gave.
    @discardableResult
    static func set(_ on: Bool) -> Result<State, Error> {
        do {
            if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
            return .success(state)
        } catch {
            NSLog("login item: \(on ? "register" : "unregister") failed: \(error.localizedDescription)")
            return .failure(error)
        }
    }

    static func openSettings() { SMAppService.openSystemSettingsLoginItems() }
}
