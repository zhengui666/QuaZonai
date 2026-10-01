# Read transport error semantics

The [native data browser acceptance](https://github.com/zhengui666/QuaZonai/pull/146)
exercised a failed artifact GET and exposed a misleading message saying the
submission result was unknown. The shared HTTP middleware used that wording for
every network failure, including reads.

Classify only GET/HEAD transport failures as `NETWORK_READ_FAILED`, with a read
failure message. All other methods retain `NETWORK_UNKNOWN` and their existing
uncertain-write behavior. Existing `ApiFailure`, cancellation, HTTP status and
asynchronous response-validator failures keep their original identity. No retry
is started automatically and no form/idempotency state changes.

Tests exercise the actual native client middleware for six methods, preserving
abort/error objects, request count, and exact write body/key on explicit retry.
The generic HEAD case uses test-only request middleware because the current
business schema declares no HEAD operation; no backend request is made. A real
native download regression expects the corrected read message and still compares
original Runtime bytes after retry.

Local typecheck, all 578 web unit tests, production/PWA build and all 350 tracked
response-contract drift checks pass. Exact-head hosted checks, independent native
review and integration after PR 146 remain required before merge. This does not
claim additional data qualification or Agent evaluation.
