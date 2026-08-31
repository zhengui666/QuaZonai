import Combine
import Foundation
import SwiftData
import UIKit

enum SessionPhase: Equatable {
    case serverSetup
    case connecting
    case loginRequired
    case trustedUnlockAvailable
    case ready
    case incompatible(String)
}


struct MutationIdempotencyRegistry {
    private var pending: [String: String] = [:]

    mutating func key(for fingerprint: String) -> String {
        if let existing = pending[fingerprint] { return existing }
        let generated = UUID().uuidString
        pending[fingerprint] = generated
        return generated
    }

    mutating func finish(fingerprint: String, key: String) {
        if pending[fingerprint] == key {
            pending.removeValue(forKey: fingerprint)
        }
    }

    mutating func removeAll() {
        pending.removeAll()
    }
}

@MainActor
final class SessionStore: ObservableObject {
    @Published var phase: SessionPhase = .serverSetup
    @Published var errorMessage: String?
    @Published var directAccessWarning = false
    @Published var eventState: EventStreamState = .disconnected
    @Published var recentEvents: [ServerEvent] = []
    @Published var language: AppLanguage
    @Published var appearance: AppAppearance
    @Published var privacyCovered = false

    let connectivity = ConnectivityMonitor()
    private let defaults = UserDefaults.standard
    private let keychain = KeychainStore()
    private var cache: CacheStore?
    private(set) var profile = ""
    private(set) var authEnabled = false
    private var client: APIClient?
    private var eventStream: EventStreamActor?
    private var mutationKeys = MutationIdempotencyRegistry()

    private let serverKey = "quazonai.server-url"
    private let languageKey = "quazonai.language"
    private let appearanceKey = "quazonai.appearance"
    private let installationKey = "quazonai.installation-id"

    init() {
        language = AppLanguage(rawValue: UserDefaults.standard.string(forKey: languageKey) ?? "") ?? .english
        appearance = AppAppearance(rawValue: UserDefaults.standard.string(forKey: appearanceKey) ?? "") ?? .system
    }

    func attachCache(_ context: ModelContext) {
        if cache == nil { cache = CacheStore(context: context) }
    }

    func begin() async {
        let environment = ProcessInfo.processInfo.environment
        if let uiServer = environment["QUAZONAI_UI_SERVER"], !uiServer.isEmpty {
            await connect(to: uiServer)
            return
        }
        if let saved = defaults.string(forKey: serverKey), !saved.isEmpty {
            await connect(to: saved)
        }
    }

    func connect(to rawURL: String) async {
        phase = .connecting
        errorMessage = nil
        do {
            let baseURL = try normalizeServerURL(rawURL)
            let next = APIClient(baseURL: baseURL)
            let bootstrap = try await next.bootstrap()
            profile = baseURL.absoluteString
            defaults.set(profile, forKey: serverKey)
            client = next
            mutationKeys.removeAll()
            authEnabled = bootstrap.authEnabled
            directAccessWarning = !bootstrap.authEnabled
            if bootstrap.authEnabled {
                phase = keychain.exists(account: profile) ? .trustedUnlockAvailable : .loginRequired
            } else {
                phase = .ready
                startEvents()
            }
        } catch APIClientError.incompatibleClient(let epoch) {
            phase = .incompatible("Capability epoch \(epoch) is required.")
        } catch APIClientError.incompatibleAppVersion(let version) {
            phase = .incompatible("QuaZonai \(version) or later is required.")
        } catch {
            phase = .serverSetup
            errorMessage = error.localizedDescription
        }
    }

    func unlockTrustedDevice() async {
        guard let client, !profile.isEmpty else { return }
        errorMessage = nil
        do {
            guard let refresh = try keychain.read(account: profile, prompt: "Unlock QuaZonai trusted device") else {
                phase = .loginRequired
                return
            }
            await client.configureTrustedCredential(refresh)
            let rotated = try await client.refreshIfPossible()
            if let credential = rotated.refreshCredential {
                try keychain.save(credential, account: profile)
                await client.markRefreshCredentialPersisted(credential)
            }
            phase = .ready
            startEvents()
        } catch {
            try? keychain.delete(account: profile)
            await client.clearCredentials()
            phase = .loginRequired
            errorMessage = error.localizedDescription
        }
    }

