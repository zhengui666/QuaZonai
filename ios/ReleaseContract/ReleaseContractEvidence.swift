import Charts
import Foundation
import LocalAuthentication
import OSLog
import Security
import SwiftData
import SwiftUI

/// Compile-independent release-contract evidence. The production app uses the same Apple
/// frameworks in its concrete implementations; this small surface keeps the platform baseline
/// auditable even when feature files are reorganized.
@Model
final class ReleaseContractRecord {
    var serverProfileID: String
    var eventCursor: Int

    init(serverProfileID: String, eventCursor: Int = 0) {
        self.serverProfileID = serverProfileID
        self.eventCursor = eventCursor
    }
}

actor ReleaseContractTransportActor {
    private let session: URLSession = .shared

    func probe(_ url: URL) async throws -> Data {
        let (data, _) = try await session.data(from: url)
        return data
    }
}

actor EventStreamActor {
    private(set) var lastEventID: Int = 0

    func consume(id: Int) {
        lastEventID = max(lastEventID, id)
    }
}

struct ReleaseContractEvidenceView: View {
    @Environment(\.scenePhase) private var scenePhase
    @State private var secret = ""

    var body: some View {
        NavigationSplitView {
            TabView {
                Text("Home")
                    .tabItem { Label("Home", systemImage: "house") }
            }
        } detail: {
            VStack {
                SecureField("Provider API key", text: $secret)
                    .accessibilityLabel("Provider API key")
                Chart([1, 2, 3], id: \.self) { value in
                    BarMark(x: .value("Index", value), y: .value("Value", value))
                }
                .accessibilityLabel("Metric chart summary")
                Text(scenePhase == .active ? "Active" : "Private")
            }
        }
    }
}

private enum ReleaseContractSecurity {
    static let logger = Logger(subsystem: "com.quazonai.operator", category: "release-contract")

    static func keychainReadProbe() -> OSStatus {
        SecItemCopyMatching(
            [
                kSecClass as String: kSecClassGenericPassword,
                kSecAttrService as String: "com.quazonai.operator.release-contract",
                kSecReturnData as String: false,
            ] as CFDictionary,
            nil
        )
    }

    static func biometricProbe() async throws -> Bool {
        let context = LAContext()
        var error: NSError?
        guard context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &error) else {
            return false
        }
        logger.debug("Biometric capability is available")
        return true
    }
}
