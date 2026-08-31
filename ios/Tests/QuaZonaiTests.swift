import XCTest
@testable import QuaZonai

final class QuaZonaiTests: XCTestCase {
    func testServerURLNormalizationRequiresHTTPSOutsideLocalhost() throws {
        XCTAssertEqual(try normalizeServerURL("https://example.com/").absoluteString, "https://example.com")
        XCTAssertEqual(try normalizeServerURL("http://127.0.0.1:8000").absoluteString, "http://127.0.0.1:8000")
        XCTAssertThrowsError(try normalizeServerURL("http://example.com"))
        XCTAssertThrowsError(try normalizeServerURL("https://user:pass@example.com"))
    }

    func testNativeLoginPayloadHasNoUsernameOrPassword() throws {
        let payload = MobileLoginRequest(
            totpCode: "123456",
            installationID: UUID(uuidString: "11111111-1111-1111-1111-111111111111")!,
            deviceName: "iPhone",
            deviceFamily: "IPHONE",
            osVersion: "26.0",
            appVersion: "1.0.0",
            appBuild: "100",
            trustDevice: true
        )
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(payload)) as? [String: Any])
        XCTAssertEqual(object["totp_code"] as? String, "123456")
        XCTAssertNil(object["username"])
        XCTAssertNil(object["password"])
    }

    func testJSONValueRoundTripPreservesServerFields() throws {
        let source = Data("{\"state\":\"PENDING\",\"expected_revision\":12,\"nested\":{\"risk\":0.25}}".utf8)
        let value = try JSONDecoder().decode(JSONValue.self, from: source)
        XCTAssertEqual(value["state"]?.stringValue, "PENDING")
        XCTAssertEqual(value["nested"]?["risk"]?.doubleValue, 0.25)
        XCTAssertNoThrow(try value.encodedData(pretty: true))
    }

    func testSevenLanguagesHaveCompleteNativeStrings() {
        XCTAssertEqual(AppLanguage.allCases.count, 7)
        XCTAssertTrue(L10n.validatesAllLanguages())
        XCTAssertTrue(AppLanguage.arabic.isRTL)
    }

    func testMinimumAppVersionComparison() {
        XCTAssertTrue(isAppVersion("1.0.0", atLeast: "1.0.0"))
        XCTAssertTrue(isAppVersion("1.2", atLeast: "1.1.9"))
        XCTAssertTrue(isAppVersion("2.0.0", atLeast: "1.99.99"))
        XCTAssertFalse(isAppVersion("0.9.9", atLeast: "1.0.0"))
        XCTAssertFalse(isAppVersion("invalid", atLeast: "1.0.0"))
    }

    func testMutationIdempotencyRegistryRetainsPendingLogicalSubmission() {
        var registry = MutationIdempotencyRegistry()
        let first = registry.key(for: "POST\n/api/v1/example\n{}")
        XCTAssertEqual(first, registry.key(for: "POST\n/api/v1/example\n{}"))
        registry.finish(fingerprint: "POST\n/api/v1/example\n{}", key: first)
        XCTAssertNotEqual(first, registry.key(for: "POST\n/api/v1/example\n{}"))
    }

    func testSwiftOpenAPIGeneratedClientIsLinked() {
        _ = GeneratedQuaZonaiClient.self
    }
}
