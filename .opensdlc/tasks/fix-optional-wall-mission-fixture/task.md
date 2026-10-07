# Isolate optional-wall credential revocation scenarios

Release 37582752653 on dev 3018f10b failed in the combined cancellation/revocation/
disabled-principal test at missions.rs:1970. The failure occurs during the second
fixture's Brief update, before its credential revocation assertion. All browser
commands use the existing OPERATOR principal scope; command receipts uniquely bind
scope, operation and idempotency key. Creating a different Brief/request with the
same literal optional-wall key correctly returns IdempotencyConflict.

Split the three independent authority scenarios into three native SQLx tests, each
with its own migrated database and the original helper/body. Preserve every literal
command key, absent CPU/wall/output/token limits, finite ownership lease and immediate
machine-session rejection assertion. The fixture explicitly verifies the unchanged
request replays every original response field and a changed request under that same
key still conflicts. No production Store, authorization, receipt or SQL rule changes.

This is a fixture isolation correction, not a relaxation of idempotency or credential
revocation. No shared database is reset and no receipt is deleted. Original failing
release evidence is retained. Source/static checks precede separately coordinated
Rust compilation and real isolated PostgreSQL execution; unrun PG checks are not passes.

The three exact cases must run before merge in the existing isolated store-postgres
job. A separate maintenance-gate candidate based on PR177 preserves its existing
12 Dataset/Forward and eight native-client selectors and adds these cases alongside
the other explicit release regressions. A listed/compiled/ignored case is not a
PG pass. This business patch does not overwrite the newer CI workflow.

## Disabled-principal fixture follow-up at PR177 head 4e746d

Run 37596191488, job 112709308070, executed the three split cases. Cancellation
and credential revocation passed. The disabled-principal case stopped at the
fixture's SQL update with SQLSTATE 23514, before checking the old credential:
`guard_principal_epoch` requires a strictly greater credential epoch whenever
`enabled` changes. The fixture had changed only `enabled=false`.

The follow-up changes only that branch to atomically set `enabled=false` and
increment `credential_epoch`, matching the existing principal-disable fixture
and migration 202609060007. It preserves the guard, original command keys,
all absent budget caps, positive session check and immediate-denial assertion.
The two already passing branches and production code/migrations are unchanged.
This candidate is source-only until the exact disabled-principal selector runs
on the next isolated CI head; the earlier failing execution remains retained.
