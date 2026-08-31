import Combine
import Foundation
import LocalAuthentication
import Network
import Security
import SwiftData
import SwiftUI
import UIKit
import QuaZonaiAPI

// MARK: - JSON wire value

public enum JSONValue: Codable, Hashable, Sendable {
    case object([String: JSONValue])
    case array([JSONValue])
    case string(String)
    case number(Double)
    case bool(Bool)
    case null

    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let value = try? container.decode(Bool.self) {
            self = .bool(value)
        } else if let value = try? container.decode(Double.self) {
            self = .number(value)
        } else if let value = try? container.decode(String.self) {
            self = .string(value)
        } else if let value = try? container.decode([String: JSONValue].self) {
            self = .object(value)
        } else if let value = try? container.decode([JSONValue].self) {
            self = .array(value)
        } else {
            throw DecodingError.dataCorruptedError(
                in: container,
                debugDescription: "Unsupported JSON value"
            )
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case let .object(value): try container.encode(value)
        case let .array(value): try container.encode(value)
        case let .string(value): try container.encode(value)
        case let .number(value): try container.encode(value)
        case let .bool(value): try container.encode(value)
        case .null: try container.encodeNil()
        }
    }

    public subscript(key: String) -> JSONValue? {
        guard case let .object(value) = self else { return nil }
        return value[key]
    }

    public var objectValue: [String: JSONValue]? {
        guard case let .object(value) = self else { return nil }
        return value
    }

    public var arrayValue: [JSONValue]? {
        switch self {
        case let .array(value): return value
        case let .object(value): return value["items"]?.arrayValue
        default: return nil
        }
    }

    public var stringValue: String? {
        switch self {
        case let .string(value): value
        case let .number(value): value.formatted(.number.precision(.fractionLength(0...8)))
        case let .bool(value): value ? "true" : "false"
        default: nil
        }
    }

    public var boolValue: Bool? {
        switch self {
        case let .bool(value): value
        case let .string(value): Bool(value)
        default: nil
        }
    }

    public var numberValue: Double? {
        switch self {
        case let .number(value): value
        case let .string(value): Double(value)
        default: nil
        }
    }

    public var displayText: String {
        switch self {
        case let .string(value): value
        case let .number(value): value.formatted(.number.precision(.fractionLength(0...8)))
        case let .bool(value): value ? "true" : "false"
        case .null: "—"
        case let .array(value): "\(value.count) items"
        case let .object(value): "\(value.count) fields"
        }
    }

    public var searchableText: String {
        switch self {
        case let .object(value):
            value.sorted { $0.key < $1.key }
                .map { "\($0.key) \($0.value.searchableText)" }
                .joined(separator: " ")
        case let .array(value): value.map(\.searchableText).joined(separator: " ")
        default: displayText
        }
    }

    public func encodedData() throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(self)
    }

    public static func decode(_ data: Data) throws -> JSONValue {
        try JSONDecoder().decode(JSONValue.self, from: data)
    }

    public func firstString(for keys: [String]) -> String? {
        if case let .object(object) = self {
            for key in keys {
                if let value = object[key]?.stringValue, !value.isEmpty { return value }
            }
            for value in object.values {
                if let match = value.firstString(for: keys) { return match }
            }
        } else if case let .array(array) = self {
            for value in array {
                if let match = value.firstString(for: keys) { return match }
            }
        }
        return nil
    }
}

// MARK: - API transport

public enum HTTPMethod: String, Sendable {
    case get = "GET"
    case post = "POST"
    case put = "PUT"
    case patch = "PATCH"
    case delete = "DELETE"

    var isMutation: Bool { self != .get }
}

public enum APIError: Error, LocalizedError, Sendable, Equatable {
    case invalidServer
    case insecureServer
    case offline
    case unauthorized
    case conflict(code: String, message: String)
    case validation(code: String, message: String)
    case server(status: Int, code: String, message: String)
    case invalidResponse
    case missingCredential
    case capabilityUpgradeRequired

