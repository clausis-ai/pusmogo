import XCTest
@testable import PusmogoKeys

final class LogTrimTests: XCTestCase {
    private func lines(_ n: Int, width: Int = 9) -> Data {
        Data((1...n).map { i -> String in
            let digits = String(i)
            return String(repeating: "0", count: max(0, width - digits.count)) + digits + "\n"
        }.joined().utf8)
    }

    func testSmallLogIsLeftAlone() {
        XCTAssertNil(LogTrim.trimmed(lines(10), maxBytes: 1000, keepLines: 3))
    }

    func testBigLogKeepsTheNewestLines() {
        let out = LogTrim.trimmed(lines(200), maxBytes: 1000, keepLines: 3)
        XCTAssertEqual(out.map { String(decoding: $0, as: UTF8.self) }, "000000198\n000000199\n000000200\n")
    }

    func testTornLastLineIsKept() {
        var data = lines(200)
        data.append(contentsOf: Array("{\"half".utf8))
        let out = LogTrim.trimmed(data, maxBytes: 1000, keepLines: 2)
        XCTAssertEqual(out.map { String(decoding: $0, as: UTF8.self) }, "000000200\n{\"half")
    }

    func testFewHugeLinesKeepTheLastOne() {
        let out = LogTrim.trimmed(lines(3, width: 600), maxBytes: 1000, keepLines: 10)
        XCTAssertEqual(out?.count, 601)
        XCTAssertEqual(out?.last, UInt8(ascii: "\n"))
    }

    func testDefaultCapKeepsAThousandLines() {
        let out = LogTrim.trimmed(lines(3000, width: 99))
        XCTAssertEqual(out?.count, 1000 * 100)
    }
}
