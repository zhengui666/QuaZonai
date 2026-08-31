# Native Release Checklist

- [ ] Canonical OpenAPI snapshot matches the running FastAPI application.
- [ ] Swift OpenAPI generated client compiles.
- [ ] Mobile login schema contains `totp_code` and contains neither username nor password.
- [ ] TOTP replay protection and source rate limiting pass concurrency tests.
- [ ] Trusted-device refresh rotates and revocation terminates subsequent API and SSE authorization.
- [ ] Direct Access and authenticated server profiles both pass.
- [ ] iPhone compact navigation exposes every Operator domain.
- [ ] iPad split navigation passes portrait, landscape, Split View, Stage Manager, keyboard, and pointer checks.
- [ ] Read caches are server-profile scoped and all mutations remain unavailable offline.
- [ ] Stable idempotency keys and optimistic concurrency fields are preserved; `409` is never auto-overwritten.
- [ ] Provider API keys and TOTP values are absent from persistence and logs.
- [ ] English, Simplified Chinese, Traditional Chinese, Japanese, Korean, Spanish, and Arabic catalogs are complete; Arabic RTL passes.
- [ ] VoiceOver, Dynamic Type, Bold Text, Reduce Motion, Increase Contrast, and Voice Control checks pass.
- [ ] Privacy Manifest, AppIcon, Launch Screen, privacy policy, metadata, and screenshot plan are present.
- [ ] iPhone tests, iPad tests, client parity, unsigned archive, backend, frontend, compose, and remote Nautilus checks are green on the same PR HEAD.
- [ ] GitHub `@codex review` explicitly reports no issues on that HEAD.
- [ ] The verified PR is merged and Main Post-Merge Verification passes on the merge SHA.
