# Expose existing recorded research inputs

## Scope

The owner's real idea-to-Alpha-to-Paper preparation exposed two operational gaps:
recorded feature provenance already has typed Contracts and Store operations but
no public HTTP/CLI entry, and the frozen Brief execution context is readable by
HTTP but missing from CLI. Self-authored parameters are not a substitute for
registered original feature source provenance. Polymarket Paper requires actual
REAL market and feature origins.

Expose the existing Store methods as project-scoped reads and exact, idempotent
Operator registration. Preserve original UTF-8 bytes, Dataset/metadata/source
bindings, data grants, the Sealed boundary and existing validation. No strategy,
PIT/fee/data declaration, model, Nautilus implementation or qualification rule
changes. Reading a frozen Brief uses its existing endpoint and DTO.

The upload uses the existing artifact body/capacity bound and detached native
command lifetime. On an uncertain commit, cleanup goes through the original
Store authority lock and cannot delete a published artifact. CLI preview stays
local and redacted, and route/body Dataset disagreement fails before sending.

## Verification

Focused native CLI route/response/preview tests cover the existing GET, exact
IDs and decimal revisions, unchanged `brief show`, scoped feature listing,
Operator/idempotency requirements, forbidden caller-origin fields and mismatched
Dataset IDs. The focused real-PG HTTP case preserves original bytes, immutable
source fields, one identity on replay, rejection of same-length byte changes,
anonymous access rejection and route/query validation. Test sources are FIXTURE;
they do not establish actual market import, verified PIT or scientific results.

Native OpenAPI and generated web consumers must be regenerated together. Full
CI, independent review, publication, deployment and real-input acceptance remain
separate verification states.

Focused native validation completed with Rust 1.98.1: three frozen-Brief command
unit tests, three actual portable CLI input/preview tests, two native HTTP
OpenAPI tests (including every operation ID's uniqueness), and Server library
Clippy with warnings denied. The new HTTP database target compiles; its database
case could not start here because `DATABASE_URL` is absent. Full CLI testing
reported 22 passing unit tests and one existing host-routing probe failing with
`NetworkUnreachable` before its assertions. Neither is reported as a business
pass. The final source, native API export and existing web contract generator
agree; 425 generated response contract files were checked without frontend or
container acceptance. Existing API paths remain unchanged.

Independent review identified colliding default OpenAPI operation IDs and an
unreachable 413 declaration. The new paths now have explicit unique operation
IDs and retain the existing JSON rejection's 422 behavior. The database test
also preserves existing project-scope hiding (404), missing-grant rejection
(403), exact-byte replay and source origin/PIT without promotion.


The existing required CI `store-postgres` job now retains both original release
stack checks and separately executes the exact recorded-feature HTTP case using
its already-started disposable PostgreSQL/PGMQ instance. The added gate requires
one exact listing, one executed passing test and zero ignored tests; Cargo,
output capture or assertion failures remain job failures. It still attempts this
boundary if an earlier check failed, unless cancelled, and preserves its separate
logs/status with the existing evidence. Full Rust regression also discovers this
integration target through the existing unrestricted Server test command.
