import XCTest
@testable import PusmogoKeys

/// Drives `KeyMachine` with timed key events and checks the intents it emits.
final class KeyMachineTests: XCTestCase {
    private var m = KeyMachine()
    private var t: TimeInterval = 100

    override func setUp() {
        m = KeyMachine()
        t = 100
    }

    private func wait(_ s: TimeInterval) { t += s }
    @discardableResult private func fn(_ down: Bool) -> KeyIntent? { m.fn(down: down, at: t) }
    @discardableResult private func alt(_ down: Bool) -> KeyIntent? { m.alt(down: down, at: t) }
    @discardableResult private func key(_ code: Int64, _ mods: Modifiers = []) -> (consume: Bool, intent: KeyIntent?) {
        m.keyDown(code, mods, at: t)
    }
    /// One quick fn tap: press, hold `hold` seconds, release.
    private func tapFn(hold: TimeInterval = 0.1) -> [KeyIntent?] {
        let a = fn(true); wait(hold); let b = fn(false)
        return [a, b]
    }

    // MARK: hold fn

    func testHoldFnStartsAndReleaseStops() {
        XCTAssertEqual(fn(true), .start)
        XCTAssertEqual(m.mode, .holding(.fn))
        wait(1.2)
        XCTAssertEqual(fn(false), .stop)
        XCTAssertEqual(m.mode, .idle)
    }

    func testShortFnTapDiscards() {
        XCTAssertEqual(tapFn(hold: 0.1), [.start, .discard])
        XCTAssertEqual(m.mode, .idle)
    }

    func testPressJustOverTapLimitIsDictation() {
        fn(true); wait(KeyMachine.tapMax + 0.001)
        XCTAssertEqual(fn(false), .stop)
    }

    func testRepeatedFnStateIsIgnored() {
        XCTAssertEqual(fn(true), .start)
        XCTAssertNil(fn(true))
        XCTAssertTrue(m.fnDown)
        wait(1)
        XCTAssertEqual(fn(false), .stop)
        XCTAssertNil(fn(false))
        XCTAssertFalse(m.fnDown)
    }

    // MARK: double and triple tap

    func testDoubleTapLocksHandsFreeAndNextTapStops() {
        _ = tapFn()
        wait(0.2)
        XCTAssertEqual(fn(true), .startLocked)
        XCTAssertEqual(m.mode, .locked)
        XCTAssertNil(fn(false), "releasing the second tap keeps dictating")
        wait(3)
        XCTAssertEqual(fn(true), .stop)
        XCTAssertEqual(m.mode, .idle)
        XCTAssertNil(fn(false))
    }

    func testSlowSecondTapIsANewHold() {
        _ = tapFn()
        wait(KeyMachine.doubleTapGap + 0.05)
        XCTAssertEqual(fn(true), .start)
        XCTAssertEqual(m.mode, .holding(.fn))
    }

    func testTripleTapCancels() {
        _ = tapFn()
        wait(0.15)
        XCTAssertEqual(fn(true), .startLocked)
        wait(0.1); fn(false); wait(0.15)
        XCTAssertEqual(fn(true), .cancel)
        XCTAssertEqual(m.mode, .idle)
    }

    func testLongDictationTapAfterTripleWindowStops() {
        _ = tapFn()
        wait(0.15)
        fn(true)
        wait(KeyMachine.doubleTapGap * 1.5 + 0.01)
        fn(false)
        XCTAssertEqual(fn(true), .stop)
    }

    func testFourthTapDoesNotLockAgain() {
        // tap, tap (lock), tap (cancel), then a quick tap: must not reopen
        // hands-free, because the lock consumed the first tap.
        _ = tapFn(); wait(0.1)
        fn(true); wait(0.1); fn(false); wait(0.1)
        XCTAssertEqual(fn(true), .cancel)
        wait(0.1); fn(false); wait(0.1)
        XCTAssertEqual(fn(true), .start)
    }

    func testDictationReleaseDoesNotArmDoubleTap() {
        fn(true); wait(2); fn(false)
        wait(0.1)
        XCTAssertEqual(fn(true), .start, "a held dictation is not the first half of a double tap")
    }

    // MARK: fn + Space

    func testFnSpaceLocksAndIsConsumed() {
        fn(true); wait(0.5)
        let r = key(KeyMachine.kSpace)
        XCTAssertTrue(r.consume)
        XCTAssertEqual(r.intent, .lock)
        XCTAssertEqual(m.mode, .locked)
        XCTAssertNil(fn(false))
        wait(4)
        XCTAssertEqual(fn(true), .stop)
    }

    func testSpaceWhileIdlePassesThrough() {
        let r = key(KeyMachine.kSpace)
        XCTAssertFalse(r.consume)
        XCTAssertNil(r.intent)
    }

    // MARK: fn + ⇧

    func testQuickFnShiftOnlySwapsLanguage() {
        fn(true)
        XCTAssertEqual(m.shift(down: true), .toggleLanguage)
        XCTAssertNil(m.shift(down: false))
        wait(0.4)
        XCTAssertEqual(fn(false), .discard)
    }

    func testFnShiftDuringLongDictationKeepsText() {
        fn(true); wait(3)
        XCTAssertEqual(m.shift(down: true), .toggleLanguage)
        wait(2)
        XCTAssertEqual(fn(false), .stop)
    }

    func testFnShiftTapDoesNotArmDoubleTap() {
        fn(true); m.shift(down: true); wait(0.1); fn(false)
        wait(0.1)
        XCTAssertEqual(fn(true), .start)
    }