    public var errorDescription: String? {
        switch self {
        case .invalidServer: "The server URL is invalid."
        case .insecureServer: "Production connections require HTTPS."
        case .offline: "The server is unavailable."
        case .unauthorized: "Operator authentication is required."
        case let .conflict(_, message): message
        case let .validation(_, message): message
        case let .server(_, _, message): message
        case .invalidResponse: "The server returned an invalid response."
        case .missingCredential: "No trusted-device credential is available."
        case .capabilityUpgradeRequired: "A newer app version is required by this server."
        }
    }

    public var statusCode: Int? {
        switch self {
        case .unauthorized: 401
        case .conflict: 409
        case .validation: 422
        case let .server(status, _, _): status
        default: nil
        }
    }
}

public struct ServerProfile: Codable, Identifiable, Hashable, Sendable {
    public let id: UUID
    public var name: String
    public var baseURL: URL

    public static func normalize(_ input: String) throws -> URL {
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { throw APIError.invalidServer }
        let candidate = trimmed.contains("://") ? trimmed : "https://\(trimmed)"
        guard var components = URLComponents(string: candidate),
              let scheme = components.scheme?.lowercased(),
              let host = components.host,
              !host.isEmpty,
              components.user == nil,
              components.password == nil,
              components.query == nil,
              components.fragment == nil
        else { throw APIError.invalidServer }

        guard scheme == "https" || scheme == "http" else { throw APIError.invalidServer }
        if scheme == "http" && !isLocalDevelopmentHost(host) {
            throw APIError.insecureServer
        }
        components.scheme = scheme
        components.host = host.lowercased()
        let cleanPath = components.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        components.path = cleanPath.isEmpty ? "" : "/\(cleanPath)"
        guard let result = components.url else { throw APIError.invalidServer }
        return result
    }

    private static func isLocalDevelopmentHost(_ host: String) -> Bool {
        let normalized = host.lowercased()
        if normalized == "localhost" || normalized == "127.0.0.1" || normalized == "::1" {
            return true
        }
        let octets = normalized.split(separator: ".").compactMap { Int($0) }
        guard octets.count == 4, octets.allSatisfy({ 0...255 ~= $0 }) else { return false }
        return octets[0] == 10
            || (octets[0] == 172 && 16...31 ~= octets[1])
            || (octets[0] == 192 && octets[1] == 168)
    }
}

public actor APIClient {
    private var baseURL: URL?
    private var accessToken: String?
    private let session: URLSession
    private let decoder = JSONDecoder()
    private let encoder = JSONEncoder()

    public init() {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.waitsForConnectivity = true
        configuration.timeoutIntervalForRequest = 45
        configuration.timeoutIntervalForResource = 120
        configuration.urlCache = nil
        configuration.requestCachePolicy = .reloadIgnoringLocalAndRemoteCacheData
        session = URLSession(configuration: configuration)
    }

    public func configure(baseURL: URL) {
        self.baseURL = baseURL
        accessToken = nil
    }

    public func setAccessToken(_ token: String?) {
        accessToken = token
    }

    public func streamConfiguration() throws -> (URL, String?) {
        guard let baseURL else { throw APIError.invalidServer }
        return (baseURL, accessToken)
    }

    public func request(
        path: String,
        method: HTTPMethod = .get,
        queryItems: [URLQueryItem] = [],
        body: JSONValue? = nil,
        idempotencyKey: UUID? = nil,
        authorizationOverride: String? = nil,
        useSessionAuthorization: Bool = true
    ) async throws -> JSONValue {
        guard let baseURL else { throw APIError.invalidServer }
        guard var components = URLComponents(url: baseURL, resolvingAgainstBaseURL: false) else {
            throw APIError.invalidServer
        }
        let root = components.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        let suffix = path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        components.path = "/" + [root, suffix].filter { !$0.isEmpty }.joined(separator: "/")
        components.queryItems = queryItems.isEmpty ? nil : queryItems
        guard let url = components.url else { throw APIError.invalidServer }

        var request = URLRequest(url: url)
        request.httpMethod = method.rawValue
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue("no-store", forHTTPHeaderField: "Cache-Control")
        if let body {
            request.httpBody = try encoder.encode(body)
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        if let key = idempotencyKey {
            request.setValue(key.uuidString.lowercased(), forHTTPHeaderField: "Idempotency-Key")
        }
        if let authorizationOverride {
            request.setValue("Bearer \(authorizationOverride)", forHTTPHeaderField: "Authorization")
        } else if useSessionAuthorization, let accessToken {
            request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        }

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            throw APIError.offline
        }
        guard let http = response as? HTTPURLResponse else { throw APIError.invalidResponse }
        if (200..<300).contains(http.statusCode) {
            guard !data.isEmpty, http.statusCode != 204 else { return .null }
            do { return try decoder.decode(JSONValue.self, from: data) }
            catch { throw APIError.invalidResponse }
        }

        let envelope = try? decoder.decode(JSONValue.self, from: data)
        let errorObject = envelope?["error"]
        let code = errorObject?["code"]?.stringValue ?? "HTTP_\(http.statusCode)"
        let message = errorObject?["message"]?.stringValue
            ?? HTTPURLResponse.localizedString(forStatusCode: http.statusCode)
        switch http.statusCode {
        case 401: throw APIError.unauthorized
        case 409: throw APIError.conflict(code: code, message: message)
        case 422: throw APIError.validation(code: code, message: message)
        default: throw APIError.server(status: http.statusCode, code: code, message: message)
        }
    }
}

