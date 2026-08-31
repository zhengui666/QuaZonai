import Foundation
import QuaZonaiAPI
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

public enum HTTPMethod: String, Sendable {
    case get = "GET"
    case post = "POST"
    case put = "PUT"
    case patch = "PATCH"
    case delete = "DELETE"
}

public struct ClientBootstrap: Codable, Sendable, Equatable {
    public let serverVersion: String
    public let authEnabled: Bool
    public let operatorClientCapabilityEpoch: Int
    public let minimumIOSCapabilityEpoch: Int
    public let minimumIOSAppVersion: String

    enum CodingKeys: String, CodingKey {
        case serverVersion = "server_version"
        case authEnabled = "auth_enabled"
        case operatorClientCapabilityEpoch = "operator_client_capability_epoch"
        case minimumIOSCapabilityEpoch = "minimum_ios_capability_epoch"
        case minimumIOSAppVersion = "minimum_ios_app_version"
    }
}

public struct MobileLoginRequest: Codable, Sendable, Equatable {
    public let totpCode: String
    public let installationID: UUID
    public let deviceName: String
    public let deviceFamily: String
    public let osVersion: String
    public let appVersion: String
    public let appBuild: String
    public let trustDevice: Bool

    enum CodingKeys: String, CodingKey {
        case totpCode = "totp_code"
        case installationID = "installation_id"
        case deviceName = "device_name"
        case deviceFamily = "device_family"
        case osVersion = "os_version"
        case appVersion = "app_version"
        case appBuild = "app_build"
        case trustDevice = "trust_device"
    }
}

public struct MobileDeviceView: Codable, Sendable, Equatable {
    public let id: UUID
    public let installationID: UUID
    public let displayName: String
    public let deviceFamily: String
    public let credentialGeneration: Int
    public let createdAt: String
    public let lastSeenAt: String?
    public let refreshExpiresAt: String?
    public let revokedAt: String?
    public let clientVersion: String
    public let appBuild: String
    public let osVersion: String

    enum CodingKeys: String, CodingKey {
        case id
        case installationID = "installation_id"
        case displayName = "display_name"
        case deviceFamily = "device_family"
        case credentialGeneration = "credential_generation"
        case createdAt = "created_at"
        case lastSeenAt = "last_seen_at"
        case refreshExpiresAt = "refresh_expires_at"
        case revokedAt = "revoked_at"
        case clientVersion = "client_version"
        case appBuild = "app_build"
        case osVersion = "os_version"
    }
}

public struct MobileTokenResponse: Codable, Sendable, Equatable {
    public let authenticated: Bool
    public let authEnabled: Bool
    public let operatorSubject: String
    public let device: MobileDeviceView?
    public let accessToken: String?
    public let accessExpiresIn: Int
    public let refreshCredential: String?
    public let refreshExpiresAt: String?

    enum CodingKeys: String, CodingKey {
        case authenticated
        case authEnabled = "auth_enabled"
        case operatorSubject = "operator_subject"
        case device
        case accessToken = "access_token"
        case accessExpiresIn = "access_expires_in"
        case refreshCredential = "refresh_credential"
        case refreshExpiresAt = "refresh_expires_at"
    }
}

public enum APIClientError: Error, LocalizedError, Sendable, Equatable {
    case invalidServerURL
    case insecureServerURL
    case incompatibleClient(requiredEpoch: Int)
    case incompatibleAppVersion(requiredVersion: String)
    case authenticationRequired
    case http(status: Int, code: String?, message: String)
    case invalidResponse

    public var errorDescription: String? {
        switch self {
        case .invalidServerURL: return "The server URL is invalid."
        case .insecureServerURL: return "Use HTTPS. HTTP is accepted only for localhost development."
        case let .incompatibleClient(requiredEpoch): return "This server requires client capability epoch \(requiredEpoch)."
        case let .incompatibleAppVersion(requiredVersion): return "This server requires QuaZonai \(requiredVersion) or later."
        case .authenticationRequired: return "Operator authentication is required."
        case let .http(_, _, message): return message
        case .invalidResponse: return "The server returned an invalid response."
        }
    }
}

public func isAppVersion(_ current: String, atLeast minimum: String) -> Bool {
    func numericComponents(_ raw: String) -> [Int]? {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        let release = trimmed.split(
            separator: "-",
            maxSplits: 1,
            omittingEmptySubsequences: false
        )[0]
        let core = release.split(
            separator: "+",
            maxSplits: 1,
            omittingEmptySubsequences: false
        )[0]
        let components = core.split(separator: ".", omittingEmptySubsequences: false)
        guard !components.isEmpty,
              components.allSatisfy({ !$0.isEmpty && $0.allSatisfy({ $0.isNumber }) })
        else { return nil }
        return components.compactMap { Int($0) }
    }

    guard var installed = numericComponents(current),
          var required = numericComponents(minimum)
    else { return false }
    let width = max(installed.count, required.count)
    installed.append(contentsOf: repeatElement(0, count: width - installed.count))
    required.append(contentsOf: repeatElement(0, count: width - required.count))
    for (left, right) in zip(installed, required) {
        if left != right { return left > right }
    }
    return true
}

