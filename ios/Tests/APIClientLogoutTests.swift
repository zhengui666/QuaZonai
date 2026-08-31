import Foundation
import XCTest
@testable import QuaZonai

private final class LogoutStubURLProtocol: URLProtocol {
    nonisolated(unsafe) static var handler: ((URLRequest) throws -> (HTTPURLResponse, Data))?

    override class func canInit(with request: URLRequest) -> Bool { true }

    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        guard let handler = Self.handler else {
            client?.urlProtocol(self, didFailWithError: URLError(.badServerResponse))
            return
        }
        do {
            let (response, data) = try handler(request)
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data)
            client?.urlProtocolDidFinishLoading(self)
        } catch {
            client?.urlProtocol(self, didFailWithError: error)
        }
    }

    override func stopLoading() {}
}

final class APIClientLogoutTests: XCTestCase {
    func testLogoutRetainsRotatedCredentialWhenServerRevokeFails() async throws {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [LogoutStubURLProtocol.self]
        let session = URLSession(configuration: configuration)
        defer {
            LogoutStubURLProtocol.handler = nil
            session.invalidateAndCancel()
        }

        LogoutStubURLProtocol.handler = { request in
            guard let url = request.url else { throw URLError(.badURL) }
            let authorization = request.value(forHTTPHeaderField: "Authorization")
            let response: HTTPURLResponse
            let data: Data

            switch url.path {
            case "/api/v1/auth/mobile/logout" where authorization == nil:
                response = HTTPURLResponse(url: url, statusCode: 401, httpVersion: nil, headerFields: nil)!
                data = Data("{}".utf8)
            case "/api/v1/auth/mobile/refresh":
                guard authorization == "Bearer refresh-1" else {
                    throw URLError(.userAuthenticationRequired)
                }
                response = HTTPURLResponse(
                    url: url,
                    statusCode: 200,
                    httpVersion: nil,
                    headerFields: ["Content-Type": "application/json"]
                )!
                data = Data(
                    """
                    {
                      "authenticated": true,
                      "auth_enabled": true,
                      "operator_subject": "local-operator",
                      "access_token": "access-2",
                      "access_expires_in": 300,
                      "refresh_credential": "refresh-2",
                      "refresh_expires_at": "2026-09-30T00:00:00Z"
                    }
                    """.utf8
                )
            case "/api/v1/auth/mobile/logout":
                guard authorization == "Bearer access-2" else {
                    throw URLError(.userAuthenticationRequired)
                }
                response = HTTPURLResponse(
                    url: url,
                    statusCode: 503,
                    httpVersion: nil,
                    headerFields: ["Content-Type": "application/json"]
                )!
                data = Data(
                    """
                    {"error":{"code":"LOGOUT_RETRY","message":"Retry logout.","details":{}}}
                    """.utf8
                )
            default:
                throw URLError(.unsupportedURL)
            }
            return (response, data)
        }

        let client = APIClient(
            baseURL: try XCTUnwrap(URL(string: "https://example.com")),
            session: session,
            appVersion: "1.0.0"
        )
        await client.configureTrustedCredential("refresh-1")

        do {
            try await client.logout()
            XCTFail("Logout should preserve credentials and surface a retryable server failure.")
        } catch let error as APIClientError {
            XCTAssertEqual(
                error,
                .http(status: 503, code: "LOGOUT_RETRY", message: "Retry logout.")
            )
        }

        let pendingRefresh = await client.pendingRefreshCredentialForPersistence()
        XCTAssertEqual(pendingRefresh, "refresh-2")
        let authorized = try await client.authorizedRequest(path: "/api/v1/system/health")
        XCTAssertEqual(
            authorized.value(forHTTPHeaderField: "Authorization"),
            "Bearer access-2"
        )
    }
}
