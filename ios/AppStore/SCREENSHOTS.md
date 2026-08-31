# App Store Screenshot Plan

Screenshots are captured from the same deterministic FastAPI/PostgreSQL fixture used by Web, iPhone, and iPad parity tests. They must contain no credentials, tokens, TOTP values, provider API keys, production server URLs, or personally identifying data.

## iPhone

1. Home — Action Center, Research Pulse, system health, and SSE state.
2. Idea Composer — natural-language idea and frozen Charter preview.
3. Research — Program detail, Mission graph, market context, and evidence ledger.
4. Approval — immutable Paper recommendation, evidence, capital context, and downstream selection.
5. Administration — runtime readiness and capability registry.

## iPad

1. Three-column Research Observatory in landscape.
2. Portfolio Candidate detail with equity, benchmark, allocation, risk, correlation, and version disclosures.
3. Approval Inbox with detail inspector.
4. Handoff & Feedback states and contracts.
5. Administration with runtime configuration, data, mandates, downstreams, plugins, capital context, and device security.

## Locales and accessibility

- English is the primary App Store screenshot locale.
- Simplified Chinese and Arabic screenshots are generated to verify long CJK text and RTL layout.
- Dynamic Type at an accessibility size is included in internal release evidence.
- Every chart screenshot has an equivalent VoiceOver text-summary assertion in UI tests.
