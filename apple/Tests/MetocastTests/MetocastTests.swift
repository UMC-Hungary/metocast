import Metocast
import XCTest

final class MetocastTests: XCTestCase {
    func testBridgeLoadsAndCreatesClient() throws {
        XCTAssertFalse(bridgeVersion().isEmpty)
        _ = try AppleClient(baseUrl: "http://127.0.0.1:3737", authToken: "test-token")
    }

    func testErrorsStayTyped() {
        XCTAssertThrowsError(try AppleClient(baseUrl: "not a URL", authToken: "test-token")) {
            XCTAssertEqual($0 as? AppleError, .InvalidConfiguration)
        }
        XCTAssertEqual(AppleError.Authentication, .Authentication)
    }

    func testTitleTemplateMatchesTheWebRenderer() throws {
        let date = try XCTUnwrap(DateComponents(calendar: .current, year: 2026, month: 9, day: 20, hour: 10).date)
        let template = "{date|YYYY.MM.DD.} {title}[ | Textus: {textus}][ Lekció: {leckio}][ | {speaker}]"
        let values = TitleValues(date: date, title: " Istentisztelet ", textus: "Jn 3,16", speaker: "Áron")
        // The empty leckió takes its label with it.
        XCTAssertEqual(renderTitle(template, values), "2026.09.20. Istentisztelet | Textus: Jn 3,16 | Áron")
        XCTAssertEqual(renderTitle("{date|D.M.YY HH:mm} {title}", values), "20.9.26 10:00 Istentisztelet")
    }

    func testEventsStayTyped() {
        let event = AppleEvent.connected(serverId: "server-1")
        guard case let .connected(serverId) = event else {
            return XCTFail("expected connected event")
        }
        XCTAssertEqual(serverId, "server-1")
    }
}
