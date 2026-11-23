import AppKit
import CoreGraphics
import PusmogoKeys

/// Global keyboard handling with a CGEventTap, so Pusmogo can *consume* its own
/// shortcuts instead of also sending them to the app. Needs Accessibility
/// permission; retries until it's granted.
///
/// The gestures themselves live in `KeyMachine` (module PusmogoKeys, unit
/// tested); this class only turns CGEvents into its inputs. Defaults:
///   hold fn            push-to-talk (release = stop and paste)
///   double-tap fn      hands-free; tap fn again = stop and paste
///   triple-tap fn      cancel (dismiss) hands-free
///   fn + Space         hands-free while holding
///   fn + ⇧             swap Hungarian ⇄ English (also mid-dictation)
///   Esc                cancel while recording
///   ⌃⌘V / ⌃⌘C         paste / copy the last text
///   ⌃⌥P               open the panel in a window
///   Right ⌥ / ⌘ / ⌃    optional second push-to-talk key, for keyboards without
///                      fn (setting); a short tap on it does nothing
final class KeyboardTap {
    typealias AltKey = PusmogoKeys.AltKey
    typealias Intent = KeyIntent

    var altKey: AltKey = .none
    /// Physical fn state, for the onboarding keycap.
    var onFnChange: ((Bool) -> Void)?

    private let onIntent: (Intent) -> Void
    private var machine = KeyMachine()
    private var tap: CFMachPort?
    private var retry: Timer?

    var isInstalled: Bool { tap != nil }

    init(onIntent: @escaping (Intent) -> Void) {
        self.onIntent = onIntent
        install()
    }

    /// Called by the app when a recording ends for another reason (error, button).
    func reset() { machine.reset() }

    private func install() {
        guard tap == nil else { return }
        let mask = CGEventMask(1 << CGEventType.keyDown.rawValue) | CGEventMask(1 << CGEventType.flagsChanged.rawValue)
        let ref = Unmanaged.passUnretained(self).toOpaque()
        let created = CGEvent.tapCreate(
            tap: .cgSessionEventTap,
            place: .headInsertEventTap,
            options: .defaultTap,
            eventsOfInterest: mask,
            callback: { _, type, event, ref in
                guard let ref else { return Unmanaged.passUnretained(event) }
                return Unmanaged<KeyboardTap>.fromOpaque(ref).takeUnretainedValue().handle(type, event)
            },
            userInfo: ref
        )
        guard let created else {
            if retry == nil {
                retry = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in self?.install() }
            }
            return
        }
        tap = created
        retry?.invalidate()
        retry = nil
        let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, created, 0)
        CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes)
        CGEvent.tapEnable(tap: created, enable: true)
    }

    private func handle(_ type: CGEventType, _ event: CGEvent) -> Unmanaged<CGEvent>? {
        let pass = Unmanaged.passUnretained(event)
        switch type {
        case .tapDisabledByTimeout, .tapDisabledByUserInput:
            if let tap { CGEvent.tapEnable(tap: tap, enable: true) }
            return pass
        case .flagsChanged:
            flagsChanged(event)
            return pass
        case .keyDown:
            return keyDown(event) ? nil : pass
        default:
            return pass
        }
    }

    private var now: TimeInterval { ProcessInfo.processInfo.systemUptime }

    private func flagsChanged(_ event: CGEvent) {
        let code = event.getIntegerValueField(.keyboardEventKeycode)
        let t = now
        switch code {
        case KeyMachine.kFn:
            let wasDown = machine.fnDown
            let intent = machine.fn(down: event.flags.contains(.maskSecondaryFn), at: t)
            if machine.fnDown != wasDown { onFnChange?(machine.fnDown) }
            emit(intent)
        case altKey.keycode where altKey != .none:
            let down = event.flags.rawValue & altKey.deviceMask != 0
            if PusmogoDebug.keys { NSLog("keys: alt \(altKey.rawValue) \(down ? "down" : "up")") }
            emit(machine.alt(down: down, at: t))
        case KeyMachine.kLeftShift, KeyMachine.kRightShift:
            emit(machine.shift(down: event.flags.contains(.maskShift)))
        default:
            break
        }
    }

    /// Returns true when the event is ours and must not reach the app.
    private func keyDown(_ event: CGEvent) -> Bool {
        let flags = event.flags
        var mods: Modifiers = []
        if flags.contains(.maskControl) { mods.insert(.control) }
        if flags.contains(.maskCommand) { mods.insert(.command) }
        if flags.contains(.maskShift) { mods.insert(.shift) }
        if flags.contains(.maskAlternate) { mods.insert(.option) }
        let result = machine.keyDown(event.getIntegerValueField(.keyboardEventKeycode), mods, at: now)
        emit(result.intent)
        return result.consume
    }

    private func emit(_ intent: Intent?) {
        if let intent { onIntent(intent) }
    }
}

/// `PUSMOGO_DEBUG_KEYS=1` logs the second push-to-talk key's presses.
enum PusmogoDebug {
    static let keys = ProcessInfo.processInfo.environment["PUSMOGO_DEBUG_KEYS"] == "1"
}