    func testShiftWithoutFnDoesNothing() {
        XCTAssertNil(m.shift(down: true))
    }

    func testFnShiftWhileLockedSwapsLanguageAndKeepsLock() {
        _ = tapFn(); wait(0.1); fn(true)   // locked, fn still down
        XCTAssertEqual(m.shift(down: true), .toggleLanguage)
        XCTAssertEqual(m.mode, .locked)
    }

    // MARK: Esc and other keys

    func testEscCancelsWhileHoldingAndIsConsumed() {
        fn(true); wait(1)
        let r = key(KeyMachine.kEscape)
        XCTAssertTrue(r.consume)
        XCTAssertEqual(r.intent, .cancel)
        XCTAssertNil(fn(false), "the release after Esc does nothing")
    }

    func testEscCancelsHandsFree() {
        _ = tapFn(); wait(0.1); fn(true); fn(false)
        let r = key(KeyMachine.kEscape)
        XCTAssertEqual(r.intent, .cancel)
        XCTAssertTrue(r.consume)
        XCTAssertEqual(m.mode, .idle)
    }

    func testEscWhileIdlePassesThrough() {
        let r = key(KeyMachine.kEscape)
        XCTAssertFalse(r.consume)
        XCTAssertNil(r.intent)
    }

    func testOtherKeyWhileHoldingCancelsAndPassesThrough() {
        // fn + arrow is a system shortcut, not dictation.
        fn(true); wait(0.05)
        let r = key(123)
        XCTAssertFalse(r.consume)
        XCTAssertEqual(r.intent, .cancel)
        wait(0.05)
        XCTAssertNil(fn(false))
        wait(0.1)
        XCTAssertEqual(fn(true), .start, "a cancelled chord is not a tap")
    }

    func testTypingWhileHandsFreeKeepsDictating() {
        _ = tapFn(); wait(0.1); fn(true); fn(false)
        let r = key(0)
        XCTAssertFalse(r.consume)
        XCTAssertNil(r.intent)
        XCTAssertEqual(m.mode, .locked)
    }

    // MARK: ⌃⌘V / ⌃⌘C

    func testCtrlCmdVPastesLast() {
        let r = key(KeyMachine.kV, [.control, .command])
        XCTAssertTrue(r.consume)
        XCTAssertEqual(r.intent, .pasteLast)
    }

    func testCtrlCmdCCopiesLast() {
        let r = key(KeyMachine.kC, [.control, .command])
        XCTAssertTrue(r.consume)
        XCTAssertEqual(r.intent, .copyLast)
    }

    func testOtherVCombosPassThrough() {
        for mods: Modifiers in [[.command], [.control], [.control, .command, .shift], [.control, .command, .option]] {
            let r = key(KeyMachine.kV, mods)
            XCTAssertFalse(r.consume, "\(mods)")
            XCTAssertNil(r.intent, "\(mods)")
        }
    }

    // MARK: ⌃⌥P

    func testCtrlOptPOpensPanel() {
        let r = key(KeyMachine.kP, [.control, .option])
        XCTAssertTrue(r.consume)
        XCTAssertEqual(r.intent, .openPanel)
    }

    func testOtherPCombosPassThrough() {
        for mods: Modifiers in [[], [.option], [.control], [.control, .option, .shift], [.control, .option, .command]] {
            let r = key(KeyMachine.kP, mods)
            XCTAssertFalse(r.consume, "\(mods)")
            XCTAssertNil(r.intent, "\(mods)")
        }
    }

    func testCtrlOptSpaceIsLeftToTheSystem() {
        let r = key(KeyMachine.kSpace, [.control, .option])
        XCTAssertFalse(r.consume)
        XCTAssertNil(r.intent)
    }

    // MARK: second push-to-talk key

    func testAltHoldDictates() {
        XCTAssertEqual(alt(true), .start)
        XCTAssertEqual(m.mode, .holding(.alt))
        wait(1.5)
        XCTAssertEqual(alt(false), .stop)
    }

    func testAltTapsNeverLockHandsFree() {
        XCTAssertEqual(alt(true), .start); wait(0.1)
        XCTAssertEqual(alt(false), .discard); wait(0.1)
        XCTAssertEqual(alt(true), .start, "double-tapping the second key is not hands-free")
        XCTAssertEqual(m.mode, .holding(.alt))
    }

    func testAltTapThenFnTapDoesNotLock() {
        alt(true); wait(0.1); alt(false); wait(0.1)
        XCTAssertEqual(fn(true), .start)
    }

    func testFnReleaseDoesNotEndAltHold() {
        alt(true); wait(0.5)
        XCTAssertNil(fn(true), "fn while the second key holds is ignored")
        XCTAssertNil(fn(false))
        XCTAssertEqual(m.mode, .holding(.alt))
        wait(1)
        XCTAssertEqual(alt(false), .stop)
    }

    func testAltPressStopsHandsFree() {
        _ = tapFn(); wait(0.1); fn(true); fn(false)
        XCTAssertEqual(alt(true), .stop)
        XCTAssertEqual(m.mode, .idle)
    }

    func testAltSpaceDoesNotLock() {
        alt(true)
        let r = key(KeyMachine.kSpace)
        XCTAssertFalse(r.consume, "Space while holding the second key types a space")
        XCTAssertEqual(r.intent, .cancel)
    }

    // MARK: reset

    func testResetReturnsToIdle() {
        fn(true)
        m.reset()
        XCTAssertEqual(m.mode, .idle)
        wait(1)
        XCTAssertNil(fn(false))
    }
}