public func normalizeServerURL(_ rawValue: String) throws -> URL {
    let raw = rawValue.trimmingCharacters(in: .whitespacesAndNewlines)
    guard var components = URLComponents(string: raw),
          let scheme = components.scheme?.lowercased(),
          let host = components.host,
          components.user == nil,
          components.password == nil,
          components.query == nil,
          components.fragment == nil,
          components.path.isEmpty || components.path == "/"
    else { throw APIClientError.invalidServerURL }

    let localHosts = Set(["localhost", "127.0.0.1", "::1"])
    if scheme != "https" && !(scheme == "http" && localHosts.contains(host.lowercased())) {
        throw APIClientError.insecureServerURL
    }
    guard scheme == "https" || scheme == "http" else { throw APIClientError.invalidServerURL }
    components.scheme = scheme
    components.path = ""
    guard let url = components.url else { throw APIClientError.invalidServerURL }
    return url
}

public actor APIClient {
    public static let capabilityEpoch = 1

    private struct RefreshFlight {
        let id: UUID
        let task: Task<MobileTokenResponse, Error>
    }

    private let baseURL: URL
    private let session: URLSession
    private let generatedClient: QuaZonaiAPI.Client
    private let currentAppVersion: String
    private var accessToken: String?
    private var refreshCredential: String?
    private var refreshCredentialNeedsPersistence = false
    private var refreshFlight: RefreshFlight?

    public init(baseURL: URL, session: URLSession = .shared, appVersion: String? = nil) {
        self.baseURL = baseURL
        self.session = session
        self.generatedClient = makeGeneratedClient(serverURL: baseURL)
        self.currentAppVersion = appVersion
            ?? (Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String)
            ?? "0"
    }

    public func bootstrap() async throws -> ClientBootstrap {
        let (data, response) = try await perform(path: "/api/v1/client/bootstrap", method: .get)
        try validate(response: response, data: data)
        let value = try JSONDecoder().decode(ClientBootstrap.self, from: data)
        guard value.minimumIOSCapabilityEpoch <= Self.capabilityEpoch else {
            throw APIClientError.incompatibleClient(requiredEpoch: value.minimumIOSCapabilityEpoch)
        }
        guard isAppVersion(currentAppVersion, atLeast: value.minimumIOSAppVersion) else {
            throw APIClientError.incompatibleAppVersion(requiredVersion: value.minimumIOSAppVersion)
        }
        return value
    }

    public func configureTrustedCredential(_ credential: String) {
        cancelRefreshFlight()
        refreshCredential = credential
        refreshCredentialNeedsPersistence = false
        accessToken = nil
    }

    public func login(_ payload: MobileLoginRequest) async throws -> MobileTokenResponse {
        cancelRefreshFlight()
        let data = try JSONEncoder().encode(payload)
        let (body, response) = try await perform(
            path: "/api/v1/auth/mobile/login",
            method: .post,
            body: data,
            authorization: nil
        )
        try validate(response: response, data: body)
        let tokens = try JSONDecoder().decode(MobileTokenResponse.self, from: body)
        accessToken = tokens.accessToken
        refreshCredential = tokens.refreshCredential
        refreshCredentialNeedsPersistence = false
        return tokens
    }

    @discardableResult
    public func refreshIfPossible() async throws -> MobileTokenResponse {
        if let flight = refreshFlight {
            return try await flight.task.value
        }
        guard let credential = refreshCredential else {
            throw APIClientError.authenticationRequired
        }

        let id = UUID()
        let task = Task { [self] in
            try await performRefresh(using: credential)
        }
        refreshFlight = RefreshFlight(id: id, task: task)
        do {
            let tokens = try await task.value
            clearRefreshFlight(id: id)
            return tokens
        } catch {
            clearRefreshFlight(id: id)
            throw error
        }
    }

    public func pendingRefreshCredentialForPersistence() -> String? {
        refreshCredentialNeedsPersistence ? refreshCredential : nil
    }

    public func markRefreshCredentialPersisted(_ credential: String) {
        if refreshCredential == credential {
            refreshCredentialNeedsPersistence = false
        }
    }

    public func logout() async {
        if accessToken != nil || refreshCredential != nil {
            _ = try? await requestJSON(
                path: "/api/v1/auth/mobile/logout",
                method: .post,
                allowRefresh: true
            )
        }
        cancelRefreshFlight()
        accessToken = nil
        refreshCredential = nil
        refreshCredentialNeedsPersistence = false
    }

    public func clearCredentials() {
        cancelRefreshFlight()
        accessToken = nil
        refreshCredential = nil
        refreshCredentialNeedsPersistence = false
    }

    public func requestJSON(
        path: String,
        method: HTTPMethod = .get,
        body: JSONValue? = nil,
        idempotencyKey: String? = nil,
        allowRefresh: Bool = true
    ) async throws -> JSONValue {
        let encoded = try body?.encodedData()
        let stableKey: String? = method == .get ? nil : (idempotencyKey ?? UUID().uuidString)
        var (data, response) = try await perform(
            path: path,
            method: method,
            body: encoded,
            authorization: accessToken,
            idempotencyKey: stableKey
        )
        if response.statusCode == 401 && allowRefresh && refreshCredential != nil {
            _ = try await refreshIfPossible()
            (data, response) = try await perform(
                path: path,
                method: method,
                body: encoded,
                authorization: accessToken,
                idempotencyKey: stableKey
            )
        }
        try validate(response: response, data: data)
        if response.statusCode == 204 || data.isEmpty { return .null }
        return try JSONDecoder().decode(JSONValue.self, from: data)
    }

    public func authorizedRequest(path: String, queryItems: [URLQueryItem] = []) throws -> URLRequest {
        var request = URLRequest(url: try endpoint(path: path, queryItems: queryItems))
        request.httpMethod = HTTPMethod.get.rawValue
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        if let accessToken { request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization") }
        return request
    }

    public func generatedContractIsLoaded() -> Bool {
        _ = generatedClient
        return true
    }

    private func performRefresh(using credential: String) async throws -> MobileTokenResponse {
        let (data, response) = try await perform(
            path: "/api/v1/auth/mobile/refresh",
            method: .post,
            authorization: credential
        )
        try validate(response: response, data: data)
        let tokens = try JSONDecoder().decode(MobileTokenResponse.self, from: data)
        accessToken = tokens.accessToken
        refreshCredential = tokens.refreshCredential
        refreshCredentialNeedsPersistence = tokens.refreshCredential != nil
        return tokens
    }

    private func clearRefreshFlight(id: UUID) {
        if refreshFlight?.id == id {
            refreshFlight = nil
        }
    }

    private func cancelRefreshFlight() {
        refreshFlight?.task.cancel()
        refreshFlight = nil
    }

    private func endpoint(path: String, queryItems: [URLQueryItem] = []) throws -> URL {
        guard let relative = URLComponents(string: path),
              relative.scheme == nil,
              relative.host == nil,
              relative.user == nil,
              relative.password == nil,
              relative.fragment == nil,
              relative.path.hasPrefix("/api/v1/")
        else { throw APIClientError.invalidServerURL }

        var components = URLComponents(url: baseURL, resolvingAgainstBaseURL: false)
        components?.path = relative.path
        components?.queryItems = queryItems.isEmpty ? relative.queryItems : queryItems
        guard let url = components?.url else { throw APIClientError.invalidServerURL }
        return url
    }

    private func perform(
        path: String,
        method: HTTPMethod,
        body: Data? = nil,
        authorization: String? = nil,
        idempotencyKey: String? = nil
    ) async throws -> (Data, HTTPURLResponse) {
        var request = URLRequest(url: try endpoint(path: path))
        request.httpMethod = method.rawValue
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.timeoutInterval = 45
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        if body != nil { request.setValue("application/json", forHTTPHeaderField: "Content-Type") }
        if let authorization { request.setValue("Bearer \(authorization)", forHTTPHeaderField: "Authorization") }
        if let idempotencyKey { request.setValue(idempotencyKey, forHTTPHeaderField: "Idempotency-Key") }
        request.httpBody = body
        let (data, rawResponse) = try await session.data(for: request)
        guard let response = rawResponse as? HTTPURLResponse else { throw APIClientError.invalidResponse }
        return (data, response)
    }

    private func validate(response: HTTPURLResponse, data: Data) throws {
        guard (200..<300).contains(response.statusCode) else {
            if response.statusCode == 401 { throw APIClientError.authenticationRequired }
            let envelope = try? JSONDecoder().decode(JSONValue.self, from: data)
            let error = envelope?["error"]?.objectValue
            let message = error?.string("message") ?? HTTPURLResponse.localizedString(forStatusCode: response.statusCode)
            throw APIClientError.http(status: response.statusCode, code: error?.string("code"), message: message)
        }
    }
}
