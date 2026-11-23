import Foundation

/// What a key gesture asks the app to do.
public enum KeyIntent: Equatable {
    case start, startLocked, stop, lock, cancel, discard, pasteLast, copyLast, toggleLanguage, openPanel
}

/// The optional second push-to-talk key. Raw values are stored in
/// UserDefaults (`altKey`).
public enum AltKey: String, CaseIterable {
    case none, rightOption, rightCommand, rightControl

    public var keycode: Int64 {
        switch self {
        case .none: return -1
        case .rightOption: return 61
        case .rightCommand: return 54
        case .rightControl: return 62
        }
    }

    /// Device-dependent flag bit for the right-hand key only (IOKit
    /// NX_DEVICER*KEYMASK), so the left key never triggers it.
    public var deviceMask: UInt64 {
        switch self {
        case .none: return 0
        case .rightOption: return 0x40
        case .rightCommand: return 0x10
        case .rightControl: return 0x2000
        }
    }
}

/// Modifier keys held during a key press.
public struct Modifiers: OptionSet {
    public let rawValue: UInt8
    public init(rawValue: UInt8) { self.rawValue = rawValue }
    public static let control = Modifiers(rawValue: 1 << 0)
    public static let command = Modifiers(rawValue: 1 << 1)
    public static let shift = Modifiers(rawValue: 1 << 2)
    public static let option = Modifiers(rawValue: 1 << 3)
}

/// The push-to-talk state machine, free of event taps and clocks so it can be
/// unit-tested. `KeyboardTap` feeds it key events with their timestamps
/// (seconds, any monotonic origin) and acts on the returned intents.
///
///   hold fn            push-to-talk (release = stop and paste)
///   double-tap fn      hands-free; tap fn again = stop and paste
///   triple-tap fn      cancel (dismiss) hands-free
///   fn + Space         hands-free while holding
///   fn + ⇧             swap Hungarian ⇄ English (also mid-dictation)
///   Esc                cancel while recording
///   ⌃⌘V / ⌃⌘C         paste / copy the last text
///   ⌃⌥P               open the panel (the menu bar icon can hide behind the notch)
///   second key         push-to-talk only; a short tap on it does nothing
public struct KeyMachine {
    public enum Key: Equatable { case fn, alt }
    public enum Mode: Equatable { case idle, holding(Key), locked }

    public static let kFn: Int64 = 63
    public static let kLeftShift: Int64 = 56
    public static let kRightShift: Int64 = 60
    public static let kSpace: Int64 = 49
    public static let kEscape: Int64 = 53
    public static let kV: Int64 = 9
    public static let kC: Int64 = 8
    public static let kP: Int64 = 35

    /// A press shorter than this is a tap, not dictation.
    public static let tapMax: TimeInterval = 0.30
    /// Second tap must start within this time after the first ends.
    public static let doubleTapGap: TimeInterval = 0.35
    /// A fn + ⇧ press released sooner than this only swapped the language.
    public static let chordOnlyMax: TimeInterval = 1.0

    public private(set) var mode: Mode = .idle
    /// Physical fn state.
    public private(set) var fnDown = false

    private var pressStart: TimeInterval = 0
    private var pressUsedForChord = false
    private var lastTapEnd: TimeInterval = -.infinity
    private var lockedAt: TimeInterval = -.infinity

    public init() {}

    /// The app ended a recording for another reason (error, button).
    public mutating func reset() { mode = .idle }

    /// fn changed state. Repeated reports of the same state are ignored.
    public mutating func fn(down: Bool, at t: TimeInterval) -> KeyIntent? {
        guard down != fnDown else { return nil }
        fnDown = down
        return down ? pressed(.fn, at: t) : released(.fn, at: t)
    }

    /// The second push-to-talk key changed state.
    public mutating func alt(down: Bool, at t: TimeInterval) -> KeyIntent? {
        down ? pressed(.alt, at: t) : released(.alt, at: t)
    }

    /// A shift key changed state; `shiftDown` is whether shift is now held.
    public mutating func shift(down shiftDown: Bool) -> KeyIntent? {
        // fn + ⇧ swaps the language; the fn press then isn't a "tap".
        guard fnDown && shiftDown else { return nil }
        pressUsedForChord = true
        return .toggleLanguage
    }

    /// A non-modifier key went down. `consume` means the event is ours and must
    /// not reach the focused app.
    public mutating func keyDown(_ code: Int64, _ mods: Modifiers, at t: TimeInterval)
        -> (consume: Bool, intent: KeyIntent?)
    {
        if code == Self.kEscape, mode != .idle {
            mode = .idle
            return (true, .cancel)
        }
        if code == Self.kSpace, mode == .holding(.fn) {
            mode = .locked
            lockedAt = t
            return (true, .lock)
        }
        if case .holding = mode {
            // Another key while holding: a shortcut (fn+arrow, ⌥ for special
            // characters), not dictation.
            mode = .idle
            pressUsedForChord = true
            return (false, .cancel)
        }
        let ctrlCmd = mods.contains([.control, .command]) && mods.isDisjoint(with: [.shift, .option])
        if ctrlCmd && code == Self.kV { return (true, .pasteLast) }
        if ctrlCmd && code == Self.kC { return (true, .copyLast) }
        // ⌃⌥Space would be shorter, but macOS uses it to switch input sources.
        if mods == [.control, .option] && code == Self.kP { return (true, .openPanel) }
        return (false, nil)
    }

    private mutating func pressed(_ key: Key, at t: TimeInterval) -> KeyIntent? {
        switch mode {
        case .idle:
            pressStart = t
            pressUsedForChord = false
            if key == .fn && t - lastTapEnd < Self.doubleTapGap {
                // Double tap → hands-free.
                mode = .locked
                lockedAt = t
                lastTapEnd = -.infinity
                return .startLocked
            }
            mode = .holding(key)
            return .start
        case .locked:
            mode = .idle
            // Triple tap = dismiss; any later press stops and pastes.
            return key == .fn && t - lockedAt < Self.doubleTapGap * 1.5 ? .cancel : .stop
        case .holding:
            return nil
        }
    }

    private mutating func released(_ key: Key, at t: TimeInterval) -> KeyIntent? {
        guard case .holding(let k) = mode, k == key else { return nil }
        mode = .idle
        let held = t - pressStart
        if pressUsedForChord && held < Self.chordOnlyMax {
            // Quick fn + ⇧ just to swap the language: nothing was said.
            return .discard
        }
        if held < Self.tapMax && !pressUsedForChord {
            // A tap, not dictation. Only fn has double-tap hands-free; a tap
            // on the second key (often used alone, e.g. ⌘ in a game) is ignored.
            if key == .fn { lastTapEnd = t }
            return .discard
        }
        return .stop
    }
}
