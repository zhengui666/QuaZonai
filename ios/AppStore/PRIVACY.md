# QuaZonai iOS/iPadOS Privacy

QuaZonai is a self-hosted, single-Operator research and portfolio-construction client. The application connects only to the server URL configured by the Operator.

## Data collection

The application developer does not receive or collect data from the application. QuaZonai does not include advertising SDKs, analytics SDKs, cross-app tracking, third-party telemetry, or crash-reporting services.

The configured QuaZonai server may process research ideas, system configuration, approvals, and other Operator data according to the server owner's own deployment and retention policy. Those records remain within the Operator-controlled deployment.

## Credentials

A current TOTP is used only for the initial server authentication request and is cleared from application state after submission. It is not written to Keychain, SwiftData, logs, analytics, or backups. A trusted-device refresh credential, when enabled by the Operator, is stored only in the Apple Keychain and may be revoked independently from the server. Codex API keys entered in Administration are transmitted directly to the configured server for that one update and are not persisted by the application.

The application does not request, store, or transmit QuaZonai browser usernames or passwords. It does not embed or use `QUAZONAI_API_TOKEN`.

## Local storage

Local storage is limited to server-profile-scoped read caches, event cursors, Idea drafts, appearance/language preferences, and non-authoritative UI state. Domain truth remains on the configured server. Sensitive mutations are disabled while offline.

## Trading boundary

QuaZonai does not store broker or exchange credentials, submit or cancel orders, hold account or position ledgers, or start, stop, recover, undeploy, or liquidate downstream trading runtimes.

## Contact

Privacy and security reports may be filed through the repository's GitHub issue tracker. Do not include credentials, TOTP values, tokens, API keys, or other secrets in a report.
