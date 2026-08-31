import XCTest
@testable import QuaZonai

final class EventStreamTests: XCTestCase {
    func testReplay() throws {
        var parser = SSEFrameParser()
        XCTAssertNil(parser.consume(line: ": keepalive"))
        XCTAssertNil(parser.consume(line: "id: 10"))
        XCTAssertNil(parser.consume(line: "event: qz-event"))
        XCTAssertNil(parser.consume(line: "data: {\"id\":10,"))
        XCTAssertNil(parser.consume(line: "data: \"kind\":\"MISSION_SUCCEEDED\"}"))

        let frame = try XCTUnwrap(parser.consume(line: ""))
        XCTAssertEqual(frame.id, 10)
        XCTAssertEqual(frame.event, "qz-event")
        XCTAssertEqual(frame.data, "{\"id\":10,\n\"kind\":\"MISSION_SUCCEEDED\"}")

        let decoded = try JSONDecoder().decode(JSONValue.self, from: Data(frame.data.utf8))
        XCTAssertEqual(decoded.objectValue?.number("id"), 10)
        XCTAssertEqual(decoded.objectValue?.string("kind"), "MISSION_SUCCEEDED")

        var cursor = EventSequenceCursor(9)
        XCTAssertTrue(cursor.accept(10))
        XCTAssertFalse(cursor.accept(10), "duplicate Last-Event-ID must not be delivered twice")
        XCTAssertFalse(cursor.accept(8), "older replayed events must be ignored")
        XCTAssertTrue(cursor.accept(11))
        XCTAssertEqual(cursor.value, 11)
    }

    func testReconnect() {
        XCTAssertEqual(
            EventStreamBackoff.delaySeconds(attempt: 1, jitterUnit: 0),
            2,
            accuracy: 0.0001
        )
        XCTAssertEqual(
            EventStreamBackoff.delaySeconds(attempt: 2, jitterUnit: 0),
            4,
            accuracy: 0.0001
        )
        XCTAssertEqual(
            EventStreamBackoff.delaySeconds(attempt: 5, jitterUnit: 0),
            30,
            accuracy: 0.0001
        )
        XCTAssertEqual(
            EventStreamBackoff.delaySeconds(attempt: 99, jitterUnit: 1),
            37.5,
            accuracy: 0.0001
        )
        XCTAssertEqual(
            EventStreamBackoff.delaySeconds(attempt: 1, jitterUnit: -1),
            2,
            accuracy: 0.0001
        )
    }
}