    func login(totpCode: String, trustDevice: Bool) async -> Bool {
        guard let client else { return false }
        errorMessage = nil
        guard totpCode.count == 6, totpCode.allSatisfy(\.isNumber) else {
            errorMessage = "Enter the current 6-digit TOTP code."
            return false
        }
        if trustDevice {
            guard await BiometricGate.authorize(reason: "Protect the QuaZonai trusted-device credential") else {
                errorMessage = "Trusted-device protection requires successful device-owner authentication."
                return false
            }
        }

        let bundle = Bundle.main
        let version = bundle.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "1.0.0"
        let build = bundle.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "100"
        let family = UIDevice.current.userInterfaceIdiom == .pad ? "IPAD" : "IPHONE"
        let request = MobileLoginRequest(
            totpCode: totpCode,
            installationID: installationID(),
            deviceName: UIDevice.current.name,
            deviceFamily: family,
            osVersion: UIDevice.current.systemVersion,
            appVersion: version,
            appBuild: build,
            trustDevice: trustDevice
        )
        do {
            let tokens = try await client.login(request)
            if trustDevice, let credential = tokens.refreshCredential {
                try keychain.save(credential, account: profile)
            } else {
                try? keychain.delete(account: profile)
            }
            phase = .ready
            startEvents()
            return true
        } catch {
            await client.clearCredentials()
            errorMessage = error.localizedDescription
            return false
        }
    }

    func load(path: String, cacheKey: String? = nil, offlineReadable: Bool = true) async throws -> JSONValue {
        guard let client else { throw APIClientError.invalidServerURL }
        do {
            let value = try await client.requestJSON(path: path)
            await persistRotatedRefreshCredentialIfNeeded()
            if let cacheKey { try? cache?.save(value, key: cacheKey, profile: profile) }
            return value
        } catch APIClientError.authenticationRequired {
            await persistRotatedRefreshCredentialIfNeeded()
            phase = .loginRequired
            stopEvents()
            throw APIClientError.authenticationRequired
        } catch {
            await persistRotatedRefreshCredentialIfNeeded()
            if let cached = cachedFallback(
                for: error,
                cacheKey: cacheKey,
                offlineReadable: offlineReadable
            ) {
                return cached
            }
            throw error
        }
    }

    func mutate(
        path: String,
        method: HTTPMethod = .post,
        body: JSONValue = .object([:]),
        idempotencyKey: String? = nil
    ) async throws -> JSONValue {
        guard connectivity.isOnline else {
            throw APIClientError.http(status: 0, code: "OFFLINE_MUTATION_BLOCKED", message: "Mutations are disabled while offline.")
        }
        guard let client else { throw APIClientError.invalidServerURL }
        let fingerprint = try mutationFingerprint(path: path, method: method, body: body)
        let tracksGeneratedKey = idempotencyKey == nil
        let stableKey = idempotencyKey ?? mutationKeys.key(for: fingerprint)
        do {
            let value = try await client.requestJSON(
                path: path,
                method: method,
                body: body,
                idempotencyKey: stableKey
            )
            await persistRotatedRefreshCredentialIfNeeded()
            if tracksGeneratedKey {
                mutationKeys.finish(fingerprint: fingerprint, key: stableKey)
            }
            return value
        } catch APIClientError.authenticationRequired {
            await persistRotatedRefreshCredentialIfNeeded()
            if tracksGeneratedKey {
                mutationKeys.finish(fingerprint: fingerprint, key: stableKey)
            }
            phase = .loginRequired
            stopEvents()
            throw APIClientError.authenticationRequired
        } catch {
            await persistRotatedRefreshCredentialIfNeeded()
            if tracksGeneratedKey && !shouldRetainMutationKey(after: error) {
                mutationKeys.finish(fingerprint: fingerprint, key: stableKey)
            }
            throw error
        }
    }