// MARK: - Trusted-device credential protection

public struct KeychainFailure: Error, LocalizedError, Sendable {
    public let status: OSStatus
    public var errorDescription: String? {
        SecCopyErrorMessageString(status, nil) as String? ?? "Keychain error \(status)"
    }
}

public actor KeychainStore {
    private let service = "ai.quazonai.operator.mobile-refresh"

    public func saveRefreshCredential(_ credential: String, profileID: UUID) throws {
        try deleteRefreshCredential(profileID: profileID)
        var accessError: Unmanaged<CFError>?
        guard let access = SecAccessControlCreateWithFlags(
            nil,
            kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly,
            [.userPresence],
            &accessError
        ) else {
            throw accessError?.takeRetainedValue() ?? APIError.invalidResponse
        }
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: profileID.uuidString,
            kSecAttrAccessControl as String: access,
            kSecUseDataProtectionKeychain as String: true,
            kSecValueData as String: Data(credential.utf8),
        ]
        let status = SecItemAdd(query as CFDictionary, nil)
        guard status == errSecSuccess else { throw KeychainFailure(status: status) }
    }

    public func readRefreshCredential(profileID: UUID, context: LAContext) throws -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: profileID.uuidString,
            kSecUseDataProtectionKeychain as String: true,
            kSecUseAuthenticationContext as String: context,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess,
              let data = item as? Data,
              let credential = String(data: data, encoding: .utf8)
        else { throw KeychainFailure(status: status) }
        return credential
    }

    public func deleteRefreshCredential(profileID: UUID) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: profileID.uuidString,
            kSecUseDataProtectionKeychain as String: true,
        ]
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw KeychainFailure(status: status)
        }
    }
}

@MainActor
public enum BiometricGate {
    public static func authorizedContext(reason: String) async throws -> LAContext {
        let context = LAContext()
        context.localizedCancelTitle = String(localized: "common.cancel")
        var error: NSError?
        guard context.canEvaluatePolicy(.deviceOwnerAuthentication, error: &error) else {
            throw error ?? APIError.unauthorized
        }
        try await context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason)
        return context
    }

    public static func authorize(reason: String) async throws {
        _ = try await authorizedContext(reason: reason)
    }
}

// MARK: - SwiftData cache, drafts, and cursor

@Model
public final class CachedResource {
    @Attribute(.unique) public var compoundKey: String
    public var profileID: String
    public var cacheKey: String
    @Attribute(.externalStorage) public var payload: Data
    public var updatedAt: Date

    public init(profileID: UUID, cacheKey: String, payload: Data, updatedAt: Date = .now) {
        self.compoundKey = "\(profileID.uuidString)|\(cacheKey)"
        self.profileID = profileID.uuidString
        self.cacheKey = cacheKey
        self.payload = payload
        self.updatedAt = updatedAt
    }
}

