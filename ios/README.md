# QuaZonai Native Operator Client

This directory contains the native SwiftUI Universal App for iPhone and iPad. It is an Operator client for the existing QuaZonai `/api/v1` service; it is not a trading runtime and does not own broker credentials, orders, fills, positions, accounts, NAV, execution risk, or downstream lifecycle control.

## Platform baseline

- Swift 6 and SwiftUI
- iOS 18 / iPadOS 18 minimum deployment target
- one Universal target for iPhone and iPad
- Swift OpenAPI Generator + OpenAPI URLSession transport
- structured concurrency and actors
- native Server-Sent Events with durable cursor recovery
- SwiftData for server-profile-scoped read caches, event cursors, Idea drafts, and local UI state only
- Keychain for trusted-device refresh credentials only
- LocalAuthentication for local credential unlock and sensitive-action confirmation
- Swift Charts and OSLog

The authoritative wire contract is `../contracts/openapi/quazonai-v1.json`. The cross-client capability contract is `../contracts/client-capabilities.yaml`; `../tools/client_parity.py` prevents Web/iPhone/iPad drift.

## Authentication

When server authentication is enabled, the native login request contains a current six-digit `totp_code` and device metadata. It contains no username or password. The initial TOTP is never persisted. A short-lived access credential remains in memory. When the Operator explicitly trusts the device, the rotating refresh credential is stored in Keychain and may be independently revoked from Administration.

Face ID or Touch ID protects the local trusted-device credential. Biometrics do not replace the server's initial TOTP verification and do not grant additional server permissions.

When `QUAZONAI_AUTH_ENABLED=false`, bootstrap enters explicit Direct Access mode with a security warning. If the server later enables authentication, the next `401` returns the app to the TOTP-only flow.

The native app never embeds or reads `QUAZONAI_API_TOKEN`; that credential remains exclusive to CLI and machine automation.

## Navigation and parity

On iPhone, the primary tabs are Home, Research, Approvals, Portfolio, and More. More exposes Idea Composer, Alpha Library, Handoff & Feedback, Administration, language, appearance, and device security.

On iPad, `NavigationSplitView` exposes all product domains in the sidebar and adapts through portrait, landscape, Split View, Stage Manager, and dynamic resizing. Compact layouts change presentation, not capability.

All server fields are available through native collection and detail views. Mutations preserve server validation, `Idempotency-Key`, `expected_state`, `expected_revision`, and explicit `409` conflict handling. Offline mode is read-only except for local Idea drafts.

## Security and privacy

- production server profiles require HTTPS
- no certificate bypass or trust-all mode
- authorization headers, TOTP values, provider API keys, and refresh credentials are redacted from logs
- Codex API keys use `SecureField`, are not written to Keychain or SwiftData, and are cleared from UI state after the request
- app backgrounding installs a privacy cover
- caches and credentials are isolated by server profile
- claimed Handoffs never expose stop, undeploy, close-position, or liquidation controls

See `AppStore/PRIVACY.md` and `Resources/PrivacyInfo.xcprivacy`.

## Verification

Pull requests run:

- backend native-auth unit/integration tests
- OpenAPI export/drift validation
- client capability parity validation
- iPhone simulator tests
- iPad simulator tests
- unsigned release archive
- localization, accessibility, privacy, and App Store metadata contract checks

The implementation PR is mergeable only after every GitHub check passes and a GitHub `@codex review` explicitly reports no issues.
