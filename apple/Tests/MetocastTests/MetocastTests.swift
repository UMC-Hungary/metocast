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

    func testEventsStayTyped() {
        let event = AppleEvent.connected(serverId: "server-1")
        guard case let .connected(serverId) = event else {
            return XCTFail("expected connected event")
        }
        XCTAssertEqual(serverId, "server-1")
    }
}
