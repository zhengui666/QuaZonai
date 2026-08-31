import Foundation
import XCTest
@testable import QuaZonai

final class EventStreamTests: XCTestCase {
    func testReplay() async throws {
        let client = APIClient(
            baseURL: try XCTUnwrap(URL(string: "https://example.com")),
            appVersion: "1.0.0"
        )
        let request = try await client.authorizedRequest(
            path: "/api/v1/events?after_id=42&limit=1000"
        )
        let components = try XCTUnwrap(
            URLComponents(url: try XCTUnwrap(request.url), resolvingAgainstBaseURL: false)
        )
        let query = Dictionary(
            uniqueKeysWithValues: (components.queryItems ?? []).map { ($0.name, $0.value ?? "") }
        )
        XCTAssertEqual(components.path, "/api/v1/events")
        XCTAssertEqual(query["after_id"], "42")
        XCTAssertEqual(query["limit"], "1000")
    }

    func testReconnect() async throws {
        let client = APIClient(
            baseURL: try XCTUnwrap(URL(string: "https://example.com")),
            appVersion: "1.0.0"
        )
        let request = try await client.authorizedRequest(
            path: "/api/v1/events/stream",
            queryItems: [URLQueryItem(name: "cursor", value: "77")]
        )
        let components = try XCTUnwrap(
            URLComponents(url: try XCTUnwrap(request.url), resolvingAgainstBaseURL: false)
        )
        XCTAssertEqual(components.path, "/api/v1/events/stream")
        XCTAssertEqual(components.queryItems?.first?.name, "cursor")
        XCTAssertEqual(components.queryItems?.first?.value, "77")
    }
}
