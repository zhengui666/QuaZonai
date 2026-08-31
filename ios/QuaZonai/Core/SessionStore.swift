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
            errorMessage = error.localizedDescription
            return false
        }
    }

    func load(path: String, cacheKey: String? = nil, offlineReadable: Bool = true) async throws -> JSONValue {
        guard let client else { throw APIClientError.invalidServerURL }
        do {
            let value = try await client.requestJSON(path: path)
            if let cacheKey { try? cache?.save(value, key: cacheKey, profile: profile) }
            if let rotated = await client.currentRefreshCredential(), keychain.exists(account: profile) {
                try? keychain.save(rotated, account: profile)
            }
            return value
        } catch APIClientError.authenticationRequired {
            phase = .loginRequired
            stopEvents()
            throw APIClientError.authenticationRequired
        } catch {
            if offlineReadable, let cacheKey, let cached = cache?.load(key: cacheKey, profile: profile) {
                return cached
            }
            throw error
        }
    }

    func mutate(
        path: String,
        method: HTTPMethod = .post,
        body: JSONValue = .object([:]),
        idempotencyKey: String = UUID().uuidString
    ) async throws -> JSONValue {
        guard connectivity.isOnline else {
            throw APIClientError.http(status: 0, code: "OFFLINE_MUTATION_BLOCKED", message: "Mutations are disabled while offline.")
        }
        guard let client else { throw APIClientError.invalidServerURL }
        do {
            let value = try await client.requestJSON(
                path: path,
                method: method,
                body: body,
                idempotencyKey: idempotencyKey
            )
            if let rotated = await client.currentRefreshCredential(), keychain.exists(account: profile) {
                try? keychain.save(rotated, account: profile)
            }
            return value
        } catch APIClientError.authenticationRequired {
            phase = .loginRequired
            stopEvents()
            throw APIClientError.authenticationRequired
        }
    }

    func logout() async {
        stopEvents()
        if let client { await client.logout() }
        try? keychain.delete(account: profile)
        phase = authEnabled ? .loginRequired : .serverSetup
    }

    func forgetServer() async {
        stopEvents()
        if let client { await client.logout() }
        try? keychain.delete(account: profile)
        defaults.removeObject(forKey: serverKey)
        client = nil
        profile = ""
        phase = .serverSetup
    }

    func setLanguage(_ value: AppLanguage) {
        language = value
        defaults.set(value.rawValue, forKey: languageKey)
    }

    func setAppearance(_ value: AppAppearance) {
        appearance = value
        defaults.set(value.rawValue, forKey: appearanceKey)
    }

    private func installationID() -> UUID {
        if let raw = defaults.string(forKey: installationKey), let value = UUID(uuidString: raw) { return value }
        let value = UUID()
        defaults.set(value.uuidString, forKey: installationKey)
        return value
    }

    private func startEvents() {
        guard eventStream == nil, let client else { return }
        let startingCursor = cache?.cursor(profile: profile) ?? 0
        let currentProfile = profile
        let stream = EventStreamActor(client: client, cursor: startingCursor) { [weak self] id in
            await MainActor.run { self?.cache?.saveCursor(id, profile: currentProfile) }
        }
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
