import SwiftData
import XCTest
@testable import QuaZonaiIssue36

final class CoreTests: XCTestCase {
    func testJSONValueRoundTripAndSearchProjection() throws {
        let value: JSONValue = .object([
            "name": .string("Program One"),
            "state": .string("ACTIVE"),
            "metrics": .object(["sharpe": .number(1.25)]),
            "enabled": .bool(true),
        ])
        let decoded = try JSONValue.decode(value.encodedData())
        XCTAssertEqual(decoded, value)
        XCTAssertEqual(decoded.firstString(for: ["state"]), "ACTIVE")
        XCTAssertTrue(decoded.searchableText.contains("Program One"))
        XCTAssertTrue(decoded.searchableText.contains("1.25"))
    }

    func testServerURLNormalizationEnforcesProductionHTTPS() throws {
        XCTAssertEqual(
            try ServerProfile.normalize("quazonai.example.com").absoluteString,
            "https://quazonai.example.com"
        )
        XCTAssertEqual(
            try ServerProfile.normalize("http://127.0.0.1:8000/").absoluteString,
            "http://127.0.0.1:8000"
        )
        XCTAssertEqual(
            try ServerProfile.normalize("http://192.168.1.7:8000/api/").absoluteString,
            "http://192.168.1.7:8000/api"
        )
        XCTAssertThrowsError(try ServerProfile.normalize("http://quazonai.example.com")) { error in
            XCTAssertEqual(error as? APIError, .insecureServer)
        }
        XCTAssertThrowsError(try ServerProfile.normalize("file:///tmp/server"))
        XCTAssertThrowsError(try ServerProfile.normalize("https://user:secret@example.com"))
    }

    func testSSEParserSupportsMultilineDataAndResetsPerEvent() {
        var parser = SSEParser()
        XCTAssertNil(parser.consume(line: "id: 41"))
        XCTAssertNil(parser.consume(line: "event: qz-event"))
        XCTAssertNil(parser.consume(line: "data: {\"kind\":"))
        XCTAssertNil(parser.consume(line: "data: \"PROGRAM_UPDATED\"}"))
        let first = parser.consume(line: "")
        XCTAssertEqual(
            first,
            ServerEvent(
                id: 41,
                event: "qz-event",
                data: "{\"kind\":\n\"PROGRAM_UPDATED\"}"
            )
        )
        XCTAssertNil(parser.consume(line: ": keepalive"))
        XCTAssertNil(parser.consume(line: "id: 42"))
        XCTAssertNil(parser.consume(line: "data: next"))
        XCTAssertEqual(
            parser.consume(line: ""),
            ServerEvent(id: 42, event: "message", data: "next")
        )
    }

    func testBootstrapCompatibilityContract() throws {
        let bootstrap = try ClientBootstrap(json: .object([
            "server_version": .string("1.4.0"),
            "auth_enabled": .bool(true),
            "operator_client_capability_epoch": .number(12),
            "minimum_ios_capability_epoch": .number(1),
            "minimum_ios_app_version": .string("1.0.0"),
        ]))
        XCTAssertTrue(bootstrap.authEnabled)
        XCTAssertEqual(bootstrap.operatorClientCapabilityEpoch, 12)
        XCTAssertEqual(bootstrap.minimumIOSCapabilityEpoch, 1)
        XCTAssertEqual(AppModel.nativeCapabilityEpoch, 1)
    }

    @MainActor
    func testSwiftDataSeparatesCacheDraftAndCursorByServerProfile() throws {
        let configuration = ModelConfiguration(isStoredInMemoryOnly: true)
        let container = try ModelContainer(
            for: CachedResource.self,
            IdeaDraft.self,
            EventCursor.self,
            configurations: configuration
        )
        let context = container.mainContext
        let first = UUID()
        let second = UUID()

        try OfflineCache.store(.object(["state": .string("ACTIVE")]), profileID: first, key: "programs", context: context)
        try OfflineCache.store(.object(["state": .string("PAUSED")]), profileID: second, key: "programs", context: context)
        try OfflineCache.storeDraft("first draft", profileID: first, context: context)
        try OfflineCache.storeDraft("second draft", profileID: second, context: context)
        try OfflineCache.storeCursor(12, profileID: first, context: context)
        try OfflineCache.storeCursor(4, profileID: second, context: context)
        try OfflineCache.storeCursor(9, profileID: first, context: context)

        XCTAssertEqual(
            try OfflineCache.resource(profileID: first, key: "programs", context: context)?["state"]?.stringValue,
            "ACTIVE"
        )
        XCTAssertEqual(
            try OfflineCache.resource(profileID: second, key: "programs", context: context)?["state"]?.stringValue,
            "PAUSED"
        )
        XCTAssertEqual(try OfflineCache.draft(profileID: first, context: context), "first draft")
        XCTAssertEqual(try OfflineCache.draft(profileID: second, context: context), "second draft")
        XCTAssertEqual(try OfflineCache.cursor(profileID: first, context: context), 12)
        XCTAssertEqual(try OfflineCache.cursor(profileID: second, context: context), 4)
    }

    func testNoExecutionControlDestinationsExist() {
        let destinations = Set(AppDestination.allCases.map(\.rawValue))
        XCTAssertFalse(destinations.contains("orders"))
        XCTAssertFalse(destinations.contains("positions"))
        XCTAssertFalse(destinations.contains("execution"))
        XCTAssertFalse(destinations.contains("runtime-stop"))
    }
}
