import LocalAuthentication

@MainActor
enum BiometricGate {
    static func authorize(reason: String) async -> Bool {
#if DEBUG
        // CI UI tests exercise the complete mutation path on an unenrolled
        // simulator. This hook is compiled out of release builds and requires
        // the explicit fixture launch argument.
        if ProcessInfo.processInfo.arguments.contains("--ui-testing") {
            return true
        }
#endif
        let context = LAContext()
        context.localizedCancelTitle = "Cancel"
        var error: NSError?
        let policy: LAPolicy
        if context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &error) {
            policy = .deviceOwnerAuthenticationWithBiometrics
        } else if context.canEvaluatePolicy(.deviceOwnerAuthentication, error: &error) {
            policy = .deviceOwnerAuthentication
        } else {
            return false
        }
        do {
            return try await context.evaluatePolicy(policy, localizedReason: reason)
        } catch {
            return false
        }
    }
}