@Model
public final class IdeaDraft {
    @Attribute(.unique) public var profileID: String
    public var text: String
    public var updatedAt: Date

    public init(profileID: UUID, text: String, updatedAt: Date = .now) {
        self.profileID = profileID.uuidString
        self.text = text
        self.updatedAt = updatedAt
    }
}

@Model
public final class EventCursor {
    @Attribute(.unique) public var profileID: String
    public var lastEventID: Int
    public var updatedAt: Date

    public init(profileID: UUID, lastEventID: Int, updatedAt: Date = .now) {
        self.profileID = profileID.uuidString
        self.lastEventID = lastEventID
        self.updatedAt = updatedAt
    }
}

@MainActor
public enum OfflineCache {
    public static func resource(
        profileID: UUID,
        key: String,
        context: ModelContext
    ) throws -> JSONValue? {
        let compound = "\(profileID.uuidString)|\(key)"
        let descriptor = FetchDescriptor<CachedResource>(
            predicate: #Predicate { $0.compoundKey == compound }
        )
        guard let item = try context.fetch(descriptor).first else { return nil }
        return try JSONValue.decode(item.payload)
    }

    public static func store(
        _ value: JSONValue,
        profileID: UUID,
        key: String,
        context: ModelContext
    ) throws {
        let compound = "\(profileID.uuidString)|\(key)"
        let descriptor = FetchDescriptor<CachedResource>(
            predicate: #Predicate { $0.compoundKey == compound }
        )
        if let existing = try context.fetch(descriptor).first {
            existing.payload = try value.encodedData()
            existing.updatedAt = .now
        } else {
            context.insert(CachedResource(
                profileID: profileID,
                cacheKey: key,
                payload: try value.encodedData()
            ))
        }
        try context.save()
    }

