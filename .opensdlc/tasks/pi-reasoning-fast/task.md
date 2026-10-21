# Pi reasoning and subscription speed

<a id="task"></a>
## Task

Support the official Pi capabilities requested by the owner, through review, applicable CI and merge into `dev`. Production deployment is outside this change.

<a id="intent"></a>
## Intent

Expose supported maximum reasoning and OpenAI subscription fast processing without changing provider implementations or existing research identities.

<a id="spec"></a>
## Requirements and design

Official pi-ai 1.1.0 already supports `max`, and QuaZonai already carries it end to end. Neither pi-ai 1.1.0 nor current official 1.3.0 supports `ultra`. Keep model-declared reasoning capabilities authoritative.

Add optional `serviceTier` (`default` or `priority`) to research inputs and nullable default settings, and report supported tiers for official OpenAI Responses and legacy Codex APIs. UI labels `priority` as Fast. Official Durable exposes no service tier option: use its public ProviderDoc identity plus pi-ai's public onPayload callback to attach the original research's tier. Restore the small runtime association from original admissions before native work resumes; register new studies before submission. Both generation and compaction use this choice. Existing inputs without a tier keep provider defaults. Reject unsupported requests rather than silently dropping them.

<a id="plan"></a>
## Implementation plan

Reuse current contracts, settings persistence, research admission/replay, official provider/auth and native execution. Parallel work covers service/contracts, Web and the research adapter/tests. Check TypeScript, research/service/CLI tests and Web build/interactions. Review the final change through the existing PR flow. No dependency upgrade, schema migration, numerical changes or new scheduler.

<a id="verification"></a>
## Verification

Official provider tests inspect actual serialized OpenAI requests using synthetic authentication and local fetch responses. They cover `max`, priority/standard/omitted tiers, concurrent studies, original replay, restart/resume and compaction immediately after reopening. No paid inference, account entitlement or model-quality evaluation is performed.

`make check-web` passed (21 unit tests and production build); five relevant Playwright interactions passed. Service model tests passed (3 tests). Full `make check-ts check-deploy` is running; hosted checks and GitHub Codex review remain pending.