    func logout() async {
        errorMessage = nil
        stopEvents()
        do {
            if let client { try await client.logout() }
        } catch {
            await persistRotatedRefreshCredentialIfNeeded()
            errorMessage = "Could not revoke the server session. The trusted-device credential was kept so logout can be retried. \(error.localizedDescription)"
            startEvents()
            return
        }
        do {
            try keychain.delete(account: profile)
        } catch {
            errorMessage = "The server session was revoked, but the local trusted-device credential could not be removed."
        }
        mutationKeys.removeAll()
        phase = authEnabled ? .loginRequired : .serverSetup
    }

    func forgetServer() async {
        errorMessage = nil
        stopEvents()
        do {
            if let client { try await client.logout() }
        } catch {
            await persistRotatedRefreshCredentialIfNeeded()
            errorMessage = "Could not revoke the server session. The server profile and trusted-device credential were kept. \(error.localizedDescription)"
            startEvents()
            return
        }
        var cleanupMessage: String?
        do {
            try keychain.delete(account: profile)
        } catch {
            cleanupMessage = "The server session was revoked, but the local trusted-device credential could not be removed."
        }
        defaults.removeObject(forKey: serverKey)
        mutationKeys.removeAll()
        client = nil
        profile = ""
        phase = .serverSetup
        errorMessage = cleanupMessage
    }

    func setLanguage(_ value: AppLanguage) {
        language = value
        defaults.set(value.rawValue, forKey: languageKey)
    }

    func setAppearance(_ value: AppAppearance) {
        appearance = value
        defaults.set(value.rawValue, forKey: appearanceKey)
    }

    private func mutationFingerprint(
        path: String,
        method: HTTPMethod,
        body: JSONValue
    ) throws -> String {
        let canonicalBody = String(decoding: try body.encodedData(pretty: true), as: UTF8.self)
        return "\(profile)\n\(method.rawValue)\n\(path)\n\(canonicalBody)"
    }

    private func shouldRetainMutationKey(after error: Error) -> Bool {
        if error is URLError || error is CancellationError || error is DecodingError {
            return true
        }
        if let apiError = error as? APIClientError, apiError == .invalidResponse {
            return true
        }
        return (error as NSError).domain == NSURLErrorDomain
    }

    private func cachedFallback(
        for error: Error,
        cacheKey: String?,
        offlineReadable: Bool
    ) -> JSONValue? {
        guard offlineReadable, let cacheKey else { return nil }
        let nsError = error as NSError
        let transportUnavailable = !connectivity.isOnline
            || error is URLError
            || nsError.domain == NSURLErrorDomain
        guard transportUnavailable else { return nil }
        return cache?.load(key: cacheKey, profile: profile)
    }

    private func installationID() -> UUID {
        if let raw = defaults.string(forKey: installationKey), let value = UUID(uuidString: raw) { return value }
        let value = UUID()
        defaults.set(value.uuidString, forKey: installationKey)
        return value
    }

    private func persistRotatedRefreshCredentialIfNeeded() async {
        guard let client, keychain.exists(account: profile),
              let rotated = await client.pendingRefreshCredentialForPersistence() else { return }
        do {
            try keychain.save(rotated, account: profile)
            await client.markRefreshCredentialPersisted(rotated)
        } catch {
            errorMessage = "The trusted-device credential rotated but could not be stored securely. Sign in again before restarting the app."
        }
    }

    private func startEvents() {
        guard eventStream == nil, let client else { return }
        let startingCursor = cache?.cursor(profile: profile) ?? 0
        let currentProfile = profile
        let stream = EventStreamActor(
            client: client,
            cursor: startingCursor,
            persistCursor: { [weak self] id in
                await MainActor.run { self?.cache?.saveCursor(id, profile: currentProfile) }
            },
            persistRefreshCredential: { [weak self] in
                await self?.persistRotatedRefreshCredentialIfNeeded()
            }
        )
        eventStream = stream
        Task {
            await stream.start(
                onState: { [weak self] state in await MainActor.run { self?.eventState = state } },
                onEvent: { [weak self] event in
                    await MainActor.run {
                        guard let self else { return }
                        self.recentEvents.insert(event, at: 0)
                        self.recentEvents = Array(self.recentEvents.prefix(30))
                    }
                }
            )
        }
    }

    private func stopEvents() {
        guard let stream = eventStream else { return }
        eventStream = nil
        eventState = .disconnected
        Task { await stream.stop() }
    }
}