    public static func draft(profileID: UUID, context: ModelContext) throws -> String {
        let target = profileID.uuidString
        let descriptor = FetchDescriptor<IdeaDraft>(predicate: #Predicate { $0.profileID == target })
        return try context.fetch(descriptor).first?.text ?? ""
    }

    public static func storeDraft(_ text: String, profileID: UUID, context: ModelContext) throws {
        let target = profileID.uuidString
        let descriptor = FetchDescriptor<IdeaDraft>(predicate: #Predicate { $0.profileID == target })
        if let existing = try context.fetch(descriptor).first {
            existing.text = text
            existing.updatedAt = .now
        } else {
            context.insert(IdeaDraft(profileID: profileID, text: text))
        }
        try context.save()
    }

    public static func cursor(profileID: UUID, context: ModelContext) throws -> Int {
        let target = profileID.uuidString
        let descriptor = FetchDescriptor<EventCursor>(predicate: #Predicate { $0.profileID == target })
        return try context.fetch(descriptor).first?.lastEventID ?? 0
    }

    public static func storeCursor(_ value: Int, profileID: UUID, context: ModelContext) throws {
        let target = profileID.uuidString
        let descriptor = FetchDescriptor<EventCursor>(predicate: #Predicate { $0.profileID == target })
        if let existing = try context.fetch(descriptor).first {
            existing.lastEventID = max(existing.lastEventID, value)
            existing.updatedAt = .now
        } else {
            context.insert(EventCursor(profileID: profileID, lastEventID: value))
        }
        try context.save()
    }
}

// MARK: - Network and SSE

@MainActor
public final class NetworkMonitor: ObservableObject {
    @Published public private(set) var isOnline = true
    private let monitor = NWPathMonitor()
    private let queue = DispatchQueue(label: "ai.quazonai.network-monitor")

    public init() {
        monitor.pathUpdateHandler = { [weak self] path in
            Task { @MainActor [weak self] in
                self?.isOnline = path.status == .satisfied
            }
        }
        monitor.start(queue: queue)
    }

    deinit { monitor.cancel() }
}

public struct ServerEvent: Sendable, Equatable {
    public let id: Int
    public let event: String
    public let data: String
}

public struct SSEParser: Sendable {
    private var id: String?
    private var event = "message"
    private var dataLines: [String] = []

    public init() {}

    public mutating func consume(line: String) -> ServerEvent? {
        if line.isEmpty {
            defer {
                id = nil
                event = "message"
                dataLines.removeAll(keepingCapacity: true)
            }
            guard let id, let numericID = Int(id), !dataLines.isEmpty else { return nil }
            return ServerEvent(id: numericID, event: event, data: dataLines.joined(separator: "\n"))
        }
        if line.hasPrefix(":" ) { return nil }
        let field: String
        let value: String
        if let separator = line.firstIndex(of: ":") {
            field = String(line[..<separator])
            let raw = line[line.index(after: separator)...]
            value = raw.first == " " ? String(raw.dropFirst()) : String(raw)
        } else {
            field = line
            value = ""
        }
        switch field {
        case "id": id = value
        case "event": event = value
        case "data": dataLines.append(value)
        default: break
        }
        return nil
    }
}

public enum EventStreamStatus: Sendable, Equatable {
    case disconnected
    case connected
    case reconnecting(Int)
}

public enum EventStreamOutput: Sendable {
    case status(EventStreamStatus)
    case event(ServerEvent)
    case unauthorized
}

public actor EventStreamActor {
    private var task: Task<Void, Never>?

    public init() {}

    public func start(baseURL: URL, accessToken: String?, cursor: Int) -> AsyncStream<EventStreamOutput> {
        task?.cancel()
        return AsyncStream { continuation in
            task = Task {
                await self.run(
                    baseURL: baseURL,
                    accessToken: accessToken,
                    startingCursor: cursor,
                    continuation: continuation
                )
            }
            continuation.onTermination = { [weak self] _ in
                Task { await self?.stop() }
            }
        }
    }

    public func stop() {
        task?.cancel()
        task = nil
    }

    private func run(
        baseURL: URL,
        accessToken: String?,
        startingCursor: Int,
        continuation: AsyncStream<EventStreamOutput>.Continuation
    ) async {
        var cursor = startingCursor
        var backoff = 1
        while !Task.isCancelled {
            do {
                guard var components = URLComponents(url: baseURL, resolvingAgainstBaseURL: false) else {
                    throw APIError.invalidServer
                }
                let root = components.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
                components.path = "/" + [root, "api/v1/events/stream"].filter { !$0.isEmpty }.joined(separator: "/")
                components.queryItems = [URLQueryItem(name: "cursor", value: String(cursor))]
                guard let url = components.url else { throw APIError.invalidServer }
                var request = URLRequest(url: url)
                request.setValue("text/event-stream", forHTTPHeaderField: "Accept")
                request.setValue("no-cache", forHTTPHeaderField: "Cache-Control")
                request.setValue(String(cursor), forHTTPHeaderField: "Last-Event-ID")
                if let accessToken {
                    request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
                }
                let (bytes, response) = try await URLSession.shared.bytes(for: request)
                guard let http = response as? HTTPURLResponse else { throw APIError.invalidResponse }
                if http.statusCode == 401 {
                    continuation.yield(.unauthorized)
                    continuation.finish()
                    return
                }
                guard (200..<300).contains(http.statusCode) else {
                    throw APIError.server(
                        status: http.statusCode,
                        code: "SSE_HTTP_\(http.statusCode)",
                        message: HTTPURLResponse.localizedString(forStatusCode: http.statusCode)
                    )
                }
                continuation.yield(.status(.connected))
                backoff = 1
                var parser = SSEParser()
                for try await line in bytes.lines {
                    try Task.checkCancellation()
                    if let event = parser.consume(line: line), event.id > cursor {
                        cursor = event.id
                        continuation.yield(.event(event))
                    }
                }
            } catch is CancellationError {
                continuation.yield(.status(.disconnected))
                continuation.finish()
                return
            } catch {
                continuation.yield(.status(.reconnecting(backoff)))
                let jitter = UInt64.random(in: 0...500_000_000)
                try? await Task.sleep(nanoseconds: UInt64(backoff) * 1_000_000_000 + jitter)
                backoff = min(backoff * 2, 30)
            }
        }
        continuation.finish()
    }
}

// MARK: - App session and capability model

public struct ClientBootstrap: Equatable, Sendable {
    public let serverVersion: String
    public let authEnabled: Bool
    public let operatorClientCapabilityEpoch: Int
    public let minimumIOSCapabilityEpoch: Int
    public let minimumIOSAppVersion: String

    public init(json: JSONValue) throws {
        guard let serverVersion = json["server_version"]?.stringValue,
              let authEnabled = json["auth_enabled"]?.boolValue,
              let operatorEpoch = json["operator_client_capability_epoch"]?.numberValue,
              let minimumEpoch = json["minimum_ios_capability_epoch"]?.numberValue,
              let minimumVersion = json["minimum_ios_app_version"]?.stringValue
        else { throw APIError.invalidResponse }
        self.serverVersion = serverVersion
        self.authEnabled = authEnabled
        self.operatorClientCapabilityEpoch = Int(operatorEpoch)
        self.minimumIOSCapabilityEpoch = Int(minimumEpoch)
        self.minimumIOSAppVersion = minimumVersion
    }
}

public enum AppPhase: Equatable {
    case serverSetup
    case connecting
    case login
    case ready
    case upgradeRequired(String)
    case failed(String)
}

public enum AppearanceMode: String, CaseIterable, Identifiable {
    case system
    case light
    case dark
    public var id: String { rawValue }

    public var colorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }
}

public enum AppDestination: String, CaseIterable, Identifiable, Hashable {
    case home
    case idea
    case research
    case alpha
    case portfolio
    case approvals
    case handoff
    case administration
    case security
    case settings

    public var id: String { rawValue }
    public var localizationKey: LocalizedStringResource {
        switch self {
        case .home: "nav.home"
        case .idea: "nav.idea"
        case .research: "nav.research"
        case .alpha: "nav.alpha"
        case .portfolio: "nav.portfolio"
        case .approvals: "nav.approvals"
        case .handoff: "nav.handoff"
        case .administration: "nav.admin"
        case .security: "nav.security"
        case .settings: "nav.settings"
        }
    }
    public var symbol: String {
        switch self {
        case .home: "house"
        case .idea: "lightbulb"
        case .research: "binoculars"
        case .alpha: "waveform.path.ecg"
        case .portfolio: "chart.pie"
        case .approvals: "checkmark.seal"
        case .handoff: "arrow.left.arrow.right"
        case .administration: "gearshape.2"
        case .security: "lock.shield"
        case .settings: "textformat.size"
        }
    }
}

@MainActor
public final class AppModel: ObservableObject {
    public static let nativeCapabilityEpoch = QuaZonaiWireContract.capabilityEpoch

    @Published public var phase: AppPhase = .serverSetup
    @Published public var serverInput: String
    @Published public private(set) var profile: ServerProfile?
    @Published public private(set) var bootstrap: ClientBootstrap?
    @Published public var destination: AppDestination = .home
    @Published public private(set) var streamStatus: EventStreamStatus = .disconnected
    @Published public private(set) var recentEvents: [ServerEvent] = []
    @Published public var bannerMessage: String?
    @Published public var appearance: AppearanceMode
    @Published public var localeIdentifier: String

    public let network = NetworkMonitor()
    private let api = APIClient()
    private let keychain = KeychainStore()
    private let stream = EventStreamActor()
    private var eventTask: Task<Void, Never>?
    private weak var modelContext: ModelContext?

    private let installationID: UUID
    private let defaults: UserDefaults

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        serverInput = defaults.string(forKey: "issue36.serverURL") ?? ""
        appearance = AppearanceMode(rawValue: defaults.string(forKey: "issue36.appearance") ?? "system") ?? .system
        localeIdentifier = defaults.string(forKey: "issue36.locale") ?? Locale.current.identifier
        if let stored = defaults.string(forKey: "issue36.installationID"), let id = UUID(uuidString: stored) {
            installationID = id
        } else {
            let id = UUID()
            installationID = id
            defaults.set(id.uuidString, forKey: "issue36.installationID")
        }
    }

    public var isDirectAccess: Bool { bootstrap?.authEnabled == false }
    public var isReady: Bool { phase == .ready }

    public func attach(modelContext: ModelContext) {
        self.modelContext = modelContext
    }

    public func setAppearance(_ value: AppearanceMode) {
        appearance = value
        defaults.set(value.rawValue, forKey: "issue36.appearance")
    }

    public func setLocale(_ value: String) {
        localeIdentifier = value
        defaults.set(value, forKey: "issue36.locale")
    }

    public func connect() async {
        phase = .connecting
        bannerMessage = nil
        do {
            let baseURL = try ServerProfile.normalize(serverInput)
            let profile = profileForURL(baseURL)
            self.profile = profile
            defaults.set(baseURL.absoluteString, forKey: "issue36.serverURL")
            await api.configure(baseURL: baseURL)
            let bootstrapJSON = try await api.request(
                path: "/api/v1/client/bootstrap",
                useSessionAuthorization: false
            )
            let bootstrap = try ClientBootstrap(json: bootstrapJSON)
            self.bootstrap = bootstrap
            if AppModel.nativeCapabilityEpoch < bootstrap.minimumIOSCapabilityEpoch {
                phase = .upgradeRequired(bootstrap.minimumIOSAppVersion)
                return
            }
            if bootstrap.authEnabled {
                phase = .login
            } else {
                phase = .ready
                bannerMessage = String(localized: "auth.direct")
                await startEventStream()
            }
        } catch {
            phase = .failed(error.localizedDescription)
        }
    }

    public func login(totpCode: String, trustDevice: Bool) async throws {
        let code = totpCode.trimmingCharacters(in: .whitespacesAndNewlines)
        guard code.count == 6, code.allSatisfy(\.isNumber), let profile else {
            throw APIError.validation(code: "TOTP_FORMAT", message: String(localized: "auth.invalid"))
        }
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "1.0.0"
        let build = Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "100"
        let family = UIDevice.current.userInterfaceIdiom == .pad ? "IPAD" : "IPHONE"
        let body: JSONValue = .object([
            "totp_code": .string(code),
            "installation_id": .string(installationID.uuidString.lowercased()),
            "device_name": .string(UIDevice.current.name),
            "device_family": .string(family),
            "os_version": .string(UIDevice.current.systemVersion),
            "app_version": .string(version),
            "app_build": .string(build),
            "trust_device": .bool(trustDevice),
        ])
        let response = try await api.request(
            path: "/api/v1/auth/mobile/login",
            method: .post,
            body: body,
            useSessionAuthorization: false
        )
        let access = try accessCredential(from: response)
        await api.setAccessToken(access)
        if trustDevice, let refresh = response.firstString(for: ["refresh_credential", "refresh_token"]) {
            try await keychain.saveRefreshCredential(refresh, profileID: profile.id)
        } else {
            try? await keychain.deleteRefreshCredential(profileID: profile.id)
        }
        phase = .ready
        bannerMessage = nil
        await startEventStream()
    }

    public func unlockTrustedDevice() async throws {
        try await rotateTrustedCredential()
        phase = .ready
        bannerMessage = nil
        await startEventStream()
    }

    private func rotateTrustedCredential() async throws {
        guard let profile else { throw APIError.invalidServer }
        let context = try await BiometricGate.authorizedContext(
            reason: String(localized: "security.biometric")
        )
        guard let refresh = try await keychain.readRefreshCredential(
            profileID: profile.id,
            context: context
        ) else { throw APIError.missingCredential }
        let response = try await api.request(
            path: "/api/v1/auth/mobile/refresh",
            method: .post,
            authorizationOverride: refresh,
            useSessionAuthorization: false
        )
        let access = try accessCredential(from: response)
        await api.setAccessToken(access)
        guard let rotated = response.firstString(for: ["refresh_credential", "refresh_token"]) else {
            throw APIError.invalidResponse
        }
        try await keychain.saveRefreshCredential(rotated, profileID: profile.id)
    }

    private func accessCredential(from response: JSONValue) throws -> String {
        guard let access = response.firstString(for: ["access_token", "access_credential"]), !access.isEmpty else {
            throw APIError.invalidResponse
        }
        return access
    }

    public func request(
        path: String,
        method: HTTPMethod = .get,
        queryItems: [URLQueryItem] = [],
        body: JSONValue? = nil,
        idempotencyKey: UUID? = nil,
        retryAuthentication: Bool = true
    ) async throws -> JSONValue {
        if method.isMutation && !network.isOnline { throw APIError.offline }
        let stableKey = method.isMutation ? (idempotencyKey ?? UUID()) : nil
        do {
            return try await api.request(
                path: path,
                method: method,
                queryItems: queryItems,
                body: body,
                idempotencyKey: stableKey
            )
        } catch APIError.unauthorized where retryAuthentication {
            if bootstrap?.authEnabled == true {
                try await rotateTrustedCredential()
                return try await request(
                    path: path,
                    method: method,
                    queryItems: queryItems,
                    body: body,
                    idempotencyKey: stableKey,
                    retryAuthentication: false
                )
            }
            await connect()
            throw APIError.unauthorized
        }
    }

    public func performSensitive<T>(
        reason: String,
        operation: @MainActor () async throws -> T
    ) async throws -> T {
        try await BiometricGate.authorize(reason: reason)
        return try await operation()
    }

    public func logout() async {
        if bootstrap?.authEnabled == true {
            _ = try? await api.request(path: "/api/v1/auth/mobile/logout", method: .post)
        }
        await stopEventStream()
        await api.setAccessToken(nil)
        if let profile { try? await keychain.deleteRefreshCredential(profileID: profile.id) }
        phase = bootstrap?.authEnabled == true ? .login : .serverSetup
    }

    public func forgetServer() async {
        await logout()
        profile = nil
        bootstrap = nil
        serverInput = ""
        defaults.removeObject(forKey: "issue36.serverURL")
        phase = .serverSetup
    }

    public func appDidEnterBackground() async {
        await stopEventStream()
    }

    public func appWillEnterForeground() async {
        if phase == .ready { await startEventStream() }
    }

    private func profileForURL(_ url: URL) -> ServerProfile {
        let mappingKey = "issue36.profile.\(url.absoluteString)"
        let id: UUID
        if let stored = defaults.string(forKey: mappingKey), let parsed = UUID(uuidString: stored) {
            id = parsed
        } else {
            id = UUID()
            defaults.set(id.uuidString, forKey: mappingKey)
        }
        return ServerProfile(id: id, name: url.host ?? "QuaZonai", baseURL: url)
    }

    private func startEventStream() async {
        await stopEventStream()
        guard let profile else { return }
        do {
            let (baseURL, token) = try await api.streamConfiguration()
            let cursor: Int
            if let modelContext {
                cursor = (try? OfflineCache.cursor(profileID: profile.id, context: modelContext)) ?? 0
            } else {
                cursor = 0
            }
            let outputs = await stream.start(baseURL: baseURL, accessToken: token, cursor: cursor)
            eventTask = Task { @MainActor [weak self] in
                guard let self else { return }
                for await output in outputs {
                    switch output {
                    case let .status(status):
                        self.streamStatus = status
                    case let .event(event):
                        if !self.recentEvents.contains(where: { $0.id == event.id }) {
                            self.recentEvents.insert(event, at: 0)
                            self.recentEvents = Array(self.recentEvents.prefix(50))
                        }
                        if let context = self.modelContext {
                            try? OfflineCache.storeCursor(event.id, profileID: profile.id, context: context)
                        }
                    case .unauthorized:
                        do {
                            try await self.rotateTrustedCredential()
                            await self.startEventStream()
                        } catch {
                            self.phase = .login
                            self.bannerMessage = error.localizedDescription
                        }
                        return
                    }
                }
            }
        } catch {
            streamStatus = .reconnecting(1)
        }
    }

    private func stopEventStream() async {
        eventTask?.cancel()
        eventTask = nil
        await stream.stop()
        streamStatus = .disconnected
    }
}
