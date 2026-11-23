import XCTest
@testable import PusmogoKeys

final class MicPickTests: XCTestCase {
    private let headset = MicOption(id: 1, bluetooth: true)
    private let builtIn = MicOption(id: 2, wiredRank: 0)
    private let usb = MicOption(id: 3, wiredRank: 1)
    private let virtual = MicOption(id: 4)

    func testKeepsANormalDefault() {
        XCTAssertEqual(automaticMic([headset, builtIn, usb], systemDefault: 3, lidClosed: false), 3)
        XCTAssertEqual(automaticMic([builtIn, virtual], systemDefault: 4, lidClosed: false), 4)
    }

    func testReplacesABluetoothDefault() {
        XCTAssertEqual(automaticMic([headset, usb, builtIn], systemDefault: 1, lidClosed: false), 2)
        XCTAssertEqual(automaticMic([headset, usb], systemDefault: 1, lidClosed: false), 3)
    }

    func testSkipsTheBuiltInMicrophoneOfAClosedMacBook() {
        XCTAssertEqual(automaticMic([headset, builtIn, usb], systemDefault: 1, lidClosed: true), 3)
        XCTAssertEqual(automaticMic([headset, builtIn], systemDefault: 1, lidClosed: true), 1)
    }

    func testKeepsTheHeadsetWhenNothingElseIsThere() {
        XCTAssertEqual(automaticMic([headset, virtual], systemDefault: 1, lidClosed: false), 1)
        XCTAssertNil(automaticMic([], systemDefault: nil, lidClosed: false))
    }
}
