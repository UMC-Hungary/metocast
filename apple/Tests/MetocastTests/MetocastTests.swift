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

    private func serviceStart() -> AutomationRecord {
        AutomationRecord(
            id: "",
            name: "Service start",
            enabled: true,
            triggers: [
                .middlecontrolPreset(preset: 3, cameraId: nil),
                .rodecasterChannel(channel: 2, label: "Wireless 1", live: true, source: .either, holdSeconds: 0),
            ],
            actions: [
                .presetRecall(preset: 3, cameraId: nil, waitForArrival: true),
                .recordStart(target: .obs),
                .recordStart(target: .rodecaster),
            ],
            cooldownSeconds: 5,
            requireEvent: false
        )
    }

    func testTheEditorAsksTheCoreWhatIsWrongWithARule() {
        XCTAssertNil(automationProblem(automation: serviceStart()))

        var bad = serviceStart()
        bad.triggers = [.middlecontrolPreset(preset: 13, cameraId: nil)]
        XCTAssertEqual(automationProblem(automation: bad), "preset must be between 1 and 12")

        var nameless = serviceStart()
        nameless.name = "  "
        XCTAssertNotNil(automationProblem(automation: nameless))
    }

    func testTheReverseOfARuleStopsRecordingBeforeItMovesTheCamera() {
        let reversed = reverseAutomation(automation: serviceStart(), cameraPreset: 1)
        XCTAssertEqual(reversed.name, "Service start (reverse)")
        XCTAssertEqual(reversed.id, "")
        XCTAssertEqual(reversed.actions, [
            .recordStop(target: .rodecaster),
            .recordStop(target: .obs),
            .presetRecall(preset: 1, cameraId: nil, waitForArrival: true),
        ])
        // The preset trigger is dropped; the channel flips to going silent for at least 5 s.
        XCTAssertEqual(reversed.triggers, [
            .rodecasterChannel(channel: 2, label: "Wireless 1", live: false, source: .either, holdSeconds: 5),
        ])
        XCTAssertNil(automationProblem(automation: reversed))
    }

    func testTheDeskRecorderAndAutomationEventsStayTyped() {
        let desk = AppleEvent.rodecasterDeskRecorder(state: RodecasterDeskRecorder(status: .recording, mediaPresent: true))
        guard case let .rodecasterDeskRecorder(state) = desk else {
            return XCTFail("expected the desk recorder event")
        }
        XCTAssertEqual(state.status, .recording)
        XCTAssertTrue(state.mediaPresent)

        let changed = AppleEvent.automationsChanged(armed: false, automations: [serviceStart()])
        guard case let .automationsChanged(armed, automations) = changed else {
            return XCTFail("expected automations.changed")
        }
        XCTAssertFalse(armed)
        XCTAssertEqual(automations.map(\.name), ["Service start"])
    }

    func testEventsStayTyped() {
        let event = AppleEvent.connected(serverId: "server-1")
        guard case let .connected(serverId) = event else {
            return XCTFail("expected connected event")
        }
        XCTAssertEqual(serverId, "server-1")
    }
}
