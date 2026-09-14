# Issue62 implementation evidence

## Native ForwardEvaluate scientific operation, 2026-09-14

EvaluateForward now binds a fixed FORWARD_EVALUATE JobSpec to NativeForwardRequestV1:
the original frozen window and all original message metadata (including superseded
versions). Only their exact REPORT objects plus PARAMETERS can be mounted; no dataset,
model or extra artifact is accepted. The existing Runtime materializer handles this
report-only task, and config accepts an explicitly registered fixed ForwardEvaluate image.
The native 256-input limit leaves at most 255 source reports, rejected without truncation;
64MiB / one-million-point total bounds and original report limits remain enforced.

The actual job subprocess reads original report objects with exact declared sizes,
reuses original revision-chain/window validation, rejects changed bindings, future source
receipt times, unknown frequency, partial/gapped/overlapping windows, and requires the
computed window to equal the frozen window. It directly invokes nautilus-analysis 0.63.0
ReturnsAverage, ReturnsVolatility and SharpeRatio through PortfolioStatistic on complete
UTC-day returns. It never synthesizes account snapshots, fitting, orders or a numerical
engine. Calendar-day statistics explicitly use 365, not the simulation's 252 trading days.

qz.forward_evaluation/1 contains only version, original window and three native statistics;
no raw returns. Domain output shape and task binding verify methods, window and source time.
The metric adapter supplies exact Forward scope/frequency/period/count/original result
Artifact and native method metadata. Unavailable native values stay null with a failure
reason. This does not yet admit a Run, freeze a protected FORWARD InputSet, publish an
immutable Evaluation/window, or grant Live/Wake authority.

verify-1L5WH3 identified the missing Runtime materializer enum branch; it was implemented.
verify-K9RldK compiled but Clippy required a name for the metric tuple type; an alias fixed it.
verify-eBKdjr then passed all-target check/fmt/strict Clippy, 170 contracts/domain/Runtime
and 21 real job subprocess tests. The new test executes original plus corrected daily
reports, counts only the latest three samples, verifies native corrected mean and 365-day
metadata, rejects source/method substitution, missing days, unknown/partial reports
and extra mounts. Extending it with native constant returns proved zero mean/volatility
but unavailable Sharpe and Failed metric status (verify-e7tGle). After registering the new
result in the domain contract, verify-lWuTc0 reran all compile gates and the full Forward
subprocess test successfully. All verifier source inventories stayed unchanged.

web-verify-SyouCv reproduced all six generated outputs twice; NativeForwardResultV1 is
present in both domain and runtime schemas. TypeScript, 505 Vitest, 5 PWA tests and Vite
build passed (native-forward-web-*; existing chunk warning only). Numerical inputs remain
controlled fixtures, not actual multi-day downstream market feedback or OCI acceptance.
No push, review request, merge or Issue closure is justified by this intermediate slice.

## Forward return frequency binding, 2026-09-14

ForwardReportContentV1 now carries optional REPORTED_OBSERVATION / UTC_DAY frequency.
Absent metadata stays unknown and preserves the original absent-field encoding; it is
not inferred from timestamps or relabeled daily. UTC_DAY reports use completed UTC days
with right-end midnight samples, midnight window bounds, and exactly one valid sample
per covered day when complete. Partial reports retain gaps without qualifying them.
All original versions in a stream must retain one frequency; a changed correction yields
FREQUENCY_MISMATCH, unknown projected frequency and zero eligible observations.
Frequency remains issuer metadata, not proof of independence or native evaluation.

verify-bU5nrO on 530b0e0c plus this patch passed all-target check, format, strict Clippy
and 195 tests: 143 contracts/domain, 28 PostgreSQL and 24 HTTP/CLI. Tests cover short
windows falsely labeled daily, missing/intermediate/final days, partial gaps, absent
metadata round trip, original report replay and a native correction changing frequency
which invalidates a previously complete four-observation stream. Sources stayed unchanged;
the verifier stopped its isolated PostgreSQL. These controlled protocol fixtures do not
prove actual multi-day market feedback or statistical qualification.

web-verify-f5byn1 reproduced all six generated outputs twice with handwriting unchanged.
forward-frequency-web-* passed TypeScript, 505 Vitest tests, 5 PWA tests and Vite build
(existing chunk-size warning only). Native ForwardEvaluate admission, protected InputSet
freezing, scientific results, immutable evaluation/window publication and Live/Wake
consumers remain unimplemented; this patch does not complete #62/#63.

## Original corrected Forward window projection, 2026-09-14

GET /api/v2/handoffs/{id}/forward-window and CLI forward window inspect one original
Handoff/stream through existing Forward read authority. The Store holds the shared
Project publication barrier, reads all bounded original reports, and checks the exact
original external-ID FORWARD_SUBMIT receipt using the existing native unique lookup.
Original report bytes, native Claim/project/downstream/Release/environment/window/count
bindings must agree. Invalid or missing provenance is rejected, never silently filtered.

The domain selector verifies each original revision chain and takes its latest version.
Only complete, sequence-contiguous, adjoining nonoverlapping windows with unique sample
times expose eligible observations. Partial/missing returns, sequence/time gaps, overlaps
or repeated timestamps clear eligible returns/counts and expose fixed metadata reasons.
An empty stream reports NO_MESSAGES. A later partial correction cannot fall back to its
old complete version. The public view never serializes raw returns; internal selection
retains original eligible returns for the still-pending native ForwardEvaluate producer.
Capacity is bounded to 10000 original messages, 64MiB and one million points, rejecting
excess history rather than truncating it into an apparently complete window.

verify-zHkSYd over 26e2fdc0 plus this patch passed all-target check/fmt/strict Clippy and
195 checks (143 contracts/domain, 28 PostgreSQL, 24 HTTP/CLI). The original native Claim/
ACK/Forward chain now checks partial/empty sources, latest complete correction, a late
sequence filling an actual gap, four exact original observations without alias duplication,
a partial correction invalidating the previous complete window, overlap/sample-duplicate
reasons and rejection of substituted report content. All reports/messages are published
through native admission, not fixture SQL. After tightening receipt lookup to the exact
original message key, final verify-ExmeYN repeated all compile gates and passed that
complete original chain. Sources stayed unchanged and owned PostgreSQL stopped.
web-verify-jnLXiU reproduced all six native generated outputs twice without handwritten
changes. forward-window-web-* passed TypeScript, 505 Vitest cases, five PWA checks and
Vite build (existing chunk warning only).
This is source selection/diagnosis, not an immutable Forward Evaluation/window record,
statistical qualification, Live promotion or Wake. Native metrics/persistence, those
consumers, delivery UI and actual market/model/OCI/full #62 acceptance remain unfinished.

## Original Forward report ingestion and logical replay, 2026-09-14

Store/HTTP/CLI now ingest ForwardMessageSubmitV1 after the exact native Claim/transfer,
using the authenticated project/downstream FORWARD_SUBMIT identity and matching header/
external message ID. New messages require CLAIMED/ACKNOWLEDGED and current enabled
integration/environment. Original report bytes are bounded, typed, microsecond-window
validated and stored as REAL EVALUATOR_ONLY qz.forward_report/1 IMPORT artifacts; Paper
and Live remain explicitly distinguished. Only native references enter normal receipts.
Project/Handoff/downstream locks serialize publication, ACK and original external aliases.
The shared Forward artifact cleanup waits on the producer's project barrier.

Same external ID or a different external ID for identical logical content returns the
original message after checking original report bytes. Reused IDs with changed content
conflict; a correction must follow the current same-stream/sequence version and retain
its window. Old reports remain immutable. Metadata pagination is exact-project scoped;
downstreams see only their own messages and cannot submit account/ledger/control fields.
Neither metadata nor the ordinary Artifact content route exposes evaluator-only reports.
This creates no Forward evaluation/window, automatic Live promotion, degradation or Wake.

Over 6d386e4f plus this patch, verify-rNJ5Ar passed all-target check/fmt/strict Clippy.
Broad verify-rzoWt0 passed those gates, 143 contracts/domain and 28 PostgreSQL tests,
six HTTP contract/weights tests and 17 existing tests in the original portfolio suite.
Its new Forward chain reached successful publication/replay/correction but a test wrongly
matched the word returns inside the stream name when checking for raw-field disclosure.
The assertion now checks JSON keys and also rejects ordinary Artifact content access.
Final verify-CHfL5n repeated all compile gates and passed the complete native Claim/ACK/
Forward CLI chain: concurrency, original aliases, changed-ID/content conflicts, wrong
claim/project/header, microsecond precision, correction fork/missing parent, immutable
report bytes, metadata pagination, rollback on injected SQL failure and original retry.
Both retained source inventory and stopped their owned PostgreSQL. The earlier missing
CLI idempotency key was fixed without relaxing the existing CLI requirement.
web-verify-HtYAMn reproduced six native generated outputs twice without handwritten
changes. forward-web-* passed TypeScript, 505 Vitest cases, five PWA checks and Vite
build; only the existing chunk-size warning remains.
These are controlled protocol observations, not actual market/model/OCI acceptance.
Forward window/metrics/promotion/Wake, delivery UI and full #62 acceptance remain open.

## Original distinct-Candidate automatic daily quota, 2026-09-14

The automatic Paper protocol fixture now takes the second original LAST_TARGET
Candidate produced by the real Build admission/publication chain. A shared native
Study helper publishes its own immutable evaluation, and the normal Release producer
creates the second Package. No qualification, Candidate, evaluation or Release is
inserted by fixture SQL. Reauthorizing the same downstream with daily limit 1 rejects
that distinct Candidate with automation_daily_quota and leaves no Approval. Explicit
reauthorization with limit 2 then produces its exact original Release Offer, proving
the prior rejection was quota admission rather than invalid source evidence.

Final verify-HkVu6Q over 65b5b6b9 plus this test patch passed all-target check/fmt/strict
Clippy and all 236 tests, with unchanged source and owned PostgreSQL stopped. This
closes the distinct-Candidate rejection gap recorded below. The reused Study helper
uses Candidate-specific native command keys; no product behavior or public schema
changed. Controlled protocol evidence remains separate from actual market/model/OCI
acceptance. Forward/promotion/Wake, UI and full #62 delivery remain outstanding.

## Atomic automatic Paper and scoped Offer discovery, 2026-09-14

Trusted Worker consumes an ACTIVE Project's current original human-authorized policy.
AUTO_PAPER and AUTO_HANDOFF both start with Paper; absent original Forward promotion
proof, Live consumption is refused. Shared human admission supplies source/license,
qualification, original report references, decision ordinal and fresh downstream checks.
Original FROZEN_POLICY Approval and Offer commit atomically. Replacing/revoking/disabling
the policy prevents future claims while preserving prior transfers and exact replay.
A native POLICY_AUTHORIZE receipt must match the complete frozen policy. UTC-day quota
counts distinct original Candidates across all project/downstream offers; replacing the
policy does not reset it. Candidate Paper offers are not duplicated by another Release.

GET /api/v2/projects/{id}/handoffs and CLI handoff list use original ID pagination.
Downstream Claim/Ack identities see only their own project/downstream history; Operator
and exact-project ResearchRead CLI may inspect project history. Reading grants no claim.

Over c09767d1 plus this patch, verify-nlq3z8 and final verify-HlPzxG passed all-target
check/fmt/strict Clippy and 236 checks (142 contracts/domain, 49 PostgreSQL, 45 HTTP/CLI/
Worker). Native tests cover rollback of Approval/Offer/evidence on insertion failure,
two actual Workers producing one Offer, direct concurrency, revoke-before-claim,
original transferred replay after revocation, replacement-policy duplicate prevention,
scoped list isolation and real CLI list/Claim/ACK. Sources stayed unchanged and owned
PostgreSQL stopped. A second distinct Candidate quota-rejection scenario remains to be
added; the current tests do not claim that case. These are controlled protocol fixtures,
not actual market/model/OCI acceptance. web-verify-4VsEA3 reproduced six generated
outputs twice without changing handwritten source; auto-paper-web-* passed TypeScript,
505 Vitest cases, five PWA checks and Vite build (existing chunk warning only).
Forward/promotion/degradation/Wake, delivery UI
and full product acceptance remain unfinished; no push/review/merge/Issue closure.

## Trusted Worker downstream observation refresh, 2026-09-14

Worker reuses the original downstream transport, secret resolution, strict capability
contract and immutable observation publisher. It uses an independent deployment
DOWNSTREAM_TARGETS allowlist, never Runtime targets or an Agent/Operator impersonation.
It reserves at most one probe locally; migration 067's per-downstream native reservation
serializes multiple Workers. Eligible work is an unexpired OFFERED or an ACTIVE Project's
current effective, unrevoked, enabled automatic policy. Fresh observations are retained;
refresh begins with at most 15 seconds remaining, adoption ends after 20 seconds and
retry is no earlier than 30 seconds after start. Original observation TTL stays 60 seconds.
Preparation/commit recheck native configuration and reservation, and network I/O remains
outside the transactions. The publisher marks Worker artifacts RUNTIME and creates no
approval/Offer/Claim. Shutdown stops acquiring probes and waits for bounded active I/O.

Cleanup now waits for the same downstream row held by both publication paths before
checking native artifact references. This closes a race where an uncertain Worker
commit could bypass the old Operator-only cleanup lock. Final database-clock checks
also reject observation/receipt inserts delayed past the adoption deadline.

Over 97e363f7 plus this patch, verify-vKu3Hd passed all-target check/fmt/strict Clippy and
233 tests (142 contracts/domain, 47 PG, 44 HTTP/CLI/Worker), including real original
Package -> Offer -> two Workers -> pinned TCP -> refreshed observation -> Claim/ACK.
The fixture waits for the original immutable freshness window; only mutable reservation
fields are changed for failure/lost-lease injection. Exactly one actual HTTP request is
observed across two Workers, failed publication leaves no observation, a lost lease
cannot publish and the native RUNTIME observation retains its original 60-second TTL.

After the cleanup/deadline fixes, final verify-oHAqrh passed check/fmt/strict Clippy and
25 targeted tests: six PostgreSQL downstream checks, 18 manual/native-transport/Worker
HTTP tests, and the original Package/dual-Worker/Claim chain. A real transaction races
cleanup against publication and preserves the committed reference; a 21-second SQL
trigger delay rolls back observation, artifact metadata and receipt. Sources stayed
unchanged and owned PostgreSQL stopped. Public HTTP schemas and frontend are unchanged.
These controlled native protocols are not actual market/model/OCI science acceptance.
Frozen-policy approval consumption, Forward/promotion/Wake and full product acceptance
remain unfinished; no push, review request, merge or Issue closure occurred here.

## Operator-frozen automation policy management, 2026-09-14

Native Store/HTTP/CLI management now reuses the original immutable automation_policies
and append-only policy_revocations. POLICY_AUTHORIZE binds the original Project and
complete request, checks Project revision, same-project Mandate and enabled downstream
environments, freezes strict formal metric criteria and switches current policy in one
transaction. It does not create approvals or delivery evidence. POLICY_REVOKE binds the
original policy and complete reason/time/CAS, remains available after archival, and
preserves original versions; consumers must check the earliest effective revocation.
List/show/revocations use exact-project research authority and original native pagination.

Over 14ea7325 plus this patch, verify-XBcozg passed workspace check/fmt/strict Clippy,
142 contracts/domain, 47 PostgreSQL and 43 existing HTTP/CLI/Worker checks. The new CLI
check exposed a fixture comparing nanosecond input with PostgreSQL microsecond time;
the fixture now obtains its deadline from the database clock and waits for the next real
TOTP window for its second human authorization. No authentication rule was weakened.
verify-16hdCv repeated check/fmt/strict Clippy and passed the corrected native CLI test:
exact human intent, original concurrent-safe receipts, current Project CAS/pointer,
unchanged old versions, pagination, revocation replay, stale revocation CAS, database
immutability, archival revocation and absence of automatically created approvals.
Both runs retained source inventory and stopped their owned PostgreSQL instances.

`.ai-bridge/web-verify-lGH6Sp` reproduced six generated outputs twice, handwritten
sources unchanged. automation-web-* verifies TypeScript, 505 Vitest cases, five PWA
checks and Vite build; the existing chunk warning remains.

This verifies policy management only. FROZEN_POLICY automatic admission, original
Forward evidence/promotion/degradation/Wake, delivery UI, actual science/model/OCI and
complete T01–T42/main migration/recovery/current-head review remain outstanding.

## Original downstream ACK and immutable approval revocation, 2026-09-14

HTTP/CLI ACK uses exact DOWNSTREAM_ACK project/downstream identity and the original
claim ID. It reuses native command receipts, accepts one terminal outcome and replays
the original result; changed IDs/content cannot rewrite it. An unclaimed Offer can only
be rejected before expiry. A claimed Package can receive late acknowledgement after
approval revocation without new source reads, delivery authority or execution controls.

HTTP/CLI approval revoke requires the exact APPROVAL_REVOKE human intent, appends an
immutable reason/effective time with latest-record CAS, and never postpones an earlier
revocation. Historical query supports native pagination. It can revoke archived projects;
only unclaimed Offers change to REVOKED. Worker maintenance reconciles due revocations
and expiry with one bounded SKIP LOCKED statement. Claim takes the shared project,
Candidate, downstream and approval lock order and independently checks effective time.
Migration 066 preserves old nullable reason codes and adds the native grant operation.

`.ai-bridge/verify-f1HGov` over 58ec17c1 plus this patch passed all-target check, fmt,
strict Clippy and 232 tests (142 contracts/domain, 47 PostgreSQL, 43 HTTP/CLI/Worker).
It includes original qualified Package chains, concurrent claim/revocation, scheduled
revocation despite a later appended date, preclaim rejection without transfer, concurrent
ACK/revocation replay, exact/foreign identities, preserved transfer history, native
CLI/TCP ACK and exact human revoke grants. Sources stayed unchanged; the owned PostgreSQL
instance stopped. Protocol fixtures do not prove actual market/model/OCI acceptance.

`.ai-bridge/web-verify-TpK0dP` reproduced all six generated outputs twice with
handwritten sources unchanged. ack-web-* logs verify TypeScript, 505 Vitest cases,
5 PWA checks and Vite build; only the existing chunk-size warning remains.

Frozen-policy consumption, delivery UI, Forward feedback/promotion/Wake and full
T01–T42/migration/recovery/current-head review remain incomplete; this is not a merge
or Issue closure claim.

## Native downstream Claim and unclaimed expiry, 2026-09-14

POST /api/v2/handoffs/{id}/claim and CLI handoff claim accept the original downstream's
DOWNSTREAM_CLAIM credential, exact project binding and native external_claim_id.
Idempotency-Key equals that ID; the receipt scope is the original downstream. Claim
reuses Offer's approval/source admission, requires an unclaimed current Offer and
matching package version, and rechecks authority/readiness/time after the native SQL
transition. Existing triggers record the actual database claim time and immutable
transfer; state, transfer and original receipt roll back together. The response contains
the original TargetPackage, not arbitrary artifact access or execution controls.

Same-ID replay returns the original transfer/package without new source reads or a
new claim; current machine authority is still required. Another Offer cannot reuse the
ID, and another ID cannot reclaim an already transferred Offer. Migration 065 adds
native downstream/external-claim uniqueness and a partial pending-expiry index. Incompatible
history is not rewritten. The trusted Worker loop calls a single bounded SKIP LOCKED
SQL statement to expire up to 128 unclaimed overdue offers; claimed rows are untouched.
Claim independently checks the database clock, including when maintenance runs late.

Final verification over b0118f88: `.ai-bridge/verify-fW0lK7` passed workspace all-target
check, fmt, strict Clippy and 231 tests: 142 contracts/domain, 47 PostgreSQL and 42
HTTP/CLI/qualification-chain/OpenAPI/Worker checks. Source inventory unchanged; owned
PostgreSQL stopped with pg_ctl status 3. Original native qualification fixtures extend
through approved offers to real Store claims, testing rollback on transfer-write failure,
concurrent same-ID replay, revoked offers, foreign/Operator identities, duplicate IDs,
original package binding, expiry/claim competition and preservation of prior transfers.
The additional real CLI/TCP/HTTP/PG test uses a native encrypted machine verifier and
original ArtifactStore; it obtains the original Package, replays it once, rejects a
second claim and reads current CLAIMED state. Twelve existing Worker loop/recovery tests
also pass after the maintenance call was added.

An early run rejected a receipt missing its required schema_version; the writer was
fixed without weakening app.document. The first HTTP fixture selected the general
research policy and correctly produced INCONCLUSIVE. It now reuses the already registered
Release protocol fixture's policy and REAL-source scenario; no production threshold,
qualification or PASS was changed or authored in SQL. These controlled protocols still
do not prove actual market/model/OCI science or production delivery acceptance.

`.ai-bridge/web-verify-QNteJP` reproduced six native generated outputs twice, handwritten
sources unchanged. claim-web-* logs show typecheck, 505 Vitest cases, 5 PWA checks and
Vite build passed, with only the existing chunk-size warning. UI source is unchanged.
ACK, explicit approval revocation and its claim race, frozen policies, UI and full
T01–T42/main migration/recovery remain unfinished. Live GitHub read still showed PR63
OPEN/Draft at 37e5713e and Issue62 OPEN; no push, review request, merge or closure here.

## Human approval consumption into Offer, 2026-09-14

POST /api/v2/handoffs / CLI handoff offer now consumes an exact HANDOFF_OFFER
human grant bound to the original approval and complete request. GET /handoffs/{id}
under /api/v2 and CLI handoff show read current state. Source revalidation is shared
with approval creation: original REAL Package, all qualified sources, licensing and
time windows are reconstructed before checking the approval's immutable downstream
revision, decision ordinal, evidence and revocations against fresh native readiness.
The current human route rejects FROZEN_POLICY authority pending its policy consumer.

Native downstream/project/Candidate/approval locks protect admission and sequence.
Each Release/downstream/environment can be offered only once, including a different
key or approval. Supersession must name the latest matching project/mandate/downstream/
environment offer. An unclaimed predecessor is revoked in the same transaction;
claimed predecessors retain their transfer facts. Migration 064 adds the immutable
predecessor FK and original-version uniqueness, failing on incompatible duplicate
history rather than altering it. Original creation replay does not change current
state or renew delivery. This implementation does not create any transfer record.

Verification over 40dc9645: `.ai-bridge/verify-g4ULV5` passed workspace all-target check,
fmt and strict Clippy; 142 contracts/domain, 47 PostgreSQL constraints/evidence/
research/downstream and 29 HTTP/CLI/qualified-chain/OpenAPI tests. Source inventory
unchanged; owned PostgreSQL stopped and pg_ctl status was 3. The original qualified
Release fixture extends through approval to Offer without SQL-authored PASS, approval
or successful offer. It checks old decision bindings, expiry, release mismatch,
concurrent replay, duplicate versions under new keys/approvals, explicit supersession,
rollback of predecessor revocation, immutable bindings, no transfer fabrication,
new failed readiness blocking an old successful approval, new successful same-revision
probe acceptance, exact downstream read authority and denial of downstream offer power.
The CLI checks exact grant/request binding through real HTTP and native child process.
Earlier compile failed on an unnecessary .into() ambiguity in the CLI route; fixed.

`.ai-bridge/web-verify-VeuZfc` reproduced six native outputs twice with handwriting
unchanged. handoff-web-* logs record passing typecheck, 505 Vitest, 5 PWA tests and
Vite build; the existing chunk-size warning remains. Source UI is unchanged.

Claim/ACK, explicit revoke/expire races and original Package transfer, frozen-policy
consumption, UI, actual native-market/model/OCI acceptance and the full T01–T42/main
migration/recovery/current-head GitHub gates remain incomplete. Protocol fixtures and
an OFFERED state do not prove downstream transfer or execution. No push/merge/closure.

## Original Release human approval, 2026-09-14

POST /api/v2/releases/{id}/approvals and CLI release approve now consume the exact
RELEASE_APPROVE human intent; GET /api/v2/approvals/{id} / CLI approval show read
immutable metadata under original project authority. Approval reconstructs the
original REAL Package from current qualified sources, checks PAPER/LIVE licensing,
expiry, latest Candidate decision CAS and fresh downstream configuration/capability
observations. It atomically freezes the original evaluation report/method references
as a PORTFOLIO InputSet; callers cannot replace that evidence or use its restricted
reports as ordinary executable inputs. No report bytes are copied or exposed.

Migration 063 records original downstream revision, decision ordinal and probe ID.
Existing approvals retain null bindings and historical meaning; future Offer/Claim
must reject them. Reopening increments the decision ordinal and requires new approval.
The original probe ID is audit evidence, not a permanent readiness capability.

Verification on the approval changes over a580e910:
- `.ai-bridge/verify-i0vLvY`: workspace all-target check, fmt and strict Clippy pass;
  142 contracts/domain tests, 21 PostgreSQL research/evidence/downstream tests,
  28 HTTP/CLI/qualification-chain/OpenAPI tests pass. Source inventory unchanged;
  owned PostgreSQL stopped (pg_ctl status 3).
- The original qualified Release test covers source licensing, missing readiness,
  evidence rollback on rejected insert, concurrent same-key replay, same-Candidate
  rejection across sibling Releases, REOPEN/CAS, immutable old approvals, expiry
  and downstream revision changes. Fixtures model protocol inputs explicitly;
  no SQL-authored approval or PASS substitutes for this admission path.
- Earlier CLI fixture used a numeric revision and correctly failed CLI_INPUT_INVALID;
  it now sends the contract's string. The positive chain exposed reuse of an
  evaluator-only reader for a DELIVERY Package. The reader now derives the exact
  access class from its internal schema argument; evaluator schemas still require
  EVALUATOR_ONLY. Both focused checks passed in verify-GtPAKk before full regression.
- `.ai-bridge/web-verify-Paw2XF`: six native generated files reproduced twice with
  handwritten sources unchanged. Typecheck, 505 Vitest cases, 5 PWA checks and Vite
  build passed (approval-web-* logs); only the existing chunk-size warning remains.

This verifies the implemented human admission slice. Automatic policies, Offer/Claim
consumption and races, approval UI, real market/model/OCI acceptance, complete
T01–T42, migration/recovery and current-head GitHub review/CI remain unfinished.
No push, merge or Issue closure is represented by this evidence.

## Downstream observations and Operator probe/readiness, 2026-09-14

POST /api/v2/integrations/downstreams/{id}/probe and GET /readiness now connect
the native downstream client to immutable qz.downstream_probe/1 ArtifactStore
objects, PostgreSQL observations and original Operator receipts. CLI downstream
probe/readiness uses those same DTOs. DOWNSTREAM_PROBE is an exact-target human
grant; DOCTOR_READ permits the pure read only. Serve has a separate, default-empty
DOWNSTREAM_TARGETS deployment allowlist using the existing native target parser.

Preparation and completion use short authority/configuration transactions around
native network I/O. Completion rechecks authority, revision, enabled state and the
20-second acceptance deadline after local publication. Observations expire at
started_at+60 seconds; metadata publication leaves configuration revision intact.
Latest means latest probe start, so an older late success cannot mask newer failure.
Readiness takes configured/observed package and environment intersections, rejects
maintenance or empty intersections, and returns stale after expiry/config changes.
Old response timestamps are classified as unavailable at the native boundary;
Store independently checks their binding to its original ticket. Replays preserve
original expiry. No approval, delivery authority or scientific qualification is
granted by these observations.

Final verify-05UOGH passed workspace check, formatting, strict Clippy, 142 contracts/
domain tests, 11 real PostgreSQL tests and 36 HTTP/CLI/TCP/TLS/OpenAPI checks with
source unchanged. The real-time expiry test waits for the actual 60-second window;
other checks cover same-key racing, newer failure ordering, revision changes,
maintenance, empty intersections, immutable rows and publication/deadline rollback.
Native TOTP/AEAD/CLI tests use the exact human grant, fail an actual database insert
after native file publication, verify cleanup, retry once, replay without another
request, and read the actual original artifact bytes. A separate case proves the
Runtime allowlist cannot authorize downstream traffic. All remote content remains
controlled protocol fixtures, not production downstream/account/market acceptance.
The verifier-owned PostgreSQL instance was confirmed stopped.

Initial checks found a test DbCounter construction error, a shared test-helper
warning, and duplicate OpenAPI operation IDs that confused Runtime frontend types.
These were fixed at source; existing global operation-ID/reference tests are now
included in the focused verifier. web-verify-jFUbs3 regenerated all six outputs
twice identically with handwritten source unchanged. Frontend typecheck, 505 Vitest
tests, five PWA tests and Vite build passed; only the existing chunk-size warning
remains. No UI layout changed and no browser run is claimed for this stage.

Actual Approval/Offer/Claim consumption, decision-based invalidation of old
approvals after REOPEN, automatic-policy readiness refresh, downstream UI and full
delivery/feedback/native acceptance remain outstanding. No push, review, merge or
Issue closure was performed.

## Native downstream capability transport, 2026-09-14

DownstreamCapabilitiesV1 defines the strict target-only capability response at
GET /downstream/v1/capabilities. The native client preserves accepting_targets=false,
validates package/environment/market versions and timestamp bounds, and shares
the existing restricted reqwest construction without exposing Runtime job methods
on DownstreamTransport. No dependencies or configuration/credential permissions
were added. The caller must resolve the DOWNSTREAM vault reference and supply a
deployment-owned allowlist; this internal method is not an Agent tool or user API.

verify-4N4qqo passed workspace check, formatting and strict Clippy with source
unchanged. Final downstream-domain logs show 142 contracts/domain tests passed;
downstream-final-transport shows 19 real TCP/TLS tests passed, including four new
downstream checks and the original Runtime capability/job transport regressions.
Coverage includes exact wire values, maintenance, invalid versions/environments,
duplicate fields/values, timestamp boundaries, credential reflection, bounded
chunked responses, authentication, denied destinations and redirect non-forwarding.
All inputs remain controlled protocol fixtures, not real downstream acceptance.

web-verify-2svnXc regenerated all six native outputs twice identically with
handwritten files unchanged. Only domain-v1.openapi.json changed; HTTP API and
frontend generated artifacts did not, so frontend/browser checks were not rerun.
Downstream observation persistence, probe/readiness HTTP/CLI, current-revision
transactional admission and Approval/Offer/Claim remain outstanding. No readiness,
approval, handoff, push, review, merge or Issue closure is claimed by this stage.

## Candidate-scoped human decision history, 2026-09-14

POST /api/v2/releases/{id}/rejections, POST /api/v2/release-decisions/{id}/reopen
and GET /api/v2/releases/{id}/decisions now share the original Operator command
receipts and native Candidate row lock. CLI release reject/reconsider/decisions
use the same strict DTOs. Reasons are bounded, decisions retain original creation
and decision times, and history includes sibling Releases of the same Candidate.
REOPEN requires the exact latest REJECT; a stale expected ID returns Conflict.
The existing immutable table and grant operation constraints were reused without
rewriting migrations, adding dependencies or creating a workflow framework.

verify-n6JTbs passed workspace check, formatting, strict Clippy and 22 real PG/TCP
checks with source unchanged. Two new CLI tests verify exact human grants and
changed-body rejection. Original controlled Release tests now verify same-key
replay, cross-Release stale rejection, PAPER/LIVE separation, exact REOPEN,
competing different-key appends with one Candidate-wide winner, original history
pagination and native rejection of historical UPDATE. No Approval or Handoff was
created by these operations. Fixtures remain controlled protocol declarations,
not real-market/model acceptance. Earlier verify-7hUNO7 passed before the final
request schema bounds, creation time and native CLI tests were added.

All 142 contracts/domain tests passed. web-verify-IrIRxC generated all six allowed
contracts twice identically with handwriting unchanged. Frontend typecheck,
505 Vitest tests, five PWA tests and Vite build passed. UI was not modified, so
browser tests were not rerun; existing chunk-size warning remains.

Approval/Offer/Claim consumption of this history remains to be implemented and
verified, including ensuring REOPEN never revives an earlier approval. Downstream
readiness, actual native positive Study, delivery/feedback/automation, outstanding
UI and complete acceptance remain open. No push, GitHub review, merge or closure.

## Release creation, original Package and HTTP/CLI, 2026-09-14

The resumed local owner retained the interrupted Release transaction and completed
POST /api/v2/releases, GET /api/v2/releases/{id}, and client release create/show.
The three-field intent binds the exact Candidate and PORTFOLIO Evaluation to the
existing RELEASE_CREATE Operator grant. Store assembles original target-only
bytes, rechecks source/authority after immutable publication, and commits Package,
Release and the original command receipt together. Readback preserves the version
and expiry; it does not grant approval or delivery. HTTP reuses existing command
ownership and confirmed-unreferenced object cleanup after failed publication.

A successful controlled protocol test exposed that the original PAPER weights
produce SYNTHETIC Candidates. Release now explicitly refuses that origin even
when independent Study passes. The successful test registers a LIVE test source
through the existing downstream API; it never changes historical origin rows.
Both branches still use controlled declarations, not real market or model proof.

verify-Y8fljk passed workspace check, formatting, strict Clippy and 20 real PG/TCP
checks with source unchanged: four native CLI authorization tests and 16 shared
HTTP/Store tests. New coverage verifies concurrent same-key publication, original
Package/readback, file-publication failure without a Release, changed-intent
rejection, referenced-object retention, scheduled source expiry during file write,
orphan cleanup and unchanged successful replay after source expiry. An earlier
fixture omitted the policy-wide sample minimum, correctly yielding INCONCLUSIVE;
it was fixed only in the explicit controlled test policy. The expiry path returns
the existing data-use domain error, which the assertion now accepts.

The final contracts/domain test run passed all 141 tests. web-verify-cUbmyo
reproduced all six allowed generated files twice, with handwritten sources
unchanged. Frontend typecheck, 505 Vitest tests, five PWA file tests and Vite build
passed; the existing large-chunk warning remains. No browser UI was changed and
this run did not repeat browser or native OCI acceptance.

This completes the tested Store publication boundary and HTTP/CLI wiring, not
native successful HTTP-to-Worker/OCI Study acceptance or the complete delivery
product. Approval, offer/claim, feedback/automation, remaining UI and all outstanding
acceptance work remain required. No GitHub review, push, merge or Issue closure.

DESIGN.md is the normative contract. This file records implementation and
version-bound evidence, not a second design or a claim that Issue #62 is complete.

## Target-only Package body and immutable binding, 2026-09-14

TargetPackageV1 now defines the strict version-one delivery body without order or
quantity fields. Its domain validator reuses the original Mandate validation and
exact Decimal types, binds project/Candidate/Mandate, capital, constraints,
tolerance, execution assumptions, current-weight source and qualifications, and
preserves the original target artifact's order, values and validity window.
Database Candidate targets are matched by instrument identity because the read
API orders those rows independently. Reference collections are bounded and unique.
This is structure and original-record binding, not current eligibility, production
provenance, independent PORTFOLIO/PASS, Package publication or Release approval.

On 35c321e8 plus the frozen patch, all contracts/domain tests passed (native
session 30522, exit 0), including 14 mutated package cases, forbidden target
fields and reordered/mutated database snapshots. Final verify-gm9EM6 passed
workspace check, format and strict Clippy with source unchanged. Earlier
verify-R4736y also passed before the snapshot-order correction; it is not the
final source evidence. web-verify-ZKFcFq regenerated only the domain OpenAPI twice
with identical bytes and unchanged handwritten sources. No browser, database,
OCI or full acceptance execution is claimed for this package-only change.
No dependencies, GitHub review requests, push, merge or Issue closure were added.
Release creation/publication, approval/delivery and the complete acceptance chain
remain required work, not a follow-up scope reduction.

## Actual Worker Study terminal publication and ACK, 2026-09-14

The positive HTTP Study test no longer calls the publication helper directly.
It consumes the original PGMQ message through Worker.process_message after a
pre-dispatch cancellation. ACK before publication fails; Worker publishes the
independent PORTFOLIO evidence and archives the original queue message. A second
call with that already archived message is rejected and the Evaluation count
remains one. Authenticated HTTP reads the resulting original evidence. Empty
RuntimeTargets ensure this path cannot contact a remote Runtime.

verify-MoG45A on 1f039ffc plus the frozen test patch passed workspace check,
formatting, strict Clippy and all 17 HTTP/CLI/shared-chain tests, source unchanged;
isolated PostgreSQL stopped. No production logic or dependencies changed. This is
real Worker terminal processing, not successful native Study execution through
Worker/OCI. That success chain, Release/Package/delivery and full acceptance remain
open. GitHub reread still showed PR63 Draft/Open on remote 37e5713e, not merged;
no GitHub write, review request, merge or Issue closure occurred.

## Positive original-candidate Study HTTP admission, 2026-09-14

portfolio_study_http reuses the original Store qualification/Candidate chain, then
performs real TCP HTTP requests with a native machine verifier and exact Operator
grant. No Candidate or qualification row is manually inserted. Two identical
requests receive 202 and the same Run with distinct fresh/replay flags. Cancelling
through Store and publishing its original terminal evidence yields a readable
PORTFOLIO/INCONCLUSIVE Evaluation over authenticated HTTP. The listener is owned
by a JoinSet and aborted on scope exit, including panic paths.

Existing compilation/forecast/validation/result-turn helpers were moved unchanged
to the existing shared experiment_tasks module. The new HTTP target also executes
the imported qualification/publication regressions; this is deliberate fixture
reuse, not additional production code or fake HTTP authentication.

Final verification on 0d328479 plus the frozen test patch:

- verify-x9edZ1: check/fmt/strict Clippy and 17 tests passed (three native CLI
  portfolio commands and 14 positive HTTP/shared chain tests), source unchanged.
- verify-wodFyg: original evidence suite passed, including Store and HTTP/CLI
  regressions after the helper move; check/fmt/Clippy passed, source unchanged.
- Initial compilation exposed implicit parent helpers; sharing the existing
  implementations fixed this. A subsequent Clippy-only failure was corrected
  before the final two passes. Both isolated PostgreSQL instances stopped.

Correction to the older verify-5qnJJj entry: evidence mode did not select
client_portfolio_build; it compiled that target but did not execute its new Study
test. Actual execution of that CLI test is now evidenced by verify-x9edZ1.
This stage does not execute the admitted Run through a real Worker/OCI chain.
The qualification fixture still uses controlled native reports and an empty Wasm
module; real Worker success requires actual compiled models and catalogs, not
those declarations. No REAL/PIT/PASS/T42 claim, push, review, merge or closure.

## Actual OCI Study output to independent metric adapter, 2026-09-14

The existing real Docker rolling Study test now sends the original downloaded
simulation request/result to the same domain metric adapter used by the Store
publisher. All three metric values must exactly equal Nautilus Returns statistics,
retain the evaluation/source references, portfolio scope, native method version
and actual daily observation count. Reusing the result with a different starting
capital must fail. No statistics or qualification records are hand-filled.

On 4534f084 plus this test patch, verify-Mx31Wx passed workspace/all-target check,
format and strict Clippy with source unchanged. owner-oci-vFYXba passed actual OCI
acceptance with source unchanged and image
sha256:2690f1ac7972a6b3e37c53ffc528ce4ade9dee93951f33992323daf5d9fd426b.
This connects actual native output to the publication metric adapter, not the full
Operator HTTP/Store/Worker acceptance chain. The catalog remains controlled and
does not establish REAL/PIT qualification or T42. No production logic/dependency
changes, push, review request, merge or Issue closure occurred in this stage.

## Explicit Study submission UI, 2026-09-14

Candidate detail now offers an explicit Study request, retaining the original
Candidate and reading its Mandate and frozen policy plan. The user selects a
running project Cycle, registered enabled Runtime and bounded resource limits;
the form reads the Runtime revision and submits only the existing six-field DTO.
It exposes no source/window/member/fee overrides. Store remains the authoritative
admission check, including original Runtime, source, policy, capability and budget.

The existing Intent, ResourceSelect, guard, counters, error display and RunDetail
are reused. Missing plans, stale/failed source reads and offline state disable new
submission. Unknown results freeze the original request and idempotency key,
including Runtime revision; a 202 receipt is labeled registration, not PASS or
delivery permission. Closing an unknown request warns that it does not cancel it.

web-verify-vEEgI2 on 6f432f2b plus the frozen UI patch passed: all six allowed
native generations repeated identically with handwritten sources unchanged,
typecheck, unit tests, build, settings browser suite and all-site browser suite.
New three-viewport tests cover missing plans/no writes, offline disabling, exact
large counters, lost response and identical retry despite Runtime revision change,
and accessibility. First standalone typecheck caught a test-only comparator typo
(GTE instead of contractual GE); the fixture was corrected before final validation.
These controlled browser records are not real market/native execution acceptance.
No Rust/native engine changes, new dependencies, push, review, merge or closure.
Positive HTTP-to-native Study acceptance and remaining delivery contracts continue.

## Formal Study HTTP/CLI and original evaluation views, 2026-09-14

POST /api/v2/portfolio-studies and client portfolio study submit the existing
strict six-field intent. PORTFOLIO_STUDY is a distinct OperatorCommand using the
existing PORTFOLIO_SIMULATE permission, exact grant normalization, idempotency,
budget and object cleanup. No plan, model, input or fee overrides are accepted.
Candidate evaluation list/detail now accept original PORTFOLIO as well as FORWARD
records and explicitly display their type; ownership checks remain unchanged.

Verified on fa5b2903 plus this patch, editing and verification serial:

- verify-5qnJJj: workspace check, fmt, strict Clippy and complete evidence suite
  passed with source unchanged. The added native CLI/TCP/PostgreSQL Study test
  was compiled here but not selected by evidence mode; its actual execution is
  recorded in verify-x9edZ1 above. It checks exact human intent, missing Candidate
  rejection, changed-intent denial and no Run creation, not full native acceptance.
- web-verify-P6DBlJ: native contract generation repeated byte-identically for
  all six allowed outputs with handwritten sources unchanged; typecheck, unit
  tests, build, settings browser checks and all 249 three-viewport browser tests
  passed. Both PORTFOLIO and FORWARD preserve original metrics, missing values
  and expiry and reject an unrelated Candidate in detail.
- cargo test --locked -p server --test request_schema_bounds --test
  openapi_operation_ids --test http_openapi_references: all seven tests passed.

No new dependencies or numerical computation. Study submission UI, positive
HTTP-to-native full-chain acceptance and remaining delivery contracts still need
completion; controlled UI/CLI fixtures do not prove scientific PASS or T42.
No push, Codex review request, merge or Issue closure in this stage.

## Independent portfolio Study publication, 2026-09-14

The existing candidate publisher now handles original PORTFOLIO/STUDY bindings
alongside FORWARD/HOLD. It verifies the accepted native task, terminal receipt,
three original reports and Arrow history, then maps native simulation metrics to
the frozen independent policy. Cancellation, infeasible allocation and missing
daily observations cannot become PASS. Published receipts replay without reading
expired sources; concurrent publication has one result. Source qualification and
data-grant deadlines, including scheduled revocations, cap evidence validity and
are rechecked after report storage. Existing Evaluation reads expose these exact
original publications. No new queue, evaluation table or numerical engine exists.

Verified on b7e81cb6 plus the frozen publisher patch, with serial editing/testing:

- verify-BMx4rw: all four original qualified portfolio chains passed, including
  Study cancellation, exact Arrow corruption rejection, write failure, concurrent
  retry and grant expiry during publication with rollback and honest retry.
- verify-aPuEdk: all evidence checks passed, including 139 Store and 24 HTTP/CLI
  tests plus native validation and publication unit tests.
- verify-s7H85U: policy regression suite passed. All three verifiers passed
  workspace check, formatting and strict Clippy with unchanged source; isolated
  PostgreSQL was stopped. Only this evidence note changed after verification.

The debug-test stack overflow was removed by awaiting the old qualified chain
and Study stage sequentially while retaining original files/database; no thread
stack enlargement or production workaround was added. The Arrow corruption test
damages the footer checked by the native reader, not an ignored prefix byte.
Controlled native protocol results establish publication behavior, not actual
market performance, REAL/PIT qualification or T42 acceptance. Formal Study
HTTP/CLI/UI, remaining delivery contracts and full acceptance are still pending.
No push, review request, merge or Issue closure occurred in this stage.

## Original-policy portfolio Study admission, 2026-09-14

Store.start_portfolio_study now accepts only Cycle, Candidate, Runtime/revision
and bounded limits. It reads the original Mandate's policy-owned plan and full
Build cohort; no Run request can override inputs, cutoffs, models or fees. It
reuses Operator PORTFOLIO_SIMULATE authority, receipts, Cycle budget and PGMQ.
Migration 060 freezes the exact Run/Candidate/policy/dataset/intent binding.
ACK requires an independent PORTFOLIO publication for that binding, not HOLD.

Admission requires the original qualified models/image and current capabilities,
REAL/PIT/AS_KNOWN_THEN research data and the policy's exact full registered window.
A shorter native quality selection cannot silently trim the policy window.
Discovery, Validation and qualification-selection Sealed metadata determine the
latest research observation availability; Sealed data is never a runtime input.
The original Candidate supplies its cohort and Mandate, not historical targets,
current account state or a live-target TTL. Study starts at simulated cash with
zero asset weights. Original fee files, first-cutoff groups, registered calendar
and rolling-liquidity policy are checked; a single historical liquidity snapshot
cannot replace per-cutoff measurement. There is no new numerical engine or queue.

Original sources are reread after parameter publication, before transactional
admission. Final qualification/grant expiry and effective revocation checks have
no following file callback. Failed admission leaves no Run, charge, receipt or
artifact row; the existing unreferenced-object cleanup handles written files.

Verification on 539aa69a plus the frozen patch (initial test-only ownership/async
borrow compilation errors were corrected before the final runs):

- verify-vUbAuB: workspace check, fmt and strict Clippy; 139 Store, 24 HTTP/CLI,
  42 native-validation and four publication tests passed (209 total). Original
  qualified chains cover no-liquidity/rolling Study admission, snapshot rejection,
  shortened-window rejection, exact cohort/Sealed cutoff and input roles, concurrent
  same-key single admission, changed intent rejection, write/read-failure rollback
  and file cleanup, cancellation and the independent publication ACK gate.
- web-verify-LIhaGq: domain-only export twice, byte-identical, handwritten source
  unchanged. The generated Study request has exactly six required fields and
  additionalProperties=false. No HTTP/TypeScript/UI contract changed; browser
  tests and OCI image execution were not rerun for this Store-only capability.
- verify-Ru1qae: check/fmt/strict Clippy and 166 domain/Runtime, 20 managed,
  31 native-Codex, 36 policy/upgrade Store and six policy HTTP tests passed
  (259 total, overlapping the evidence suite). Both verifier source guards passed.

Relational/native declarations remain controlled fixtures, not real market/PIT,
actual Study numerical output or scientific PASS. Independent PORTFOLIO result
publication and formal HTTP/CLI/UI are still missing; cancelled Study currently
remains unacknowledged rather than being adopted through the HOLD publisher.
Release/Package/delivery, all remaining work packages and full acceptance remain
unfinished. GitHub reread: PR #63 still Draft/Open at 37e5713e, Issue #62 Open;
no push, review request, merge or closure occurred in this stage.

## Policy-owned immutable portfolio Study plans, 2026-09-14

EvaluationPolicy now optionally freezes portfolio_study_plan with the original
PORTFOLIO InputSet, evaluation start and optional complete manual cutoff list.
The policy must also define independent portfolio criteria. The plan selects
exactly one original research dataset; evaluation end remains its original end,
not a caller-selected second window. Start is strictly inside that range; manual
cutoffs are 2..256 strictly ordered times beginning at start and ending before
the source end. Times must be nonnegative, native-nanosecond-representable and
PostgreSQL-microsecond exact. Native schedule/TTL/model-availability/capability
checks still belong to formal admission, not policy registration.

Migration 059 stores the optional JSON and a native generated input ID with an
exact project/InputSet foreign key; existing immutable policy triggers apply.
Old and pure-Alpha policy rows stay null without inventing a plan. Policy creation
reuses its original receipt, lineage and family transaction. Comparison, extra
Sealed and Study sources are collected into the existing sorted source/Runtime/
grant lock pass; current time and committed revocations are checked after all
lock waits. The shared validator takes a slice rather than independently locking
and checking the new plan before the old comparison sources.

The real PostgreSQL test covers policy-owned input identity, wrong project/purpose,
ambiguous datasets, start/end/manual boundary errors, concurrent exact-key replay,
immutable plan rejection, changed-intent conflict, and no new consumption after
license revocation. Exact old receipt/readback survives revocation and no Run,
Evaluation or qualification is created. HTTP tests create and read the plan through
the original authenticated endpoints and preserve microsecond cutoff values.
The first policy run's HTTP test retained an obsolete single-input pagination
expectation; it now verifies both original inputs in descending order.

On 722912f4 plus frozen patches, final verify-acHR86 passed workspace check/fmt/
strict Clippy and 259 policy/domain/native/HTTP tests, including upgrade preservation.
web-verify-coxbqs reproduced all six generated outputs twice with handwritten source
unchanged, passed typecheck/build, 505 unit tests, 36 settings browser tests and all
243 browser tests in 5.2 minutes. The existing Ant Design form exposes the plan only
under independent portfolio criteria, retains exact time text, clears disabled
manual lists to null and retries the unchanged intent after a lost response.
Final verify-aplpbW passed another 209 research/publication/native/HTTP/CLI checks
and workspace check/fmt/strict Clippy. Test-family totals overlap. No native Job
operation changed, so OCI was not rerun for this policy-registration stage.

Ponytail reuse adds no dependency, new queue or generic workflow. Formal Study Run
admission, original-model availability binding and independent PORTFOLIO Evaluation
publication are still required, as are release/delivery/recovery and complete T42.
No push, review, merge or Issue closure occurred.

## Store InputPurpose versus original dataset partition, 2026-09-14

The shared dataset reader previously decoded InputSet.purpose as DataPartition,
which cannot represent PORTFOLIO. It now reads InputPurpose, requires the caller's
explicit purpose allowlist, then uses the same domain partition rule as InputSet
creation. PORTFOLIO retains original DISCOVERY/VALIDATION members; it is neither
a new partition nor an alias accepted by old callers. All existing callers were
updated with their original allowed purposes. Source metadata, version, origin,
event/available range, row count, immutable partition, frozen-input and license
rechecks remain unchanged. This is required source plumbing, not Study admission.

Ponytail reuse extracts the existing domain match and reuses the existing real
registration/file fixture, rather than duplicating a data adapter or fixture tree.
The new database case registers both research partitions under a research/paper
grant, creates an actual PORTFOLIO InputSet, reads exact original identities/roles
and cutoff, rejects research-only licensing, rejects unrequested purposes before
file reads, rejects changed original row counts and confirms standalone
DATA_VALIDATE cannot consume the new purpose or create a Run/Evaluation/qualification.
A domain matrix checks all five purposes against all four partitions.

On c4992e9b plus frozen patches, final verify-qAN5Ba passed 278 source/native/
domain/Store/HTTP/CLI tests, including both real source-binding cases in 4.40s.
verify-uXW9qF passed 209 research/publication/native-validation/HTTP/CLI tests;
verify-M4XVWu passed 234 Mandate/domain/science/Store/HTTP/CLI tests. All three
passed workspace check/fmt/strict Clippy with source unchanged; counts overlap.
Earlier attempts exposed a test async capture, duplicate fixture-module loading,
nanosecond rather than database-microsecond test time and stale fixture field
paths. The test now reuses the original fixture and database clock; no lint was
suppressed and no production validation was relaxed to make the test pass.

No external schema/UI/native task behavior changed, so generation/browser/OCI
were not rerun in this stage. Formal Study plan/admission and independent
PORTFOLIO Evaluation, release/delivery/recovery and complete T42 remain unfinished.
No push, review, merge or Issue closure occurred.

## Native Study research-partition binding, 2026-09-14

portfolio-study/6 corrects the native Study-only dataset role to DISCOVERY or
VALIDATION, matching the normative PORTFOLIO InputSet contract. FORWARD and SEALED
are rejected; existing Build, sequence and Candidate HOLD role checks are unchanged.
Runtime still compares the task role with the registered immutable partition and
checks the original catalog selection. No directory is relabeled at runtime and
no shared data-use license rule was weakened. Capability map, native stack/image
label and command documentation now identify version 6 rather than accepting an
old role contract as the new behavior. Ponytail reuse changes the existing task
branch; no new source adapter, dependency or compatibility mode.

On de9de6b0 plus frozen patches, initial verify-aU8ACV exposed two catalog-scope
tests still assigning FORWARD to Study; they failed their valid baseline before
reaching the intended range checks. Those fixtures now use VALIDATION for Study
while preserving other operations and every original cutoff/instrument/range
counterexample. Final verify-Is2ox4 passed workspace check/fmt/strict Clippy,
164 contracts/domain/Runtime tests, 20 managed tests, 31 native Codex tests and
118 Job science tests (the managed tests are included in the Job total).
The managed Study fixture executes both permitted research partitions and checks
that FORWARD and SEALED are rejected without changing the other source bindings.

owner-oci-jKBVGR built the new image and passed all 15 actual OCI tests in 39.93
seconds with source unchanged. The rolling Study scenario registers its original
catalog as VALIDATION, uses the matching task role and observes portfolio-study/6
in the resulting manifest; original model/calendar/liquidity/report assertions
remain in place. Image:
sha256:fcd7984c00a2b1b2b5bd086715002fcb6c7caef1c86159d7bb043028a6234f9c.
This verifies the native prerequisite, not a formal Store Study admission,
independent PORTFOLIO Evaluation/PASS, REAL/PIT or complete T42 delivery. No schema
or UI change, no generated/browser rerun, no push/review/merge/Issue closure.

## Store rolling Build admission and publication expiry, 2026-09-14

Build now rereads the original registered rolling policy through the existing
artifact/InputSet/Runtime/license checks, freezes its PARAMETERS input and requires
portfolio-build-rolling/1. Declared SYNTHETIC policy bytes do not become market
origin evidence. Historical snapshot handling remains unchanged. Publication
requires the original manifest capability and policy, binds the original report
measurements and checks their expiry against database time. The earliest BAR
event plus maximum age is rounded down to PostgreSQL microseconds and bounds the
existing final window query, without changing weights or target TTL contracts.
Corruption remains retryable Integrity; genuine expiry retains the solver status
but publishes INVALID with no usable target. Expiry across file publication rolls
back the transaction; a retry seals the expired result without targets.

On 65580f2a plus frozen patches, verify-bXpZVx passed workspace check/fmt/strict
Clippy and all four original qualified-portfolio database chains in 36.85 seconds.
The new rolling chain uses actual registration, review, qualifications, admission,
publication and ACK APIs with explicitly controlled numerical report bytes.
Changed policy files fail admission and publication. The expiry test writes a
target while current, waits across its age boundary, verifies transaction rollback,
reclaims both unreferenced callback objects and retries to OPTIMAL/INVALID with
no target/cash/target rows; replay reads and writes nothing. Existing no-liquidity
and snapshot chains pass. Initial runs exposed two incomplete test adaptations:
the policy-ID projection and the controlled report's capability map; both were
corrected before the final passing run, without relaxing production checks.

verify-HhmJ4t passed all 232 Mandate/domain/science/Store/HTTP/CLI checks;
verify-WFnFkL passed 209 research/publication/native-validation/HTTP/CLI checks.
Both also passed workspace check/fmt/strict Clippy with source unchanged. These
families overlap and are not a count of unique acceptance cases. No HTTP contract,
frontend, native Job or Runtime source changed in this stage, so generated assets,
browser tests and OCI were not rerun. Prior native OCI evidence remains separately
version-bound below, not a claim of a new end-to-end OCI Store acceptance run.

Ponytail reuse adds no dependency, migration, generic service or qualification
shortcut. Independent PORTFOLIO Study/Evaluation, REAL/PIT, release/delivery,
recovery and full T42 remain unfinished. No push, review, merge or Issue closure.

## Native Build consumption of rolling BAR policies, 2026-09-14

portfolio-build-rolling/1 adds the original rolling policy to native Build requests,
mutually exclusive with a historical report snapshot. The original PARAMETERS file
is reread; hand-supplied asset notionals are rejected. Build measures the original
selection with the same Nautilus BAR valuation used by Study/DATA_VALIDATE, records
bar_notionals and derives allocation assets through their shared identity/currency/
availability/age check. The existing cost adapter then applies original fees and
slippage. Result validation reconstructs those same assets and checks all original
input/solver bindings. No new numerical engine, dependency or per-BAR business rows.
The task whitelist only grants the bound policy's PARAMETERS role. Store continues
to refuse rolling-policy admission until formal source/publication handling is wired.

On 22ca2f75 plus frozen patches, verify-NCndbh passed workspace check/fmt/strict
Clippy, 164 contracts/domain/Runtime tests, 31 native Codex tests and all 118 Job
science tests. Its separate managed target passed 20 tests (also in the Job suite).
The new real subprocess test covers original file consumption, missing/replaced
policy, wrong input role, expired BAR, hand-supplied notional and changed report
values/currency/time. Study regression tests passed through the shared validator.
verify-PQNLAT passed both existing full qualified-portfolio chains in 14.85 seconds,
with unchanged source and isolated PostgreSQL stopped. web-verify-OpKS2e reproduced
the six allowed generated outputs twice with handwritten source unchanged; only
domain/runtime JSON changed, no HTTP schema or handwritten frontend change.

The first actual OCI run found the test's later snapshot scenario retained its
new rolling policy. The fixture now removes that policy and input when switching
modes, preserving production exclusivity. An added direct comparison initially
failed compilation because the report type lacks PartialEq; the existing serialized
value comparison was used without changing production types. Final owner-oci-fV3dqt
passed all 15 actual OCI tests in 39.82 seconds. Rolling Build measurements match
an independent DATA_VALIDATE container's original values, and the same scenario
also exercises slippage and historical-snapshot Build. Source remained unchanged.
Final verify-AMixOA passed workspace check/fmt/strict Clippy after these test fixes.
Image: sha256:aec50a3008877dad028c10b3af93a9987e719308cb32dc8215e48ccd3765dd66.

This proves native consumption with controlled market fixtures, not formal Store
rolling admission, current-age Candidate publication, independent PORTFOLIO PASS,
REAL/PIT or T42. Those and delivery/recovery remain required. No push, review,
merge or Issue closure occurred in this stage.

## Frozen rolling BAR liquidity policy registration, 2026-09-14

The existing execution-assumptions command accepts optional rolling_liquidity,
mutually exclusive with the original single-snapshot bar_liquidity. Migration 058
stores the declared policy without backfilling old rows; the existing immutable
liquidity artifact/participation fields bind its exact PARAMETERS file. It remains
SYNTHETIC/CONSERVATIVE_ASSUMPTION, not measured volume or DATA_BACKED. The command
requires current portfolio-rolling-liquidity/1 capability and preserves original
dataset/input/Runtime/fee/license binding. No expiry is invented for the policy:
each actual native cutoff still checks the age of its own historical observation.
The single-snapshot Build reader explicitly refuses rolling policies instead of
treating them as absent participation constraints. Formal Study admission and Build
rolling consumption remain separate required work, not implied by registration.

Ponytail reuse kept the existing command, transaction, native policy validator,
artifact storage and receipt. The callback can publish the fee and policy files;
HTTP cleanup now reclaims every allocated unreferenced file on failure. Original
CLI and Ant Design forms expose the same mutually exclusive modes and exact
decimal policy values. Neither mode switch is enabled offline or during submission.

On e3870bfa plus frozen patches, verify-XjMeXA passed workspace check/fmt/strict
Clippy and 231 tests (137 contracts/domain, 54 native science, 14 Store, 4 source/
publication units, 22 HTTP/CLI). New checks cover invalid/mixed policies, two-file
publication, exact original bytes, immutable rows, original receipt replay and an
actual PG failure after file writes with exact cleanup and same-key HTTP retry.
The native CLI consumed the rolling-policy request through real HTTP/PG. Source
remained unchanged and the isolated PostgreSQL cluster was confirmed stopped.

web-verify-JTBKl0 initially passed. Review then corrected explicit Checkbox disabled
props to retain offline/in-flight disabling and added browser assertions. Final
web-verify-z5B8dz reproduced all six generated outputs twice without handwritten
changes; typecheck, 505 unit tests, wire/build/help checks, 36 settings-browser and
237 full-browser tests passed, including no-policy/snapshot/rolling modes at three
viewports. owner-oci-9vADmB rebuilt and passed 15 actual OCI tests in 39.74 seconds
with unchanged source. Its subsequent changes were only the two web files above;
native source did not change. Image:
sha256:94040ad0a5ba9a05700ec0461ed4ef203b666ca1958b50910c68c02e6ac69eda.

Controlled catalog, provider and market fixtures do not establish REAL/PIT,
scientific PASS, full independent PORTFOLIO evaluation or T42. No push, review,
merge or Issue closure occurred; Build rolling consumption, formal Study, delivery
and recovery contracts remain required.

## Original calendar source registration, 2026-09-14

Dataset registration now consumes optional original Runtime calendar_sessions in
the existing immutable Universe metadata batch. Migration 057 adds a nullable
calendar_artifact_id; absent historical calendars remain absent. The existing
transaction, receipt, source license checks and unreferenced-file cleanup handle
the additional qz.calendar_sessions/1 PARAMETERS object. Universe reuse requires
identical presence and all original calendar values. Runtime portfolio-calendar/2
also compares the full registered table, rejecting substituted sessions even when
name and version match. Shared structural validation replaces duplicate checks;
no calendar service, holiday rules, dependency or Python runtime was introduced.

On d731a83a plus the frozen patch, verify-qC2Cz3 ran the full Store/server suite.
The new calendar tests passed; its sole failing target was research_review, whose
historical-policy assertion omitted the previously added nullable portfolio metric
column. The assertion now explicitly requires both new threshold columns to be
null and compares all original facts unchanged; no production gate was weakened.
Final verify-tyG9xr passed workspace check/fmt/strict Clippy and 274 tests: 164
contracts/domain/Runtime, 19 managed, 31 native Codex, 3 native studies, 36 Store
and 21 HTTP/CLI. These include real PG/file concurrent publication, immutable
calendar binding, injected post-file database failure with exact cleanup and
same-key retry, original-value conflict and the corrected upgrade test. Source
remained unchanged and the isolated PostgreSQL cluster was confirmed stopped.

web-verify-3gLTdB reproduced the six generated outputs twice, with handwritten
source unchanged. Typecheck, 505 unit tests, wire checks, build, 36 settings-browser
tests and 234 full-browser tests passed. owner-oci-rX2wPd rebuilt the native image
and passed all 15 actual OCI tests in 39.45 seconds, including original calendar
file consumption and substituted-session rejection. Source remained unchanged.
Image: sha256:9ea9cc73120a8012407f2c522efae37928f77554696610757f18d3faa3cbfd91.

Controlled source metadata and market fixtures do not certify exchange accuracy,
REAL/PIT eligibility or scientific PASS. This adds source registration, not formal
independent PORTFOLIO admission/publication, Release/delivery or complete recovery.
The full suite was not rerun after the test-only correction; the affected migration
target and calendar chain were rerun successfully. No push, review, merge or Issue
closure occurred in this stage.

## Original calendar session consumption, 2026-09-14

portfolio-study/5 and portfolio-calendar/1 consume an original complete session
PARAMETERS file. Job rereads its exact structured values; Runtime binds calendar
name/version to the registered Universe. The thin Rust adapter applies the explicit
signed seconds offset to original UTC closes, with no holiday rules, URL fetching,
new dependency or Python runtime. It checks original availability at the first
evaluation cutoff, shifted coverage, strict nonoverlapping sessions and original
Mandate timezone, then reuses all study frame/fuel/TTL/output-binding checks.
Manual overrides and unrelated calendar files are rejected. Source completeness
and licensing remain the source owner's separate formal admission responsibility.

Three study tests passed, including actual subprocess file consumption, missing or
changed originals, coverage edges, duplicate/overlapping sessions, future availability,
wrong calendar/timezone and manual mixing. Explicit UTC DST/early-close fixtures
verify positive, zero and negative offsets without claiming native holiday generation.
Review corrected an unnecessarily early calendar-availability boundary: calendars
may be published after model research, provided they are known by evaluation start;
the exact cutoff is accepted and one nanosecond later rejected. Final study tests
passed again after that correction (11.87 seconds).

On 6874f11b plus frozen patches, final verify-j4VCXo passed 229 mandate tests with
workspace check/fmt/strict Clippy, unchanged source and isolated PostgreSQL stopped.
Runtime's final 27 tests passed. web-verify-PlaLkF reproduced all six generated
outputs twice without handwritten changes; later availability correction changed
no contract shape. Final owner-oci-shkTER rebuilt the image and passed all 15 actual
Docker tests, including original calendar/liquidity consumption, rejection of
foreign registered calendar name/version, one account and all three bound artifacts.
Source remained unchanged. Image identity:
sha256:5fe27ea316545092c7900ecd83394615fc73021127e85ad7ed8db24ba1eb8b44.

This completes native session-table consumption, not authentic source registration,
REAL/PIT qualification or formal PORTFOLIO admission/publication. Independent policy,
Release/delivery and recovery remain required. No push, review, merge or Issue
closure occurred in this stage.

## Frozen manual study cutoffs, 2026-09-14

portfolio-study/4 adds MANUAL study schedules through manual_cutoffs_ns in the
original task parameters. The existing cutoff preparation, one-account execution
and report binding are reused, without a scheduler, dependency or inferred calendar.
Both supported schedule kinds share frame/fuel/range and target-coverage checks:
manual cutoffs must start at evaluation_start_ns and strictly increase inside the
original window; each target must cover the next cutoff and the last the window end.
Fixed intervals reject manual overrides. No sorting, deduplication or repair occurs.

On b3838210 plus frozen patches, the two actual study CLI tests passed, including
three irregular manual cutoffs, original report rejection after a schedule change,
missing/empty/duplicate/oversized/out-of-range cutoffs and inadequate TTL coverage.
verify-gBCQNc passed 229 mandate tests with workspace check/fmt/strict Clippy,
unchanged source and isolated PostgreSQL stopped. Runtime's 27 tests passed.
web-verify-GJSXH3 reproduced the six generated outputs twice without handwritten
changes. owner-oci-SGzTtp rebuilt the native image and passed all 15 actual Docker
tests; the rolling study used both the original liquidity policy and irregular
manual cutoffs, with full three-artifact adoption. Source remained unchanged.
Image identity:
sha256:663142d65b4b28515a543ae286b5642dbc412a99493517b820d73b04e4ec82a8.

No calendar capability, formal Store study admission/publication or scientific
qualification is established by these controlled fixtures. Calendar sessions,
independent policy/publication, Release/delivery and recovery remain required.
This stage did not push, request review, merge or close the Issue.

## Original per-cutoff BAR liquidity, 2026-09-14

STUDY_PORTFOLIO now requires portfolio-study/3 and portfolio-rolling-liquidity/1.
An original PARAMETERS policy freezes BAR maximum age and participation; the
Mandate references that exact file and matching participation. Each study prefix
uses the same native Nautilus notional calculation as DATA_VALIDATE. No caller
supplied available-notional values, expired snapshot reuse, synthetic quality
report, new numerical engine or dependency was added. Actual account execution
rechecks age before allocation, and participation uses actual simulated equity.
Original frame values, identities, cutoff and execution times are bound on adoption.

The two actual study CLI tests passed, including changing per-cutoff notionals,
future availability, altered values/policy, zero volume, tiny participation and
an original 60-second policy which is fresh at cutoff but expired at execution.
The managed study test passed all four liquidity/no-liquidity and feasible/
infeasible variants, including missing original policy and exact artifact binding.
Runtime's 27 tests passed. On f8f76320 plus frozen patches, verify-I1DLWv passed
229 mandate checks and verify-TUlkYU passed 206 evidence checks, with workspace
check/fmt/strict Clippy, unchanged source and isolated PostgreSQL stopped.
web-verify-jB6oxh reproduced all six generated outputs twice without handwritten
changes. owner-oci-FkPfDu rebuilt the image and passed all 15 actual Docker tests
in 39.84 seconds; its rolling study consumed the original policy and downloaded
all three bound artifacts. Source remained unchanged. Image identity:
sha256:82bbbdefaaba1da540e482da7ff9a0eeda1fb2586f641fed9e11de2450ea39af.

These controlled native execution proofs do not establish DATA_BACKED, REAL/PIT,
scientific PASS or formal PORTFOLIO publication. Complete schedules, Store policy
admission, independent publication, Release/delivery and recovery remain required.
No push, GitHub review, merge or Issue closure occurred in this stage.

## Native Arrow portfolio target history, 2026-09-14

STUDY_PORTFOLIO now requires portfolio-study/2 and portfolio-history/1. The original
quality report and study JSON share one manifest with qz.portfolio_history/1 TARGETS,
an actual Apache Arrow IPC File. A thin contracts module owns its exact schema and
native read/write, reused by the job and final output adoption. It preserves UTC
nanoseconds, decimal128(38,18), separate cash, original frame/asset order and null
weights for infeasible frames. No fake cash instrument, f64 conversion, hash identity,
scientific engine or new upstream package was added. Native Cargo only added the
three already-locked Arrow 56.2.0 dependency edges to contracts.

The existing bounded immutable publisher now writes JSON and Arrow through the same
file limit/freeze path. Arrow is read back before the index is sealed; final adoption
compares every row, column and metadata value against the original request/report.
Tests cover full decimal precision, nanoseconds, zero versus null, truncated files,
foreign schema/metadata, multiple batches, missing history, altered time, altered
exact weights and wrong media type. Both successful and infeasible genuine managed
jobs emit and bind their history. The Runtime HTTP contract declares binary Arrow,
and actual OCI downloads check their returned content type.

Verified on eb6f9963 plus frozen patches: verify-HfOfo4 passed 229 mandate checks;
verify-9dNijo passed 206 evidence checks, with workspace check/fmt/strict Clippy,
unchanged source and isolated PostgreSQL stopped. Runtime's 27 tests passed.
web-verify-WsifEV reproduced all six generated outputs twice without handwritten
changes. owner-oci-sUtuCV rebuilt the native image and passed all 15 real Docker
tests, including the three original rolling-study artifacts and their full binding.
Image identity:
sha256:71bd5d668411c742687f9f6b45e1b54c0bef26ac971c67c07aba9c95fa84737f.

These are controlled format and execution proofs, not REAL/PIT qualification or
formal PORTFOLIO publication. Rolling liquidity, complete schedules, Store admission,
independent policy/publication, Release and delivery/recovery remain required.
No GitHub review or merge gate is satisfied by this stage.

## Original model-driven fixed-interval portfolio study, 2026-09-14

STUDY_PORTFOLIO and the trusted local `job study-portfolio` command now share the
existing Build model/catalog preparation, Clarabel allocation and Nautilus account
execution. Each original cutoff uses its own historical prefix and divided fuel
budget. Rebalances read actual simulated equity, positions and current completed
prices inside one account; they do not reuse target weights as observed holdings,
restart the account or fabricate historical Candidate/snapshot identities.
Original settings/model bytes and per-frame inputs/targets are bound. Infeasible
allocation retains its diagnostic frame, stops further solves and emits neither
simulation nor fabricated cash/targets. Fully invested native margin denial remains
a failure; the positive fixture explicitly freezes a distinct 1% cash reserve.

Actual CLI and managed subprocess tests cover three cross-day rebalances, changed
equity/weights, original objects, missing/wrong-role inputs, seven report mutations
and infeasibility. All job tests passed. Runtime's 27 tests passed, including nine
catalog-scope operations and the complete generated download response references.
On e96952b0 plus frozen patches, verify-XQgVlX passed 228 mandate checks and
verify-PoKuiH passed 206 evidence checks, with workspace check/fmt/strict Clippy,
unchanged source and isolated PostgreSQL stopped. web-verify-NIVcrc reproduced all
six generated outputs twice without handwritten changes. The explicit domain
request/frame/result exports are covered by the existing schema regression.
Final owner-oci-LSKjSU rebuilt the image and passed all 15 real Docker tests,
including original rolling models, three solves, one account and native daily
returns. Image identity:
sha256:e0084e56df9c71de78643c5dcfa99e5988cf6aaf6f1172eed8fc661fd2260139.

This proves a controlled native execution slice, not REAL/PIT qualification or
formal PORTFOLIO publication. Calendar/manual schedules, rolling liquidity,
Arrow target history, complete Store admission/policy/publication and Release,
delivery/recovery remain required. No dependency, additional account ledger or
second simulation engine was added. No GitHub review/merge gate is satisfied.

## Original target sequence native binding, 2026-09-14

SIMULATE_PORTFOLIO_SEQUENCE binds 2..253 original Candidate target files and their
trusted availability to one frozen simulation request and original fee file. It
reuses the existing Nautilus shared account, quality report, output adoption and
HOLD target-source check. Duplicate sources, changed targets/settings, noncausal
order, expiry gaps, missing objects and non-Forward inputs are rejected. The 253
limit leaves the catalog, settings and task parameters within the existing 256
input limit; no dependency, account ledger or simulation engine was added.

Runtime materialization now checks the original quality source_selection for both
HOLD and sequence tasks, not merely the narrower replay window. Real SQLite scope
regressions cover all eight data operations. This exposed an older Build fixture's
stale fee count and missing original settings object; the fixture was corrected,
not the production admission checks. The existing native OCI HOLD acceptance is
reused for a separate sequence case through the actual gateway and Docker image.

Verified on f1cf8acd plus frozen patches: verify-pWOi46 passed 227 mandate checks,
workspace check/fmt/strict Clippy; verify-TKykGq passed 205 evidence checks. Both
database verifiers kept source unchanged and stopped their isolated PostgreSQL.
Runtime's 27 default tests passed. owner-oci-mgN34I passed all 14 native tests in
32.08 seconds, including two original target files, one actual account and native
cross-day returns. Image identity:
sha256:fe15b1dce45f885b2ebe217272f94504b5e489fd23848f248db4fa5dfcb3add8.
web-verify-aeOzOV reproduced all six native-generated outputs twice without changing
handwritten sources; only the domain OpenAPI changed. No frontend source changed,
and browser tests were not rerun for this native-only slice.

These are controlled original-target replay results, not original Store Candidate
publication or REAL/PIT qualification proof. Offline rolling model/allocation
generation, Arrow target history, complete cohort/policy admission, independent
PORTFOLIO publication and Release/delivery/recovery remain open. Creating a business
Candidate per historical bar or backfilling availability is not an acceptable
substitute for that missing offline study. No GitHub review or merge gate is met.

## Native allocation to shared-account replay, 2026-09-14

A controlled managed-job check now feeds original native Wasm/Clarabel allocation
output into the actual Nautilus single-account replay. The build consumes only
the original cutoff prefix despite later rows being present in the catalog;
replay uses the resulting targets, original settings and original Mandate TTL.
Two distinct Alpha inputs produce actual orders in one account. Intraday returns
remain INSUFFICIENT_DATA, not invented daily observations or scientific PASS.

This exposed a real compatibility gap: pinned Nautilus 0.63.0 rejects CurrencyPair
in a single-base-currency CASH venue. One domain check now serves original fee
admission/rechecks and both native build/replay market checks. CurrencyPair requires
MARGIN; Equity retains CASH/MARGIN. Unsupported immutable settings are rejected,
not converted or rewritten. Controlled FX fixtures now explicitly declare MARGIN.
The positive fixture explicitly freezes sufficient capital for native lot rounding;
production tolerances and capital defaults remain unchanged. Temporary diagnostic
prints were removed before final verification.

Verified on e989bc92 plus frozen patches: verify-8EaopY passed 226 mandate checks;
the simulation binary passed all 11 tests; verify-DsZzgL passed 204 evidence checks.
Workspace compilation, formatting and strict Clippy passed, both isolated database
runs stopped their PostgreSQL, and source snapshots stayed unchanged. Native OCI
verification owner-oci-BVhiP1 rebuilt the real image and passed all 13 tests with
unchanged source. Image identity:
sha256:a743cb8b3799e4868d72b074312089ebd35c84aaa3d16529215aa6ba7aeec226.

This is actual controlled native execution, not REAL/PIT proof, formal independent
PORTFOLIO evaluation, Release eligibility or T42 completion. FORWARD/HOLD evidence
cannot substitute for the Release's PORTFOLIO evaluation. Full portfolio science,
Release/Package/delivery/recovery and the remaining acceptance work are still open.

## Immutable EvaluationPolicy browser authoring, 2026-09-14

The Portfolio page now exposes the existing project-scoped EvaluationPolicy
list/detail/create APIs. The Ant Design form explicitly authors Selection, both
native split shapes, original source references, independent Validation/Sealed
requirements, optional portfolio requirements and common evidence limits. Decimal
and DbCounter values remain strings. Disabling portfolio criteria sends null,
never an inherited threshold. No policy engine, backend API, dependency, database
change or generated contract change was added.

The existing Intent, authentication, error and unsaved-work handling are reused.
Unknown write outcomes retain the original input/key for explicit retry. Original
versions are read-only; no data-derived thresholds, raw Sealed access, experiment
start, qualification or delivery authority are introduced. Project/record identity
checks reject unrelated read responses; pagination can recover to the prior page.

Verified on b5ad52c0 plus frozen UI patches: the final targeted browser run passed
all nine desktop/tablet/mobile cases in 33.9 seconds, including actual form fills,
exact large integers/decimal endpoints, both split kinds, independent criteria,
null portfolio criteria, same-key lost-response retry, foreign-project rejection
and Axe checks for editor/detail. Earlier test locator/fixture errors were fixed;
the actual read-only drawer keyboard-scroll finding was fixed with a focusable
policy text region, not by disabling accessibility checks.

web-verify-aHIvj3 then passed full native generation reproducibility, typecheck,
505 unit tests, five PWA file checks, numeric wire checks, production build,
36 settings-browser checks and all 234 browser checks (4.5 minutes). Handwritten
sources remained unchanged. These are controlled browser contract tests, not a
new claim of native policy admission, real market science or full T42 completion.
Input preparation, complete research/portfolio science, cost sources and the
Release/Package/delivery/recovery workflow still require the remaining development.

## Candidate Evaluation read projections, 2026-09-14

Published Candidate HOLD evaluations now have a paginated Operator/same-project
RESEARCH_READ CLI projection at GET /api/v2/portfolio-candidates/{id}/evaluations.
The existing Evaluation detail/metrics endpoints accept only the original sealed
Candidate simulation binding, terminal receipt, frozen Forward input and original
producer report. Alpha Validation selection remains unchanged; Sealed and unbound
historical reports remain excluded. No report bytes or storage locators are read.

CLI portfolio candidate evaluations and the React Candidate drawer use this same
projection. The existing Evaluation detail component is reused with exact subject
checks. Original metric zero, missing reasons, method/version, units, frequency,
annualization and expiry remain distinct. Pagination can return after a failed
read; neither the page nor a historical decision grants qualification or delivery.

Verified on 32c98dfe plus the frozen implementation patches:

- verify-LarMSm passed all 203 evidence checks, workspace compilation, formatting
  and strict Clippy. Both original qualified chains read their actual cancelled
  and insufficient-data publications, exact subjects, metrics and keyset pages.
  Real HTTP/CLI checks include the new command and cross-project rejection;
  existing Sealed/unbound exclusions remain passing. Source unchanged and isolated
  PostgreSQL stopped. This is controlled publication evidence, not native market PASS.
- Extending the long-chain test exposed async stack exhaustion. Boxing the two
  test entry futures fixed it without host/test stack settings or production changes.
  The extra inner box was removed before the final 203-check run.
- web-verify-R05PFM completed reproducible native generation of the six contract/
  client outputs, typecheck, 505 unit tests, five PWA file checks, numeric wire
  checks, production build, 36 settings-browser checks and all 225 browser checks
  across desktop/tablet/mobile. Handwritten sources remained unchanged. The prior
  web-verify-o3xCPy was interrupted without a final browser receipt and is not PASS.

No new dependency, numerical engine, mutable Candidate field or raw-report route
was introduced. Genuine cross-day Store-to-native PASS, formal portfolio science,
full cost sources, Release/Package/delivery/recovery and complete T42 remain open;
these read projections do not satisfy the PR merge boundary by themselves.

## Candidate HOLD Evaluation publication and ACK, 2026-09-14

The trusted scientific continuation now consumes the original immutable Candidate
simulation binding, terminal receipt, Attempt/spec, parameters and both original
manifest outputs. It publishes one sealed FORWARD/HOLD Evaluation and native
MetricValues using the separate frozen portfolio requirements, original-window
registered-row coverage, actual daily samples, source eligibility and validity.
No criteria means INCONCLUSIVE. Failed/cancelled execution has no invented metrics.
This is not a Release forward-evidence window, strategy walk-forward or approval.

The original Run lock and existing evaluation publication trigger own idempotency
and metric membership. ACK requires this exact Run/Candidate/policy publication.
The existing report writer is reused. Original target/settings/metadata/fees/member
and input eligibility are checked before and after report publication. The shared
final SQL window now includes the current Forward input as well as the original
Build and assumptions inputs. Report/transaction failure leaves the original
message for recovery; replay does not republish or refresh expiry.

Verified on 3b3249db plus frozen patches:

- verify-78DZwU passed both original qualified chains with cancelled-run ACK
  blocking, failed report rollback, concurrent one-publication/replay and final ACK.
- verify-Fe0EN7 added successful-process, insufficient-daily-data publication from
  a controlled original double-output protocol receipt. No SQL qualifications were
  authored; this controlled receipt is not a native numerical execution claim.
- Additional pre/post-publication cost-file mutation checks exposed caller-future
  stack exhaustion; the shared continuation now uses standard Box::pin, without
  changing test/host stack configuration. verify-tAzoxC passed both chains after
  that fix, including original independent criteria, three missing-value metrics,
  retained validity, source-change rollback, concurrent replay and ACK recovery.
- verify-bRNrAr passed all 203 evidence regressions and strict checks. After the
  final all-input expiry check, verify-S3W4Tg again passed both original chains and
  verify-6Ojl3X passed all 225 mandate/native/Store/source/window/HTTP/CLI checks.
  All final verifiers reported unchanged source; isolated PostgreSQL was stopped.

No contract/native-job/API/UI change was made in this slice. Candidate Evaluation
read projections, genuine cross-day Store-to-native PASS, formal portfolio science,
Release/delivery and complete T42 remain outstanding. Do not claim them from these
controlled protocol tests or merge PR #63 on this partial evidence.

## Original-source coverage beside Candidate simulation, 2026-09-13

candidate-simulation/2 freezes source_selection from the original registered
metadata independently of the effective HOLD window. The managed job reuses the
same DATA_VALIDATE loader and quality output path, then runs the existing shared
account simulation. Its original manifest requires both qz.data_quality and
qz.native_simulation; the existing quality binding check now also checks Candidate
source identity, selection and check time. No estimator or calendar was added.

Verified on aa58e82a plus frozen patches:

- Actual managed Candidate test passed 18 scenarios (9.90 seconds), including
  original 40-row coverage despite a later simulation start, mismatched source
  selection rejection, daily positions, daily cash and intraday missing returns.
- verify-T7Lvnz passed 225 mandate domain/native/Store/source/window/HTTP/CLI tests
  and workspace check/fmt/strict Clippy, source unchanged.
- web-verify-rTwxRH generated six outputs twice identically, handwritten source
  unchanged. Only domain OpenAPI changed; this was generation-only, not a new
  browser acceptance run or UI implementation.
- owner-oci-YjDPjE passed all 13 actual OCI tests (27.99 seconds), including both
  downloaded outputs and original manifest binding, with image
  sha256:428822fa5fd1b1e4ec37427365a61210b74a8974a14d52833ec0b762146ad8cc.
- Store chain verification first found a missing quality capability in a controlled
  fixture, then its duplicate in the shared Cycle fixture. Fixed both fixture
  construction paths without relaxing production admission. verify-uaYESg then
  passed both original qualified chains and strict checks; original source window
  remains wider than the effective simulation window. Final verify-lpClK6 passed
  the evidence regression suite after those fixture-only changes. All verifier
  processes ended, source checks passed and isolated PostgreSQL was stopped.

Markets/scientific declarations remain controlled, not REAL/PIT/full T42 proof.
Candidate Evaluation publication/ACK, formal portfolio science and delivery are
still outstanding; source coverage is not itself a PASS or Release authority.

## Durable original Candidate simulation admission, 2026-09-13

Migration056 adds immutable candidate_simulation_tasks with original Run, Candidate,
policy, Forward dataset revision and complete request. Admission saves it in the
existing Run/native-task/PGMQ transaction after parameter publication; no new queue
or mutable result state. Replay returns the original Run and binding.

verify-3aWy9R passed both original qualified portfolio chains on a89a11f4 plus frozen
patches, including parameter-publication rollback (no binding), exact Candidate/
policy/dataset/request readback, one binding after replay, and database UPDATE/DELETE
rejection. Workspace check/fmt/strict Clippy passed; source unchanged and isolated
PostgreSQL stopped. These controlled scientific declarations are not REAL/PIT/T42.
This durable relation does not yet publish a Candidate Evaluation or change ACK.

## Native portfolio metric mapping, 2026-09-13

The domain adapter binds the original simulation request and canonical account/time
identities, then copies three original Nautilus 0.63.0 Returns-group statistics:
daily mean, annualized volatility and Sharpe. Daily frequency, units, sample count,
actual simulation period, source artifact and evaluation identity are retained.
Volatility/Sharpe retain native 252-day annualization; no formula, dependency,
calendar filling or position-return fallback was added. Unavailable returns keep
their original state; unavailable statistics remain null/FAILED with native reason.

Verified on af976ea0 plus frozen patches:

- Locked job simulation target passed all 11 tests (3.40 seconds), including actual
  native subprocesses for intraday, two-day held positions and two-day all-cash.
  Mapped values equal native statistics exactly; real zero and undefined Sharpe
  remain distinct. The existing exact policy comparator rejects insufficient
  samples; missing native keys and wrong starting-account binding are rejected.
- Strict Clippy passed domain/job all targets. verify-8J28Jv also passed workspace
  check/fmt/strict Clippy and the mandate domain/science/Store/source/window/HTTP/CLI
  checks, with no failures and unchanged source. Isolated PostgreSQL was stopped.

These are synthetic-market adapter checks, not REAL/PIT qualification or full T42.
No immutable Candidate Evaluation publisher, Release authority, new API or UI is
claimed by this slice. It maps native results for the existing policy gate only.

## Independent portfolio policy criteria, 2026-09-13

EvaluationPolicy now freezes optional portfolio_metric_requirements separately
from Alpha validation and Sealed criteria. Null means no portfolio PASS criteria,
not permission to inherit another group's thresholds. Non-null lists use the
existing exact threshold/duplicate/method/required-metric validator. Migration055
adds the nullable column without backfilling immutable historical policies; native
policy immutability covers this field. Create/read/replay preserve the original
intent and exact decimal values through the existing policy API/CLI.

Verified on b996b57f plus frozen patches:

- verify-HkDo2n passed 203 evidence tests and check/fmt/strict Clippy, source
  unchanged. Actual HTTP policy creation/readback/replay includes independent
  portfolio conditions; scientific execution is not implied by this metadata test.
- Locked domain research target passed all 5 tests, including absent criteria,
  independent conditions, empty list, duplicate code/scope and no-required rejection.
- web-verify-0qmb2P generated six native outputs twice identically; handwritten
  source unchanged. Typecheck, wire checks, build, 505 unit, 36 settings-browser
  and 219 full-browser tests passed. Domain/API JSON, TypeScript and Ajv JS changed.
- Final verify-3zoPjT passed all 203 evidence tests and strict checks after adding
  direct database immutability coverage. Changing only portfolio conditions under
  the same key conflicts; a new policy version preserves them while the old version
  remains null. Attempted direct backfill is rejected by the original trigger.
  Source unchanged; isolated PostgreSQL stopped.

No policy editor currently exists in the React UI; this is not a claimed UI feature.
Native portfolio metric adaptation and immutable Evaluation publication/ACK are
still unfinished. No estimator/formula/dependency was added, no native scientific
qualification or Release was granted, and no GitHub write/merge/Issue closure occurred.

## Original Candidate simulation admission and HTTP/CLI, 2026-09-13

POST /api/v2/candidate-simulations and client portfolio simulate now accept only
Candidate/Cycle/InputSet/Runtime references, exact runtime revision and bounded
limits. PORTFOLIO_SIMULATE has its own exact human intent/grant (migration054).
The Store resolves original published Candidate targets with the same reader
as LAST_TARGET, freezes original availability, checks original Build/Mandate,
policy, image and execution settings, and binds one same-Universe REAL/PIT Forward
catalog. The requested window is narrowed to original availability/target validity.
Original target/settings files are reread after object publication; original
member eligibility and input grants are rechecked. The existing final database
window check runs after Run/task binding and authority recheck. Original budget,
command replay, PGMQ and native task binding are reused, not a second scheduler.
HTTP Build and Simulate share their object publication/failure-cleanup facade.

Verification on 2ab3fbfe plus frozen source snapshots:

- Initial verify-VUxMYT correctly rejected both controlled qualification chains:
  their Runtime fixture omitted qz.native_simulation output capability. Added the
  missing fixture declaration; production checks were not relaxed.
- verify-F85Val passed both original-source chains. verify-dvqQRb subsequently
  passed all 203 evidence tests and strict checks, including stale Forward,
  changed settings, publication failure/retry, original availability, same-key
  no-I/O replay and absence of fabricated Evaluation.
- Final HTTP/CLI/source patch: verify-8jDNMx passed check/fmt/strict Clippy and
  224 tests (134 domain, 51 native, 13 Store, 1 original-source SQL, 1 liquidity,
  2 window checks, 22 HTTP/CLI). Actual CLI/TCP/PG tests exercise missing authority,
  exact simulation grant, missing Candidate, altered intent and zero admitted Runs.
- web-verify-KCecxz regenerated six native outputs twice identically, with no
  handwritten changes. Domain/API JSON, TypeScript and Ajv JavaScript changed.
  Typecheck, wire checks, build, 505 unit, 36 settings-browser and 219 full-browser
  tests passed. No new browser simulation form is claimed.
- Final verify-3pU0h1 repeated check/fmt/strict Clippy and both qualified chains
  after the last database window check and generated contract changes; passed,
  source unchanged, isolated PostgreSQL stopped.

Store qualification tests use controlled scientific declarations and real PG/files;
they do not prove actual Store-to-OCI market execution. Separate native synthetic
daily-return/OCI acceptance is recorded below. This admission returns a Run, never
Evaluation/PASS/Release. Immutable portfolio Evaluation publication, complete
scientific/qualification/delivery acceptance and all remaining Issue62 work are
unfinished. No push, GitHub review request, merge or Issue closure in this slice.

## Candidate adapter native daily-return acceptance, 2026-09-13

The earlier Candidate adapter tests proved only intraday INSUFFICIENT_DATA.
Extended the same managed test to 16 scenarios: two UTC days of held targets
must produce finite, nonzero native portfolio daily returns; two days of all-cash
targets must produce actual zero returns with no orders or positions. The
intraday cases still require empty returns and INSUFFICIENT_DATA. No numerical
code, model, dependency, source contract or qualification rule changed.

On 3967b70a plus the frozen test patch, the targeted managed test passed all
scenarios in 8.22 seconds, and strict Clippy for managed/native_oci passed.
owner-oci-3WHi16 passed all 13 actual OCI tests in 26.76 seconds with unchanged
source and the same native image
`sha256:95285da1667ffef52b7a3a398f106cbbc90b77dd9252e6fee6ee99269d133e76`.
Its Candidate test now executes the two-day held-target input through Runtime
and validates the original daily-return output. This is synthetic numerical
acceptance, not proof of sufficient evaluation samples, REAL/PIT, strategy
walk-forward, formal Candidate admission or Evaluation publication. Those remain
unfinished; no push, review request, merge or Issue closure.

## Original Candidate hold simulation adapter, 2026-09-13

SIMULATE_CANDIDATE reuses the existing native shared-capital simulation, reading
the original qz.portfolio_targets/1 REPORT and execution-settings PARAMETERS.
Identity, currency, weights/cash, original validity and complete settings must
match the frozen request. Only one FORWARD catalog and one target point are
accepted. Start is max(original asof, Candidate availability); end cannot exceed
the original validity. This prevents applying final weights before availability.
The shared target document replaces the private LAST_TARGET decoding type;
existing Store behavior is unchanged. No new engine or dependency was added.

Verification of c9e8249b plus the frozen source patch:

- verify-rG5DpQ: check/fmt/strict Clippy and 223 tests passed (134 domain,
  51 native, 13 Store, 1 original-source SQL, 1 liquidity source, 2 publication
  windows, 21 HTTP/CLI); source unchanged and isolated PG stopped.
- verify-5OryWW: check/fmt/strict Clippy and 203 tests passed (4 publication/source
  unit, 38 native, 137 Store, 24 HTTP/CLI); source unchanged and isolated PG stopped.
  This includes the original LAST_TARGET reader and controlled qualified chains.
- Managed Candidate test covers original success, delayed-availability success,
  changed identity/cash/weights/settings, missing source files, wrong input role,
  Sealed input, early start, excess validity and duplicate target points.
- web-verify-l81dVL regenerated all six named native outputs twice identically;
  handwritten source unchanged. Only domain JSON changed. This final generation
  did not rerun browser tests; the earlier web-verify-7TMpcJ full run passed
  505 unit, 36 settings and 219 browser tests before the availability-field change.
- owner-oci-ennKAN rebuilt native image
  `sha256:95285da1667ffef52b7a3a398f106cbbc90b77dd9252e6fee6ee99269d133e76`;
  all 13 actual OCI tests passed, source unchanged. New test uploads original
  documents through Runtime, runs the native job, reads/binds its report and
  replays the same job. Runtime/image advertise candidate-simulation/1.

This is a hold adapter, not strategy walk-forward or formal independent portfolio
Evaluation. Trusted Store admission must still bind original availability and
sources, policy/metrics/expiry and immutable Evaluation publication. Native fixtures
use synthetic markets; intraday results remain INSUFFICIENT_DATA, never PASS.
No REAL/PIT/T42, DATA_BACKED, actual-account restoration or Release claim. No push,
GitHub review request, merge or Issue closure in this slice; all remaining work
and final exact-head gates remain required.

## Native nonzero-slippage planning, 2026-09-13

The patch over `4591056d` maps the frozen Nautilus DefaultFillModel L1 one-tick
rule into the explicit proportional expectation in DESIGN A5.2. Native Build
reads the original last BAR and Instrument tick, records slippage_references and
uses the resulting planning coefficient in the existing solver. No RNG, fill,
fee engine or new dependency. Request asset rates remain original taker rates;
result rates must equal the reference-derived mapping, rounded upward to 18
decimal places. Zero probability has no slippage references. Actual simulation
still uses original models/rates and native Money rounding, without deducting
the planning coefficient again. This is not a future cost bound or DATA_BACKED.

Runtime/image and nonzero admission/results require portfolio-slippage/1. Native
tick presence is checked at source admission; publication rereads original tick
and fees. Publication fee checks were found inside the old group-constraint
branch; all portfolios now reread Forward metadata, including no-group mandates.
Existing group semantics remain unchanged: unused group classifications are not
frozen as constraints. Both source corruption and recovery remain covered.

- Domain independent values include 0.010505 and upward-rounded
  0.011683333333333334, plus invalid reference/time/currency/tick cases.
  Managed native p=0,0.5,1 checks use actual last price 1.02 and native event times.
- The two original qualified Store chains now cover zero/no-group and
  nonzero/historical-liquidity/group configurations. Both reject changed Forward
  fees; the latter rejects changed tick, then recovers Candidate/LAST_TARGET.
  First evidence/chain runs failed obsolete fixture assumptions that every
  mandate had groups; those expectations were made configuration-specific,
  without weakening production fees or actual constrained-group checks.
  Final verify-nH2x9o passed both chains.
- Final verify-audUEn passed static gates and 222 checks: 134 contracts/domain,
  50 native, 13 Store, 1 source SQL, 1 expiry, 2 windows and 21 HTTP/CLI.
- Final verify-SBhp7s passed 202 checks: 4 unit, 37 native, 137 Store and
  24 HTTP/CLI. Both verifiers recorded unchanged source and stopped temporary PG.
- web-verify-J3rWhF generated all six outputs twice identically; only Runtime
  JSON changed. Typecheck, wire checks, build, 505 unit, 36 settings and 219
  browser checks passed, handwritten source unchanged.
- owner-oci-PWdozz rebuilt image
  `sha256:d28831c016df1c0639aa4c664ce34b22332925683fa3ce4272aeb176e3ad7c98`;
  all 12 actual OCI tests passed in 22.40 seconds, source unchanged. The first
  portfolio and its subsequent liquidity run use p=0.5, actual original BAR/tick,
  positive planning coefficients and the new manifest capability.
- Locked job simulation test target passed all 10 actual Nautilus checks,
  including original fees, slippage/seed, latency and shared-capital accounting.

Controlled Store declarations and synthetic native/OCI markets are not REAL/PIT
or full T42 proof. DATA_BACKED sources, independent Candidate evaluation and all
remaining delivery contracts are outstanding. No push, review request, merge or
Issue closure occurred in this slice.

## Forward instrument fees agree with original settings, 2026-09-13

The patch over `8ce40c06` closes a concrete cost mismatch: Build previously
validated its original settings document but not the current Forward instruments'
fees, while simulation rejected the mismatch later. Native Build now reuses
simulation's existing market/fee validation. A shared domain check binds original
catalog instrument identities, currency and maker/taker rates during assumption
creation, Build admission and Candidate publication. No new dependency or fee
algorithm. The native fixture now takes its fee values from its actual catalog.

The managed counterexample changes both the original cost document and frozen
copy consistently, and still rejects their disagreement with the native catalog.
Both full Store qualified chains reject same-size Forward taker-fee corruption
at admission and publication, then recover original Candidate/LAST_TARGET flow.
These remain controlled scientific declarations, not SQL-authored qualifications
or REAL market evidence.

- Initial verify-GRKcqz passed functional tests but failed strict Clippy on one
  needless borrow. The reference was removed without suppressing the lint.
- Final verify-VZSo26 passed static gates and all 220 selected backend checks.
- verify-3VQ2mb passed static gates and 201 cross-boundary checks: 4 unit,
  36 native, 137 Store and 24 HTTP/CLI. Source unchanged; temporary PG stopped.
- Locked `cargo test -p job --test simulation --locked` passed all 10 actual
  Nautilus simulation tests, including native fees, slippage/seed/latency,
  shared-capital rebalance, daily returns and independent concurrent accounts.
- owner-oci-tl2nZq rebuilt image
  `sha256:a8620bc439f3efc71564b4ffd5a330a261b4670b19642225dbb9b1e948722163`;
  all 12 actual OCI tests passed in 21.75 seconds, source unchanged.

No HTTP/schema or frontend changes; no new generated outputs. Nonzero-slippage
planning, DATA_BACKED cost sources, independent Candidate validation and remaining
delivery contracts are not complete. No push, GitHub review, merge or Issue close.

## Original execution settings through portfolio Build, 2026-09-13

The patch over `4f0c09aa` binds the complete original execution settings to the
native Build request, mounts transaction_costs_ref as PARAMETERS and compares
its parsed original document with the frozen copy before numerical execution.
The domain checks currency, capital, exact per-asset taker fees and the existing
explicit zero-slippage restriction. Candidate publication rereads the document
and compares both the frozen request and immutable saved settings. Admission,
Runtime image label/capabilities and result publication require
portfolio-cost-source/1. Existing readers/types are reused; no new dependency,
cost formula, nonzero-slippage support or DATA_BACKED claim.

Managed native tests cover the original document, changed seed, wrong role and
missing file, with no output index on failure. Domain counterexamples cover
currency, capital, fees, missing assets and nonzero slippage. Both original Store
qualified chains reject same-size cost-document corruption at admission and
publication, then recover the original Candidate/LAST_TARGET flow. Qualifications
are not authored with SQL; scientific declarations remain controlled fixtures.

- Initial verify-BZkM0q failed strict Clippy because a shared fixture module was
  loaded twice. Callers now reuse the same module, without suppressing the lint.
- verify-yrbJrv passed both qualified chains. Final verify-JS0HfD passed static
  gates and 220 checks: 133 contracts/domain, 49 native, 13 Store, 1 source SQL,
  1 liquidity source/expiry, 2 publication windows and 21 HTTP/CLI.
- verify-L11ffg passed 201 cross-boundary checks: 4 unit, 36 native, 137 Store
  and 24 HTTP/CLI. Source remained unchanged and temporary PostgreSQL stopped.
- web-verify-9GWm9V generated all six outputs twice identically; only domain JSON
  changed. Typecheck, wire checks, build, 505 unit, 36 settings and 219 full-site
  browser checks passed, handwritten source unchanged.
- owner-oci-86XKav rebuilt image
  `sha256:b0103bb9d1b6f3143028011514fcece03f4ade6795faddafc2a9900e5356c41e`;
  all 12 actual OCI tests passed in 22.39 seconds, source unchanged, including
  original cost document mounting/consumption and the declared capability.

Actual OCI inputs are synthetic, not REAL/PIT or full T42 evidence. Nonzero
slippage, DATA_BACKED cost adaptation, independent shared-capital Candidate
validation and remaining delivery contracts are still outstanding. No push,
GitHub review request, merge or Issue closure occurred for this slice.

## Store admission and publication of historical liquidity, 2026-09-13

The patch over `534ebf78` connects the original historical source to Build
admission and Candidate publication. The shared frozen-source reader revalidates
the original assumption, Dataset/selection, accepted native report and current
licenses, checks the unchanged derived expiry, and distinguishes immutable-source
corruption from genuine expiration. Admission freezes original per-asset notionals,
report input and configuration; source origin uses the existing downgrade rules.
Publication rereads the same original bytes and compares the frozen binding and
values. Corruption preserves retry; expiration cannot grant a new target. The
final database-only eligibility snapshot includes the original assumption expiry.
No new cost formula, qualification shortcut or origin upgrade.

Runtime and its image label expose portfolio-liquidity/1; admission requires it
and a bound result must declare it. The existing controlled qualified-chain test
now runs both without and with liquidity created before the original policy.
Both follow actual Store admissions/reviews/qualifications, Build, Candidate and
LAST_TARGET, with idempotent replay. Original report currency corruption fails
admission; same-size changed numbers or currency fail publication, then original
bytes recover successfully. No SQL-authored qualifications or substituted policy.
A separate real PostgreSQL/file test observes the actual expiry clock, rejects
the expired source and retains its historical timestamp.

`verify-zqM5KR` passed both qualified chains. `verify-DF3aEY` passed static
gates and 218 checks: 132 contracts/domain, 48 native science, 13 Store, 1 existing
SQL, 1 original-source/expiry test, 2 publication-window tests and 21 HTTP/CLI.
The external verifier now explicitly selects the new library tests.
`verify-PcTnig` passed check/fmt/strict Clippy and 200 wider checks: 4 unit,
35 native, 137 Store and 24 HTTP/CLI.

OCI initially stopped before build because its development inventory could not
record a deleted/moved helper; it now records deletion and still checks the exact
before/after snapshot. The next run rejected every submission because the image
label lacked the new stack entry; synchronizing the label fixed the cause without
relaxing the gateway check. `owner-oci-9hYhrr` then passed all 12 actual tests on
`sha256:6d33d72c92687839ebda3df3cb7c097578754e1e925cde61277b6e976753e2fa`.
The first portfolio test submits actual DATA_VALIDATE and passes its downloaded
original bytes to another OCI Build, checking the manifest version, original
output binding and optimal constrained result. `web-verify-7yeHLc` passed six
twice-identical generated outputs, typecheck, 505 unit, wire/build, 36 settings
and 219 full-browser checks; no generated file changed. Source remained unchanged
within successful verifiers.

The Store scientific responses and market data remain explicitly controlled;
the OCI data are synthetic. These prove this historical-assumption adapter,
not REAL/PIT, DATA_BACKED, nonzero-slippage support, independent Candidate
evaluation, Release/Package/delivery or complete T42. Remaining development and
current-head review/CI/merge/main verification gates are still required.

## Native Build consumption of original historical liquidity, 2026-09-13

The patch over `3270eabe` freezes optional NativePortfolioLiquidityV1 in the
native Build request. Mandate report/participation and every numerical notional
must be bound; no source means no unbound reference, ratio or available_notional.
The job reads the original DATA_QUALITY artifact, reuses the complete native
quality validator and checks original Dataset/selection, currency, age, asset
order and exact measured values before the existing Clarabel solve. Only that
bound report uses the native quality-output size allowance; other object limits
are unchanged. No new solver, valuation formula or dependencies.

The managed native regression actually runs DATA_VALIDATE on synthetic Parquet,
then consumes its output in an actual Build. A constrained solve passes; changed
frozen numbers, wrong Dataset, expired age, wrong input role, changed original
report values and missing observations all fail without a published index.
Additional domain checks reject unbound references, participation and numbers.
`verify-S5tY8u` passed initially; after the extra original-byte and unbound tests,
`verify-aXG7Pq` passed check/fmt/strict Clippy and 215 checks: 132 contracts/domain,
48 native science, 13 Store, 1 SQL and 21 HTTP/CLI. `web-verify-D1zVCR` passed six
twice-identical generated outputs, typecheck, 505 unit, wire/build, 36 settings
and 219 full-browser checks. Only generated domain JSON changed. Handwritten
sources remained unchanged within each verifier.

This is native consumption, not complete Store admission or Candidate publication
revalidation. Store still constructs no binding and keeps its participation
refusal gate. Runtime capability gating, actual OCI consumer evidence and the
original-source/current-expiry checks at admission and publication remain required.
These synthetic numerical tests are not REAL/PIT, complete cost support, independent
Candidate validation or delivery, and do not satisfy final review/CI/merge gates.

## Immutable historical BAR liquidity source, 2026-09-13

The patch over `9ccc2054` adds optional explicit historical liquidity assumptions:
original accepted DATA_VALIDATE report, maximum age and exact participation limit.
Creation verifies the original Run/Attempt/task/manifest/output mapping and bytes,
same project/Runtime/frozen input/Dataset/selection, current license, non-Sealed
use, measured asset values and base currency. Same-schema registration is not an
adopted native result. The independent exclusive expiry uses the earliest original
event plus explicit age, floored to PostgreSQL microseconds. Creation rechecks
expiry after object publication; configuration and expiry cannot be changed.
This remains CONSERVATIVE_ASSUMPTION, never future depth or DATA_BACKED evidence.

The existing HTTP/CLI request and read response carry the optional configuration
and original expiry. React uses existing Ant Design fields and original retry
intent; disabling the option clears it and sends null, re-enabling requires fresh
explicit values. Detail shows original report/ratio/age/expiry, not eligibility.
No new dependency, alternate transaction engine or default age/ratio.

`verify-4m1zPg` first caught missing schema_version for app.document and a
Store-only fixture helper included by HTTP tests. The configuration now uses
SchemaV1 and the helper lives with its sole Store consumer; constraints were not
weakened. `verify-JVFELC` then passed check/fmt/strict Clippy and 213 tests:
131 contracts/domain, 47 native, 13 real PostgreSQL/file Store, 1 SQL and 21
HTTP/CLI. The new test rejects registration-only and expired sources, adopts a
controlled original native receipt, and checks replay and immutable expiry.
It does not claim actual numerical computation or REAL/PIT qualification.
`web-verify-xYgbEF` passed six twice-byte-identical generated outputs, typecheck,
505 unit, wire/build, 36 settings-browser and 219 full-browser tests. Handwritten
sources remained unchanged during each verifier.

Build consumption, native source binding and Candidate publication revalidation
are still required; existing participation refusal remains. Full cost adapters,
independent Candidate validation, delivery, REAL/PIT acceptance and the final
current-head GitHub review/CI/merge gates are not complete.

## Native last-bar notional observations, 2026-09-13

The patch over `8825d0b6` adds optional last_bar_notionals to original native
DATA_VALIDATE results. For each non-Sealed asset, the job takes its last selected
bar and calls locked Nautilus Instrument::try_calculate_notional_value with the
original volume, close and use_quote_for_inverse=false. Native contract multipliers,
currency, Money rounding and arithmetic errors remain upstream-owned. The report
retains instrument ID, event/availability times, close, volume, notional and currency.
No new dependency or custom valuation formula. Runtime/image capability is
bar-notional/1. Unmeasured is null, not zero; Sealed jobs omit the detail and
Sealed metadata registration rejects it.

Domain checks bind ordering, identity and causal times to the original quality
report, reject malformed/negative values and retain actual zero volume. Native
managed Parquet checks observe USD 10.2m/20.2m for 10m volume at 1.02/2.02, while
a separate actual managed Sealed task emits no bar detail. This is historical
volume valued at the closing price, not actual trade-by-trade turnover, future
liquidity, an impact model or DATA_BACKED cost eligibility.

`verify-HdC300` passed check/fmt/strict Clippy and 210 checks: 129 contracts/domain,
47 native science, 12 Store, 1 source SQL and 21 HTTP/CLI. After adding the explicit
Sealed-metadata regression, all 6 native_outputs tests passed. `web-verify-7lKGaV`
passed six twice-identical generated outputs, typecheck, 505 unit, wire/build,
36 settings-browser and 216 full-browser checks. `owner-oci-akszvs` built image
`sha256:79d8c613ee5c34e9601db35857961e3d7aab48772365e0ee360343e5035f31d0`;
all 12 actual OCI tests passed. The original catalog-backed portfolio test also
submits a separate native DATA_VALIDATE task, downloads its original manifest/body,
checks task binding and verifies the native observations. `verify-VhQt4r` then
passed static gates and 198 checks: 4 portfolio unit, 34 native validation,
136 Store and 24 HTTP/CLI. Sources stayed unchanged within each verifier.

This implements the native observation producer, NOT the complete liquidity
adapter. ExecutionAssumptions/Mandate/Build still need adopted original report
consumption, source/decision/currency/validity checks and publication revalidation.
Metadata registration also creates same-schema quality artifacts without a native
producer; schema names alone must not qualify them as measured native evidence.
Existing participation/cost refusal gates remain intact. Complete cost adapters,
independent Candidate validation, delivery, full REAL/PIT acceptance and final
current-head review/CI/merge gates remain required.

## Original temporal Universe groups in portfolio builds, 2026-09-13

The patch over `5c7fd0f2` binds existing native group constraints to original
Forward Universe membership. Optional groups distinguish unknown classification
from an explicitly empty set. Grouped builds resolve each asset's unique active,
already-available member at the original decision cutoff, within Universe coverage
and selection time. Missing, future, expired, overlapping or unknown classification
and requested groups without participating assets fail admission. Ungrouped builds
do not require classification. No classification engine, new optimizer or dependency.

Frozen assets retain these groups through the existing Clarabel constraints and
native result binding. Candidate publication rereads the original registration at
the same cutoff. Immutable selection/group mismatch is an Integrity failure that
preserves retry, not a final INVALID Candidate. The shared registered-metadata
reader likewise treats malformed original bytes as corruption; current eligibility
and permission checks retain their existing separate behavior.

`catalog_groups` covers temporal boundaries, ambiguity, unknown/explicitly empty
classification and group bounds validation. `verify-R39AAI` passed the original
qualified Build/Candidate/LAST_TARGET PostgreSQL chain. The expanded negative case
in `verify-8oxxKV` rejects both same-size changed group IDs and malformed group text,
without modifying original files, then publishes successfully with the original
source. That evidence run passed check/fmt/strict Clippy and 197 checks: 4 portfolio
unit, 33 native validation, 136 Store and 24 HTTP/CLI. `verify-bfpZuB` passed the same
static gates and 208 checks: 128 contracts/domain, 46 native science, 12 Store,
1 source SQL and 21 HTTP/CLI, with unchanged source during each run.

`web-verify-v2ZhGq` regenerated six outputs twice byte-identically and passed
typecheck, 505 unit, decimal/counter/fraction wire, build, 36 settings-browser and
216 full-browser checks. `owner-oci-kaEdwR` built image
`sha256:77f9c6823fd15cc5b90ccf5756dddc71cc84d023be9bd1c33cedf5c4318a2d2d`;
all 12 actual OCI tests passed. The four portfolio modes resolve synthetic original
metadata groups before submitting the real catalog/Wasm task and verify retained
groups/bounds in its native output. These are controlled SYNTHETIC numerical and
protocol checks, not REAL/PIT or complete qualification/Issue #62 acceptance.
Data-backed costs, liquidity/participation, independent Candidate validation,
delivery and remaining development/acceptance/review/CI gates remain required.

## Native CVaR risk budgeting and original dual witness, 2026-09-13

The patch over `37305c7d` extends the existing explicit risk-budget settings to
CVAR, preserving the original confidence and complete equal-weight scenarios.
The native Clarabel LP minimizes empirical CVaR with a weighted-geometric-mean
budget constraint assembled from native three-dimensional PowerConeT objects.
Original asset order and Decimal prefix shares determine the cone coefficients;
zero shares fix zero weights. A common return scale does not change normalized
targets. No new dependency, numerical optimizer or scenario selection algorithm.
First-stage gap uses min(solver_tolerance, exposure_tolerance squared); this does
not relax feasibility, publication or inaccurate-status authorization. The second
stage retains the existing fixed-weight constraints/fees and shared iterations.

Successful CVAR budgets preserve the first native scenario duals in
cvar_risk_budget_witness. Publication checks finite nonnegative probability mass,
sum, original tail cap, empirical CVaR optimality and Euler contributions using
the original returns and stored Decimal weights. Tail ties permit valid dual
choices, not arbitrary chosen scenarios or normalized/clipped witnesses. Other
objectives and failed results cannot carry the witness. Nonpositive risk does
not become a budget; actual unbounded/zero-risk failures have no targets. Ordinary
MIN_RISK/MAX_UTILITY CVaR continues to permit negative risk.

Initial exponential-cone and coarse-gap attempts failed the contribution check;
they were removed. Native tests now cover confidence 0.6/0.7/0.8 with analytical
weights [1/3,2/3], [1/2,1/2], [2/3,1/3] for shares [0.2,0.8], original dual weights
at a tail tie, fraction/tail/dual corruption, costs, tiny returns, normal and tiny
infeasible bounds, iteration exhaustion, unbounded gains, zero risk, zero shares
and SHORT direction. A three-asset power-cone chain independently gives
[8/19,6/19,5/19], including reordered budget IDs. The managed task reads actual
synthetic falling Parquet bars and Wasm and republishes the original witness.

`verify-nfXgbv` passed check/fmt/strict Clippy and 207 checks with unchanged source:
127 contracts/domain, 46 native science, 12 Store, 1 source SQL, 21 HTTP/CLI.
Real PG cases reject missing portfolio-cvar-risk-budget/1 or POWER_CONE without
partial writes; the existing portfolio-cvar/1 and LINEAR_PROGRAM gate remains.
`web-verify-zh5k9m` passed two byte-identical generations, typecheck, 505 unit,
wire/build, 36 settings-browser and 216 full-browser checks. The AntD form keeps
exact budgets/confidence across unknown-response retries and preserves clearing
rules when switching away from a budget objective.

`owner-oci-e8TuZm` built image
`sha256:30c55038fe75f5f950ef08af26d587047af6eda7a22ee881eaedf14e85820b6c`;
all 12 actual OCI tests passed with unchanged source, including original falling
catalog/Wasm CVaR risk budgeting and independent domain witness verification.
These are SYNTHETIC numerical/protocol fixtures, not REAL/PIT, qualification or
full Issue #62 acceptance. Complete cost/source adapters, independent Candidate
validation, delivery and remaining acceptance/review/CI gates are still required.

## Native variance risk budgeting, 2026-09-13

The patch over `734fbe44` implements explicit VARIANCE risk budgets: original
asset IDs, nonnegative shares totaling exactly one, LONG/SHORT directions and
positive risky gross exposure. No equal-budget or gross defaults. Clarabel's
native second-order cones maximize a shared contribution lower bound under a
unit-risk gauge; DESIGN records the equivalent product constraints and proof.
The existing nalgebra Cholesky factor and one common covariance scale are reused.
Normalized native weights become equalities in the original constrained portfolio
problem, retaining cash/exposure/groups/turnover/participation/risk bounds and
fees. Both stages share the frozen iteration budget and original solver tolerance.
Publication checks stored Decimal weights against original covariance, actual
contributions, direction and gross; incompatible constraints do not get clipped.

The initial exponential-cone implementation failed correlated-contribution
checks; tighter stopping tolerances then produced unauthorized AlmostSolved.
`owner-oci-LDN9qy` caught the small-covariance catalog failure (10 of 11 passed).
That implementation and its special gap tolerances were removed, not kept as
a fallback. The native quadratic-cone replacement passed 54 direct checks:
11 job unit, 23 allocation, 10 managed and 10 validation. Numerical cases include
diagonal and correlated analytical solutions, unequal shares, original ID order,
fees, short directions, zero shares, impossible constraints, iteration exhaustion
and the actual small-variance catalog regression.

`web-verify-PXoMK5` passed two byte-identical generations, typecheck, 505 unit,
wire/build, 36 settings-browser and 213 full-browser checks. AntD preserves exact
Decimal budgets through unknown-response retry and clears old budgets on objective
switch without defaults. `verify-JVEyRZ` found one Clippy indexed-loop warning;
after the standard iterator change, `verify-XMVN01` passed check/fmt/strict Clippy
and 203 tests: 127 contracts/domain, 43 native, 11 Store, 1 SQL and 21 HTTP/CLI.
Real PG tests reject either missing portfolio-risk-budget/1 or SECOND_ORDER_CONE
without partial writes, then save/reread the original budget when both exist.

`owner-oci-aSojeY` built and verified
`sha256:3d17269c2cff561267dd506053adfa68e9a2d7372b446e9403fe60f6389c4fb8`;
all 11 actual OCI tests passed with unchanged source, including the original
catalog/Wasm risk-budget task and domain contribution recheck. These remain
SYNTHETIC numerical/protocol inputs, not REAL/PIT/qualification/full acceptance.
CVaR risk budgeting, complete cost/source adapters, independent Candidate
validation, delivery and remaining Issue #62 work are still required. No merge
or closure claim; latest-head review/CI gates apply after all development.

## Native scenario CVaR, 2026-09-13

The patch over `c17edbd2` adds CVAR MIN_RISK/MAX_UTILITY through the existing
Clarabel LP solver, not a replacement optimization algorithm. Confidence is an
explicit frozen Decimal in optimizer.parameters.cvar_confidence, required for
CVAR and absent/null for VARIANCE. Every original aligned return-history column
is one equal-probability loss scenario; covariance/Cholesky do not run for CVAR.
The original shared-capital, exposure, turnover, group and participation rows
remain, with the optional CVaR bound in the same native linear problem.
Publication recomputes the empirical expected shortfall using native ndarray
losses and standard-library order selection, retaining fractional tail mass,
ties and negative risk. No Gaussian fit, annualization or default confidence.

Direct native allocation/validation tests passed 31 cases. Independent examples
yield [2/3,1/3] at confidence 0.8 versus [1,0] at 0.6; utility plus risk bound 0.08
yields [0.6,0.4], while bound 0.06 is infeasible and targetless. Publication tests
reject corrupted targets and cover fractional tails, ties and confidence
0.999999999999999999. Constant positive-return scenarios solve with negative
risk despite singular covariance. Invalid/missing/mismatched confidence rejects.

`web-verify-6V9741` passed reproducible generation, typecheck, 505 unit tests,
wire checks, build, 36 settings browser checks and 210 full browser checks.
The AntD form requires explicit confidence, preserves exact decimal retry and
clears it when changing risk measure. No Web/API source changed after this run.
`verify-MyMy0X` caught an OCI-test temporary-reference lifetime error; after the
test-only correction `verify-nyRA4q` passed check/fmt/strict Clippy and 199 tests:
127 contracts/domain, 40 native science, 10 Store, 1 source SQL, 21 HTTP/CLI.
The real PG capability cases require both portfolio-cvar/1 and LINEAR_PROGRAM,
leave no partial Mandate on rejection, then save/reread the original constraint.

`owner-oci-CkO1UU` built image
`sha256:c7f6cb8d8449fc199f30e1bc8a7768334415cc2790436c324e620399f678fb08`;
all 10 actual OCI tests passed with unchanged source, including the new original
catalog/Wasm CVaR task and existing variance-bound task. Native results preserve
risk/confidence and satisfy the domain publication check. Inputs remain explicit
SYNTHETIC numerical/protocol fixtures, not REAL/PIT or full qualification evidence.
Risk Budgeting, complete cost/source adapters, independent Candidate validation,
delivery and remaining Issue #62 acceptance are still required. No merge claim.

## Native per-horizon variance bound, 2026-09-13

The patch over `a510ba0c` implements the optional positive VARIANCE bound with
the installed nalgebra Cholesky factor and Clarabel 0.11.1 second-order cone.
The existing ndarray-stats covariance adapter moves to domain ownership; solver
and publication reuse it, without a second covariance implementation or wrapper.
Publication recomputes variance from stored Decimal targets and frozen returns;
its allowance is bound times exposure_tolerance, not absolute variance tolerance.
Mandate creation and Build admission share the capability gate requiring both
portfolio-variance-bound/1 and SECOND_ORDER_CONE. The AntD form preserves the
optional exact Decimal through failed-response retry; API descriptions and user
instructions specify per-decision-horizon variance, not annualized volatility.

Native analytical cases check correlated covariance [[1,1],[1,5]], binding
weights [0.75,0.25] at variance 1.25, rejection of corrupted weights, an infeasible
0.9 bound with no targets, and relative publication tolerance at tiny variance.
`verify-ohXuq4` exited 0 with unchanged source: check/fmt/strict Clippy and 195
tests (127 contracts/domain, 37 native science, 9 Store, 1 source SQL, 21 HTTP/CLI).
The controlled PostgreSQL test rejects either missing capability without saving
a partial Mandate, then saves and rereads the original constraint when both exist.
`web-verify-bQCE0G` exited 0: reproducible generated artifacts, typecheck, 505 unit
tests, wire checks, build, 36 settings browser checks and 210 full browser checks.

`owner-oci-pslvBN` built and tested image
`sha256:73f23e90d905aaca4d5cadeab5a43e83b02fb95df6f2c9b542ab6c355702ff79`:
all 9 actual OCI tests passed with unchanged source. Its original catalog/Wasm
portfolio test now executes with a variance bound, checks the new manifest
capability and reapplies domain validation to the actual result. Market/model
fixtures remain SYNTHETIC, not real market acceptance or qualification evidence.
CVAR, Risk Budgeting, complete cost/source adapters, independent Candidate
validation and delivery remain required; this patch is not Issue #62 completion.

## Original Candidate and LAST_TARGET recovery, 2026-09-13

The test patch over `0c1b268b` extends the same two-original-qualification chain
through native-result adoption and Candidate publication. It verifies original
members, one target, VALID/OPTIMAL with SYNTHETIC Paper provenance, refusal to ACK
before publication, and exact Candidate replay without reads or writes. Native
result values are explicitly controlled, not claimed solver or market execution.

A second Build consumes this published Candidate as LAST_TARGET. Reusing the old
catalog snapshot correctly fails: a later InputSet cutoff cannot expand its
attested visibility to include the newer Candidate. A newly registered Forward
snapshot permits the new decision without modifying the old snapshot or target.
Failure at the second publication (derived weights then parameters) leaves Run,
task, artifact-row and reserved-CPU counts unchanged. The original cleanup API
removes only those newly allocated, unpublished files. Retrying the same command
publishes both objects and a new VALID Candidate retaining the original source
Candidate ID, LAST_TARGET marker and SYNTHETIC origin, with no snapshot-source FK.

`verify-GHVGVv` / `verify-YW4mVS` exposed the old-snapshot cutoff rejection; it is
now an explicit negative case, not a weakened production gate. The focused retry
`verify-tDDXCG` passed. Final `verify-R3Zfuq` exited 0 with unchanged source:
check/fmt/strict Clippy and 195 tests (4 portfolio unit, 31 native-validation,
136 Store, 24 HTTP/CLI). No product code, API or native ABI changed in this patch.
Actual full native science/market acceptance, remaining product functionality,
review/CI gates and main verification remain required; no merge/closure claim.

## Source-bound portfolio admission, 2026-09-13

The patch over `c4f58614` extends the two-qualified-source test through actual
Forward registration, frozen input, original downstream PAPER weights submission,
Mandate creation, Portfolio Build admission/replay, native claim and JobSpec.
Every referenced artifact is read from the original local object store. The fee
assumption is created through its real Store API before freezing the original
policy; no policy/qualification/source rows are rewritten to fit a later Mandate.

This exposed a production defect: the execution-assumption API stores declared
settings as SYNTHETIC parameters, while Build required that artifact to be REAL.
Build now consumes the original declared parameter type, still checks exact
settings, original immutable source relationships and license, and retains the
separate REAL/PIT requirements for Forward data and Alpha qualifications. The
test explicitly checks the fee parameters remain SYNTHETIC. No market-origin
upgrade, new dependency, API or native ABI was introduced.

`verify-sL6MlR` first rejected a Research-only test grant for Forward; the original
controlled grant now explicitly allows Research + Paper. `verify-Vx6zo2` then
reproduced the incompatible cost-origin rejection before the production fix.
Final `verify-YWmssJ` exited 0: check/fmt/strict Clippy, 4 portfolio unit, 31 native
validation, 136 Store and 24 HTTP/CLI tests (195 total). `verify-vieWae` also exited
0 for the actual App Server independent-Reviewer qualification regression, with
the new original source-bound assumptions. Source stayed unchanged during both.

Runtime declarations, market bytes and model answers remain controlled protocol
input; this is not genuine REAL/PIT, full native science/OCI or T42 acceptance.
Positive Candidate publication on this complete source chain and the remaining
Issue #62 functionality/acceptance are still required. No GitHub write or merge.

## Two original qualified sources, 2026-09-13

The test patch over `cf50a166` prepares two original trials in one project/Cycle
and frozen policy. Each uses actual Store compilation/forecast/Validation admission
and result publication, then its own calibrated Alpha version and result feedback.
Research ACK admits the independent Reviewer; both original review targets receive
separate bounded turns before two Sealed jobs are queued together under the frozen
parallel limit of two. Reviewer completion precedes those jobs' execution, matching
the Worker order. Original Sealed result publication and competing ACKs produce
exactly two distinct qualifications, versions and qualifying evaluations, with
both active Alphas QUALIFIED. No qualification or evaluation rows are hand-authored.

All native result bytes, session/turn receipts and REAL/PIT runtime declarations
in this test are controlled protocol input. It exercises real PG/PGMQ and artifact
files, not model inference, native numerical execution or genuine market evidence.
`verify-A7bdzM` exited 0 with unchanged source: check/fmt/strict Clippy plus 4
portfolio unit, 31 native-validation, 136 Store and 24 HTTP/CLI tests (195 total).
Full positive Portfolio Build/Candidate acceptance and all remaining Issue #62
development still remain; this evidence is not release, merge or closure approval.

## Original reviewed qualification transaction, 2026-09-13

The test patch over `8f9fbc3b` reuses the original Mission/Validation/independent
Reviewer/Sealed flow for two controlled runtime declarations. The existing FIXTURE
case still receives no qualification. The REAL/PIT declaration is supplied before
immutable registration, never by rewriting a stored fixture. Its original reviewed
Sealed PASS registers one qualification despite ACK replay, keeps the original
calibrated active version, and does not extend either scientific evidence deadline.
Provider responses and scientific bytes remain controlled: this tests actual
PostgreSQL/PGMQ, artifact files and native App Server control flow, NOT genuine
market provenance, native scientific computation, paid-account acceptance or T42.

`verify-geOCmt` exited 0: check/fmt/strict Clippy and all 26 Mission/profile/MCP
tests passed, including both origin branches. The shared-fixture regression
`verify-7k08Gf` also exited 0 for check/fmt/Clippy and evidence-related native,
Store and HTTP/CLI tests. Handwritten source stayed unchanged during each run.
No production gate, interface, dependency or native ABI changed. Positive complete
Portfolio Build admission, actual REAL/PIT scientific acceptance and all remaining
Issue #62 work are still required; this checkpoint authorizes no merge or closure.

## Original LAST_TARGET weight source, 2026-09-13

Working source over `022ece07` replaces the build intent's snapshot-only field
with the strict FORWARD_SNAPSHOT / LAST_TARGET reference union. Migration 052
keeps original requests and references, adds the prior-Candidate FK and requires
exactly one source. Admission and publication share the same source resolver.
LAST_TARGET checks the published original Candidate, target file and every stored
target/cash/currency/time value. The derived weights retain original file order,
decision/publication availability and deadline; they never become account positions
or upgrade a SYNTHETIC source to REAL. Derived weights and parameters share the
original transaction and multi-object rollback cleanup; no new queue or native ABI.

`verify-wDb0q3` exited 0 with check/fmt/Clippy and 194 evidence-related tests,
including real PG/file LAST_TARGET tests for publication sealing, source/order/time
preservation, foreign references, altered file weights and native source XOR.
`verify-C7Rykr` also exited 0 for contracts/domain, native allocation, Store and
HTTP/CLI command/grant coverage. `web-verify-cOSeGY` reproduced all six generated
outputs and passed typecheck, 505 Vitest, Node/wire, build, 36 dedicated and 210
full browser cases; handwritten source stayed unchanged during verification.
The source fixture is explicitly SYNTHETIC relational evidence, not native execution
or positive REAL qualification. Full successful Build admission and derived-file
failure recovery through that full path still require acceptance; this is not T42.

## Candidate publication and read workflow, 2026-09-13

Local commits `7dd87b75` and `27ac4787` bind one immutable Candidate to the original
terminal Build Run, with original task/manifest/report checks and atomic members,
targets and diagnostics. ACK is refused before publication. Real PG/files tests
cover cancelled publication rollback/cleanup/concurrent replay and original
controlled successful report adoption followed by expired-target INVALID without
rewriting SUCCEEDED/OPTIMAL. Mismatched report Alpha identity fails before writes.
Controlled source bindings are not positive scientific admission or native OCI proof.

Working source over `27ac4787` adds original Candidate header/detail contracts,
Store, GET list/detail HTTP endpoints, native `portfolio candidate list/show` and
React/Ant Design candidate tab. Reads expose only sealed snapshots and original
references, not evaluator bytes, storage locators or new authority. PG tests cover
uncommitted invisibility, stable cursor identity and exact decimal/member/target
reads; real CLI/TCP/HTTP tests cover scoped reads and foreign-project denial.
Browser tests preserve decimal strings, keep invalid candidates targetless, check
keyboard scrolling, and return to the original page after a next-page read failure.

`verify-QVbOm6` exited 0: check/fmt/strict Clippy, 3 portfolio internal, 31 native,
135 Store and 24 HTTP/CLI tests passed. `web-verify-v0BR7C` exited 0: all six native
generated outputs reproduced, typecheck, 505 Vitest tests, Node/wire checks, build,
36 dedicated browser and 210 full browser cases passed with handwritten source
unchanged. Earlier new-table accessibility failures were fixed, not suppressed.
This does not complete positive REAL qualification/build admission, source-change
concurrency, independent shared-capital validation, Release or full Issue62 acceptance.

## Portfolio build admission and command boundary, 2026-09-13

Working source over `22c4d123093edae586583130ca462097139d2711` adds source-reference
PortfolioBuildRequestV1, Operator command/grant, migration 050 and the Store
transaction on the existing Cycle budget/PGMQ/native task binding. HTTP
`POST /api/v2/portfolio-builds` and native `client portfolio build` use the existing
bounded object read/publication and Operator-lock cleanup. The request cannot supply
forecasts, current positions, model paths or costs. Original Mandate, Forward data,
downstream weights, qualifications, independent Reviewer PASS, REAL evaluation
report/Attempt, native parameters/image and current licenses are checked in code.
Qualification rows use FOR UPDATE, including conflict with revocation FK key-share;
checks after file publication repeat eligibility and expiration. Cost document bytes
must equal immutable original settings. Current adapter only accepts conservative
BAR with explicit zero slippage and original taker fees; remaining full-cost/group/
liquidity/participation adapters are not implemented or silently relaxed.

`verify-JFADjl` exited 0 with source unchanged: all-target check/fmt/strict Clippy,
contracts/domain, 34 numerical/native, 8 Store, one original qualification SQL
preparation and 20 HTTP/CLI tests passed. New domain cases reject duplicate
qualifications, non-unit/negative weights, unbounded limits and caller-invented
weights. Real PostgreSQL verifies missing-Cycle admission publishes no file, Run or
receipt. The real TCP/native CLI test verifies exact human grant, failure retry
without consuming it, changed-intent rejection and no admitted Run. An earlier
test used numeric Runtime revision instead of its string contract; corrected.
The SQL preparation check proves PostgreSQL accepts the actual source query,
not that a valid REAL qualification was created or successfully admitted.

`web-verify-gymnbK` full verification exited 0, six generated outputs reproduced,
handwritten sources unchanged: type/unit/build/wire/native help and both browser
suites passed. This phase does NOT prove successful original qualification-to-build
admission, concurrent revocation behavior end to end, Candidate publication, shared
capital evaluation, Release or T42. Those are required next, alongside the remaining
full Issue #62 scope. No GitHub CI/review/merge or Issue closure is claimed.

## Authenticated downstream current-weight ingestion, 2026-09-13

Working source over `623f033aa6537659291b1cf4a1883ed2a31b99c1` adds the strict
Forward weights HTTP/native CLI contract and migration 049. Store uses the existing
DOWNSTREAM/FORWARD_SUBMIT identity, exact project/integration/environment, original
external message receipt and project transaction lock. It publishes the immutable
REPORT bytes and source relationship atomically, rejecting changed-message replay,
invalid exact weight totals and unavailable/expired observations. Failed or unknown
publication cleanup takes the same project lock before inspecting committed object
references. No new dependency, queue, account ledger or execution control is added.
PAPER is SYNTHETIC; LIVE records an authenticated external source, not independent
research evidence or portfolio qualification. Existing rows are not rewritten.

`verify-gRSpuO` exited 0 with unchanged sources: all-target check, formatting and
strict Clippy; contracts/domain, 34 numerical/native tests, 7 Store tests and 19
HTTP/CLI tests passed. The three new real PostgreSQL tests cover concurrent original
message replay, changed content, project/environment rejection, immutable rows,
rollback, and cleanup waiting for both committed and failed producers. The new real
TCP/native CLI test uses native authentication, actual object files and PostgreSQL;
it verifies original bytes, source identity, PAPER provenance and changed-message
409 even with a different transport key. Integration registration is controlled,
not proof that an external downstream service was contacted. An initial test-only
Id conversion compilation failure was fixed before this final verification.

`web-verify-c5wBPM` full verification exited 0 with handwritten sources unchanged
and six generated outputs reproduced byte-for-byte. Typecheck, unit, build, wire,
native CLI help and both browser suites passed. Existing HTTP schemas are unchanged;
the new route and its reachable types are added by the native generator. Native job
and OCI image sources did not change in this phase. Formal source resolution into
Portfolio admission, current qualification/policy/license checks, last-Candidate
resolution and Candidate publication still remain, along with the other full
Issue #62 requirements. No GitHub review, merge or Issue closure is claimed.

## Original current-weight inputs for native portfolios, 2026-09-13

Working source over `c4367087245e6c154e8cbbc34a29c1e6d165f791` replaces managed
portfolio's standalone current_cash_weight with an explicit current-weight REPORT
identity and frozen PortfolioCurrentWeightsV1. The native job reads that exact
object and compares its typed document before catalog/model execution. Source
is FORWARD_SNAPSHOT or LAST_TARGET, never an implicit all-cash fallback; original
timestamps, currency, ordered asset weights and cash must match the request and
Mandate's freshness/tolerance. Domain output adoption binds the same frozen cash
and asset values. This is input consistency, not downstream identity verification
or a claim that LAST_TARGET represents actual positions. No account/NAV/credential
fields or new numerical dependency were added. Single allocate stays numerical.

Managed subprocess regression passes 9/9 (1.85s): missing REPORT, equal-length
original-document substitution, future/expired snapshot, wrong currency and
mismatched weights are rejected while original catalog/model allocation works.
An initial test modified only in-memory spec; corrected to write the subprocess's
actual spec. Broader Runtime catalog-scope tests then required their own explicit
weight report; fixtures were updated, with all original six-operation rejection
cases retained. Final `verify-W3mFvO` exited 0 with source unchanged: all-target
native-feature check/fmt/strict Clippy, 153 contracts/domain/runtime, 9 managed,
31 native Codex and 92 Job tests passed (overlapping subsets).

`web-verify-tuHPTz` exited 0: six generated outputs reproduced byte-for-byte
(only domain OpenAPI changed), handwritten sources unchanged; type/build/wires/
help, 505 Vitest/5 Node, 36 dedicated and 201 full browser tests passed.
`owner-oci-8nfObR` built portfolio-weights/1 image
`sha256:c1975242cd381a40957251bc9c138cc54f11d960e9f040f65a607bbcbd71be59`;
9 real OCI tests passed in 14.66s, including the original weight REPORT in the
portfolio job. The fixtures do not prove real downstream ownership or qualified
portfolio admission. Store-side original snapshot/last-Candidate resolution,
current qualification/policy/license checks and Candidate publication remain
required, alongside the other outstanding Issue #62 work. No merge or closure.

## Explicit native execution models, 2026-09-13

### Source-bound execution assumptions authoring, 2026-09-13

Working source over `c440135c128eeb83b8faa5d44822d60ddbd71854` adds immutable
Operator execution-assumption creation, project pagination and original reads.
The transaction binds frozen non-Sealed inputs, registered Dataset metadata,
current Runtime/image/model capabilities and exact native currency/maker/taker
fees. Original settings are published as PARAMETERS, linked by migration 048's
immutable source relation. Publication failure rolls back; original command
receipts replay without re-reading/re-publishing. Historical data is not rewritten.
This declarative path records conservative BAR assumptions only, not DATA_BACKED,
liquidity/participation evidence, qualification or a deliverable Candidate.

A real native serialization regression first disproved the old flat type/id
assumption. Registration checks, Runtime venue discovery and fee binding now read
the original externally tagged Rust InstrumentAny structure through one domain
helper. Actual CurrencyPair serialization and simulation tests passed 10/10 in
3.65s. No Python conversion, fallback fee, RNG or matching algorithm was added.

Final `verify-hUiruG` exited 0 with unchanged handwritten sources: all-target
native-feature compilation, formatting and strict Clippy; contracts/domain,
34 native science, 4 Store and 18 HTTP/CLI tests passed. New cases include actual
PG immutable source/settings/receipt behavior, forged fees/source rejection and
failed publication, real authenticated HTTP creation/reads/replay/empty/missing,
and CLI subprocess -> TCP -> exact human grant -> PG creation/replay/reads.
Catalog/probe observations in these relational tests are controlled fixtures,
not REAL-market evidence. Shared CLI transport helpers avoid duplicate modules.

`web-verify-lgjFjA` exited 0: six native outputs reproduced byte-for-byte,
typecheck/build/wires/help, 505 Vitest/5 Node, 36 dedicated and 201 full browser
tests passed. The Ant Design execution-assumptions tab preserves exact decimal
values and integer strings, explicit model parameters/seed, offline and dirty
input handling, original retry keys and original artifact details. Its new
contract tests pass on desktop/tablet/mobile including accessibility. Subsequent
changes were Rust test organization and documentation, not frontend source.

`owner-oci-w9JEEa` rebuilt image
`sha256:8ac35f36fd1e6b4745fc4c20f23bc0453c0195e7a56f0e53e5cc2e01c0e44458`;
all 9 existing real OCI tests passed in 15.01s, source unchanged. This validates
the updated Runtime catalog handling, not complete assumptions-to-Candidate
publication. Full DATA_BACKED/other price modes, liquidity/participation,
authoritative qualified portfolio admission, Candidate/Release/Claim/Forward,
positive REAL qualification and remaining W0-W8/T01-T42 still require delivery.
No final GitHub review, merge, Issue closure or release-ready claim was made.

Working source over `ced7fcadbcef1fb8019d3a9119e54e7fb8cac174` removes implicit
venue fill selection and the standalone insert-latency setting. Simulation now
requires exact fee/fill/latency NativeModelRefV1 roles, classes and 0.63.0 versions.
Closed parameters require explicit fill/slippage probabilities and RNG seed,
base/insert/update/cancel latency; aggregate insert delay is positive and sums are
checked before calling upstream. Native DefaultFillModel, MakerTakerFeeModel and
StaticLatencyModel receive these values in actual SimulatedVenueConfig. Target
causality/expiry uses the same aggregate insert delay. No new dependency, model
fallback, compatibility field or handwritten fill/fee/RNG algorithm was added.
Runtime and image declare simulation-models/1; producer requests and independent
output bindings validate the model roles. Original catalog fee matching remains.

Direct actual `job --test simulation` passed all 9 tests in 3.34s, including
slippage changing the same shared account, fixed seed repeating the result,
equivalent base delay and rejected roles/classes/versions/probabilities. These are
controlled synthetic market inputs, not REAL provenance or qualification.
Initial `verify-x5um5q` found a large enum variant and the old three-model schema
assertion. SimulatePortfolio request now uses the existing boxed-request pattern;
all six native identities and closed parameter schemas remain asserted, no lint
suppression. Final `verify-78PYyY` exited 0: check/format/strict Clippy, 153 contracts/
domain/runtime, 8 managed subprocess, 31 native Codex and 90 Job tests passed,
zero failed/ignored (overlapping subsets), source unchanged.

`web-verify-RtwvqJ` exited 0: six native outputs reproduced byte-for-byte, handwritten
source unchanged, typecheck/build/wires/CLI help, 505 Vitest/5 Node, 36 dedicated
and 198 full browser tests passed. No HTTP route changed; model/request schemas
changed in domain/API/runtime OpenAPI, TypeScript and Ajv JavaScript outputs.
`owner-oci-ryyW3Y` built actual image
`sha256:944a190112ab011f6eeea75ca9315d33f0f31e3487dce5322fefbefddff93e30`;
9 existing native OCI regressions passed in 14.53s, zero failed/ignored, source
unchanged and new model capability verified. The new execution-model numerical
counterfactuals run in actual local Job subprocesses, not those OCI regressions.
Only this entry and the reuse-source note changed after verification.

This is a prerequisite, not formal execution-assumption authoring or trusted
portfolio admission. Original fee/weight sources, qualified member assembly,
Candidate simulation/publication, Release and all remaining acceptance gates
still require implementation. No push/review/merge/Issue closure occurred.

## Original qualification history, 2026-09-13

Working source over `80428ea256e3ad9a76b7cafc2890e175bc6c3a7b` exposes original
qualification metadata through the existing version-scoped evidence authority,
GET qualifications, native alpha qualifications CLI and an explicit Ant Design
history drawer. One SQL statement snapshot/clock returns exact grants, original
policy/evaluation references, expiry and earliest revocation, including scheduled
future revocations. Historical expiry/revocation does not remove the original row.
grant_window_open checks only the grant/revocation interval, not current policy,
Alpha lifecycle, REAL/PIT, licenses or portfolio admission. No Sealed report bytes,
metrics, new grant, approval or delivery authority are returned.

`verify-yYlQvS` exited 0: workspace check/format/strict Clippy, 30 native scientific,
131 Store and 23 HTTP/native CLI tests passed, zero failed/ignored; handwritten
source unchanged and owned PostgreSQL stopped. The new PostgreSQL test uses
explicit relational fixture grants, not scientific qualification; it checks
original identity/expiry, scheduled and effective revocation, expired history and
cursor pagination. HTTP checks empty/missing versions and invalid parameters;
native CLI checks routing/error decoding and the shared command tree.

`web-verify-hZjiOx` found inaccessible horizontal scroll content on mobile/tablet
in the new drawer. The existing table-header focus pattern fixed the actual
keyboard issue without changing the check. Final `web-verify-fVoToY` exited 0:
six generated artifacts reproduced byte-for-byte, handwritten source unchanged,
typecheck/build/numeric wires/CLI help, 505 Vitest/5 Node, 36 dedicated and 198 full
browser tests passed. The new three-viewport test covers explicit loading, exact
version, paging, scheduled revocation, failure refresh/stale data, recovery and
accessibility. Only domain/API OpenAPI, TypeScript and Ajv JavaScript generated
files changed. This evidence entry is the only subsequent handwritten change.

No native Job/Runtime implementation or image changed in this stage. Qualification
history does not implement trusted portfolio admission or Candidate/Release;
execution-assumption authoring and original cost/weight source assembly still need
implementation alongside the remaining full acceptance scope. No push, GitHub
review request, merge or Issue closure occurred.

## Catalog-backed native portfolio inputs, 2026-09-13

Working source over `7253784e58c382848029e462dbf2ca277626587d` replaces the managed
caller-filled AllocationInput with a fixed Forward catalog selection, original
Wasm/calibration artifacts, frozen Mandate, member weights and current-weight/fee
inputs. Existing catalog/forecast/frozen-calibration/ndarray/Clarabel functions
produce and validate the numerical input inside the native job. Common windows
must match exactly; no filling, intersection, recalibration or fallback targets.
The output is qz.native_portfolio/1 with generated input, allocation and observable
fuel. Runtime catalog scope, model roles and original-request output bindings are
checked; capability is portfolio-models/4. Local allocate remains a numerical CLI,
not an authoritative Candidate route. Old managed input/output is not retained.

Initial `verify-MqSeon` exposed an obsolete NativeJsonOutput assertion; the assertion
now names the new report. Final `verify-QYiKAD` exited 0: workspace check/format/
strict Clippy, 153 contracts/domain/runtime, 8 managed subprocess, 31 native Codex
and 89 Job tests passed, zero failed/ignored (overlapping subsets). Runtime catalog
scope tests include all six operations. `web-verify-SaT9p2` exited 0: six generated
outputs reproduced byte-for-byte, handwritten source unchanged, typecheck/build/
numeric wires/CLI help, 505 Vitest, 5 Node, 36 dedicated and 195 full browser tests
passed. Only domain/runtime generated OpenAPI changed.

`owner-oci-lbWQ4X` built image
`sha256:a5fc24e19fd5d27ab2947d9584015fd605423eea4e7f78cda4900a16f0a5f75c`;
all 9 actual OCI tests passed in 15.21s, zero failed/ignored, source unchanged.
The portfolio test mounts actual synthetic Parquet, uploads two original Wasm
modules, observes 0.01/0.03 forecasts and their 0.025 weighted ensemble, checks
original return history and optimization, downloads the bound result and replays
the same job. `verify-QvvS4T` exited 0: check/format/strict Clippy, 126 domain,
34 scientific/managed, 2 Mandate Store and 15 HTTP/native CLI tests passed, zero
failed/ignored; source unchanged and owned PostgreSQL stopped. This evidence entry
is the only subsequent handwritten change.

These synthetic inputs do not prove REAL provenance or Alpha qualification.
Trusted Store admission/publication must still bind current qualifications,
policies, licenses, original artifacts and current-weight/fee sources; independent
Candidate simulation, Release and the rest of T42 remain required. No push,
GitHub review request, merge or Issue closure occurred in this stage.

## Original return history enters native covariance and allocation, 2026-09-13

Working source over `439f2f57214c36412f2006c5cc8141831f7f4da8` removes the
caller-provided covariance matrix from AllocationInputV1. Strict return_history
and covariance_estimator now enter the same local/managed allocation path.
Asset/bar order, currency and horizon must match forecasts; completed windows
are strictly ordered, availability is causal at decision time, dimensions and
aggregate size are bounded, and finite simple returns cannot be below -1.
Existing ndarray-stats 0.7.0/ddof=1 estimates the matrix before the original
Clarabel problem. There is no imputation, annualization, jitter or matrix fallback.
Runtime, image and Mandate admission now require portfolio-models/3.

The controlled five-observation fixture has exact sample covariance diag(1,4),
preserving independent 0.8/0.2 and original mixed-forecast 0.82/0.18 oracles.
New checks reject future/early availability, duplicate window ends, mismatched
identities/bars/horizons/currency, missing rows, impossible/nonfinite returns,
wrong model roles and removed matrix/missing-history inputs. Translation invariance
is checked through the actual estimator and solver, not a supplied matrix.

Initial `verify-Xx1IEC` completed functionals but failed two strict Clippy index-loop
warnings. The symmetric native matrix is now traversed with iterators, without
suppression. Final `verify-fgpKLp` exited 0: workspace check/format/strict Clippy,
126 contracts/domain, 34 scientific/managed Job, 2 Mandate Store and 15 HTTP/CLI
tests passed, zero failed/ignored; source unchanged and owned PostgreSQL stopped.
`web-verify-0hiWA0` exited 0: six native artifacts reproducible, source unchanged,
TypeScript, 505 Vitest/5 Node, numeric wire checks, build, 36 dedicated Codex
settings checks and 195 full browser tests passed (overlapping browser subsets).
Only the generated domain OpenAPI bytes changed.

`owner-oci-Juuj7f` built actual image
`sha256:2b579faa6077202822ed911669139fa2b13a6f117fe8cc1cb39f80496bb5dafa`;
all 9 native OCI tests passed in 14.62s, zero failed/ignored, source unchanged,
including original allocation through portfolio-models/3. This evidence entry is
the only handwritten change after that verification. These controlled numerical
histories are not REAL market provenance or proof of qualified Alpha ownership.
Trusted frozen catalog/artifact/license/qualification assembly, complete Candidate
simulation/Release and T42 remain required. Live read-only GitHub verification
still showed draft/open PR63 at remote 37e5713 and open Issue62; no write or merge.

## Native Mandate CLI and shared pagination fix, 2026-09-13

Working source over `abde0edcf7e4afb6fb82336d750f1ecfda39b1e9` adds actual CLI
subprocess/TCP/native browser enrollment/TOTP/Bearer/PostgreSQL coverage for Mandate.
Existing listener, invocation and controlled relational Mandate setup are reused.
The test proves the original one-time human grant, exact creation/replay content
and target, changed-intent rejection, scoped list/detail, cross-project denial and
one persisted version. Runtime observations remain fixtures, not OCI qualification.

`verify-II5c4B` and diagnostic `verify-DHVcQi` exposed a real CLI parser crash:
the derived `List` argument group collided with named `List` subcommands before
network dispatch. One explicit `Pagination` group identity in the shared args fixes
all callers, without renaming public commands or adding compatibility wrappers.
A native Clap check now builds and validates the entire CLI command tree.
`verify-VSD8PZ` confirmed original reads worked, but the new cross-project test
expected 403; existing scoped authority deliberately returns 404. Only the test
expectation was corrected; that existing behavior was not changed.

Final `verify-57PPWa` exited 0 with workspace check/format/strict Clippy,
126 contracts/domain, 33 scientific/managed Job, 2 Mandate Store and 15 HTTP/CLI
tests passing, zero failed/ignored. Source unchanged; owned PostgreSQL stopped
and confirmed stopped. Only this evidence entry changed afterward. No HTTP schema,
generated artifact, browser source or OCI runtime change occurred in this stage.
Full authoritative portfolio assembly, qualification, Candidate/Release and T42
remain required; these checks do not establish current-head GitHub gates or merge.

## Mandate Ant Design authoring and immutable reads, 2026-09-13

Working source over `0b7e9f74bfc51308a01814b8bfb252b614b19d4e` replaces the
portfolio placeholder with actual project-scoped API listing, full configuration
creation and immutable server-content detail. Existing ResourceSelect, Form,
Intent, dirty guard and offline protection are reused, without a new dependency.
Decimal amounts and Runtime revisions remain strings; unused rebalance fields
are explicitly null. Unsupported native objectives/risk constraints are identified,
not substituted. Saving configuration does not start a build or grant qualification.
References currently require existing exact IDs, not inferred first versions.

Initial TypeScript failure referenced a nonexistent named generated parameter
schema; the existing NativeModelRef union now supplies that type. Initial browser
failures were required-label/virtual-option/drawer selectors in the new test;
these were corrected using existing visible-option and semantic-dialog patterns.
Targeted three-viewport creation/replay/detail checks passed before full regression.

Final `web-verify-jUWY6t` exited 0: six native generated artifacts reproducible,
handwritten source unchanged, TypeScript, 505 Vitest and 5 Node tests, three numeric
wire checks, production build, 36 dedicated Codex settings browser checks and
195 full browser checks passed. The dedicated subset overlaps the full suite.
New checks cover exact large decimals/revisions, original lost-response retry,
unused schedule fields, immutable detail accessibility, missing references,
dirty-input confirmation and offline prohibition on desktop/tablet/mobile.
These controlled browser fixtures are presentation evidence, not real portfolio
qualification, PostgreSQL authority or complete Issue #62 acceptance. This entry
was the only handwritten change after that verification.

Specific native CLI grant/transport and reference/permission edge cases,
authoritative forecast/covariance assembly, full supported objectives and complete
Candidate/Release/T42 delivery remain required. No push/review/merge is claimed.

## Immutable Mandate Store, HTTP and CLI, 2026-09-13

Working source over `05db3c874be5451a5e09fbbcd29acb50bbe50832` adds actual
MandateCreate/Content/View contracts, Operator command/grant mapping and migration
047's original-response receipt requirement. Existing immutable Mandate rows,
Operator transaction, project lock and Runtime observation are reused. Same-key
replay returns the original version; new keys allocate monotonic project versions.
No update/delete endpoint or new repository framework is added. Current probe,
three native model versions, CONVEX_QP, execution image, project policy, currency,
capital, fees, liquidity, participation and calendar references are checked.
Reads reuse the Operator/scoped-CLI evidence authorization; no Mission role gains
configuration access. HTTP creation/list/detail and typed CLI routing are wired.

risk_aversion moves from allocation's top level into the frozen optimizer
parameters consumed by the existing native solver. Runtime stack/manifest now
require portfolio-models/2; old input/image compatibility is not retained.

Initial `verify-85bfpH` failed an unnecessary CLI `.into()` inference; `verify-DwHQKs`
failed a test Revision conversion from &str. Both were corrected. `verify-Adu1hQ`
then completed 153 domain/Runtime, 8 managed Job, 31 native Codex and 543 Store/
server executions, all zero failed/ignored, but its overall result failed Clippy
for duplicate test module loading. Owned PostgreSQL stopped; source unchanged.
The duplicate import was removed, not suppressed. Final `verify-dvsaB3` passed
workspace check/format/strict Clippy plus 126 contracts/domain, 33 native scientific/
managed, 2 Mandate Store and 13 HTTP/CLI executions, zero failed/ignored, source
unchanged and owned PostgreSQL stopped. Counts overlap between runs/groups.

New real PostgreSQL tests cover same-key concurrency, versions 1..3, pagination,
immutable rows, changed-intent conflicts, unavailable-probe replay versus new
creation, and reference/model/revision rejection rolling back both rows and
receipts. Real browser middleware tests cover POST201, original replay, reads,
PATCH405, conflict409, extra-parameter422 and unauthenticated rejection. Runtime
observations in these tests remain explicitly controlled relational fixtures.

`web-verify-HH8kqS` generated all six native artifacts twice identically, hand-written
source unchanged; this generate-only run is not browser/typecheck evidence.
The existing built server's `client portfolio mandate --help` exited 0. An earlier
help-only Cargo invocation used the wrong debug profile; that exact build was
interrupted (130), with no rustc left, rather than mistaken for a failed CLI.
`owner-oci-QJVqP3` built native image
`sha256:6367fe330a0d5748bbe2ff119783aea6e9f1af591554f85d3fd68c938f7ec217`;
all 9 actual OCI tests passed (15.12s), zero failed/ignored, source unchanged,
including the original 0.82/0.18 allocation with portfolio-models/2.

Ant Design Mandate forms, specific CLI grant/transport coverage, stronger reference/
permission edge cases, authoritative forecast/covariance assembly and complete
Candidate/Release/qualification/T42 delivery remain required. No GitHub write,
current-head CI/review completion or merge is claimed.

## Explicit native covariance estimator reference, 2026-09-13

Working source over `7e7304eecd96d151fa139431818063ef6c95265b` adds the strict
SAMPLE_COVARIANCE model reference to the existing ndarray-stats 0.7.0 adapter.
Its exact class is ndarray_stats::CorrelationExt::cov; closed parameters require
ddof=1. The original sample_covariance function now consumes that reference and
passes the validated ddof into the native API. No default wrapper, estimator,
annualization, missing-value filling, regularization or dependency is added.

`verify-SxzHOs` passed workspace check/format/strict Clippy, 153 domain/Runtime,
8 managed Job, 31 native Codex and 88 scientific Job executions, zero failed/
ignored and source unchanged. Groups overlap. Actual covariance remains the
independent centered diag(1,3) reference and is translation invariant; invalid
role/class/version/ddof/extra parameters and incomplete/nonfinite rows fail.
Locked upstream correlation.rs confirms rows are variables, columns observations
and native cov divides by n-ddof; the existing minimum-two-observations check
prevents the upstream invalid-ddof panic. The generated model-schema test includes
this third closed variant with exact class/version and typed ddof bounds.

`web-verify-U34Fsk` domain-only generation passed, reproducible and handwritten
source unchanged. No new HTTP/CLI/MCP operation, Runtime capability or OCI evidence
is claimed. Trusted return-artifact assembly, allocation covariance provenance,
Mandate operations and full Candidate/Release delivery remain required.

## Strict native optimizer and ensemble references, 2026-09-13

Working source over `936f607d79ab9a91676cd4c113080b25e99cf00b` replaces allocation's
top-level settings with mandatory optimizer and alpha_ensemble NativeModelRefV1.
Only actual Clarabel QP 0.11.1 and ndarray fixed-weight dot 0.17.1 adapters are
accepted, with exact upstream classes and typed closed parameters. Original
solver settings feed execution, iteration limits and result acceptance; swapping
roles, unknown classes/versions, missing references and legacy settings fail.
No new numerical engine, import facility or dependency is added.

Initial `verify-O69lqS` and `web-verify-QjWEzu` passed, but inspection found the
derived tagged-enum OpenAPI omitted closed-object constraints. Native schema
generation was corrected using existing Utoipa builders and shared identity
constants, with a runnable schema test. Generated files were not hand-edited.
Final `verify-D9yaUn` passed check/format/strict Clippy, 153 domain/Runtime,
8 managed Job, 31 native Codex and 87 scientific Job executions, zero failed/
ignored, source unchanged. Groups overlap. Final `web-verify-5byHB0` passed
double deterministic generation, typecheck, 505 unit, wire/build/help, 36 Codex
browser and 189 full-browser tests, handwritten source unchanged. Inspection
confirmed both generated alternatives reject extra fields and pin class/version.

`owner-oci-jKC2wV` built and verified native image
`sha256:97a343a005ff13ae92a404a431a57e3ba1dc2e66a825994233802850f95bd1c7`;
all 9 actual OCI tests passed, zero failed/ignored, source unchanged. The original
portfolio request executes with explicit model references, returns the independent
0.82/0.18 reference, and manifests portfolio-models/1; old images are not accepted.

This does not yet implement covariance model references, Mandate Store/API/CLI/UI,
trusted qualified forecast assembly or complete Candidate/Release delivery.
PR #63 remained OPEN/Draft at remote head 37e5713ed6252e5935787201914d42f241582a4f
and Issue #62 OPEN on live readback. No GitHub write or full T42 claim occurred.

## Managed and actual OCI forecast aggregation, 2026-09-13

Working source over `fa1dd722efbae2c7dcbae9779c62e79a60b594dc` makes the original
forecast collection mandatory in AllocationInputV1. Per-asset expected_return is
removed, with no compatibility path. Domain admission/output checks share exact
mixture weights and asset/currency binding; the existing optimizer performs native
bar alignment and ndarray aggregation before constructing the Clarabel objective.
MIN_RISK does not bypass the original input contract. No new numerical engine or
dependency is added. Synthetic managed execution now verifies the independently
derived 0.82/0.18 optimum and rejects a single effective Alpha identity.

`verify-NRqF6q` passed check/format/strict Clippy, 152 domain/Runtime, 8 managed,
31 native Codex and 86 scientific Job executions, zero failed/ignored, source
unchanged. These groups overlap. This run preceded the new OCI test only.
`web-verify-zbgaC8` passed deterministic double generation, typecheck, 505 unit,
wire/build/CLI checks, 36 Codex browser and 189 full-browser tests, with handwritten
source unchanged. Only domain-v1.openapi.json changed among the generated outputs.

`owner-oci-IUsTIK` built and ran native image
`sha256:0b35a9d86352928ff237b463b6bb3b4d7bf825c1534cb792d06388e2946d4146`.
All 9 actual OCI tests passed (14.62s), zero failed/ignored, source unchanged.
The new test uploads original PARAMETERS, runs PORTFOLIO_BUILD, downloads and
validates the original manifest/report, obtains 0.82/0.18, checks container exit 0
and original submission identity on replay. Manifest versions include
portfolio-ensemble/1 and ndarray/0.17.1; the runtime rejects the previous stack.
Final `verify-ZYN4Fv` passed workspace check, format and strict Clippy including
native-oci/native-codex targets after the OCI test was added; source unchanged.

This proves native execution, not authoritative forecast-artifact assembly,
current REAL qualification/license binding, Mandate operations or shared-capital
Candidate/Release delivery. Those and full Issue acceptance remain required.
No GitHub CI/review/merge or production T42 completion is claimed.

## Original forecast alignment before native aggregation, 2026-09-13

Working source over `c546bdc5fb20862ed902821b59e1eb748636183c` adds typed
AlphaForecastV1/PortfolioForecastInputV1 and connects their alignment checks to
the existing ndarray aggregation. Version IDs cannot repeat; at least two
different Alpha IDs must have positive mixture weights. Original return units,
currency, fixed-bar horizon, forecast time, available time, maximum age, asset
ordering and complete finite coverage are checked before matrix multiplication.
NaN serialization fails rather than becoming null. IDs/labels remain supplied
metadata, not authoritative qualifications or license evidence.

The existing Nautilus catalog BarType parser is shared with this entry point.
Canonical external LAST time bars must match asset identity and a common native
BarSpecification, preventing identical bar counts at different cadences from
being treated as a common horizon. No parallel parser or optimizer is added.

Initial `verify-RfvXxO` passed before the bar/positive-distinct-Alpha refinements.
Final `verify-fZ8VrV` passed workspace check/format/strict Clippy, 152 domain/Runtime,
8 managed Job, 31 native Codex and 86 scientific Job executions; zero failed/
ignored and source unchanged. Groups overlap. The original independent 0.82/0.18
Clarabel reference now consumes the aligned input. Counterexamples include
duplicate versions, one effective Alpha, score/residual units, wrong currency,
horizon/time/order, future or stale input, missing/NaN forecasts, malformed native
bars and mixed minute/hour bars. Different versions do not count as different
Alphas, but are not categorically forbidden when two real contributing IDs exist.

These are controlled numerical/structural tests, not DB-resolved Alpha ownership,
current REAL qualification, calendars/licenses or full shared-capital delivery.
Trusted original-artifact assembly, Mandate model/API/CLI/UI and the complete
Runtime/portfolio pipeline still require work. No HTTP route, generated public
request schema or Runtime capability is newly advertised; no GitHub write occurred.

## Native fixed-weight forecasts into one allocation, 2026-09-13

Working source over `edcbae8d6db7858d549333015f0cb5f106e5e0fd` adds the fixed
numerical ensemble adapter alongside existing native covariance/calibration.
It delegates row-vector/matrix multiplication to locked ndarray 0.17.1 and
checks bounded dimensions, finite forecasts/results, nonnegative exact Decimal
weights summing to one and at least two positive members. No normalization,
missing-value fill, fitted calibration or alternative optimizer is invented.

`verify-Hm6uHp` passed workspace check/format/strict Clippy, 152 domain/Runtime,
8 managed Job, 31 native Codex and 85 scientific Job test executions; zero failed/
ignored and source unchanged. Groups overlap, so these are not a unique total.
The new real ndarray-to-Clarabel test mixes two explicitly synthetic forecast
vectors with 0.25/0.75 weights, obtaining 0.25/0.05 expected returns and the
independently derived 0.82/0.18 asset optimum. Mixture weights remain separate
from asset weights. Invalid dimensions, non-finite/missing values, negative,
single-effective-member and inexact Decimal weight sums are rejected.

This is a numerical adapter, not proof that rows belong to distinct qualified
Alpha versions or share permitted data/units/horizon/cutoff. Trusted assembly,
model capability binding, Mandate APIs and independent shared-capital simulation
remain required; no Runtime capability or public endpoint is newly advertised.
No current market/account T42 or GitHub CI/review/merge completion is claimed.

## Shared Mandate constraints and rebalance intent, 2026-09-13

Working source over `f73a6265463993b6bb1acc88ee4b4d99e6fb1f55` extracts existing
structural portfolio checks into the function reused by actual allocation and
publication. Asset/group membership, covariance and feasibility remain tied to
the actual frozen native input; no dummy allocation is invented to validate a
Mandate. RebalanceScheduleV1 declares all three required kinds with strict
kind-dependent fields, positive input age/TTL and native IANA timezone validation.
chrono-tz 0.10.4 was already locked; no new calendar/scheduler engine is built.

`verify-ekWa6V` passed workspace check/format/strict Clippy, 152 domain/Runtime,
8 managed Job, 31 native Codex and 83 scientific Job test executions, zero failed/
ignored, source unchanged. Test groups overlap and are not a unique test total.
New counterexamples cover duplicate bounds, reversed bounds, mixed schedule
fields, zero interval/TTL/age, unknown timezone and unknown wire fields.
This stage does not implement Mandate model references/capability binding,
Store/API/CLI/UI, calendar execution, multi-Alpha allocation or delivery.
RebalanceScheduleV1 is not yet exposed as a public HTTP request; existing HTTP
and generated wire artifacts are unchanged. No PR/CI/merge completion is claimed.

## Cancelled original native Thread recovery, 2026-09-13

Working source over `4e979a25fc47aefb5e7d2ce4ff52a78b388bd7b2` fixes the shared
Mission launcher rejecting every cancellation before considering its persisted
session. Both roles now enter cancellation reconciliation before normal stage
preparation. Only an existing Thread can resume; no Mission credential is minted,
MCP is disabled without a token, and no new Turn is prepared. Original resource
limits remain, with a bounded cleanup window rather than renewed research time.

`verify-YOTtwB` passed 24 tests but the new recovery case failed at native
thread/resume. The disabled MCP entry initially omitted transport fields;
retaining command/arguments while disabling it and omitting credentials corrected
the request. `verify-d0K0Yi` then passed the exact recovery case (118.97s).
Final `verify-uomhbK` passed check/format/strict Clippy and all 25 actual native
profile/MCP/Mission tests (4 + 5 + 16; Mission group 478.97s), zero failed/ignored,
source unchanged and owned PostgreSQL confirmed stopped.

The extended case closes the original process after a real completed notification,
withholds final usage, cancels the Run and redelivers its original queue message.
Native resume succeeds, provider calls and credential counts do not increase,
one original session remains, no receipt/archive is fabricated, and the Run stays
CANCEL_REQUESTED. Controlled upstream responses are not real account/market T42;
this does not prove recovery of every lost streaming notification or final usage.
No push, review request, merge or Issue closure occurred.

## Original Sealed qualification transaction, 2026-09-13

Working source over `da4ed4da349f6dadff53b193cc1efff7c87c6888` connects
qualification to the original scientific ACK transaction under DESIGN A4.14.
It reuses frozen input/license and Sealed binding checks, requires original
independent review and REAL/PIT evidence, caps expiry to original evidence and
licenses, and preserves revocations, active pointers and suspended lifecycle.
No new manual qualification endpoint, numerical engine or dependency is added.

`verify-SOfTZf` completed workspace check/format/strict Clippy and 150
domain/Runtime, 8 managed Job, 31 native Codex and 540 Store/server tests;
zero failures/ignored, source unchanged, owned PostgreSQL stopped. The extended
App Server/PGMQ test publishes an explicitly FIXTURE scientific PASS, ACKs it
twice and confirms no qualification or extra model call. PostgreSQL prepares
the exact production INSERT against actual migrations without fabricating REAL
evidence. This proves negative eligibility and SQL validity, not positive REAL
qualification, all expiry/revocation/rollback behavior, or market/account T42.
Public qualification/disclosure, portfolio/delivery and remaining contracts
still require implementation and acceptance. No GitHub CI/review/merge is claimed.

## Automatic original Reviewer-to-Sealed admission, 2026-09-13

Working source over `88e099a842a9f2f0ef2e87369dd258476b2ea916` extracts the
existing Sealed preparation transaction for both trusted callers. The Operator
wrapper retains authorization, exact replay and final authority recheck; the
internal Reviewer caller takes no Operator grant. Each queue consumption admits
at most one original PASS target after all independent answers are recorded.
Migration 046 binds the original review reservation to its exact Alpha, source
Validation, Cycle/policy and one Sealed Run. Parameters, association, Run/PGMQ
and budget commit together. Mission success/ACK wait for required associations;
scientific execution/publication remain independent and no qualification is granted.

`verify-IdcgcG` passed workspace check/format/strict Clippy, extended actual
App Server/PGMQ test (305.28s) and two cancellation tests (57.61s), zero failed/
ignored, source unchanged and owned PostgreSQL stopped. The test proves injected
association failure leaves Run count/resource reservations/tasks unchanged,
then retries the original target without an extra model call or trial charge.
It also executes a delayed publication failure beyond a one-second lease:
the expired worker cannot change Cycle state, and real takeover subsequently
admits the task. No Operator evaluation command or Sealed capability is fabricated.
The lease test explicitly checks the delayed callback ran after real Runtime
refresh; the earlier preflight-failure test run was not reported as success.

`verify-2ZmSNk` passed check/format/strict Clippy, 29 native validation,
130 Store and 21 HTTP/CLI tests, zero failed/ignored, source unchanged and owned
PostgreSQL stopped. Existing manual Sealed authorization, original calibrated
target, replay, publication and root-opportunity tests remain green.

The previous exact `88e099a8` full regression (`verify-bpEiG9`) passed 150
domain/Runtime, 8 managed Job, 31 native Codex and 539 Store/server tests. That
full result belongs to the prior commit, not this new source or GitHub CI.
Controlled upstream/science responses do not establish real market/account T42.
Qualification, complete Reviewer error/multitarget/workspace limits, portfolio,
delivery, migration/recovery and all remaining Issue contracts still require work.

## Independent native Reviewer stage, 2026-09-13

Working source over `c63e126ccce86394c8f3563fbc9ba4b37c5eb8e1` admits an
independent Reviewer in the original successful Researcher ACK transaction,
using frozen selection/profile and cumulative budget. It reuses Run/PGMQ,
native Turn reservation/usage/summary and immutable ArtifactStore inputs.
Migration 045 binds each selected original target to one Reviewer Turn and its
settled public JSON assessment. Invalid answers become INCONCLUSIVE without a
new paid repair. No research chat, credential, Sealed rows or calibration
coefficients are supplied; Reviewer cannot upload artifacts or propose trials.

`verify-gk3jA4` passed check/format/strict Clippy and 159 Store tests before the
final native-test/context additions. Two new Store/server review unit checks
passed separately. `verify-CVwsFe` passed final-source workspace check/format/
strict Clippy, the extended real App Server/HTTP/PGMQ Mission test (306.15s)
and both cancellation regressions (57.33s), zero failed/ignored, source unchanged,
owned PostgreSQL stopped. The native test executes actual file tools (including
native asynchronous process continuation), verifies separate Thread identity,
original target/summary, scopes, cumulative token usage and no qualification.
An injected Reviewer association failure rolls back selection/admission/ACK;
concurrent original ACKs create one Reviewer. Repeated ACK is idempotent;
claiming an already archived message is rejected without another model request.

Earlier native runs failed because the controlled response fixture assumed
immediate shell output and then incorrectly expected archived messages to be
claimable. Those failures were retained and corrected, not reported as green.
Only provider/science responses are controlled: this is not real account/model
inference, actual market Mission-to-OCI acceptance or complete T42 evidence.
Automatic Sealed continuation, qualification, whole-workspace quota and the
remaining W0–W8 contracts are not completed by this stage. No GitHub review,
CI, merge or Issue closure is claimed.

## Frozen original review target, 2026-09-12

Working source over `8fa4635ae90647dfd5b0c3c4296fcb0aa3f68e0a` adds nullable
review_alpha_version_id to the original selection member, contract and read-only
Web detail. Original alpha_version_id still names Validation's source version.
Only selected, currently passed original Validation evidence supplies a target;
SCORE resolves its actual next calibrated version through immutable relations,
not the active Alpha pointer. Migration 044 guards that relation and leaves old
snapshots null. No extra target table, queue, model call or qualification grant.

`verify-Sn9W2W` passed workspace check/format/strict Clippy, 29 native validation,
130 Store and 21 HTTP/CLI tests, zero failed/ignored, source unchanged and owned
PostgreSQL stopped. The real PG selection test moves active_version_id back to
the original before freezing and still gets the original calibrated target.
MAXIMIZE/MINIMIZE tests retain ranked scientific REJECT rows without review
targets, and unselected/unexecuted rows stay null. Existing rollback, concurrent
ACK and immutable membership checks still pass.

`web-verify-832Bj1` passed double native generation of all six named outputs,
typechecking, 505 unit tests, wire checks, build, CLI help, 36 Codex browser
tests and 189 complete browser tests; handwritten files unchanged. The selection
browser check displays the original and review versions separately as nonqualification.
`verify-pci9pO` passed check/format/strict Clippy, one native same-Thread science
test (200.52s) and two cancellation recovery tests (51.22s), zero ignored,
source unchanged and owned PostgreSQL stopped. Native binary execution uses
controlled model/science responses; it is not real-account/market T42 acceptance.

Automatic Reviewer admission, independent input delivery/assessment, automatic
Sealed continuation and qualification remain unfinished. Frozen target metadata
alone must not be used to claim any of them or merge Issue #62.

## Shared Mission Runtime refresh, 2026-09-12

Working source over `2d5971ea362a0a2602aab74760d6e68b62433c4f` removes the
Researcher-only filter from the existing owner-fenced Runtime probe. Both original
Mission roles use the same frozen Runtime revision, enabled-state, lease and
deadline checks. No new endpoint, grant, queue or artifact access is introduced.
The controlled Reviewer PostgreSQL test now verifies that changing its Runtime
configuration raises RevisionConflict instead of silently skipping the refresh.

`verify-2iSUVs` passed workspace check, format, strict Clippy and 159 Store tests,
zero failed/ignored, source unchanged, owned PostgreSQL stopped. This is not
automatic Reviewer admission, native Reviewer inference, qualification or full
Issue acceptance. Automatic role driving and its original input/assessment
publication remain unfinished.

## Honest unsent Sealed opportunity rejection, 2026-09-12

Working source over `bb46dcc3aad9840ad290970e7739f35fc0623cc3` fixes permanently
unavailable Sealed tasks waiting until their deadline. The existing unsent Run
settler uses the same root-locked disclosure/quota check as first capability
reservation. Only NOT_SENT tasks without an existing native capability can end
early as FAILED/SEALED_OPPORTUNITY_UNAVAILABLE. An existing opportunity is retained;
already-sent work still requires reconciliation. The lease and deadline are checked
again after the root lock wait. No caller-supplied failure authority or new queue.

`verify-raD4uv` passed check/format/strict Clippy, 29 native validation, 130 Store
and 21 HTTP/CLI tests, zero ignored/source unchanged and owned PG stopped. Real
PostgreSQL checks cover a competing quota loser, a granted winner not rejected
by its own use, prior disclosure, original failure publication before ACK and
expired lease after a root-lock wait leaving no terminal receipt or opportunity.
Failed evidence is INCOMPLETE/INCONCLUSIVE, not a fabricated remote observation.

`web-verify-3O0UoY` passed all six reproducible native generated outputs, type
checking, 505 unit tests, five Node tests, wire checks, build, CLI help, 36 Codex
browser cases and 189 full browser cases, handwritten sources unchanged. A new
SSE decoder test retains the explicit unsent reason instead of a runtime failure.
No native image/dependency changes. This is not automatic Reviewer or complete
Issue62 acceptance; no GitHub review/merge gate is claimed.

## Original Mission role on request and summary artifacts, 2026-09-12

Working source over `c4dc9cae5bda51e2b89fb4630fd8ca36a901bc6a` uses the immutable
native session role in shared Turn publication, replay, send-time reading and
summary publication. Reviewer envelopes and summaries are EVALUATOR_ONLY;
researcher behavior is retained. Reviewer admission accepts only its own trusted
Run/Attempt qz.mission_turn envelope, not arbitrary evaluator-only parameters.
Migration 043 retains exact native success/producer checks with role-specific
summary visibility. Ordinary artifact/MCP reading is not expanded.

`verify-fIK1r0` passed workspace check/format/strict Clippy and 158 Mission/ledger
Store tests, zero ignored, source unchanged and owned PG stopped. The new test
uses controlled Reviewer association and native receipts, not an automatic
Reviewer creator or actual model. It proves original request read/replay,
summary settlement/publication/replay, restricted normal reads and rejection of
an arbitrary evaluator-only task envelope. `verify-HXw44A` also passed the actual
native same-Thread scientific continuation and both cancellation recovery tests,
zero ignored, source unchanged and owned PG stopped. Responses/scientific bytes
are controlled. No new protocol, dependency or native image; automatic Reviewer,
its frozen input manifest/assessment and qualification are still unfinished.

Before this change, exact committed `c4dc9cae` passed the full `verify-5Dmt0B`
regression: workspace check/format/strict Clippy, 150 contracts/domain/Runtime,
eight managed Job, 30 native Codex and 535 Store/Server tests, zero failed/ignored,
source unchanged and owned PostgreSQL/PGMQ stopped. This supersedes the earlier
full run's two subsequently corrected test failures, not missing product work
or current-head GitHub review/CI gates.

## Cycle-funded Operator Sealed admission, 2026-09-12

Working source over `f7c9df08d2d108a0f489f8bf9e1eef61e1b072ad` adds the explicit
Operator API/CLI/Web request in DESIGN A4.12. It reuses frozen Cycle context,
original paid compilation/Validation/calibration, bounded Run admission, native
task bindings and the same final source checks used before the first capability.
Migration 042 extends existing command grants and requires the original Run
receipt. No new queue, dependency, native operation or compatibility layer.
The original compilation trial is not charged again; CPU and other limits remain
Cycle-funded. Request replay returns the original Run without object I/O.

`verify-lNs4Da` passed workspace check/format/strict Clippy, 29 native validation,
128 Store and 21 HTTP/CLI tests, zero ignored, source unchanged and owned PG/PGMQ
stopped. Opportunity tests now use the production creator. The new authenticated
HTTP test follows original compilation/Validation/calibration through admission,
202/replay/conflict, opportunity timing, formal publication and ACK. Scientific
reports are controlled protocol fixtures, not actual market computation or T42.

The first web run `web-verify-mbKSQT` found two strict TypeScript errors in the
new test's captured-array reads; browser tests passed but that run was not green.
After an explicit missing-capture guard, `web-verify-vtyKYm` passed type checking,
504 unit tests, five Node tests, wire checks, build, CLI help, 36 Codex browser
tests and 189 full browser cases. All six native generated outputs were produced
twice with identical bytes and handwritten sources remained unchanged. The new
case tests explicit Cycle selection, exact bigint/frozen request and identical
idempotency retry after a lost reply across three viewports. UI fixtures prove
presentation only. The 202 receipt is not qualification or Reviewer approval.

Automatic continuation, durable independent Reviewer, qualification and the rest
of Issue62 remain unfinished. No current-head GitHub CI/review/merge is claimed.

## Formal Sealed publication before ACK, 2026-09-12

Working source over `672a7d30a8afa008b21a91ee677295e8f6d9629f` extends the existing
Worker publication entry to original SEALED evaluations. It reuses the immutable
object reader, native result associations, metric projection and MetricGate.
Original accepted Attempt/manifest/PARAMETERS/calibration and reserved exposure
are required for successful execution. Metrics use the independently frozen
Sealed requirements, with origin/PIT, registered rows, missing data, observation
minimum and completion-based validity checks. Cancellation/failure is explicitly
INCOMPLETE/INCONCLUSIVE. No experiment verdict, Alpha or calibration is rewritten.
The complete Evaluation and metrics commit before ACK; replay performs no file
I/O. Worker object recovery remains shared with the existing Validation path.

`verify-XPBVCz` passed check/format/strict Clippy, 29 native validation tests,
128 Store tests and 20 HTTP/CLI tests, zero ignored/source unchanged; owned PG/PGMQ
stopped. Two new real PostgreSQL tests verify cancellation, publication rollback,
original opportunity references, ACK blocking/replay, and a controlled native
value 0.15 rejected by Sealed's 0.2 threshold rather than borrowing Validation's
0.1 threshold. Later disclosure does not rewrite the original reservation.
These explicitly controlled reports are not native scientific computation or
qualification proof; actual Job/OCI computation is recorded separately below.

After documentation alignment, `verify-aPAgQG` passed check/format/strict Clippy,
the actual native Codex same-Thread scientific continuation test (207.67s) and
both cancellation recovery tests, zero ignored/source unchanged; owned PG/PGMQ
stopped. Model responses and scientific bytes in these Worker tests remain
controlled. This is not a real-account complete market research acceptance.
No protocol DTO, dependency or native image changed. Trusted Sealed admission,
independent Reviewer and qualification remain unfinished; no GitHub gate is claimed.

## Original Sealed opportunity reservation, 2026-09-12

Working source over `814409168b19563d5653217a43b8f481d07435dc` adds immutable
Sealed task associations and original Attempt-to-exposure records (migration 041).
The existing first native JobSpec transaction locks the root lineage and reserves
an opportunity before returning any capability. It requires the canonical formal
Validation projection, original model/calibration, exact policy/dataset and
inherited data origin. Prior disclosure blocks independent access. Root-wide
usage is not refunded after cancellation; replay uses the original reservation.
After a root-lock wait, capability/deadline and Worker lease are checked again;
failure rolls back both the new JobSpec and opportunity.

`verify-sx1Gdc` passed check/format/strict Clippy, 29 native validation tests,
126 Store tests and 20 HTTP/CLI tests, zero ignored, source unchanged and owned
PostgreSQL/PGMQ stopped. Three new real PostgreSQL tests cover concurrent
same-project max-one reservations, original Attempt replay/cancel-no-refund,
prior Operator summary disclosure and actual lease expiry while a root lock is
held. The tests use controlled scientific outputs and trusted test administration
that pays an ordinary trial; they do not prove production non-trial Sealed
admission, cross-project concurrency or scientific qualification. The first
passing run `verify-5JImI0` preceded reuse of the canonical source projection;
the final run above verifies that change. No protocol or native image changed.

Trusted Sealed task creation, formal publication and independent Reviewer flow
remain unfinished. This is not full Issue #62 acceptance or GitHub CI evidence.

## Managed held-out operation and real OCI, 2026-09-12

Working source over `941269e50ab448ed7ea86d52e79369ca1da89e1c` adds the internal
EVALUATE_SEALED_ALPHA operation and qz.alpha_sealed output. Its original JobSpec
binds a Sealed dataset, Wasm MODEL, optional original calibration MODEL and
PARAMETERS. SCORE requires that calibration; EXPECTED_RETURN forbids it.
The Job and Store adoption read the original model, not a second copy of fit
configuration inside PARAMETERS. Existing readers now allow the second immutable
object read. Generic output shape and exact input association remain distinct.
Runtime selection checks include this fifth data operation; the image marker
now requires alpha-sealed/1. No public researcher admission was added.

`verify-WCmzXU` passed check/format/strict Clippy, 150 contracts/domain/runtime,
8 managed, 30 native Codex and all 83 Job tests, zero ignored/source unchanged.
The earlier response-contract expected list omitted the new variant; it was
corrected and this suite rerun. `web-verify-mDEjVM` passed all six native double
generations with byte equality and handwriting unchanged, typecheck,
504 Vitest + 5 Node, wire/build/CLI checks, 36 Codex browser tests (33.4s) and
186 full browser tests (2.5m). Only Domain/Runtime schema bytes changed.

`owner-oci-LSjLSA` passed all 8 actual Docker tests (13.51s), zero ignored, using
image `sha256:409d22803b5eae314d9e8cda0657dfa58814a9090ebac5963329fb624057ec57`.
The new test uses actual Parquet/Wasm/frozen native OLS and registered FIXTURE
metadata, uploads exact objects through Runtime HTTP, executes the image,
downloads and binds its report, and compares it with the original native result.
Its first attempt failed before container execution because native catalog
loading requires multi-thread Tokio; only the test runtime flavor was fixed.
The final source was also checked by `verify-lX8JmJ`: check/format/strict Clippy,
29 native validation, 123 Store and 20 HTTP/CLI tests, zero ignored, source
unchanged and owned PostgreSQL/PGMQ stopped.

The Runtime test reuses existing Job fixture code and already pinned scientific
packages as dev-dependencies; Cargo.lock adds only those dependency edges.
This is synthetic OCI computation evidence, not trusted Sealed admission,
opportunity reservation, formal evaluation/qualification or full T42 delivery.

## Held-out metric projection and freeze compatibility, 2026-09-12

Working source over `e309821299ed7f33c9b2ce1f5ac6dd539cb6aca0` projects the original
Sealed report into existing MetricValue records and native capabilities. Asset
scopes remain `asset:N`; native values, missing reasons, paired counts, methods,
original artifact references and covered time intervals are retained, without
averaging Validation folds. Freeze/start checks require explicit supported Sealed
requirements and matching registered asset/bar order. Metadata reads declare
their allowed partition; ordinary DATA_VALIDATE still rejects Sealed before I/O.

`verify-X51wgQ` passed check/format/strict Clippy, 150 contracts/domain/runtime,
7 managed, 30 native Codex, 91 Store and 41 HTTP/Worker tests, zero ignored.
Source remained unchanged and the owned PostgreSQL/PGMQ cluster stopped.
The final direct Sealed suite passed all 5 actual Parquet/Wasm/CLI tests, including
numeric gate projection and absent-label INCONCLUSIVE. Earlier runs caught a
missing selected SQL column and a test Clippy violation; both were fixed before
this successful rerun. No protocol DTO changed, and no web/OCI rerun is claimed
for this slice. Managed Sealed admission, exposure, publication and qualification
remain unfinished; this is not full Issue #62 or current-head GitHub CI evidence.

## Native held-out computation primitive, 2026-09-12

Working source over `de5a23140146b1268a159d5f5cec073b080f3004` adds the restricted
`job evaluate-sealed-alpha` computation, not a managed Runtime operation or an
access grant. It calls the existing native catalog/causal EMA/Wasm forecast,
applies persisted OLS coefficients without fitting, and reuses the existing
ndarray-stats metrics. Original scores and per-row normalized returns coexist;
warmup and incomplete-label tails remain explicit. Research availability, exact
calibration asset order/bar/horizon, original calibration report/time, finite
values and complete result associations are checked. No-label metrics have
`INSUFFICIENT_DATA/NO_COMPLETE_LABELS`, not zero. No new dependency was added.

`verify-n1FyGw` passed check, format and strict Clippy; 150 contracts/domain/runtime,
7 managed, 30 native Codex and all 81 Job scientific tests, with zero ignored.
The four new Sealed tests use actual Parquet/Wasm, persisted native fits and the
real CLI. They check original-score preservation, independent RMSE reference,
stable past predictions when future data extends, time/asset/horizon and missing
fit rejection, result tampering, absent-label states and safe CLI failure.
All data are synthetic; no qualification or full Mission-to-market proof follows.

`web-verify-9EqbPA` passed all six native double generations with byte equality
and handwritten files unchanged, typecheck, 504 Vitest + 5 Node tests, decimal/
bigint/fraction wire checks, build/CLI help, 36 Codex browser tests (34.1s) and
186 full browser tests (2.6m). Only Domain schema bytes changed; API/Runtime and
generated frontend bytes are unchanged. Chromium viewports are not Safari/device
acceptance. The external OCI verifier now inventories untracked source contents
as well as tracked files, so new source is covered by the frozen-source check.

`owner-oci-WopDT1` built native image
`sha256:2a4745de5908e81086702739623af959d08e56aab9fd155fc5ce1673ae83b0f8`
and passed all 7 real OCI regression tests (11.26s), zero ignored, source unchanged.
These cover existing compile/identity/cancellation/restart/kernel boundaries, not
Sealed market execution. Exposure reservation, managed Sealed producer adoption,
formal evaluation, independent Reviewer and qualification remain unfinished.
No GitHub push, review request, merge or Issue closure is claimed.

## Separate frozen Sealed policy intent, 2026-09-12

New policy creation requires explicit `sealed_metric_requirements`, independently
validated and persisted alongside Validation requirements. Selection binds its
own evaluation kind. Migration040 leaves historical policies unchanged with a
NULL new column; no threshold copying, retrospective authorship or Sealed
qualification is inferred. This increment does not implement Sealed execution,
exposure reservation, independent Reviewer admission or qualification.

On working source over `2c47c7ede4a507b950ad918ad21e300216ad2904`, full native run
`verify-yMEX97` passed compilation, formatting, strict Clippy, 150 contracts/domain/
runtime tests, 7 managed tests and 30 native Codex tests. Store/Server completed
526 passed and 2 failed, none ignored: the old migration comparison omitted the
new NULL column, and a publication test manufactured a calibration without its
now-required native producer. This full run is **not** a green full-suite claim.
Only those test assumptions were corrected: explicitly assert historical NULL,
and use a valid DEMO Release consumer for the generic same-transaction seal test.
No production constraint was weakened. Targeted `verify-0tmoOd` then passed the
same compile/format/Clippy and 150/7/30 checks, plus 35 Store policy/publication/
upgrade tests and 6 policy HTTP tests. Both runs confirmed unchanged source and
stopped their own temporary PostgreSQL/PGMQ clusters.

`web-verify-phIEnJ` passed byte-identical double native generation of all six
outputs with handwritten files unchanged, typecheck, 504 Vitest and 5 Node tests,
decimal/bigint/fraction wire checks, production build and CLI help. Browser checks
passed 36 Codex settings tests (37.7s) and 186 full-site tests (2.8m), covering three
Chromium viewports, not Safari or physical devices. Semantic changes are confined
to policy DTOs and their response wrappers; routes and Runtime contract are
unchanged. No GitHub publication/review/merge or full Issue62 acceptance is claimed.

## Runtime admission and Mission owner-fence checkpoint, 2026-09-10

The current increment adds typed integration settings, write-only encrypted
credentials, revision-bound native TCP/TLS Runtime observations, formal Brief
execution-context freeze and atomic Cycle/initial-Run/PGMQ startup. Registration
is not a successful connection; queue admission is not proof of Worker execution.
The frontend uses native Rust-generated contracts, shared Ajv validators and
Rollup splitting without increasing the Workbox 2 MiB per-file precache limit.

Migration `202609100023_mission_owner_fence.sql` permanently binds each new
Mission credential to both the active Attempt and its current owner epoch.
Common machine authority checks that binding and the live database lease on reads
as well as writes. Historical Mission issuances with an unknown owner remain
unchanged audit records and require new issuance; valid CLI credentials survive
that Mission-only cutover. Existing applied migration files are not rewritten.

The regressions in `apps/server/tests/support/mission_attempt.rs` use native
claim/renew and historical SQLx upgrades. `mission_owner_wait.rs` additionally
proves actual PostgreSQL lock blocking before letting the native lease expire,
then requires HTTP 401 or issuance SQLSTATE 23514 after the lock is released.
It also rejects Attempt/owner injection into non-Mission credentials. These
fixtures do not constitute the full production Worker, Codex or T42 workflow.

Local `.ai-bridge/runtime-check-GZmgAU` recorded successful targeted HTTP/Store,
native TCP/TLS and strict Clippy checks with unchanged source. Formatting failed;
its proposed formatting was subsequently applied by the web author. That run is
not full-stack or publication approval. The final checkpoint requires a fresh
stable full Rust/Web/real-browser run and independent generated-output compares.
Private raw logs, test databases and test credentials must not be committed.
Latest committed-Head CI and an explicit clean Codex review remain separate gates;
PR #63 stays Draft until all Issue #62 work packages and acceptance are complete.

## Local review-patch verification, 2026-09-07

This is local evidence for the web-authored working-tree patch over
`4ce0fcbbaf530ba8a02124fa9ff84becf67bb156`, not a new GitHub CI result or complete
Issue62 acceptance. The execution adapter requested `gpt-5.6-luna`; the web author
wrote the source and scripts. Only native generated build artifacts were published
by the explicitly approved build step; the executor did not author source fixes.

- Rust **1.98.1** was installed in an isolated user-owned development prefix and
  explicitly selected for all current checks. Format check, contracts/domain/store/
  server compilation and Clippy with `--all-targets -- -D warnings` passed.
- Contracts and domain suites passed **76 tests** with no failed/ignored tests.
  Both real HTTP OpenAPI reference tests passed. Both native generators ran;
  their reviewed outputs were regenerated byte-identically and published to
  `contracts/generated`. All three Node wire-corpus commands passed.
- Docker access was unavailable in the local executor. Rather than weaken socket
  permissions or touch existing services, the executor built official PostgreSQL
  **18.6** and exact upstream PGMQ **1.10.0** in the user-owned development prefix,
  then initialized an independent disposable cluster. The official PostgreSQL
  archive checksum and PGMQ Git release commit were checked. This is native
  PostgreSQL evidence, not evidence for the separately pinned OCI image.
- The focused atomic-Cycle/Run/role suites passed **45 tests**. The subsequent
  complete `cargo test --locked -p store -p server -- --test-threads=4` passed
  **196 Store + 40 server = 236 tests**, zero failed/ignored, including all eight
  transaction-composition regressions. The temporary server was stopped and the
  tracked-source diff was unchanged. Database execution ended at
  **2026-09-07T18:11:26Z**.

Local raw logs are `.ai-bridge/verify5`, `.ai-bridge/native-contract-output`,
`.ai-bridge/contract-publication` and `.ai-bridge/native-db-1`; these private build
logs, disposable database data and test credentials must not be committed. CI
must independently reproduce the committed Head and retain public artifacts.
Formal Brief freeze/Cycle-start services, independent scientific publication,
Agent/portfolio/delivery/UI and the remaining W0–W8/T01–T42 are still required;
these local results do not authorize merging the incomplete integration PR.

## Implemented source boundaries

The first-party tree uses `apps/job`, `apps/server`, `crates/contracts`,
`crates/domain`, `crates/store` and `crates/integrations`, without `qz-` directories.
Legacy backend/frontend/plugin/deployment implementations, compatibility services,
obsolete tests and archived designs have been removed. Git history, LICENSE,
NOTICE, third-party notices and user-data boundaries remain intact.

Scientific foundations reuse Rust-native Nautilus0.63.0, Clarabel0.11.1 and
Arrow56.2.0 on Rust1.98.0. There is no production Python bridge. Native feasibility
is not complete shared-capital research/portfolio acceptance. The earlier native
fixture produced weights 0.7999999999997491 / 0.20000000000025078, 745 iterations,
12 native orders and 24 native events with an Arrow round-trip. It is explicitly
FIXTURE, `deliverable=false`, `python_runtime=false`, not investment evidence.

The typed contracts and pure rules include decimal/bigint boundaries, metric
nullability, UUIDv7, ISO currencies, Codex default-setting omission, budget and
mission-turn limits, qualification, cancellation, version and lease fencing.
Both Rust parsing and generated schemas consume shared decimal/bigint corpora.
These rules do not replace persistent authorization or native component tests.

SQLx0.8.6 owns migrations, transactions and independently migrated test databases;
PostgreSQL/PGMQ owns durable queue delivery. The Store owns project-specific
relationships, immutable identities, budgets and reconciliation decisions. A
Mission has one immutable Session/Thread. Reservation plus PGMQ send is atomic;
one committed dispatch intent grants one send, while retries reconcile. Native
binding, terminal and usage receipt are separate immutable facts. Missing usage
retains reservations; pause, cancellation or lease loss never fabricates a refund.
Exact receipts precede queue acknowledgement.

The relational constraints bind artifacts, inputs, experiment ancestry, Alpha
qualification, frozen candidate contents, evaluations, Release and approval/offer
identities. They also bind machine principals/scopes, forward corrections,
Codex profile revisions, event cursors and run/attempt result manifests. Creating
these records does not itself expose or implement all corresponding services.

The HTTP authentication vertical in `apps/server` reuses Axum0.8.9,
tower-sessions0.14, PostgreSQL PostgresStore0.15, totp-rs5.7, Argon2id,
RustCrypto XChaCha20Poly1305 and cap-std. There is no memory-session fallback.
Clap exposes `init-state`, `migrate`, local one-use `bootstrap`, `serve` and native
OpenAPI. Login uses TOTP only, with a bounded bootstrap capability, native private
cookie, database replay prevention, expiry/epoch/revocation and atomic rate limits.
Device lists paginate and used trusted devices update activity. This vertical
does not provide research, Reviewer or approval authority to an Agent.

## Evaluation publication and revocation correction

The additive `202609060005_evaluation_publication.sql` seals a completed
Evaluation and its metric membership together. A deferred native constraint
trigger publishes the aggregate before commit; consuming references can seal it
earlier in that same transaction. Later metric inserts are rejected. Candidate
and allocation-Evaluation circular creation retains its existing deferred foreign
key and is covered by a positive regression. Upgrade backfills only the internal
publication markers and does not rewrite historical evidence.

Degradation observations must join the exact project, policy mandate, Release
candidate, FORWARD Evaluation and frozen InputSet, backed by the corresponding
Forward evidence window. Historical incompatible rows abort migration rather than
being deleted or silently relabeled. This relationship check does not replace
current policy authorization, freshness, degradation thresholds or Wake admission.

Browser session epochs cannot move backward. Equal epochs allow ordinary state
maintenance; newer epochs invalidate old authority permanently, with native bigint
overflow failure rather than wrapping. The regressions check actual browser/device
authority and a real concurrent row-lock wait, not just the stored integer.

The native Codex probe validates the exact observed `originator/version` product
token against pinned upstream source and publishes that observed value. It rejects
version prefixes, prerelease/build suffixes and a matching string elsewhere in the
user agent. These narrow format regressions do not count as real-account model
inference. The normal native stdio probe remains a separate required CI execution.

The source adds twelve PostgreSQL regression functions and two native-version
unit tests. Their definitions alone are not acceptance evidence: each execution
must identify its exact source/tree, committed lock, command and outcome. Temporary
public development-input exporters are removed after tools are retrieved; no tool
archive, user database, secret or standalone patch publisher is part of the product.

## Historical evidence: exact baseline, not the current Head

At `e8668ca850def834735414ed9ba94fed38d4aa7e`, the suites contained 8 contract,
28 domain and 8 native/report tests: **44 total**.
[CI 33962063262](https://github.com/zhengui666/QuaZonai/actions/runs/33962063262)
validated that historical dependency lock. The upstream feasibility run
33952841460 is a development study, not a final product gate.

The latest baseline inspected for this correction is
`19496333808afc71d794af0871e5ef9704a3507a` with
[CI 34007972993](https://github.com/zhengui666/QuaZonai/actions/runs/34007972993):

| Job | Actual result at that exact baseline |
| --- | --- |
| `database-native` | Success: native PostgreSQL/PGMQ transaction contract |
| `store-postgres` | Success: **61 Store + 7 server = 68 tests**, zero failed/ignored |
| `rust-native-contracts` | Failure at `cargo fmt --all -- --check` on `evidence_bindings.rs` |
| `foundation-checks` | Failure, because not every required job succeeded |

The 61 Store tests in that job were auth 8, authority invariants 7, relational
constraints 7, device activity 3, evidence bindings 12, terminals 4, turn recovery
4 and turn transactions 16. The server suite had 7 tests; its database-backed
cases include an actual loopback HTTP listener using a distinct non-owner login.
The artifact `store-evidence-19496333808afc71d794af0871e5ef9704a3507a`
contains `tests.log`, `tested-commit.txt`, exact migrations, Cargo.lock and native
database/image versions. This count describes that artifact only.

Clippy, Rust tests, native scientific verification and Codex protocol probes were
**not completed by the failed native job at that baseline**. The separate passing
Store job cannot turn those skipped steps into success. Earlier unqualified
52/87/97-test snapshots are retired as current evidence: never combine different
commands, development locks or commits into a latest-Head pass count.

## Review correction in this source

`verify_runtime_role` inspects native PostgreSQL role membership, ACLs and ownership
across the entire `app` schema rather than sampling `operator_auth_state`. It
rejects destructive table privileges, schema CREATE, app object ownership and
elevated roles reachable by inheritance or SET ROLE, including an elevated
`session_user` hidden behind a restricted `current_user`. Ordinary non-owner DML
access remains supported. This is a startup guard, not a substitute for continuing
least-privilege administration or a complete machine-authorization service.

Before a new reservation or first dispatch, the Store holds the referenced Brief
lock and requires its frozen budget to equal the Cycle snapshot. The complete JSON
comparison includes tokens, costs/currency, turns, repairs and resource limits.
Existing receipt/terminal reads and actual-usage reconciliation remain separate
from permission to start new spending. Historical malformed snapshots cannot
expand new work or justify deleting actual consumption.

The additive `202609060004_doctor_boundary.sql` migration confines DOCTOR_READ to
a read-only CLI/AUTOMATION credential, never DOWNSTREAM/MISSION or a mixed research/
delivery scope. Existing issuer, epoch, project, Mission-lifetime and lock checks
remain. Previously issued invalid Doctor credentials require an explicit effective
revocation before migration can succeed; their immutable issuance stays in audit.
The upgrade regression uses native SQLx to apply the old migrations, verifies the
specific failing migration/SQLSTATE, then revokes and reapplies without rewriting
historical rows or changing applied migration checksums.

Terminal retries first verify the exact reservation/attempt binding, then compare
all immutable terminal fields before requiring a live fence. They recheck after
lock waits. Identical committed facts remain readable after lease expiry/takeover;
conflicting outcome, native identity, reason or observation time is a conflict.
Creating a missing terminal still requires current owner/epoch/lease. A terminal
never creates a usage receipt, releases a reservation or acknowledges the queue.

The correction adds 12 PostgreSQL regression test functions: 4 runtime-role,
2 budget-authority, 2 Doctor boundary/upgrade and 4 terminal retry/race tests.
The source also formats the existing evidence-binding suite without weakening CI.
**A test definition or successful `--no-run` compilation is not a passed database
test.** The modified source must obtain its own published-Head CI logs and review;
the historical 68-test result above must not be reused for it.

## Native upgrade cutover and control-plane vertical

The current source adds `Store::migrate` with the native SQLx migration lock on a
closed-on-exit dedicated connection and a transaction covering ordered PostgreSQL
application-table write barriers plus all pending native migrations. The existing
migration files retain their original bytes/checksums. Additive migration 0006
repairs a possible pre-guard cutover gap and invalidates all historical browser
epochs once, with an immutable audit receipt; it does not delete historical facts.
Upgrade regressions use real prior-schema writers, lock waits, failed batches,
connection cancellation and a bad observation committed while migration waits.

`contracts::control`, `domain::control`, `store::authority/commands/control` and
`server::access/control` implement a real Project/identity vertical. Native
Argon2id verifies bounded opaque Bearer capabilities from encrypted SecretVault
objects. Cookie and Bearer channels cannot be mixed or used as fallbacks. The
locked business transaction rechecks exact credential epoch, expiry, revocation,
project/Mission and scopes. Recent human authority, one-time CLI grants with full
request snapshots, CAS and immutable original response receipts are enforced in
the same transaction. No token/verifier enters public DTOs or receipts; issuance
retries never return the raw secret again.

The source tests cover real HTTP/private-cookie/TOTP/Argon2/SecretVault/database
requests, cross-project reads, forbidden automation management, grant substitution
and parent-resource binding, actual revocation lock waits, concurrent same-key
commands, stale CAS and rollback. These are bounded implementation proofs, not a
claim that the still-missing research/CLI/MCP/UI/worker/production acceptance is
complete. Current totals and outcomes must be taken from the exact tested commit's
CI log; historical totals above remain explicitly historical.

## Control review corrections: replay, verifier ownership and native rate limits

The grant HTTP path checks the authenticated CLI's exact existing receipt before
fresh TOTP/reauth quota, while still enforcing current authority and global epoch.
Credential issuance now holds the existing command transaction before materializing
a verifier. Reconciliation locks that same authority row and consults immutable
credential history on the primary before deleting only an authenticated, unpublished
MACHINE_VERIFIER object. A local maintenance command covers cancellation/crash orphans;
unknown database outcomes preserve files. No secret enters receipts or metadata.

PostgreSQL shared global/per-credential windows bound failed/in-flight native Argon2
checks. Successful verification refunds only its original windows, with independent
machine and human crypto slots. Native HTTP, SQL concurrency/rollback, filesystem
purpose/symlink and bounded-window tests exercise these paths. Counts and results
belong to the exact CI Head, not an unversioned claim in this document.

The database-native CI waits for final TCP readiness, not the official image's
initialization-only Unix socket server. The PGMQ contract itself remains unchanged.

## Verification commands and evidence rules

Run with the committed lock; do not format or regenerate tracked code inside a
read-only acceptance job. Generated outputs must compare equal to committed files.
Use an isolated native PostgreSQL/PGMQ database, never an existing user database.
The CI workflow pins and verifies the exact pull-request Head before execution.

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked --workspace --all-targets
cargo test --locked --workspace --exclude store --exclude server
DATABASE_URL=postgres://TEST_USER@127.0.0.1:55432/postgres \
  cargo test --locked -p store -p server -- --test-threads=4
cargo run --locked -q -p contracts --example generate > /tmp/domain-v1.openapi.json
diff -u contracts/generated/domain-v1.openapi.json /tmp/domain-v1.openapi.json
cargo run --locked -q -p server -- openapi > /tmp/api-v2.openapi.json
diff -u contracts/generated/api-v2.openapi.json /tmp/api-v2.openapi.json
node tests/contracts/decimal-wire.mjs
node tests/contracts/bigint-wire.mjs
```

The workflow additionally executes the native solver/Nautilus/Arrow report,
locked official Codex stdio/schema probe, PostgreSQL/PGMQ transactions and legacy-
path rejection. The aggregate requires every foundation job. Record the actual
commit, command, zero/nonzero exit, passed/failed/ignored counts and artifact/run;
compile-only, missing credentials, skipped steps or cancelled runs are not passes.
No test fixture or source archive is evidence of a real subscribed model request.

Store restart tests recreate a client, not an OS/container crash. Real lock waits,
lease clock checks, missing-queue rollback and independent test databases cover
specific relational/concurrency contracts; they do not prove deployed isolation,
production TLS, backup restoration or end-to-end research. Crypto is native, but
a passing authentication test does not establish all role-specific research rights.

## Native service schema hardening (2026-09-06)

Based on remote `0f71046d4bb69b8d37b76429000e2d33daea57be`, the runtime
privilege query now covers app, tower_sessions and pgmq; missing service schemas,
object/schema ownership, CREATE, TRUNCATE, TRIGGER and reachable delegated
privileges fail closed. Ordinary native session and queue DML remains allowed.
The session catalog contract also verifies persistence, RLS, constraints, triggers,
rules, inheritance, column semantics and native index operator classes.

Three new PostgreSQL runtime-role tests and one real CLI migration test exercise
these boundaries. The migration test tries fourteen incompatible definitions and
checks that the epoch, exact migration history and existing session bytes stay
unchanged after each failed command; an ordinary expiry index and native session
CRUD remain positive controls. The two representative new tests were also run
against the unchanged baseline SQL and reproduced the original missing rejection,
not a connection/setup failure. No existing migration or Cargo.lock was edited.

The full locked local workspace, strict Clippy, formatting, build and native
contract/scalar comparisons passed for this candidate. Exact test totals and the
candidate tree are recorded in the PR evidence comment; this paragraph is not a
permanent claim that every future Head passed. Remote CI and independent review
must validate the published Head separately, and none of these checks replaces
complete product acceptance.

## Administrative delegation and exact evidence producers (2026-09-06)

This iteration starts from remote `6cada81a6f94bfa29831cf4ede220c8d3ca4e711`.
Native PostgreSQL membership traversal includes ADMIN OPTION even when INHERIT
and SET are false, follows delegated paths, and retains the original session
identity. It also rejects reachable native server-file/program roles. Four new
PostgreSQL tests include a real self-regrant counterexample, multi-hop and masked
session cases, and harmless-membership positive controls; no OS command is run.

Downstream principals require a project when created. The domain and Store
regressions prove rejection without a stranded principal or command receipt.
The new additive `202609060010_evidence_producers.sql` binds evaluation reports
and metric sources to the exact project/run and expected artifact roles.
Approval context must be frozen, belong to the release project, and contain its
exact evaluation reports. Four PostgreSQL tests cover both valid source layouts,
wrong projects/runs/kinds, draft or unrelated approval contexts, and rollback of
an upgrade containing incompatible historical evidence. Prior migrations, the
product Cargo.lock, original test assertions and historical evidence are retained.

Local validation of this code snapshot used Rust 1.98.0, the committed Cargo.lock,
and isolated native PostgreSQL 18.1 with PGMQ 1.10.0. The complete workspace test
command passed 206 tests, zero failures and zero ignored; strict Clippy, rustfmt,
the all-target build, generated API/domain diffs, and the 204 decimal plus 242
bigint shared Node cases passed. Exact source-tree identity and published-Head
CI are recorded in the PR, not retroactively assigned to these local logs.
These source/authorization regressions do not establish the still-missing
production research, runtime-isolation, portfolio or protected-account acceptance.

## Immutable research preparation (2026-09-06)

This implementation starts from remote `a41e8709576d344919ffbe121b2619932793fe0d`
and adds typed InputSet and EvaluationPolicy HTTP/Store commands, with DESIGN A4.3
written before code. Public input metadata never contains native storage references
or sealed bytes. Metadata registration is neither algorithm execution nor capability,
PIT, PASS, qualification or deliverability proof. Complete Brief/worker/native runtime
admission remains separate work, including rechecking current data authority.

The existing OperatorCommand/receipt path authorizes and atomically publishes a
frozen input aggregate or a policy/family bound to the project's immutable lineage.
Policy versions are allocated under the project lock. Dataset role/cutoff, source
and runtime availability, current immutable grant/revocation and exact artifact
ownership are rechecked with native row locks. Revocation inserts lock the same
grant; a separate read after lock acquisition uses a fresh READ COMMITTED snapshot.
Only exact scoped machine reads are enabled; no new MachineScope or SQL backdoor.

Twenty-two new tests cover three native wire/schema cases, two pure contract cases,
twelve real PostgreSQL cases and five real Axum/native-authentication cases. The
latter use actual Cookie/Bearer, TOTP, Argon2 and SecretVault with disposable migrated
databases; the new HTTP suite uses the actual router, not a mocked API. Real lock
waits, opposite-order revocation/consumption, concurrent idempotency, versions,
sealed/WF identity rules, complete CLI-grant request binding, safe field diagnostics,
large request limits and full rollback of failed receipts are exercised.

These tests exposed an existing zero-argument `guard_revision` defect: PostgreSQL
provides NULL TG_ARGV, and FOREACH failed with SQLSTATE 22004 for otherwise legitimate
Runtime/Downstream updates. The additive 0012 migration coalesces only that native
argument array; the regression executes the unchanged original function to reproduce
22004, then verifies updates, versioning and immutable-identity/source rejection
under the replacement. Applied migrations and Cargo.lock remain untouched.

Validation results, exact source tree and the independent published-Head CI/review
are recorded in the PR, not inferred from test definitions or a historical Head.
Parallel Run/SSE/CLI and Ant Design implementation is preserved and is not claimed
as part of this preparation slice.

## Explicit gaps and completion boundary

Complete Run admission/takeover, research services and machine authorization,
remaining API/Worker/CLI/MCP, same-Thread model/tool/job/evidence integration,
independent Reviewer, PIT/sealed isolation, multi-Alpha shared-capital portfolio,
target-only approval/feedback/wake services, Ant Design UI/PWA, user-data migration,
backup/restore, protected real-account acceptance and production deployment remain
incomplete. JSON container/version checks are not complete policy validation.
Removing legacy tests is not acceptance, and foundation CI is not full W0–W8/T01–T42.

Keep PR #63 Draft until the entire Issue #62 contract is implemented and evidenced.
Completion requires the PR, all applicable CI on its latest Head passing, all
review findings resolved and an explicit no-findings Codex review of that Head;
only then merge. Verify main and required migration/isolation/recovery/end-to-end
evidence before closing #62. **On GitHub, Codex is review-only: never ask it to fix,
implement, commit or autonomously handle problems.**

## Run/Attempt 与持久 SSE 实现增量

新增 `crates/store/src/lifecycle.rs`、`crates/contracts/src/lifecycle.rs` 和
`apps/server/src/runs.rs`。复用 SQLx/PostgreSQL/PGMQ/Axum/Tokio，提供受信任域服务
准入、单 Attempt 接管/发送意图、取消/终态回执/归档，以及带认证作用域的 Run 查询
和 SSE；不是开放任意命令执行，也未把远端结果当成科学 qualification。

验证入口：

```sh
cargo test --locked -p store --test run_lifecycle
cargo test --locked -p server --test runs_http
cargo test --locked -p contracts --test lifecycle_wire
```

数据库套件验证同键并发、预算超发、PGMQ/事件故障整体回滚、lease/epoch、丢 ACK、
取消/完成竞态、锁等待后过期、回执关联与真实失败代码。HTTP套件以真实 TCP listener、
Axum middleware、原生加密/会话及 PostgreSQL 检查 cookie/Bearer、跨项目拒绝、
SSE 多批终态重放/断点续读/权限撤销/连接上限和断线不取消。共享 fixture 仅提供
领域记录，不模拟 HTTP 认证或将 FIXTURE 变为 REAL。

首次回归复现原有零参数 revision trigger 错误；新增迁移修复空 TG_ARGV。
其余已应用迁移原样保留，两个新增领域表的期望计数及生成合同同步更新。CI仍须在
推送后的精确 Head 上运行；测试定义、本地编译或本节文档均不能冒充远端 CI/Review
通过。没有在本节写入随后可能失效的当前测试总数或 release-ready 声明。

## 2026-09-06：交付边界与原生仓位审查增量

基于已入 PR 的 `0584a0c34ee931bdf7d31f64ee23c6e017ef17ed`，本轮新增
`202609060013_delivery_boundary.sql`，原有 SQLx 迁移与 Cargo.lock 不变。
覆盖 Package 精确项目/角色/版本、REAL 来源元数据、DEMO 不可审批或发出 Offer、
Research lineage 无环、Claim 行锁后 DB 实时过期检查、Forward 已转移状态与拒绝竞态。
升级检测到非法历史时原记录及全部迁移历史保持不变，不改写、删除或重新标注证据。

新增9个真实 PostgreSQL测试和1个真实Nautilus引擎测试。修复后的全workspace执行
得到238项通过，0失败、0忽略；严格Clippy与all-target构建通过。Nautilus Rust0.63.0
实际输出745 iterations、12 orders、24 events、**12 positions**；报告包含原生
position count并拒绝零仓位。它仍明确是 `FIXTURE`、`deliverable=false`、无Python。

旧审批/反馈关系测试的正向对照现在创建独立 PACKAGE 元数据和显式 REAL 关系样例，
DEMO helper始终保留FIXTURE且不能审批，反馈正例必须先真实执行Claim状态转换。
这些仅是隔离测试库里的身份/约束样例，不包含实际市场数据或声称科学资格，也没有
生产测试开关、重标现有fixture、下游真实交易或绕过产品Gate的代码。

此处结果不替代新增Head的GitHub CI和独立Review；完整产品仍按DESIGN验收。

## 2026-09-06：研究用途、Policy/Family 与字段诊断审查修复

基线为 PR Head `6191ceadb79ab2db03c2af1d0f214687200ef2d3`。
锁住的 DataUseGrant 事实现在包含封闭 allowed_uses；RESEARCH 不能登记
PORTFOLIO/FORWARD 用途，较高授权仅允许对应准备，不替代后续 Live 发布检查。
Policy 与 Family 通过原生生成的 UUIDv7 关系列、两个精确复合延迟外键一一绑定，
同事务可按任一顺序创建；不存在/错项目/错根/额外 Family 均不能永久提交。
新增 015 迁移审计历史而不改写它，保留 014 的 Brief 作者工作包序号。

指标代码、scope、方法列表及各项字符串的边界由原生 utoipa 发布到两份生成合同，
不手写第二套 schema。精确 Decimal 比较器保持不变，阈值错误现在指向真正的
缺失/多余端点，BETWEEN 倒序同时标记两端，不向客户端回显数值。

修复前对原 6191 源码实跑：6 项真实 PostgreSQL 回归、1 项原生 schema 回归、
1 项 Domain 诊断回归均复现失败。修复后新增总计 13 项回归，包括 10 项真实 PG、
1 项真实 HTTP、1 项生成 schema、1 项比较器诊断；还覆盖合法历史升级、坏历史
整体回滚、生成列不可覆盖和用途矩阵。原并发测试的 pg_stat_activity 精确查询
前缀同步新增 allowed_uses 字段，仍须观察真实 PostgreSQL 锁等待，未删除断言。

当前候选源使用原样 Cargo.lock、Rust 1.98.0、真实 PostgreSQL 18.1/PGMQ 1.10.0
执行全部 273 项 workspace/all-target 测试：0 失败、0 忽略。严格 Clippy、fmt、
全目标构建和两份原生生成合同已核对；204 Decimal、242 Bigint 共享语料通过。
本地结果不代表新提交 CI 或独立审查已通过；这些仍必须针对实际推送 Head 完成。
本增量不改变完整 W0–W8/T01–T42 验收、Draft 状态或 Codex review-only 边界。


## 2026-09-06：Run 恢复、当前许可与 Forward 来源审查修复

本地基线是远端 a52fa0fd88f44aeeee59a6d7f1bccfadf7b11fe0 的原生 tree
469e918b0eacaf193416e8d2cf133c38e1e57bdd；原样 Cargo.lock/Rust1.98。
新增016/017迁移，001–015原字节保留，014仍为Brief作者流程的预留序号。

修复前新增回归实跑复现六项故障：10ms请求超时杀掉25ms迁移DDL、过期NOT_SENT
仍被续租、终态Attempt仍可改、成功Attempt含错误码、未知兼容事件拒绝、旧浏览器
可取消。另用未安装017的真实数据库复现了跨项目Forward报告被接受。负向结果是
旧缺陷证明，不作为通过结果。原有终态重传测试改为真正等待活动期设置的短租约
过期，不再通过修改已终结Attempt安排测试；PGMQ批量读取的测试一次保留两个消息，
不在其visibility窗口内错误地二次读取。并发测试仍要求观察原生锁等待。

实现复用SQLx原生事务/锁、PGMQ、BigDecimal、utoipa和Axum SSE：新消费重新授权，
确切回执与未知结果对账不受事后撤销抹账；无Cycle只允许有界管理任务；未发送过期
保存NOT_DISPATCHED事实而非伪造远端失败；已终结Run的任何Attempt写入均拒绝。
部署连接独立取消statement_timeout而不污染请求池。政策fraction与capabilities
由原生schema生成并用214条共享精确语料检验，不重建Decimal实现。

Forward保存一次性领取元组和显式legacy来源，拒绝领取前的历史消息、凭拒绝状态
伪造领取，以及不符合精确项目/角色/schema/origin/access的报告。合法既有反馈在
随后拒绝时保留；坏历史令升级整体回滚。测试中的REAL元数据仅为可丢弃库内的
关系正例，没有市场数据/报告字节，不宣称产品验收。

验证入口：全workspace/all-target locked测试、严格Clippy、all-target构建，
contracts/example generate与server openapi原生生成后比对，两份schema的204
Decimal、242 Bigint、214 Fraction共享语料；真实PostgreSQL18.1/PGMQ1.10与HTTP。
实际通过结果绑定发布Head的CI/PR证据，不在本节冒充远端Review已通过。
完整W0–W8/T01–T42仍须继续交付，Codex仅独立review。


公有开发依赖/工具已取回到隔离开发目录，删除完成的client-development-inputs、
prepare-web-dependencies、web-development-inputs工作流；它们不是验收，不保留
随每次PR提交重新取依赖的永久开发任务。正式CI仍只读、使用已提交锁文件。

## 2026-09-06：Brief 草稿作者流程与 SSE payload 合同

本地基线为远端205bc4c0fd16b593263dd744895826a7ee5456cf的完整tree
5456cc177ceb45d0b59e1e46fed89cb3b22dd2f6。该基线独立CI34049936238成功，
但其Codex审查指出payload生成schema缺少对象/版本约束，不能将Completed当无问题。

新增真实Brief create/read/list/PATCH：完整非秘密规范意图绑定路径project_id与
schema_version，既有Operator单次CLI grant、幂等原始响应、项目锁/CAS和绑定
替换同事务。014使用预留迁移号，不改任何已提交迁移或Cargo.lock。父Brief锁将
DRAFT编辑与FROZEN成员封口串行化。运行身份只新增brief_data_bindings一张表的
DELETE权限，旧不可变历史守卫与其他表无DELETE/TRUNCATE/TRIGGER边界保持。
保存草稿不证明当前许可、PIT或原生能力，不提供假成功的freeze入口。

205的payload审查通过原生utoipa发布JSON object、必需schema_version=1及可扩展
公开属性修复；已有事件类型运行时仍严格校验，未知兼容事件不改状态投影。

新增18项Brief测试（9真实PostgreSQL、4真实HTTP/私有Cookie/TOTP/Argon2、3领域、
2线协议）和1项SSE schema测试。非owner真实部署身份可创建/替换草稿，不能删除
其他表或改冻结成员；原生PG锁等待覆盖编辑/冻结，故障注入证明绑定与CAS回滚。
旧浏览器测试通过原生SessionStore关联合法历史BrowserLogin，不倒退认证时间，
不关闭触发器或改变生产鉴权。

本地对本增量完整源码运行Rust1.98与原样Cargo.lock：313项workspace/all-target
测试通过，0失败、0忽略；fmt、严格Clippy、all-target build通过。两份OpenAPI
由实际Rust生成。发布必须再跑对应新Head独立只读CI和review；本地结果不等于
完整W0–W8/T01–T42、原生账号验收或可合并状态，Codex仅review。

## 2026-09-09：可执行原生预测、组合求解与共享资金模拟

在 `ace5c6502dbf1988a4891aa572da6ddbe735842b` 上的网页作者工作树，执行器实际运行
`cargo test --locked -p job --tests`（62通过）、`cargo test --locked -p contracts -p domain`
（25+55通过）、workspace/all-targets Clippy 和格式检查，均为exit 0。原生任务覆盖
目录Parquet读写、严格资产/时间/步长检查、受fuel限制的Wasm预测、冻结标签、Clarabel
约束、原生Walk-forward/CPCV/协方差/OLS、真实CLI和独立并发模拟进程。

该回执绑定计划 `3ab3630ca819cd57b9f65627da5be7615ce61ba2e8cdaab833e785ee0b57e10b`，
结束于2026-09-09T09:15:24.624Z。原始命令输出位于本地忽略目录
`.ai-bridge/owner-science-9/`；本记录不是远端CI成功声明，也不包含本轮Store/Server
数据库回归。之后网页作者按同次原生输出补全domain OpenAPI；其一致性需再次执行比对，
HTTP OpenAPI在该次原生生成中无差异。不要把handoff进程exit 0替代每个命令的结果。

发现并修复的实际问题：Wasmi默认分派在未优化无限循环回归中溢出宿主栈，使用上游
portable-dispatch后保留预算和回归；Nautilus logger使用具体LoggerConfig；原生收益的
position fallback不能当组合资本收益，适配改为唯一账户原生equity snapshots的日收益；
跨日全现金0收益与日内样本不足分开。同进程内核复用导致相互干扰的测试改为实际生产
一任务一进程边界，并用四个同时运行的原生job保留账户、权益及0收益断言，不加串行锁。

这些计算入口仍不拥有HTTP/MCP调用方的任意路径、REAL来源、PIT、资格或交付授权。
正式数据登记、Brief冻结/Cycle启动、Worker/Codex、独立评估发布、两Alpha组合和
交付/Forward/Wake以及完整UI验收仍按DESIGN完成；本增量不得被当作整个Issue62关闭依据。

## 2026-09-11：原生账号及 Mission 接线

本机接管后的账号/配置增量 `d849dc35eb056393600d1f13ecdcb37701c39810` 已通过
原生账户策略/注销、PostgreSQL/HTTP、10项CLI传输、503项Web单元、5项PWA文件与
150项三视口浏览器检查。原生发行版不支持debug-only登录issuer覆盖；没有将模拟
OAuth成功当成真实账号验收，没有读取生产profile或完成受保护账号推理。

后续Mission适配使用同一官方0.144.4：native tool_search → 原生MCP namespace调用
→ 实际stdio服务 → HTTP/认证/PostgreSQL → 原生shell工作区写入 → 进程退出/重启
→ 原Thread恢复。一个Turn中4次受控模型请求累计48个原生报告token，下一Turn
再增加12；不是只取最后一次请求。随机工作区外文件与服务环境canary未进入模型请求。
个人全局提示文件/配置覆盖在Thread发送前拒绝，原生数据和用户文件不被复制或修改。
锁定版本的default_permissions、精确二进制只读范围和原生工具发现差异见reuse记录。

本地focused验证目录 `.ai-bridge/verify-DwWYyZ` 的编译、fmt、严格Clippy、领域、
managed、native Codex、Store及HTTP/MCP检查均exit0；源码快照未变，独立测试PG已停止。
模型上游和父研究数据仍是明确fixture；尚不是Cycle自动Mission启动、真实科学Job/
Evaluation返回同Thread、完整T07/T42或可合并证据。CI已登记上述native测试，远端实际
结果必须绑定推送后的Head；本记录不代替GitHub review或完整Issue62交付。

## 2026-09-11：Cycle 原生配置选择与续轮版本

CycleStart 必须明确 researcher_profile/reviewer_profile 的 profile_id 与十进制
expected_revision；migration028 将两个选择随 cycle_startups 封口，历史空选择保持未知。
启动复用原生配置快照锁和现有事务，配置过期、缺失或账号操作进行中不产生 Run/PGMQ
半状态。修改 Profile 不改写旧 Cycle 或原始响应，准确重放仍读取原回执。
新模型轮预约和首次发送重新检查 Session 的 Profile 与锁等待后的 Attempt lease；
已发送调用仍可绑定原生 Turn、对账和结算，不因配置变化退款或重发。

本地 `.ai-bridge/verify-VBYni7` 全量验证 exit0：编译/fmt/严格Clippy、领域及
managed/native Codex 检查通过，Store+Server 共451测试通过、0失败、0忽略；
源码快照未变，独立测试 PostgreSQL 已确认停止。之前 `.ai-bridge/verify-vUFFS9`
只有固定79张表的旧测试断言失败，已改为重复迁移前后的完整表清单一致及必需表检查。
OpenAPI/TypeScript/Ajv 在 `.ai-bridge/web-verify-zO8r57` 两次生成逐字节一致，
手写源未变，构建、wire、150浏览器用例通过；新增合同测试的TypeScript缺值检查
随后修正，原生命令复核 typecheck、504 Vitest 与5个PWA文件测试全部通过。
这些是本增量实际代码/原生事务和受控协议证据，不是完整Cycle→Mission→科学Job→
Evaluation、真实账号T07、完整T42或远端最新Head的CI/review/可合并结论。

## 2026-09-12：原生准备到 Mission 准入与 Thread 绑定

029迁移增加run_missions的不可变驱动/角色/Profile快照；科学Worker在两条终态
ack路径前统一推进正式Cycle。成功必须具有精确的原生DATA_QUALITY生产者证据；
暂停和账号操作保留通知，配置/输入错误与预算耗尽如实收束。同Cycle同角色只入队一次。
Mission与科学消费者共用PGMQ原生条件读取，各自不隐藏或claim另一类任务。

Thread回执仅保存原生身份、版本、公开有效设置和实际非秘密覆盖项，默认设置保持省略，
不复制原生聊天。当前fence绑定唯一Session，重复回执逐项一致；配置修改、取消或接管
不会替换Thread。新turn可在已绑定的DISPATCHING/RECONCILING Mission中预约，
但要求已提交的首次发送意图、当前Profile/lease、有效预算和前轮已真实结算；不假造RUNNING。

历史失败：`.ai-bridge/verify-NkKl06`暴露外键误指admission主键而不是run_id；
`.ai-bridge/verify-nxpReW`全量456通过/5失败/0忽略，失败均是新Profile快照缺少
schema_version封套；`.ai-bridge/verify-Sr5WGg`仅剩首轮不能在DISPATCHING预约。
以上均已修正。当前`.ai-bridge/verify-CYPMGw`定向检查exit0：check/fmt/严格Clippy
及真实PostgreSQL的Mission12、turns18、恢复8、Run生命周期30、约束7，共75通过，
0失败/0忽略；源码快照未变，独立PG已停止。数据库测试中的原生返回仍是明确fixture，
不是实际自动Codex Worker、独立Reviewer、T07/T42或最新GitHub Head的全绿验收。

可信Mission签发沿用现有随机能力、Argon2与SecretVault；固定角色范围、当前Attempt/
owner、每owner一次签发与接管失效已通过真实PostgreSQL及原生验证器测试，包含撤销、
禁用、取消、重复请求和错误科学Run。测试秘密只存在独立临时目录。
后续全量 `.ai-bridge/verify-4YvHCB` exit0：check/fmt/严格Clippy，领域149、managed6、
native Codex25、Store+Server465通过，0失败/0忽略；源码未变，独立PG停止。
这些是本地增量回归，仍不代表Mission Worker已接通或完整Issue62可合并。

## 2026-09-12：可信 Mission bootstrap 与原生接管

`MissionLauncher`复用原生Deployment配置/账号/catalog解析，创建独立空Git工作树，
签发固定MCP范围，凭Run首次发送许可创建持久Thread并保存Session。探测不付费，
默认模型覆盖仍省略。恢复使用原Thread/工作树并核对原始公开设置；未知start和未产生
原生rollout的空Thread不创建替代Session。Git原生执行版本2.55.0；不克隆产品仓库、
复制认证或读取原生聊天文件。

`.ai-bridge/verify-WRDho0`暴露测试辅助函数参数数目及原生恢复失败；静态原因投影
随后在`.ai-bridge/verify-KepXdL`确认RESUME_THREAD/-32603。根因是底层机器授权
允许RECONCILING而MCP身份入口和实验提案仍拒绝，已统一两入口，未伪造RUNNING。
当前`.ai-bridge/verify-7FUpAq` exit0：check/fmt/严格Clippy，Profile HTTP4、MCP5、
Mission bootstrap3，共12通过/0失败/0忽略；源码未变，独立PG停止。实际官方进程
重启、同Thread上下文、同账本两轮累计24个原生报告token及旧owner不能重发均通过。
上游模型回答和市场准备仍是fixture；测试手动调用可信逐轮Store，生产Worker消费/
资源约束/请求产物/科学结果回送尚未完成，不能当作完整T07/T42或GitHub当前Head验收。

后续全量`.ai-bridge/verify-DHzFHw` exit0：check/fmt/严格Clippy、领域149、managed6、
native Codex25、Store+Server469通过，0失败/0忽略；源码快照未变，独立PG确认停止。

## 2026-09-12：逐轮公开请求与恢复投影

`prepare_mission_turn`复用原预约事务逻辑，将原始公开prompt的固定schema产物、
预算预约和PGMQ消息原子发布；文件I/O之后复核租约/期限，字节计入输出预算。
重放逐字节比较，不接受改prompt/改key；失败或过期回滚全部数据库半状态，未知文件
提交沿用现有Run锁定的未引用对象对账。恢复只读取该Run/Session/Attempt/command的
原请求，checkpoint区分已发送但无ACK、原生Turn绑定、终态与已结算用量。
配置变化不擦除旧请求或虚构退款。没有读取或复制原生聊天/隐藏推理。

`.ai-bridge/verify-FBFVNA` focused检查exit0：check/fmt/严格Clippy、领域、managed、
原生Codex及相关Store/HTTP/MCP通过，源码未变、独立PG停止。新增事务并发/失败/
租约测试与原生同Thread两轮测试均通过；后者现在使用实际保存和重新读取的公开请求，
不再借准备任务参数代替prompt。模型回答/市场准备/费用仍是标明的fixture；真实
Worker逐轮驱动、资源约束、定价未知处理和科学结果回送尚未完成，不作为全量或CI证据。

## 2026-09-12：原生单轮驱动与未知结果保留

`MissionConnection::drive_turn`现在直接消费真实请求产物及唯一发送许可，核对原生
Thread/Turn，按同Session累计token差结算。首次原生终态使用数据库接收时间，重复
查询保留原时间；无用量时只保存终态，不补零或释放预约。未知发送ACK不按列表顺序
猜身份，不重发。Run或未结算Turn到期由共享数据库入口先提交取消，再调用原生
interrupt；interrupt ACK不等于用量结算或整个Run已取消。没有可信费用报价时，
新费用上限调用在发送前明确拒绝；原生费用不可用模式不伪造金额。

第一轮`.ai-bridge/verify-kCWWbm`为14通过/1失败，费用分支测试错误地使用默认
UNAVAILABLE fixture。已在Draft阶段显式建立USD/ESTIMATED测试预算后冻结；未修改
冻结预算或放宽断言。原生两轮测试改为调用生产驱动；新增丢ACK、丢用量、费用拒绝及
真实原生interrupt故障验收，慢模型响应仅是明确的本地HTTP fixture。

随后全量`.ai-bridge/verify-TczpoZ` exit0：check/fmt/严格Clippy、领域149、managed6、
native Codex25、Store+Server475，共655通过/0失败/0忽略。源码快照未变，独立PG
已确认停止。这是628b18fe基础上的本地工作增量证据，不是新GitHub Head的CI结果。
Worker队列/CLI接入、整个进程树资源限制、自动科学结果回送与Reviewer仍未完成，
不能把单轮驱动通过当作T07/T42、Issue62完成或允许合并。

## 2026-09-12：原生进程树资源与取消竞态

Mission复用Linux systemd user scope与prlimit，对整个原生进程树施加冻结CPU速率、
内存、进程数和剩余墙钟。连接持有经自身PID成员校验的原cgroup.kill描述符，关闭和
异常Drop清理原组，不按可复用名称误杀后来的组。原生内部文件单独限64MiB，不把
QZ较小的研究产物预算误用作Codex SQLite迁移上限；两者不是工作区总磁盘配额。
整个bootstrap统一限110秒，低CPU下单RPC60秒/MCP启动45秒仍在此界限及Run期限内。

真实故障定位包括：外部验证器缺XDG_RUNTIME_DIR、systemd只接受百分数两位小数、
1MiB文件限制使原生SQLite WAL触发SIGXFSZ、主进程退出后遗留bwrap阻塞同Run重连。
固定0.144.4源码及运行还证明turn/start先确认入队，不能假造RUNNING或立即中断
未开始的Turn；驱动现在等待真实开始，完成竞态的拒绝只按精确原Turn查询终态。
deadline测试保留DB取消意图先于真实原生终态、未知用量不补零的断言；启动阶段
可能已被中断，因此不要求一定发生Provider请求，但禁止重复请求。

`.ai-bridge/verify-wOL0cl`局部16项通过；最终全量`.ai-bridge/verify-OWRz0r` exit0：
check/fmt/严格Clippy、领域149、managed6、独立native Codex28与Store/Server478
均通过，0失败/0忽略；native组有重复执行的Server用例，不是另外28项独立覆盖。
内核实测包含内存耗尽SIGKILL、剩余墙钟到期SIGTERM、实际cgroup/rlimit读回及
主PID退出后的后代清理。源码未变，独立PG已确认停止。模型回复和市场准备仍是
显式fixture，不含真实账户或生产数据。这是eb115840基础上的本地增量证据，不是
GitHub CI。Worker/CLI自动Mission、科学结果闭环、Reviewer、工作区总磁盘配额及
完整恢复/用户流程仍未完成，不能据此合并或关闭Issue62。

## 2026-09-12：逐轮超预约后的新发送阻断

新预约和首次派发现在共用同Cycle不可变reservation/receipt的原生SQL比较；token或
精确小数费用超过原预约，即使Cycle总额未超限也阻断新发送。原真实结算、原回执重放
和确认未发送的退款不受此新准入检查影响，不增加另一套可重置计数或标记。
`.ai-bridge/verify-Vtz2Vq`在ed8f9632基础上运行check/fmt/严格Clippy及真实PGMQ/PG
Mission、turn、recovery、Run生命周期、约束回归，80项通过/0失败/0忽略，源码未变、
独立PG确认停止。新反例保持used+reserved+requested低于Cycle总额，分别验证token
超预约和仅小数尾部费用超预约，并验证其他Mission事先预约的首次发送也被阻断。
这是局部业务账本证据，不是Provider实际计费、全量CI或Issue62完成证明。

## 2026-09-12：浏览器冻结、明确启用与双角色启动

复用既有严格HTTP DTO、ResourceSelect、ProjectEditor和RunDetail，新增Brief执行确认与
持久Cycle列表，不新增依赖或平行业务状态机。非归档项目可冻结，成功后还须显式启用
才能启动；Profile分别明确选择，Project/Brief/Runtime/Profile修订保持原字符串。
未知回执保留原内容和幂等键，后台失败或配置刷新不能改写原意图；首次明确409要求
重载，未知请求后再收到409仍不能声称之前未执行。排队只显示Cycle/准备Run事实。

浏览器回归定位到共用QueryPanel的真实卸载问题：AntD Space会展开Fragment并以位置
给无key子项编号，插入两条读取警告使既有表单重新挂载。现在共用内容容器有稳定key，
原表单/待确认请求跨警告插入和移除保留；三尺寸丢ACK与后台失败回归覆盖这条路径。
局部TypeScript/Vite及33项浏览器回归通过（58945）；合成响应只证明页面合同，不是
真实数据冻结、模型研究、资格或完整Issue62证据。全量生成与浏览器验证另行记录。

最终`.ai-bridge/web-verify-eDjimU` exit0：Rust原生合同双次导出、TypeScript/Ajv双次生成
逐字节一致，Runtime合同不变；TypeScript、504项Vitest、5项PWA文件测试、三组数值
wire检查、生产构建、Codex设置专项及全部168项三尺寸浏览器测试、CLI帮助均通过。
手写源码前后不变，生成文件无差异。这是ca22a9f4基础上的本地增量验证，不是GitHub
当前Head CI；未运行真实账户付费研究，也不表示全量W0–W8/T01–T42或交付链路完成。

## 2026-09-12：工具续轮失败不能结算中途用量

锁定原生0.144.4的真实反例：第一条合成模型响应调用原生shell并报告12个token，
实际工具完成后第二条模型请求收到不含用量的断流。原驱动错误地返回
FAILED/actual_tokens=12完整回执。`.ai-bridge/verify-3rRkL9`因此16通过/1失败，
check/fmt/Clippy通过且源码未变、独立PG已停止；不是推测或仅mock客户端。
修复只在共用原生驱动中限制自动结算为COMPLETED，失败/中断保留终态与未知预约。
不新增表或计费器；独立可信完整用量仍可由既有Store账本结算。

最终`.ai-bridge/verify-Ftn1NN` exit0：全工作区check/fmt/严格Clippy，以及原生
HTTP Profile4、MCP5、Mission8共17项通过/0失败/0忽略，源码未变、独立PG确认停止。
故障回归还核实第二次原生请求包含实际shell结果，不把仅生成工具调用当执行成功。
这是88990e23基础上的局部增量证据，未证明Provider真实计费、运行中预算中断、
生产Worker自动Mission、完整科学/Reviewer链路或GitHub最新Head完成。

## 2026-09-12：原生超额停止与终态来源核验

沿用现有Run事件和Turn账本：首次达到预约的原生部分用量形成`mission.token_limit`
及同事务取消意图，随后才原生中断；未结算期间同Cycle的新预约/首次派发被拒绝。
最终回执不能低于已记录观察，足额真实回执和旧请求对账仍可进入，观察历史不改写。
没有新增计费器、队列、表、依赖或业务hash；原生rollout budget的跟踪/提醒没有当成
硬性额度保证（[官方配置说明](https://learn.chatgpt.com/docs/config-file/config-reference)）。

真实原生测试发现两条边界，而非只放宽断言：一是工具续轮请求可能早于用量通知，
因此中断不能承诺撤回已经在途的第二次模型请求；二是0.144.4的真实断流通知为
FAILED，而`thread/turns/list(itemsView=notLoaded)`重建列表显示COMPLETED。
`.ai-bridge/verify-VKCPI9`的单例诊断实际观察2次请求及InProgress/Completed，
60秒内没有列表Failed。此前`zgJQv9`遇到只读列表-32603，`gjlqCg`只读重试仍超时；
均真实失败、源码未变且各自独立PG已停止，未把这些结果记为通过。

修复后只以真实`turn/completed`通知或其既有持久记录确认终态，通知立即入账。
ACK/列表只恢复身份；丢失真实终态时保留UNKNOWN，-32600也不能补造取消。
迟到部分用量不能把已失败Turn变为成功/取消，仍记录超额且保留全部未结算预约。

验证边界：首轮完整`.ai-bridge/verify-HnefIj`的check/fmt/严格Clippy、domain149、
managed6、native28通过，Store/Server482通过/1失败（错误地要求续轮请求必为1次）；
这不是最终全绿证据。最终Store代码在`.ai-bridge/verify-kuNCRF`通过82项真实PG回归，
含并发停止幂等、旧fence拒绝、跨Mission未结算门禁和39不能结算已观察40的反例。
最终原生状态修复在`.ai-bridge/verify-PQwg2g`通过精确反例，再以相同源码在
`.ai-bridge/verify-i4VkRN`通过check/fmt/严格Clippy及Profile4/MCP5/Mission10，
共19项通过/0失败/0忽略。两种原生顺序分别断言真实Cancelled和真实Failed、
120/100用量事件、原预约未结算；缺通知的列表完成不能生成终态，已有真实终态则
保持首次数据库观察时间。全部验证器确认源码未变和各自独立PG停止。

这是6d5a4ae6基础上的本地增量；合成模型响应/市场准备不冒充真实付费账户或科学
结果，未证明严格token/美元上限、生产Mission自动领取、科学/Reviewer完整链路、
GitHub最新Head CI或全量W0–W8/T01–T42完成。

## 2026-09-12：幂等首轮准备

首轮准备复用既有公开请求产物、逐轮预约和PGMQ事务；冻结Brief与剩余token账本
决定请求，已有任意预约则保留原请求，包括人工接续、未知发送与Profile后续变化。
费用受限但缺原生计费时拒绝准备，不插入产物、预约或队列消息。余额读取提交后
才进入原事务再次检查，避免嵌套持锁或把旧余额当发送许可。

`.ai-bridge/verify-phLsaW` exit0：全工作区check/fmt/严格Clippy与84项真实PG回归
通过（constraints7、missions19、run_lifecycle30、turn_recovery8、turns20），
0失败/0忽略；源码前后不变且独立PG确认停止。新增检查验证产物发表失败不留下
预约、首次额度10000、未知发送不换请求、已有人工请求不被首轮覆盖，以及无价格
不产生可发送工作。此为587803bd基础上的本地准备增量；尚未接入常驻Worker，
不代表实际模型发送、科学结果、Mission/Cycle完成或GitHub当前Head验收。

## 2026-09-12：常驻 Mission 首轮消费

复用Worker、原PGMQ、Run租约、MissionLauncher和逐轮账本，科学/Mission分别有界。
显式部署三项配置才领取Mission；未配置不隐藏其消息。整个驱动维护10秒续约60秒，
关闭/失去续约回收本机原生子进程，不造取消或退款。已结算轮接管不再开连接/重发，
Mission消息保留给尚需实现的科学、结论及完整收束阶段。

真实守护进程回归通过：未配置队列read_count不变；自动准备、发送并以12token结算
唯一原生Turn，重启不重复发送/预约/ack；等待实际模型响应时，单Mission槽不妨碍
科学终态消息被处理并archive，数据库租约真实延长；关闭后原预约、消息和未知用量
保留，Run不假称终态。CLI三种不完整配置均在访问数据库之前以exit2拒绝。
市场准备和模型响应仍是受控fixture，不是付费模型研究或真实科学结果。

首轮完整`.ai-bridge/verify-Fw5gJ0` exit1：check/fmt/严格Clippy、domain149、managed6、
native28通过，Store/Server488通过/1失败/0忽略。唯一失败是旧迟到用量回归将列表
Completed当成必然；本次实际返回Failed。实时快照与历史重建允许这两种投影，
测试现在仍拒绝其他状态，且驱动前必须没有持久终态；最终真实Failed、120/100事件、
不结算部分用量和时间先后断言全部保留，不修改业务驱动或放宽最终结果。

相同业务代码在`.ai-bridge/verify-aVDYqb` exit0：check/fmt/严格Clippy、Profile4、
MCP5、Mission13共22项通过/0失败/0忽略。两次源码前后不变，各自独立PG确认停止。
前一完整结果仍不是全绿。同步明确Issue62第6.3–6.4节的原生Turn语义：内部工具
续请求共享本Turn预约并累计用量，新turn/start单独预约；不宣称限制Provider HTTP
请求次数，不新增模型代理或接管原生工具循环。这是50c0d024基础上的本地增量，
不是GitHub当前Head CI或全量W0–W8/T01–T42完成证据。

## 2026-09-12：提案与编译任务的原子关联

新增030的不可变experiment_compilations边，复用已有Run/Attempt/原生定义/PGMQ。
可信当前Researcher Mission可为同Cycle的正式提案准备一次CompileModel；原代码、
参数指针在关联建立后冻结，旧来源不回填。任务只携CODE和服务生成的参数，无任何
Dataset挂载；准备的experiments=0，其他资源仍由原准入事务预约，期限不越过Mission。
重放取原Run，不重新发表或收费；没有新引擎、队列、公共执行DTO或应用hash。

`.ai-bridge/verify-6zRwrd` exit0通过check/fmt/严格Clippy及104项真实PG回归。
之后只补最终关联写入失败的故障断言：原生文件已发表，Run/任务定义/PGMQ已在
同一事务准备，再由独立测试数据库触发器拒绝最后插入；五项计数确认Run、产物
记录、队列、原生定义、CPU预算全部回滚，已发表对象保留，移除测试触发器后可
正常重试。没有通过删除文件或修改正式迁移来让检查通过。

最终`.ai-bridge/verify-2LK0GF` exit0再次通过check/fmt/严格Clippy与104项PG回归
（constraints7、data_validation7、experiment_compilations2、experiments11、missions19、
run_lifecycle30、turn_recovery8、turns20），0失败/0忽略。两次源码前后不变、独立PG
确认停止。正例检查并发只建一个Run/一份预算、原生参数精确引用原CODE且无市场
输入；反例检查陈旧fence、发表失败、非零试验计数、执行后改代码及删关联均拒绝。
这是79bb243c基础上的任务准备证据，不代表真实编译/预测执行、同Thread结果回送、
Alpha资格或完整交付；Worker后续阶段尚须接通。

## 2026-09-12：编译生产者到Discovery预测的原子关联

031迁移保存不可变experiment_forecasts，原提案的科学run_id与关联同事务提交且
不能换Run。可信准备入口要求原编译真实SUCCEEDED及其原生采纳的MODEL，明确
选择冻结Discovery版本，并以同一元数据适配生成有限选择。固定bars horizon与
Brief一致，预测参数复用原生任务校验；只挂载一个Dataset、原MODEL和服务参数，
不挂载代码或Sealed。existing Run准入一次预约experiments=1，并发重放返回原Run。

`.ai-bridge/verify-JzgA0E` exit0：check/fmt/严格Clippy及106项PG回归。随后只加强
参数负例并同步CLI/用户文档/薄Skill：保持文件字节长度，使Sealed数据选择、错误
horizon、超额fuel、错误EMA周期和自报MODEL分别触发真正语义校验，不只靠大小
不符失败。`cargo test --locked -p domain` exit0，96项通过/0失败/0忽略。
最终`.ai-bridge/verify-P9fRv6` exit0再次通过check/fmt/严格Clippy和106项PG检查
（constraints7、data_validation7、experiment_compilations4、experiments11、missions19、
run_lifecycle30、turn_recovery8、turns20）。两轮源码前后不变，独立PG均确认停止。
新正例验证未完成编译拒绝、原MODEL绑定、并发一个Run/一份试验与CPU预约、原生
Job输入及冻结run_id；负例验证错误参数/陈旧fence、最后关系插入故障回滚全部
Run/产物记录/PGMQ/预算，正常重试可继续。

这里的编译输出为明确标注的受控协议fixture，证明真实数据库/文件/队列事务，
不冒充执行rustc、Wasmi或真实市场预测；本地b1da5b7b上的此增量尚未接入Worker
自动科学推进/同Thread反馈，更不代表分折、Reviewer、资格或完整PR验收。

## 2026-09-12：常驻Mission自动准备科学步骤与过期探测恢复

Worker在最新原生Turn已确认结算后，按ordinal从现有正式提案中选取一个就绪的
编译或预测步骤，复用原Mission资源分配和原有Run准入；排队/运行中的身份不重建。
原生Client先关闭，科学Worker独立执行；准备不是Mission完成，消息不提前ack。
首轮公开请求新增确切Cycle及冻结政策family ID、现有Wasm ABI/参数合同，避免
Agent在仅有Brief/Mission工具的情况下猜测必填提案身份。复用共享fixture，API与
Worker使用同一真实ArtifactStore目录，不重复构造另一套原生编译回执。

完整`.ai-bridge/verify-AydzuH` exit1：前置check/fmt/domain/managed/native通过，
Clippy发现共享测试的一条多余import；Store/Server为493通过、1失败、0忽略，失败
是新增自动编译用例。删除import后，单例`.ai-bridge/verify-l1WeIe`仍exit1，安全
诊断明确是runtime_probe_stale：模型首轮约77秒，原Runtime探测有效期只有60秒。
没有通过忽略错误、改变预期、延长探测TTL或删除断言获得通过。

修复复用原有fenced Runtime探测：当前活跃Researcher Mission可刷新其冻结Runtime，
准备/发表重验状态、租约、版本及期限，真实HTTP仍在事务外；不要求已派发的模型
Mission回到NOT_SENT。临时诊断已移除。新增用例等待真实60秒探测过期，官方App
Server完成真实Turn后通过带原Vault凭据的原生HTTP刷新，随后关联原编译/受控采纳
MODEL/预测Run，多次接管仍只有一个模型请求和一份科学身份。未结算慢Turn即使
已有提案也不准备编译、不探测、不伪造用量或Mission终态。

`.ai-bridge/verify-osf4B5` exit0通过check/fmt/严格Clippy及该原生单例（126.55秒）。
最终完整`.ai-bridge/verify-QL1AOB` exit0：check/fmt/严格Clippy、149项领域/合同/
Runtime测试、6项真实原生Job子进程、28项原生Codex检查和494项Store/Server回归，
全部0失败/0忽略。检查期间源码未变，独立PG确认停止。上述成功仍只是5c7d78c2上
的本地增量证据；编译输出fixture不冒充真实rustc/Wasmi科学执行，完整同Thread
结果反馈/结论、独立Reviewer、评估/资格和剩余产品链路尚未完成，不是PR合并证据。

## 2026-09-12：原Thread科学反馈与一次性修复预约

在8c57d16d上增加已采纳编译失败/Discovery预测终态的公开反馈。复用已有原Session
Turn命令唯一键和同事务请求产物/预算/PGMQ发表，未增加反馈队列或状态表。最新
模型Turn没有完整usage结算时不继续；失败回送计REPAIR，预测成功计RESEARCH。
预测只读取原生产者的RESEARCH报告，保留origin/原生版本/fuel及全体观察计数，
明确标注首尾各16条抽样，形式评估为NOT_PERFORMED；不制造指标或资格。
失败只陈述公开Run原因，不捏造尚未接入的详细编译器诊断。修复保留原实验父血缘。

`.ai-bridge/verify-WJqhOH` exit0：check/fmt/严格Clippy及107项真实PG回归，0失败/
0忽略。新例证明失败编译不读取预测或诊断正文、未知模型用量不接续、产物发表
失败回滚预算/消息、并发结果选择只有一份修复预约、结算后重放不重复扣费及旧
owner拒绝。抽取的原请求事务仍由原有Turn/恢复用例覆盖。

`.ai-bridge/verify-SLsSW3` exit0：check/fmt/严格Clippy及真实原生科学/Mission单例
通过（212.20秒）。它等待原探测真实过期，首轮完成后准备原编译/预测，采纳受控
40条预测输出，验证反馈中的36条预测/31条完整标签/32条首尾抽样及原产物身份。
然后启动新App Server进程恢复同一Thread，实际收到首轮上下文和科学反馈；总共
两次受控Provider请求、两份真实原生Turn结算，重投无第三次调用且Mission未提前
终结。原生进程/Thread恢复是真实执行；编译/预测/Provider内容是明确fixture，
不是付费模型推理、rustc/Wasmi数值执行或市场/Alpha资格证据。
两次验证均源码不变、独立PG确认停止；之后只追加本证据。研究结论、完整科学
评估、Reviewer、资格及其余产品流程仍未完成，尚未push/review/merge或关闭Issue。

## 2026-09-12：原生公开回答与中断后的摘要恢复

在b3f07d81上接入锁定原生协议的summary视图，只读取精确Turn最后的公开回答，
不读取full/items、rollout或隐藏推理。公开text/phase/原生item身份在完整成功终态
及usage结算后作为不可变qz.mission_summary保存，沿用原Mission输出预算；重放
比较原字节，发布失败保留已结算用量。真实原生测试验证重启及第二轮后的分页
仍返回原回答，且没有额外模型请求；PG用例验证来源、并发重放、回滚和旧owner。

首轮完整验证`.ai-bridge/verify-RLFnHY` exit1：Store/Server496通过、1失败。
失败发生在daemon立即重启原Mission时，PROFILE_CONNECTION/Unavailable；原
cgroup.kill已发送，但systemd尚未回收同名scope。修复位于共享Mission启动入口：
只查询精确scope的原生LoadState，最多等3秒且不超过剩余墙钟，不杀活跃owner、
不换名字重开任务。`.ai-bridge/verify-Ny0zbH` exit0验证实际scope活跃时拒绝、
退出后复用，以及强制摘要发布失败后的真实Worker恢复：仍只有1次Provider请求。

最终`.ai-bridge/verify-187ozo` exit0：check/fmt/严格Clippy，149项领域/合同/
Runtime、6项真实Job子进程、30项原生Codex和498项Store/Server检查，全部0失败/
0忽略。验证期间源码未变，独立PG确认停止；随后仅追加本证据。受控Provider/
科学fixture不是付费账号、科学数值或完整T08/T42证明；Mission收束、正式评估、
独立Reviewer及后续产品仍未完成，不能据此push review、合并或关闭#62。

## 2026-09-12：有界Mission执行收束与终态后ACK恢复

在23ae6736上复用原Run终态事务/Attempt结果引用/PGMQ完成会话收束。只有最新
成功公开回答、全部Turn终态和完整用量，以及已完成并回送原Thread的全部关联
科学任务齐全时才提交；未处理提案、未知任务、缺用量/回答均不能凭空结束。
最后的原摘要是结果引用，不另建完成表/包装报告。原取消CAS保持优先；会话
SUCCEEDED不修改Cycle、Experiment outcome、Evaluation或Qualification。

`.ai-bridge/verify-ZBGCKi` exit0：check/fmt/严格Clippy及109项真实PG回归，0失败/
0忽略。新增/扩展用例覆盖缺失终态/用量/摘要、旧owner、终态写入失败整事务
回滚、并发一次采纳、原摘要关联、先终态后归档、重复ACK，以及先取消、缺用量
仍未决、完整成功回答到达后保留CANCELLED且不生成资格。

`.ai-bridge/verify-Kt3j7G` exit0：check/fmt/严格Clippy、真实scope复用检查及实际
App Server/Worker恢复用例。注入摘要发布失败后重启，再注入PGMQ归档失败；
终态已经提交、消息仍待ACK，后续Worker只归档，总Provider请求仍为1。
`.ai-bridge/verify-Wlzimi` exit0：check/fmt/严格Clippy及实际两轮研究用例。原
编译/预测在途时不收束，原预测报告回到同Thread且保存第二个回答后，唯一
Mission终态与PGMQ归档成立，2次Provider调用不变。

三次验证期间源码不变、独立PG均确认停止。它们是本地定向增量证据，不是新Head
全量CI或完整T08/T42。受控科学输出/Provider不冒充真实数值或付费账号；没有
完整用量的失败/取消、预检/详细编译诊断、正式评估、Reviewer、Cycle结论及
其余产品工作仍须继续。尚未push/review/merge或关闭Issue。

## 2026-09-12：原预测的未授资格Alpha版本及初次实验裁决

在b83fe34e上复用已有Alpha/Version表和command_receipts，从精确成功Discovery
预测创建一次RESEARCH版本。版本保存原MODEL/CODE/根血缘/镜像及冻结Brief的
单位和horizon；不填校准、不改变PENDING、不授资格。Worker自动登记后才能继续
回送与收束；原生两轮回归确认此元数据步骤不增加Provider请求。

迁移033修正先建立评估主体便永远冻结PENDING的顺序冲突：被消费实验的输入仍
不可变；初次结果仅能与精确Alpha/冻结政策的新Evaluation在同事务发布。旧评估
已封口不能补写裁决；结果必须匹配真实评估状态，已发布裁决不能再改。关系测试
中的受控PASS只验证约束，不是数值评估或资格证明。

首次`.ai-bridge/verify-02OdRu`编译因Option::map调用笔误失败，修复后全量
`.ai-bridge/verify-DHZfNf`功能测试全部通过：149领域/合同/Runtime、6真实Job、
30原生Codex、501 Store/Server，0失败/0忽略；但Clippy拒绝新增测试的复杂元组，
因此该轮整体exit1。随后仅将该断言改为sqlx::Row命名列读取，不改变产品源码；
`.ai-bridge/verify-BcfPh7` exit0通过check/fmt/严格Clippy及110项相关真实PG回归。
两轮验证期间源码未变、独立PG均确认停止；新初次裁决关系用例包含于前一轮全量。
以上不是远端最新Head CI，也不代替正式评估、独立Reviewer、全部W0–W8/T01–T42。
未push、请求review、合并或关闭Issue，继续完成剩余开发。

## 2026-09-12：独立原生Alpha分折、训练校准及逐折指标

在8281f731上实现本地`job validate-alpha`，不接管PG或资格权限。复用原目录读取
与同一EMA特征迭代器、solow-cv0.7.3、Wasmi2.0.0、linregress0.5.4及已安装的
ndarray-stats0.7.0；没有新增依赖或自写统计估计器。逐资产/折/训练/测试/不连续
块重建模型，累计fuel不重置；只将训练标签给OLS，全部测试标签在模型执行后
独立形成。保留全部折、原ordinal、预测、校准状态、原生IC/RMSE及缺失原因。
跨折重复观察仅去重计数，不冒充统计独立样本；无全局平均、PBO或自动PASS。

真实合成Parquet+Wasmi+原生统计回归覆盖原训练范围/OLS系数、测试收益、逐折
IC及独立RMSE参考、CPCV全部组合与不连续块的状态重置、全任务fuel耗尽、不一致
horizon、样本不足、常数校准、原生溢出及实际子进程严格JSON/安全失败输出。
原forecast特征重构的因果预测/未来扩展不变及跨资产状态回归保持通过。

原生Rust合同以相同命令两次独立导出：`.ai-bridge/web-verify-4lk6qB` domain-only
exit0，输出逐字节一致，手写源码不变；仅更新domain-v1.openapi.json，不宣称
已接通HTTP/Gateway合同。最终`.ai-bridge/verify-jgaEza` science exit0：全工作区
check/fmt/严格Clippy、149领域/合同/Runtime、30原生Codex及72项Job测试全通过，
0失败/0忽略（72包含另行通过的6项managed测试）。验证期间源码未变。
这是本地原生计算与合同证据，不是新的远端CI、真实市场有效性或完整T08。
受管ValidateAlpha协议、原Run/政策采纳、正式Evaluation、sealed独立评估、Reviewer
及其余全部开发仍须继续；没有push、请求review、合并或关闭Issue。

## 2026-09-12：受管Alpha分折与原请求逐折采纳

在bad625f2上将同一原生计算接入`VALIDATE_ALPHA`/ALPHA_EVALUATE，输入仅原
VALIDATION目录、MODEL和PARAMETERS，输出封口为qz.alpha_validation.v1。
分折适配移至domain::execution::validation，Job与结果采纳复用同一锁定solow-cv；
没有复制切分算法、新增队列或依赖包。逐资产source_row_count与原warmup/horizon
用于重建全部原生train/test索引；漏折、替换索引、顺序/原来源/重复标签冲突、
超fuel、越过cutoff、校准/指标状态不一致均拒绝。保持原生缺失/非有限原因，不填0。
Runtime能力从既有输出登记表派生；镜像标签增加实际数值版本与alpha-validation/1，
旧镜像须重建登记新OCI ID，不能借旧native-stack声明新能力。

真实Parquet/Wasmi/CV/OLS回归覆盖WalkForward、全部CPCV组合、重复源标签一致性、
常数/溢出及17种报告篡改；真实managed子进程拒绝DISCOVERY/SEALED训练，沿用原
输出限额和封口。SQLite目录范围回归覆盖四种数据操作的注册类型、资产、事件及
可见截止；这些不是新镜像的实际OCI执行证据。

`.ai-bridge/web-verify-EzTVis` exit0：三份Rust合同各两次导出、客户端两次生成
逐字节一致，手写源码不变；504前端单元、168浏览器回归、wire、类型及构建通过。
`.ai-bridge/verify-3ljWRQ`全量轮的501 Store/Server、7managed、30原生Codex通过，
但Runtime旧测试仍断言5种JSON报告，实际新增为6种，因此整体exit1。随后仅修改
该测试为精确类型清单及镜像标签一致性检查，不改产品/生成物。
最终`.ai-bridge/verify-gPLRYG` science exit0：check/fmt/严格Clippy、149领域/合同/
Runtime、30原生Codex和74项Job测试通过（含另行运行的7managed），0失败/0忽略。
两轮源码均冻结；全量轮独立PG已确认停止。未重跑新镜像OCI，未称为完整T08或
远端CI。正式评估准入/发布、政策/资格、Reviewer及其余工作仍继续；未push/review/merge。

## 2026-09-12：原生逐折指标到既有政策检查

在af6acfbc上新增可信`alpha_validation_metrics`转换：先重验原请求全部分折，
再原样保留每个原生值/状态/原因，关联原evaluation/report身份、资产/折scope、
原生方法/版本、单位、完整bar规格与固定horizon。纳秒原报告不变，MetricValue的
微秒时间边界只向外取整；IC和RMSE分别计实际配对数，缺校准不是零收益或有效
预测。相同可信适配提供能力记录给既有精确Decimal阈值函数，没有另写估计器、
全局平均或资格机制；尚未写入数据库Evaluation。

真实Parquet/Wasmi/CV/OLS用例逐项核对全部转换记录、原数值、上下时间边界、
方法/单位/周期、样本数及引用；错误单位为UNSUPPORTED，缺校准为INCONCLUSIVE，
部分报告拒绝。纯数值阈值的PASS断言不是FIXTURE市场资格或完整评估。
`.ai-bridge/verify-XRwigA`的149领域/合同/Runtime、30原生Codex和75Job全部通过
（75含单独通过的7managed），但Clippy拒绝测试的两处chunks_exact(2)，整体exit1。
仅改为原生as_chunks::<2>()后，`.ai-bridge/verify-HTTaGY` compile exit0通过全工作区
check/fmt/严格Clippy；最终原生Alpha验证7项再次通过，0失败/0忽略。验证期间源码
均冻结，无DTO/生成器变更。继续修正首阶段试验计数并接正式评估，未push/review/merge。

## 2026-09-12：首阶段只计一次试验

按B1将原编译阶段改为预留/结算一次试验，预测延续阶段为零；同步纠正A3.5–A3.7
之前相反的描述。复用同一事务准入与结算，不增加公共免计费参数；普通AlphaEvaluate
零试验仍拒绝。预测必须关联原已计一次的编译准入。迁移034只约束新增阶段关联，
不重写、退款或清零历史试验；原命令重放只读回同一Run。

真实PG回归确认并发编译只预留一次、失败编译仍计一次、重放不重复计费、发布回滚
不漏预留，后续预测不再计试验但继续计CPU。`.ai-bridge/verify-hKBrqV` mission-store
exit0：118项相关PG回归、check/fmt/严格Clippy通过；`.ai-bridge/verify-Avh8yE`
mission-science exit0：真实原生Mission两轮会话及编译/预测/Alpha登记链路通过，同样
通过check/fmt/严格Clippy。两轮源码冻结、独立PG均确认停止；未重跑全量501后台
测试，未宣称远端CI或完整T08。正式验证与发布等全部剩余开发继续，未push/review/merge。

## 2026-09-12：冻结与启动核验原生验证方法

复用原生指标方法登记和分折参数边界，在Brief冻结、Cycle启动时读取原已登记
Validation元数据，检查Selection的准确方法/版本/单位/周期及资产/折scope，逐项
核对required指标。原生CPCV组合数超过256时直接拒绝，不生成部分折。当前受管
合同仅一个Validation目录版本（可多资产），不默选多个版本中的首项、不将Sealed
选择意图改为普通验证。登记政策不是可执行证明；历史意图和证据不被重写。

真实PG测试核对只读原Validation元数据、缺对象或损坏字节不留下冻结上下文/回执，
原成功回执重放不再读对象。真实原生Alpha用例覆盖12种错方法/版本/单位/周期/scope/
选择类别、required未实现指标与CPCV组合上限；optional缺失不冒充required通过。
`.ai-bridge/verify-IGh6He` full exit0：502 Store/Server、149领域/合同/Runtime、
30原生Codex、7managed通过，零失败/零忽略，独立PG确认停止；随后
`.ai-bridge/verify-BNLrpn` science exit0再次通过149领域/合同/Runtime、30原生Codex
和75Job（含另行通过的7managed）。两轮check/fmt/严格Clippy均通过，源码冻结。
无新依赖、DTO或生成物变化；没有构建/运行新OCI镜像，正式Evaluation与后续交付仍
须继续实现，以上不是最新远端Head CI或完整T08/T42。未push/review/merge或关闭Issue。

## 2026-09-12：原试验的正式Validation准入

内部Store新增正式验证准入，与Discovery预测共享原MODEL/提案参数、预算和原生
发表事务；只换为冻结Validation输入、split/target/政策。迁移035原子绑定原试验/
Alpha/政策/目录/Run，参数与报告为EVALUATOR_ONLY；不改变原Discovery Run、不计
第二次试验，不授予Evaluation或资格。两种Alpha阶段均拒绝冻结执行镜像漂移，验证
同时重查真实方法能力。编译及Alpha阶段按缩短后的剩余墙钟重算CPU，容量不足拒绝。

真实PG回归验证缺原Alpha、镜像/方法变化、发表失败回滚、并发唯一Run/CPU预留、
原试验计数、准确政策/模型/目录/参数、原生JobSpec及不可变关联。CPU最窄单元测试
通过。`.ai-bridge/verify-ssV6Mq` mission-store exit0：119项相关PG回归零失败/忽略；
`.ai-bridge/verify-NY1BQ2` mission-science exit0：原生App Server会话与原提案/科学
阶段准入及反馈回归通过。两轮check/fmt/严格Clippy通过，源码冻结，独立PG确认停止。

证据范围澄清：此处及前述mission-science用例真实运行Codex App Server，但其
Responses Provider与编译/预测终态是受控fixture；它不实际执行科学Job。真实Job/
分折计算证据来自独立75项Job及managed测试，不能拼称为完整端到端链路。新原生
镜像/OCI链路尚未执行。本次也未将Validation接入自动Mission调度、Evaluation发布
或受控正式反馈；这些与其余全部合同继续实现，无新增公开DTO或依赖，未push/review/merge。

## 2026-09-12：真实OCI镜像装配与编译验收

首次真实构建在`job --version`失败：usr-merged宿主的ldd把ELF所需`/lib64`
加载器解析到`/usr/lib64`，旧装配漏掉绝对请求路径。复用原文件复制逻辑保留两者。
第二轮镜像可启动，但7项OCI仅4通过；原宿主私有工具链的700/600权限被原样带入，
固定容器用户不能执行rustc或读取库。仅在新镜像根中规范选定公开文件的读/执行
权限，不改变宿主文件、Docker权限或生产状态；构建新增真实rustc版本启动检查。

最终外部回执`owner-oci-40q4sS`（会话25182）exit0：基于989401ea及上述冻结补丁，
真实构建job/runtime、装配、Docker构建、job/rustc启动和全部7项native-oci通过，
零失败/零忽略，验证前后源码快照一致。Docker原生测试镜像ID为
`sha256:43c8a3369210baaf74006b6eee03caab9e6feedec2fdbb4791d4df051d739b7e`。
检查覆盖实际Rust→Wasm、并发重放唯一容器、完成任务崩溃恢复、Gateway退出后的
原生墙钟截止、取消晚到屏障及现有内核边界回归；5项原生文件复制测试也通过。
此前两轮失败均保留外部回执，不能归为成功。没有以root运行Cargo、挂载生产数据
或扩大用户Docker权限。此项仍非正式Validation目录OCI链路、完整Mission科学链路
或T01–T42/远端CI通过；所有剩余开发继续，未push/review/merge。

## 2026-09-12：正式Validation评估与原队列收尾

原生Worker统一在首次终态和终态重投的ACK前发表评估；复用原Run锁、冻结政策和
已采纳参数/manifest/全部分折报告。Evaluation、所有逐折指标、受限完成报告和实验
首次裁决同事务封口，不重跑模型、不依赖Mission在线、不新增队列。有效期来自原生
完成时间；来源/PIT、实际测试样本、登记行缺失和精确阈值仍独立判定。失败/取消
保留INCONCLUSIVE及空指标，不制造原生方法记录；无资格或Reviewer批准。

基于92861a及冻结补丁，`verify-mVUVWY`完整回归508 Store/Server通过、1失败：
新唯一索引错误限制了非正式验证的既有评估场景。修正为只锁定原
experiment_validations关联的数据库防重复检查，保留原失败测试预期；增加正式Run
直接SQL重复发表拒绝。随后`verify-R6K6Y6`相关137项PG回归全部通过、零忽略，
check/fmt/严格Clippy通过，源码冻结，独立PG确认停止。新增五项PG用例覆盖原子回滚、
文件/指标失败、并发一次发布、生产者/全部指标、ACK、重放、精确5%缺失边界、
失败阈值、未派发取消和拒绝manifest。科学字节是显式受控fixture，不是实际Job执行。
此前完整回归的149领域/合同/Runtime、7managed、30原生Codex均通过，但该完整
回执整体仍为失败；局部复测不改称全绿。没有DTO/依赖/生成物变更。
自动Mission验证、正式反馈与全部后续合同继续开发；未push/review/merge或关闭Issue。

## 2026-09-12：自动正式验证及原Thread评估反馈

Mission在原Discovery成功后依次登记RESEARCH Alpha、准备正式Validation；复用原
阶段准入与PGMQ，不为中间观察抽样额外创建模型Turn。完整评估发表后，仅从数据库
投影原Evaluation和按冻结口径选定的MetricValue；不读取EVALUATOR_ONLY报告字节。
反馈保留来源、期限、原生产者及缺值，不是跨试验排名、校准、资格或Reviewer批准。
Mission正常收束新增正式验证/评估/反馈回答屏障；公开实验元数据只披露精确原验证
关联的首次裁决，原报告GET仍受限。没有新队列、依赖或公开DTO/生成物变化。

基于5525ec72及冻结补丁，`verify-QEYJux`相关137项真实PG回归通过、零忽略。
`verify-TdMh6y`原生扩展用例在发布故障恢复与ACK之后失败：测试错误地重新领取已
归档消息；原生PGMQ不再返回它，幂等入口是acknowledge_run。仅修正测试，未添加
消费兼容路径。最终`verify-F4Oqew`原生用例通过（218.27秒），check/fmt/严格Clippy
通过，源码冻结，独立PG确认停止。实际App Server验证过期探测刷新、原编译/预测/
验证准入、Worker发布故障/终态恢复/ACK、无报告字节读取的反馈、原Thread保留先前
上下文的第二轮、两个完整用量/公开回答及最终Mission归档；一项试验、一个正式
评估、零资格。Responses Provider与科学报告仍为受控fixture，不是实际Job/OCI
全链路或真实账号T42；不能与独立数值测试拼称端到端完成。
取消/期限/未知用量恢复、独立Reviewer、完整试验选择、Sealed/校准/资格、Alpha与
组合交付操作面、部署/迁移/恢复及其余验收仍待完成；未push/review/merge或关闭Issue。

## 2026-09-12：取消与到期后的已对账 Mission 收束

可信Worker在新连接、科学步骤或结果反馈之前复用原Run收束入口。取消/真实到期
不再要求尚未准入的后续科学阶段或新模型总结；已准入编译/预测/验证仍须有原
Attempt（含未派发null）的真实终态。已开始Validation的评估发布消息独立保留，
Mission取消不自动取消别的Run，不抹除原试验，也不冒充评估已发表。
无发送意图的原预约通过既有用量结算，在同一个Run事务记录NOT_SENT和零用量；
任何发送意图或缺失最终用量仍保留预约。取消可没有Thread/摘要，没有产物就不写
accepted_at；正常成功仍要求公开回答与全部科学/反馈证据。不增加取消队列或账本。

在34a961e9及冻结补丁上，`verify-gfQI0z`首轮137项通过、6个新增场景失败，均因
无摘要却写accepted_at，被accepted_manifest_present约束拒绝。沿用原生终态写法
修正该一处写入，未修改数据库约束。`verify-ya6yVQ`全部143项真实PG回归通过、
零忽略，覆盖原事务回滚/并发重放、零轮次、未知发送/晚到用量、真实期限、编译
结束但不继续预测、已排队验证及后续评估发布。check/fmt/严格Clippy均通过。
`verify-ni8gQZ`四个原生App Server场景通过：两个新增Worker取消场景（63.62秒），
原未知发送ACK场景（52.32秒）和原生期限中断/不得补造usage场景（74.16秒）。
两次最终验证源码不变，独立PostgreSQL均确认停止。原生模型响应为受控Provider；
科学任务内容为有明确标识的fixture，不声称实际Job/OCI完整链路、真实账号T42或
GitHub当前Head CI。未知原生用量仍可能保持待对账；完整选择/Sealed/校准/Reviewer/
资格、Alpha与组合交付操作面、部署/迁移/恢复及其余验收仍须继续，未请求review或合并。

## 2026-09-12：原 Alpha 与正式 Validation 只读操作面

按 DESIGN A3.12 复用原表、授权、UUID游标和MetricValue合同，实现6个HTTP GET、
对应原生CLI命令与Ant Design Alpha页面。仅Operator或精确项目RESEARCH_READ的
CLI可读取完整操作视图；Mission保留原冻结选择口径反馈，不扩大权限。版本按原
Alpha和正十进制版本号读取，不替换活动版本；来源取原Discovery数据任务，不把
生成CODE的SYNTHETIC当市场来源。只有原正式Validation/政策/输入/终态Attempt/
生产者/发布标记全部匹配的评估可见；Sealed和非正式旧记录不可见。指标分页不补
零、不重算，保留方法/单位/期间/计数/来源；原有效期与数据库读取时间分开显示。
这些读取不产生计算、暴露预约、资格或交付，不读取受限报告字节。

基于b85e06d9与冻结补丁，前三次编译检查发现新增CLI名称冲突和测试的原生String/
UTC类型构造错误，修正调用而未加兼容层。最终`verify-oLlzjL`check/fmt/严格Clippy
均为0；40项Store及15项HTTP/CLI回归全通过，0忽略，独立PG确认停止，源码不变。
其中实际原生CLI经过TCP/Axum/Bearer/PG，HTTP核对原版本/分页与大计数；科学结果
仍是明确的受控fixture，不能替代完整Job/OCI或真实研究链路。

`web-verify-QI0NUw`的171项浏览器检查通过、3项因测试使用不存在的Drawer CSS类
失败；改用实际dialog语义。`web-verify-rWkyH6`的3项失败随后定位到指标横向表格
没有键盘焦点；核查所有Table，同类撤销历史表也修复。复用原生onHeaderRow和
tabIndex，使空表也可聚焦，不加滚动脚本、不关闭axe规则、不强制点击。
最终`web-verify-eSjgEy`通过：三份Rust导出与TypeScript/Ajv生成重复字节一致，
手写源不变；typecheck、504项Vitest、5项Node检查、decimal/bigint/fraction线协议、
build和CLI help通过；36项Codex设置浏览器检查及全站177项三视口检查通过。
新回归实际验证Tab/方向键滚动、空指标、0与null区别、精确计数、原来源、分页、
切换项目清理旧详情及读取故障不当空结果。旧HTTP路径和已有schema语义未改变。

此阶段没有新依赖、迁移或资格入口。完整选择/校准/Sealed/独立Reviewer/资格、
双Alpha组合交付、部署/迁移/恢复及T01–T42剩余证据仍须继续；未push、请求review、
合并或关闭Issue。GitHub只读复核PR63仍为Draft、远端37e5713e，Issue62仍开放。

## 2026-09-12：原 Mission 确认前的冻结试验选择

按DESIGN A4.9复用原acknowledge_run事务、project→cycle→run锁与PGMQ archive，
只为原RESEARCHER Mission形成一次选择。取消亦等待已准入原生任务终态及正式
Validation发表。cycle_selections以原Cycle为身份及成员封口；成员先写、deferred FK
指向最后写入的头，头和成员不可改删，形成后不能追加试验；原已授权命令可重放。
未新增Run、队列、模型轮次、依赖或应用级hash身份。失败保留原队列，原终态不撤销。

原生SQL包含同项目/Family/根血缘全部登记历史，每个实验一次；冻结原Run/Alpha/
Evaluation/Metric引用、当时执行状态和排除理由。历史未绑定正式流水线的Run仍保留
原引用，不冒充未执行或正式证据；其他Cycle后来完成或追加试验不回写旧快照。
按原冻结比较输入/执行假设和完整正式发表关联筛选，不能误用含独立MODEL的派生
InputSet UUID。原required指标完整、VALID且精确方法/单位/频率的finite值由PG排序；
MAX/MIN及正负零平手均按原UUID次序。科学REJECT可参与同口径比较，不变成批准。
候选不足/未完成可比试验为INCONCLUSIVE，COMPLETE只表示比较集合数量满足。

两个只读GET、原生cycle selection/trials CLI和Cycle的Ant Design Drawer展示原
规则/计数/成员/排名/缺值/来源，分页按原实验UUID；不存在形成前的空成功快照、
客户端选赢家或重新排名入口。复用原Operator/精确项目CLI权限，Mission不得借用
完整操作视图。UI保留0/null、大计数和原Run，原生表头焦点/方向键支持窄屏横向查看。

以0eeef659及冻结补丁验证。首轮verify-iAlLuL的Rust check/fmt/严格Clippy通过，
新迁移的PL/pgSQL IF内CASE缺括号导致依赖迁移的测试失败，未当作业务通过；修正后
verify-K6ilpf全部通过：119项真实Store测试、19项HTTP/原生CLI测试，0忽略，原
PGMQ回滚/并发/重放、正负零/方向、历史原Run和原生取消评估均核验。源码不变、
临时PG停止。HTTP验证原Mission空取消快照、形成前404、严格分页及原回执。

web-verify-GRep8i完成六个原生生成物的重复字节核验，但新Drawer一处JSX闭合错误
使typecheck/build失败，浏览器未启动；修复后web-verify-It81tU全部通过：六个
原生生成物再次重复一致、手写源不变；typecheck、504项Vitest、5项Node、decimal/
bigint/fraction线协议、build及CLI help通过，36项Codex设置与全站183项三视口
浏览器检查通过（2.6分钟）。API语义只增加两个GET和五个schema，唯一已有schema
变化为CycleReadAction新增VIEW_SELECTION；无已有HTTP路径改动或删除。

随后仅增加常驻Worker断言，verify-sJFjr0捕获测试的UUID/领域Id类型混用，修正后
verify-uM2UGy check/fmt/严格Clippy及三项原生用例通过：真实Codex 0.144.4/App
Server原Thread反馈用例258.88秒，两个取消用例66.28秒。快照在测试手工ACK重放前
已由Worker自动形成，精确引用原试验/评估/指标，仍只有两次原Provider模型请求；
无Turn/未发送Turn取消保持零Provider请求，未为快照补开Thread。原消息已归档，
临时PG确认停止，源码不变。受控Provider/科学报告仍不是完整真实数值Job/OCI或真实账号T42。

此阶段未push、请求GitHub review、合并或关闭Issue。最终校准/Sealed/独立Reviewer/
资格、双Alpha共享资金组合交付、迁移/部署/恢复及T01–T42其余证据必须继续完成。

## 2026-09-12：冻结原生 SCORE 校准与原 Evaluation 原子发布

按DESIGN A4.4复用已经实际拟合的linregress0.5.4模型，不增加科学Run、队列、
模型轮次或依赖。原生每折输出训练标签的真实available纳秒；固定选择各资产最后
原生折，不以指标选赢家、不混入测试/Sealed标签再拟合。全部最后折均有可用SCORE
拟合才冻结；一项失败不回退早折，EXPECTED_RETURN不伪造校准。JSON模型只保留
原生系数、精确训练子集、资产/bar规格/horizon和原报告引用，不复制训练标签。
持久模型使用已锁定ndarray乘加应用原生仿射参数，并与真实RegressionModel.predict
实际比对；新预测须晚于整个模型训练截止。DB微秒向上取整，JSON保留原纳秒。

原Validation的SUCCEEDED + VALID发布事务先写全部指标及实验首次结论，再创建
EVALUATOR_ONLY校准MODEL和calibrations引用，由已有引用触发器封口Evaluation。
038迁移检查原Run/Attempt、数据来源、训练InputSet/cutoff、horizon及原生估计版本，
不回填历史已封口评估。失败回滚两份新产物的元数据和实验结论；Worker复用原Run锁
确认无引用后分别回收对象。重放不读取或再写模型，不加trial。科学REJECT保持REJECT，
原AlphaVersion仍不可变且未附加校准，本阶段不生成资格或Sealed读取能力。

验证使用e7699eca及冻结补丁，编辑/验证串行：

- verify-iWmise编译失败：ndarray0.17只读视图需要借用后做乘法；未启动PG。
- verify-tjWS5l原生28项通过，但Store六项捕获校准引用过早封口Evaluation，
  违反原实验首次结论守卫。只调整同事务写入顺序，不放宽不可变约束。
- verify-Wx6Ch2全通过；复核后将训练时间的DB边界由执行墙钟改为原冻结输入的
  decision_cutoff，并增加超界一微秒的触发器注入/完整回滚检查。
- 最终verify-LO0XsC：check、fmt、严格Clippy通过；原生28项
  （alpha_validation 9、validation 9、managed 7、science_cli 3）、Store119项、
  HTTP/CLI19项通过，0失败/忽略。包括第二文件失败、校准行失败、输入超界、并发
  唯一发布/重放、真实纳秒保留、不可改删及缺证据不产生模型。源码不变，PG确认停止。
- web-verify-WTmW94：六个允许的原生生成输出两次逐字节一致，手写源不变；
  typecheck、504项Vitest、5项Node、decimal/bigint/fraction、build及CLI help通过；
  Codex设置36项和全站183项三视口浏览器测试通过（2.7分钟）。语义差异仅domain
  新增两个冻结校准schema，domain/runtime的NativeValidationFoldV1新增训练截止；
  所有HTTP路径和server API schema不变，六个输出中仅两份JSON实际改变。
- 随后只改原Worker恢复测试，verify-cmw5AQ check/fmt/严格Clippy通过；真实Codex
  0.144.4/App Server原Thread用例221.34秒通过，两个取消用例66.92秒通过。校准行
  失败后对象目录精确恢复原集合，成功恢复在手工ACK前已有唯一校准，仍只有两次
  Provider请求；受限模型未进入反馈正文。源码不变，PG确认停止。
- owner-oci-QnSnJk重建原生Job/Runtime和镜像
  `sha256:60489424d37c17de87ddbaf2720037331cff2f099e53dc325c589f7e2d04277e`，
  实际job2.0.0-dev.1/rustc1.98.1，七项真实OCI编译/原任务恢复/取消/既有边界测试
  全通过（11.92秒），源码不变。没有替换生产服务或修改Docker socket权限。

原生OLS/Parquet/Wasmi数值测试使用明确合成行情；Store与App Server用例使用受控
科学报告/Provider，OCI用例不是完整市场Validation或Sealed测试，不能拼成真实账号
T42已过的声明。本阶段未push、请求review、合并或关闭Issue；须继续不可变校准
版本附加、Sealed预约/执行/披露、独立Reviewer/资格、组合交付及全部剩余合同。

## 2026-09-12：不可变校准版本与只读来源

按DESIGN A4.4在原Validation发布事务创建同Alpha的下一版本，附加原冻结校准；
039迁移约束精确源版本/原实验/血缘/CODE/Wasm MODEL/单位/horizon/镜像，并阻止
重复附加。原版本与评估不改写；新版本没有借来的Validation或Qualification。
活动指针只在RESEARCH且仍指原版本时推进，保留清空指针、SUSPENDED和RETIRED。
未产生可用校准不创建派生版本，科学REJECT仍为REJECT；原候选快照继续引用源版本。
复用原事务和队列，不增加拟合、Run、依赖或候选映射表。

一个只读GET及alpha calibration CLI、Ant Design来源Drawer返回校准元数据和源
版本原Validation，不读取模型/标签/索引字节。沿用Operator/精确项目CLI权限；
Mission不能借用。保留原决定/有效期、精确大整数和向上取整微秒时间；读取失败
不是空成功，源评估不是新版本评估。原生生成仅增加CalibrationView和一个GET，
已有API/schema和Runtime合同语义均不变。

以c69a085e及冻结补丁验证，编辑/验证串行：

- 首轮verify-vNqpB6有一项新测试失败：通用fixture签发一小时Mission凭据，超过
  真实Run期限。改用现有正式Mission签发入口，未放宽生产约束。
- 最终verify-4mGTyJ的check/fmt/严格Clippy及28项原生科学、122项真实Store、
  20项HTTP/原生CLI测试全部通过，0忽略，源码不变，临时PG确认停止。覆盖版本
  写入故障与两对象/评估/结论回滚、并发唯一发布、重放、原版本归属、选择不随
  活动指针变化，以及生命周期/人工指针保留。
- web-verify-QTvol7：六个允许原生输出重复字节一致，手写源不变；typecheck、
  504项Vitest、5项Node、204个decimal/1694个bigint/214个fraction用例、build与
  CLI help通过；36项Codex设置和全站186项三视口浏览器测试通过（2.7分钟）。
  实际变化为domain/API JSON、TypeScript及Ajv JavaScript四个生成文件。
- verify-0m2jrz的check/fmt/严格Clippy与真实Codex0.144.4/App Server原Thread
  用例通过（218.81秒）；两个取消用例通过（70.64秒）。故障改注入alpha_versions
  写入，原两对象精确回收；Worker恢复后已产生附加校准版本，反馈仍引用原评估，
  不披露校准模型，仍只有两次Provider请求。源码不变，临时PG确认停止。

原生科学数据与Provider仍为显式受控输入；这些测试不是完整真实账号、市场Job/OCI
或Sealed/T42证据。本阶段未push、请求review、合并或关闭Issue；继续Sealed预约/
实际执行/披露、独立Reviewer/资格、组合交付、迁移/恢复与全部剩余合同。

## 2026-09-14：原生 ForwardEvaluate 可信准入

Store 内部准入复用原 Run/PGMQ、原 Candidate 的 Runtime 与当前原生能力观测，
在 Project 锁内验证原 REAL Claim、当前冻结政策及完整原始反馈来源。068 迁移
冻结 FORWARD 输入及原生参数，约束固定资源上限与唯一 Run；通用研究输入权限
不变。派发前重新核对政策和全部来源，新增/更正反馈使旧未发任务失效。重复
来源只返回原 Run，不重读私有报告或写参数；超大正反馈期限受政策期限约束。

以 7ce2b68a 及冻结补丁串行验证：

- verify-QFFPiA 捕获参数 created_by 使用非法枚举；改用已有政策授权 OPERATOR
  路径。随后修正 app.document 的版本化任务内容。verify-sHbia7 捕获旧原生任务
  守卫未注册 ForwardEvaluate；068 仅增加精确的原生绑定分支，保留其他限制。
- verify-jouJuR 定向通过。verify-xo9m83 的 check、fmt、严格 Clippy 与 143 项
  合同/领域、67 项真实 PostgreSQL/PGMQ、24 项 HTTP/CLI 测试全部通过。
- 最后调整重复来源的元数据快路径和期限溢出处理，verify-WDHUMr 再次通过
  check、fmt、严格 Clippy 及原生准入数据库用例。该用例覆盖并发唯一入队、
  派发前更正/撤权、事务故障完整回滚、未引用对象精确回收、通用输入拒绝、
  i64 最大反馈期限及重放回调禁止执行。两轮验证源码不变，隔离 PG 确认停止。

Claim、Release、Candidate 及 Runtime 基础关联使用明确关系型 fixture；报告
接收和政策授权/撤销使用原生入口。历史日期仅由隔离测试连接的事务局部时钟
提供，未关闭触发器或改变宿主时间。这些测试不证明真实多日市场反馈、完整
原生 Claim、模型或 OCI 生产验收。当前未接 Worker 自动调度和 Evaluation/
forward_evidence_windows 发布；不代表 Live 准入完成。本阶段未 push、请求
review、合并或关闭 Issue。没有合同变化，不重新生成前端产物。

## 2026-09-14：原生 Forward 终态测量发布

按 DESIGN A7.7 接入原 Worker 的 publish_scientific_result（终态采纳后、ACK 前）。
复用 Project/Run 锁、精确原 Attempt/JobSpec/Manifest/输出与原对象发布/清理；
Evaluation 保留原 Mandate 的 EvaluationPolicy 血缘，AutomationPolicy 单独引用。
测量 decision 始终 INCONCLUSIVE；完整原生统计且原来源/授权仍有效才为 VALID。
更正/撤权/过期或缺统计为 INCOMPLETE，失败/取消不补造指标。原冻结期限及
已登记未来撤权限制有效期。先写指标再引用窗口封口，重放不读写对象。

以 09ad79e5 及冻结补丁串行验证：

- verify-A7kp2A 编译/Clippy/准入测试通过，格式检查失败（本机 rustup 不在 PATH）；
  使用已安装工具链绝对路径格式化。verify-Sbwuxt 定向通过。
- verify-LGfT36 捕获新独立测量流使旧全项目计数断言不成立；改为精确原 daily
  流计数，保留原未发送任务被纠正阻断的测试。verify-1srepE/85zRt8 捕获新增
  未来撤权测试的类型名及必需版本字段遗漏；改用原 PolicyRevokeV1 和原撤权 ID。
- verify-wxN23F 的全部编译门禁、143 项合同/领域及 67 项 PG/PGMQ 测试通过；
  HTTP/CLI 23 项通过、原交付链 1 项在并发刷新租约断言失败（两个 Some）。
  根因为 LEFT JOIN 候选旧快照与无条件冲突更新允许覆盖新租约。原刷新 upsert
  增加已有 lease_until/next_attempt_at 均到期的原子条件，失败返回无预约。
- 最终 verify-wfrA1W：check、fmt、严格 Clippy、143 项合同/领域、67 项真实
  PG/PGMQ、24 项 HTTP/CLI 全部通过，0 忽略；包含原刷新租约竞争和完整原交付
  HTTP/CLI 回归（18 项组合测试耗时 207.47 秒）。源码保持不变，临时 PG 停止。

Forward 扩展用例经原生准入、首次发送、接受回执、受控 Manifest/输出采纳到
正式测量/指标/窗口发布；覆盖窗口写入故障整体回滚及未引用对象回收、并发
唯一发布、无 IO 重放、有效期被未来撤权截短、不可改删、取消终态零指标/零
有效观察。统计值和基础 Claim/Candidate/Runtime 仍是明确 fixture，不证明
真实多日市场、完整原生 Claim 或 OCI 科学运行；前阶段原生数值测试不能拼成
完整生产验收。本次无对外合同变化，不重新生成前端。尚待自动调度、Live/Wake
消费与全部剩余合同；未 push、请求 review、合并或关闭 Issue。

## 2026-09-14：原 Worker 自动反馈调度与恢复

按 DESIGN A7.8 复用五秒项目轮询：Paper 尝试之后独立处理至多一个原反馈流，
仍由 A7.6 原生准入决定是否入队。069 迁移只增加每 Handoff/stream 的可变重试
预约，未尝试/最早尝试优先，失败三十秒后重试，新消息/纠正可提前；相同已冻结
来源不重新调度。来源计数仅为不可变消息的重试提示，不代替每个原 ID 的准入
核对，也不创建证据、试验或第二个任务队列。参数失败仍由原对象清理回收。

为复用现有明确关系型历史输入，将原 Store 测试初始化移至 tests/support/forward.rs；
新增真实 Worker/ArtifactStore/PGMQ 集成用例，未复制产品业务。原生队列读取和
直接领取均注册带原生绑定的 FORWARD_EVALUATE，Mission 驱动拒绝领取。

以 e370aab3 及冻结补丁串行验证：

- verify-Rv5aen 首轮编译/格式/Clippy/原 Store 用例通过。加入实际 Worker 用例后，
  verify-nHX1Nj 捕获测试多余导入、未处理返回值和未初始化临时 Vault；
  verify-fT2YXD 捕获临时 Vault 目录遗漏，按原生密钥/0700目录初始化修正。
- verify-Ppm07l 的实际 Worker 用例发现原生队列 kind 列表未注册 Forward，
  已同时修复读取和领取入口。verify-sn4EOi 已完成首次发布/归档，但测试错误地
  再领取已归档消息；改为原 ACK 归档事务故障后重试仍在队列的原消息，未放宽
  队列身份约束。
- verify-M1prpi 定向全部通过：Store 预约竞争、缺数据流不阻塞下一流、纠正
  提前重试、撤权停止选择，以及实际 Worker 并发唯一入队、原参数文件、驱动
  分类、取消测量发布、ACK 失败后评估保留/队列保留、恢复后唯一归档。
- 最终 verify-LI6TC7 的 check、fmt、严格 Clippy、143 项合同/领域、67 项
  PG/PGMQ、25 项 HTTP/CLI/Worker 全通过，0 忽略，源码不变，隔离 PG 停止。
  其中原交付链 18 项通过，耗时 199.35 秒。没有对外合同变化，无需重生前端。

Worker 测试使用真实对象存储与原生队列/终态发布；基础历史 Claim/Candidate/
Runtime 仍是显式关系型 fixture，取消路径没有实际 Runtime/OCI 科学执行，
不是多日市场或完整生产验收。Live 晋级、劣化 Observation/Wake/Cycle 消费和
其他剩余合同继续实现。本阶段未 push、请求 review、合并或关闭 Issue。
GitHub 实时复核：PR63 OPEN/Draft，远端仍为37e5713e；Issue62 OPEN。

## 2026-09-14：原生观察与待处理劣化 Wake

按 DESIGN A7.9 明确两组原冻结指标要求的消费顺序，复用 evaluate_metrics 的
required/方法白名单/观察数/十进制阈值语义。维持边界不通过为 DEGRADED；维持
通过而晋级指标不通过为 WATCH；两组通过为 HEALTHY；测量不再当前、缺失或
不支持为 INSUFFICIENT_DATA。比较使用同一固定原生方法注册，不从提交的指标
反推“支持能力”，没有增加科学计算。Evaluation 的 INCONCLUSIVE 决定不改写。

Worker 在测量已提交后、ACK 前调用 Store::observe_forward。Project/Run 锁下
重验原 native Evaluation/window/终态/原消息及政策，追加原 Observation。070
迁移的 forward_observation_publications 按原 Run 唯一保存原生来源回执，并验证
原 frozen Forward 输入、精确 Release/Policy/终态 Evaluation；不改旧关系型观察。
只有当前有效 DEGRADED 同事务创建唯一 PENDING/DEGRADATION Wake。观察/回执/
Wake 失败整体回滚，但不撤销既有 Evaluation；原消息留给 Worker 恢复。重放只
返回原观察，未来消费必须再次检验新鲜度/纠正/撤权，不能借旧风险快照自动开工。

以 02127cf8 及冻结补丁串行验证：

- verify-HSU2oR 首轮 check、fmt、严格 Clippy 与原 Store/Worker 定向用例通过。
- verify-kTnFEI 新定向检查全部通过：四种分类、缺失与未知原生版本、受控负均值
  仍为 VALID/INCONCLUSIVE 测量而按维持政策产生 DEGRADED、Wake 插入故障使
  Observation/来源回执回滚且 Evaluation 保留、并发唯一观察/Wake、原生回执不可
  删除、取消/撤权后记录不足且无 Wake。实际 Worker 的 ACK 故障恢复保留唯一
  INSUFFICIENT_DATA 观察，不创建 Wake。
- 最终 verify-288RFP：check、fmt、严格 Clippy、144 项合同/领域、70 项 PG/
  PGMQ、25 项 HTTP/CLI/Worker 共239项全通过，0忽略；包含原三项劣化关联约束。
  源码保持不变，隔离 PG 确认停止。没有对外合同变化，不重新生成前端产物。

统计值仍来自受控原生协议 fixture；测试证明真实 PostgreSQL/Worker 的关联、
分类消费、故障事务与恢复，不证明实际市场/OCI统计或完整生产链。PENDING Wake
不是已启动 Cycle；受限自动 Cycle、冷却/每日预算/原生上下文裁决、Live 晋级
及相应界面与完整验收仍须继续。本阶段未 push、请求 review、合并或关闭 Issue。


### 2026-09-14：原生 Wake → 受限 Cycle

- DESIGN A7.10 明确原人工作业上下文继承：原 Candidate Run/Cycle/startup、当前冻结 Brief、真实 CYCLE_START 回执和两个 Profile 原 ID/版本。没有 Operator 伪装、Agent 入口或新增模型/运行时选择。
- 共用 Store::admit_cycle 原生准备/预算/PGMQ/启动绑定；人工路径保留最终真实授权复核。可信消费在 Project/Wake 锁内复核原观察、Evaluation/window 和完整 Forward 来源/当前政策，发布参数后再次复核；取消失效旧 Wake，保留历史观察。
- 按所有原项目 Cycle 的 UTC 日额度和冻结冷却延后；暂停不启动。每轮公平预约一项三十秒重试，沿用现有 Worker 项目轮询与未引用对象清理；过期政策的待处理 Wake 仍能被轮询清理。新迁移071约束原 Wake/Cycle 双向绑定并冻结终态，原生研究入队与消费同事务。
- 原生 PostgreSQL/PGMQ 检查使用真实人工 Brief 更新/冻结/启动，Forward 科学响应和 Candidate/历史 Claim 仍是明确的关系/协议 fixture。覆盖暂停、冷却、日额度、预约并发、消费并发唯一、最后写失败回滚、重放不读写对象、终态不可重开；无原人工上下文拒绝，更正来源取消旧 Wake。未把这些测试称为真实多日反馈、生产 OCI 或模型执行验收。
- 首次 verify-hyhXdE：编译/格式、原启动14项、新旧Forward3项及Worker检查通过；严格Clippy发现测试模块重复加载，之后改为复用原模块。
- 最终 verify-pDzDP4：全workspace/all-targets编译、格式、严格Clippy通过；144合同/领域 + 86 PostgreSQL + 25 HTTP/CLI/Worker = 255测试通过。验证期间全部源文件不变，隔离PG退出码0。无wire合同变化，无生成物手改。
- 尚未完成实际Worker消费成功后到原生模型/新一代Candidate再反馈的整链验收、Live晋级及其余#62合同。未push、未请求最新Head review、未合并、未关闭Issue。


### 2026-09-14：Wake 参数发布期间的授权到期

新增原生PG回归先由真实人工授权登记未来撤权时间，再实际写出新Cycle参数，使用数据库时间等待该时间到达。最终来源复核拒绝提交，Cycle/Run admission/PGMQ数量与调用前相同；原参数对象按Project锁确认无引用后清理。下一次消费不读写对象而取消旧Wake，保留DEGRADED原观察。没有修改宿主时钟、改写历史Evaluation或伪造成功Cycle；现有提交前复核已满足该边界，未改产品实现。

verify-f9DuXU：workspace/all-targets编译、格式、严格Clippy通过；分类1 + 原Cycle14 + Forward/Wake4 + Worker1 = 20项定向测试通过，验证期间源文件不变，隔离PG停止。该受控协议/关系测试不是生产模型、多日真实反馈或OCI整链证据。实际Worker成功消费后到Mission及后代反馈链、Live晋级与完整#62验收仍未完成。


### 2026-09-14：实际 Worker 的 Wake → Mission 与原生恢复

共用 tests/support/forward_result.rs 保存既有受控 Forward 科学协议，Store 与 Worker 测试消费同一原参数/manifest/结果，未改变产品实现。新的 mission_worker 正向分支从真实人工 Brief 冻结/Cycle_START 开始，明确关系型 Candidate/历史 Claim 和科学响应夹具；实际 Worker 先发表原 Forward Evaluation/Observation/Wake，再并发轮询消费唯一 Wake。数据库确认两个 Cycle、仅一个人工 CYCLE_START 回执、此时零 CodexSession。新 DATA_VALIDATE 的受控终态由实际 Worker 处理并启动原 Mission。

继续使用官方 App Server：按原 Profile 默认设置启动受控 local_fixture，完成两轮 Responses 请求；关闭原连接，按原生租约接管并恢复同一 Thread/Session/工作目录，核对原上下文、一次 Mission 身份与24个原生实际 token。原人工启动路径同样复跑恢复断言。该检查满足受控原生会话与 Worker 组合路径；生产账户推理、真实科学执行、多日市场反馈与后代 Candidate 再反馈仍需各自验收。

verify-s99RmH 首次编译发现共享请求的Box返回类型，已修正。verify-P5uhet 正向测试通过，Clippy指出该集成仅使用共用夹具部分字段，依现有约定仅在测试模块限定dead_code。最终 verify-udfQpS：workspace/all-targets编译、格式、严格Clippy全过；分类1、Cycle14、Forward4、原Worker1、自动Wake原生恢复1、原人工原生恢复1，共22项定向测试通过。验证期间源文件不变，隔离PG停止；无模型/生成合同/产品依赖变更，未push、未review、未合并或关闭Issue。


### 2026-09-14：原生 Paper 观察集合驱动自动 Live

DESIGN A7.11明确完整原始Paper流集合、每流样本/时长/双指标、当前政策及原数据用途要求。Worker复用既有审批/Offer/Claim事务消费AUTO_HANDOFF；migration072将完整排序Observation UUID集合与自动Live Approval一并冻结。首次Claim重新核对同一完整集合及所有原来源，不替换证据、不复制Candidate清零日额度。每日额度按同一Candidate的Paper/Live合并计数。人工Live与已Claim历史重放保持原语义。

新原生测试从原资格/Release链开始，通过实际数据注册接口显式授予ResearchPaperLive，旧夹具保留原用途；实际Forward Run/result/Evaluation/Observation，再原生审批/Offer/Claim。并发仅一Offer；插入Offer故障回滚Approval/证据/Offer；新报告流未测量和测量为HEALTHY后分别阻止旧Claim；冻结证据不能删除；撤销原政策后已有Claim仍按原键无文件IO重放。共用Forward夹具现在使用原生Downstream创建与probe，科学平均值显式传入，原劣化用例保持-0.1。

verify-QLotT2首次完整Live定向通过。补充回滚/撤权断言后的verify-rJw65l通过workspace/all-targets编译、格式、严格Clippy及257项回归（domain/contracts144、Store87、HTTP26），源文件验证期间不变，隔离PG停止。此前测试失败分别揭示不可变对象重复写、原始授权只含Paper及旧关系夹具无效凭据引用，均在测试设置修正，没有放宽产品检查。

这里的历史Paper Claim时间与科学响应是显式受控夹具，不证明真实多日市场观察或OCI/生产模型整链。Worker真实Live交付的完整部署验收、同Candidate跨环境当日额度专项、后代Candidate再反馈、UI及其余T01–T42/main恢复迁移仍需完成。GitHub当次回读PR63仍OPEN Draft且远端Head为37e5713ed6252e5935787201914d42f241582a4f，Issue62仍OPEN；本阶段未push、未请求review、未合并或关闭Issue。


### 2026-09-14：实际 Worker Live 消费与跨环境当日额度

在既有原资格/Forward正向测试中，Live成功分支改用实际Worker.process_automation与真实ArtifactStore。并发tick遵守SKIP LOCKED的可退让语义，随后普通tick完成唯一Live Offer，再次轮询不增加Offer；之后仍走原Downstream Claim及撤权后原键重放。首次verify-c2L7I2表明并发首轮可能都退让，不将其误报为必须即时创建；verify-e28IgD在正常后续tick下通过。

额度专项使用真实当前UTC日，不改变时钟或历史记录：新下游先由原生人工审批/Offer创建Live，再冻结max_rebalances_per_day=1的AUTO_PAPER政策。原同一Candidate自动Paper成功，数据库当天两个Offer、一个distinct Candidate；重复消费不读文件且不新增Offer。没有绕过已领取Candidate不可重复Paper的规则。

最终verify-HjhW7s通过workspace/all-targets编译、格式、严格Clippy与完整Live定向用例（含上述两个场景及既有原证据变更拒绝/回滚/撤权重放），验证期间源文件不变、隔离PG已停止。本次只增加测试和证据，产品实现未变。实际Worker单tick调度/IO不等同完整部署daemon、真实多日行情、OCI科学或生产模型验收；后代Candidate再反馈、交付UI和其余T01–T42/main恢复迁移仍待完成。未push、未review、未合并或关闭Issue。


### 2026-09-14：Release 冻结界面与原版本分页

复用原Release事务与元数据映射，新增项目内ID倒序分页HTTP/CLI；精确项目RESEARCH_READ及既有跨项目404隐藏语义不变。Ant Design组合候选从原独立PORTFOLIO/PASS确认冻结，只提交原Candidate/Evaluation；未知响应同键同请求重试，离线禁止新提交。交付页读取原Release分页/详情，保留原期限及来源，明确冻结不是审批。审批/Offer/自动化政策操作界面仍未完成。

verify-k8Clgr：workspace/all-targets编译、格式、严格Clippy与29项原生测试（client_portfolio_build10、portfolio_study_http19）全部通过，源文件不变，隔离PG停止。原资格链创建的两个Release验证原ID分页；实际HTTP和CLI验证空列表、非法limit及真实其他项目隔离。此前verify-v9mGlo指出映射多余借用及测试将既有404误写为403，已修正，未放宽权限。

web-verify-DyagBY：真实Rust合同导出及客户端生成逐字节可复现，手写源不变；TypeScript、505项单元及5项静态文件检查、wire精度检查、Vite构建、Codex浏览器子集和267项全浏览器测试通过。1440/768/390视口覆盖分页、跨项目错误响应、独立评估限制、未知请求重试、离线与无障碍。浏览器使用显式受控响应，只证明界面行为；真实数据库资格由上述原生链验证，不能替代完整T01–T42部署验收。本阶段未push、未review、未合并或关闭Issue。


### 2026-09-14：原交付历史网页查询

交付页新增“交付记录”标签，复用既有项目Handoff分页和单条HTTP接口。表格及详情显示原环境、状态、原审批/Release/前版关联、精确字符串交付序号、期限与原Claim/ACK事实；未确认不补成功，跨项目响应拒绝呈现。没有增加写请求、改变原下游权限或把ACK当作成交。审批/Offer/政策操作表单仍待完成。

web-verify-SvLLD1完整通过：真实Rust导出与客户端生成逐字节可复现，手写源不变；TypeScript、505项单元及5项静态文件检查、wire精度、构建、Codex浏览器子集和273项全浏览器测试通过。新增受控历史用例覆盖三视口分页、超过JS安全整数的原序号、原Claim/空ACK、错项目响应和无障碍；这些是界面证据，未替代原生交付或T01–T42部署验收。首次类型检查修正测试Revision必须为字符串，未改产品合同。GitHub回读仍为Draft/Open PR63、远端37e5713ed6252e5935787201914d42f241582a4f，Issue62开放；未push/review/merge/close。


### 2026-09-14：原 Release 审批历史分页

新增GET /api/v2/releases/{id}/approvals与client approval list RELEASE_UUID，复用原审批元数据映射和精确项目读取授权，按原ID倒序分页所有历史版本。不会将旧审批重新判为可用、刷新期限或读取Package/私有报告字节；为网页选择原审批提供服务端入口，网页审批/Offer操作尚待接通。

verify-XKnWFS通过workspace/all-targets编译、格式、严格Clippy及两项原生定向测试（原CLI授权意图、原资格Release审批链）。扩展真实审批链核对两次审批的分页、旧决定序号保持0和不存在Release返回NotFound。该链中的原生事务不是浏览器fixture；本轮未新增完整HTTP身份矩阵或浏览器操作证据。web-verify-XUqZpb真实Rust合同与客户端生成逐字节可复现、手写源不变。未push/review/merge/close，完整#62仍待完成。


### 2026-09-14：Release 原审批历史界面

Release详情按需展开原审批历史，复用原分页接口，核对项目/Release/Candidate三重关联；显示原授权来源、环境、期限、证据集合、自动化政策、下游版本/决定序号/就绪观察。历史null保留为“历史未记录”，不转换为0或当前版本，不提供审批/发送资格。

本轮仅前端展示变化，npm --prefix apps/web run typecheck及直接Vite构建通过；npm run test:e2e -- tests/delivery.spec.ts tests/portfolio-candidates.spec.ts --workers=2通过51项三视口定向回归（56.9秒）。新增用例覆盖原审批分页、历史空值、错Candidate响应拒绝与无障碍；原候选独立评估/冻结同键重试一并复跑。受控浏览器响应只证明UI，不替代原生HTTP身份矩阵或完整T01–T42。未push/review/merge/close。


### 2026-09-14：网页人工审批原目标包

新增Release审批表单，复用原生POST、Intent、离线与未完成操作保护；显式选择下游/Paper或Live/截止时间。读取全部原Candidate决定分页后按该下游/环境最高ordinal绑定REOPEN/null，不以第一页缺项推断首次审批。人工REJECT、停用或不支持的环境阻止提交；服务端仍重验所有资格及来源。未知响应保留原请求/键；回执核对项目/Release/Candidate/下游/环境/配置版本/人工来源，成功不发送Offer。

TypeScript及Vite构建通过；delivery与portfolio-candidates共54项三视口浏览器回归通过（55.5秒）。新增实际表单操作的受控响应验证第二页原决定、BOTH下游配置、原配置版本与截止时间、丢失响应后同body/key重试；没有用浏览器fixture证明原生审批准入。类型检查发现测试将环境枚举误作数组，已同步修正BOTH判断再验证。Offer/政策界面、完整HTTP矩阵及T01–T42仍待完成，未push/review/merge/close。


### 2026-09-14：网页人工 Offer

原审批历史可打开Offer确认，重新读取精确审批并遍历项目交付全部分页，按BigInt原delivery_sequence选择相同Mandate/下游/环境前版。重复Release或同Candidate已领取拒绝新提交，非OPERATOR及缺失原绑定的历史审批不能用于人工Offer。截止时间不能超过原Release/审批；POST只提交原Release/Approval/前版/期限。响应未知保持原body/key，回执核对原绑定和截止时间，成功不假称Claim或成交。

TypeScript、Vite构建以及delivery/portfolio-candidates共60项三视口浏览器定向回归通过（约1分钟）。新用例覆盖第二页前版、超过JS安全整数范围的相邻序号、重复候选阻止、丢失响应同键重试及Modal无障碍。受控UI响应不替代原生审批准入/Offer竞争；自动化政策界面、审批撤销/拒绝管理、原生整链和其余T01–T42仍待完成。未push/review/merge/close。


### 2026-09-15：审批列表原生 HTTP 与 CLI 范围验证

扩展原资格Package→审批→Offer→Claim/ACK原生测试，使用实际临时TCP服务、真实机器凭据与原审批记录。未认证读取401，下游领取身份403，同项目RESEARCH_READ CLI读取原审批及证据引用，其他项目CLI404，只有EVIDENCE_READ的CLI403；非法limit为422。实际client approval list RELEASE_UUID --limit 100成功返回该原审批，非浏览器模拟响应。

verify-VxO3kE首次编译发现测试ProjectCreate字段拼写错误，已按原合同修正。最终verify-Ha79sd通过workspace/all-targets编译、格式、严格Clippy和扩展original_package_claim_cli_transfers_once_and_replays测试（82.36秒），包含既有真实Claim/ACK/重放与新增读取断言。验证期间源文件不变，隔离PG已停止；只补测试，产品权限未改。该协议/数据库链仍不等于完整部署或所有T01–T42验收，未push/review/merge/close。


### 2026-09-15：网页追加审批撤销

原审批历史新增撤销入口；读取全部原撤销分页并显示历史，绑定最大原ID作CAS，立即生效以null交由数据库时钟判定，预约须未来且不晚于已有最早撤销。原因代码/原因按合同长度检查；提交和未知重试保留原请求/键，回执核对审批/原因/显式生效时间。成功仍保留已领取及ACK事实，未提供撤单/平仓操作。

TypeScript及Vite构建通过，delivery/portfolio-candidates共63项三视口浏览器定向回归通过（约1.1分钟）。新增用例验证原预约撤销可追加立即撤销、绑定原ID、丢失响应同键重试及Modal无障碍；预约日期边界和原生并发仍依赖对应服务端测试及后续整体验收，浏览器fixture不是数据库证明。自动化政策、人工拒绝/重新考虑、完整研究和其余T01–T42仍待完成，未push/review/merge/close。


### 2026-09-15：撤销预约与离线边界

扩展原撤销浏览器用例：已有2099年预约时，2100年推迟与2020年回填均禁止提交，2098年提前允许；切回立即后离线禁用且零写请求，恢复网络后原CAS/立即null及丢失响应同键重试正常。TypeScript和三视口定向Playwright通过（3项，8.6秒），使用上一轮相同产品字节的Vite产物；本轮产品实现未变。该证据补足网页日期/离线交互，未替代数据库时钟及并发验证或完整#62验收。


### 2026-09-15：网页人工拒绝与重新考虑

Release详情新增人工决定入口，完整读取同Candidate跨Release历史，按所选下游/环境最高ordinal绑定原最新决定。停用下游仍可选择管理历史；首次拒绝绑定null，重新考虑只接受当前REJECT并使用其原Decision路径，不复制Release清零。原因验证、未知请求同键重试、关闭保护及原回执关联检查复用现有模式；成功不恢复旧审批或发Offer。

TypeScript、Vite构建通过，delivery/portfolio-candidates共69项三视口浏览器定向回归通过（约1.2分钟）。新增首次拒绝与跨Release重新考虑用例使用停用下游、原ordinal/路径和丢失响应重试，并通过Modal无障碍；服务端原生决定CAS仍是最终准入，浏览器fixture不是实际数据库验证。政策/构建界面、完整研究和其余T01–T42验收仍待完成，未push/review/merge/close。


### 2026-09-15：网页冻结自动化政策

交付页新增自动化政策分页/原版本展开及完整冻结表单，绑定原项目revision，选择既有同项目Mandate/启用下游；模式、样本数、精确时长、每日额度、晋级/维持两组独立指标及期限均显式填写。复用原评估政策Requirements字段与counterRules，不新增指标引擎。允许新再平衡默认false；未知响应保留原完整body/key，保存不代表Worker执行或Paper/Live交付。政策撤销界面仍待完成。

TypeScript、Vite构建通过，automation-policies/evaluation-policies/delivery共54项三视口定向浏览器回归通过（1.8分钟）。新表单用例验证超过JS安全整数的项目revision及秒数、两组独立阈值、默认未启用与丢失响应原键重试；共享评估政策字段的原用例同步通过。受控HTTP响应不是原生政策准入/自动交付证明；完整T01–T42、构建与部署仍待验收，未push/review/merge/close。


### 2026-09-15：网页自动化政策撤销

自动化政策行新增撤销入口，复用审批撤销的日期/历史/重试组件；分支调用原生policy或approval端点，各自验证原目标字段。政策只提交reason，审批仍提交reason_code；原CAS、最早生效约束、未知请求同键重试和Claim/ACK保留语义不变，没有新增领域引擎。

TypeScript与Vite构建通过，automation-policies/delivery共42项三视口浏览器回归通过（48.8秒）。新增原政策历史/立即撤销/丢失响应重放用例确认精确路径、原最新ID及无审批reason_code；既有审批预约时间/离线/原键用例同步通过。浏览器fixture不替代原生撤权竞争或完整T01–T42；构建界面、全链与部署仍待验收，未push/review/merge/close。


### 2026-09-15：构建输入的原权重快照查询

新增项目forward-weight-snapshots分页API及client forward weights，复用原DownstreamWeightsViewV1与研究读取授权，保留原精确内容/报告引用/期限，无文件读取或新资格判定；下游写入身份不获得该研究读取权限。为构建表单提供真实来源选择，Build仍重新验证原环境/资产/币种/有效期。

verify-fFQRUg的domain144及HTTP25项通过、Store27通过1失败；唯一失败为新增第二条快照后旧全局回执计数仍要求1。调整测试顺序，在新增快照前保留原并发同消息唯一回执断言，再验证两页；verify-wkgDPw通过workspace/all-targets编译、格式、严格Clippy及全部3项forward_weights测试，源文件不变、隔离PG停止。web-verify-KWli6e真实Rust导出/客户端生成逐字节可复现、手写源不变。未将首次失败整轮标为通过，未更改产品幂等或权限。Build网页及完整T01–T42仍待完成，未push/review/merge/close。


### 2026-09-15：网页原来源组合构建

原Mandate详情接入PortfolioBuild，配置绑定所选项目；复用分页ResourceSelect、精确计数预算、Intent重放、关闭保护和RunDetail。显式选择Cycle/Runtime及原修订、Forward输入、环境、下游原快照或LAST_TARGET原候选，以及Alpha→版本→资格和十进制聚合权重；切换父引用清空子引用、重复Alpha不能提交。历史窗口不代替服务端资格重验，202只展示原Run回执。

最终TypeScript/Vite通过，portfolio-build/portfolio/portfolio-candidates共51项三视口回归通过（1.5分钟），覆盖两种来源、第二页原快照、重复成员、父引用清空、离线、精确数值/Runtime修订、未知响应原内容/键重试及Modal无障碍。首次45通过6失败来自测试输入/权重空数组违反原生合同，修正fixture而未放宽产品校验；一次错误cwd未修改测试便启动的运行已主动中断，不计通过。浏览器受控响应不等于真实科学/构建运行或T42端到端验收，完整T01–T42及自动再平衡/部署仍须复核，未push/review/merge/close。


### 2026-09-15：T33 新cutoff与独立目标包的事务验收

实读Worker::process_automation仅调用automate_paper、automate_live、process_forward、process_wake；Build/Study/Release入口仍为Operator原意图，缺少按冻结调仓日程创建新候选/独立Study/Release的可信调度链。自动交付原Release不能替代这个缺口，OPERATIONS已明确标示。GitHub本轮读取PR63仍OPEN/Draft、远端Head37e5713ed6252e5935787201914d42f241582a4f，Issue62仍OPEN。

新增原生Store/PGMQ链路用例rebalance_new_cutoff_requires_new_evaluation_and_preserves_original_package：使用已有真实命令产生的两个原资格，在新Forward cutoff发布新Candidate/LAST_TARGET来源，核对原cohort不变、输入与目标产物不同；旧Candidate的Evaluation以release_portfolio_evaluation拒绝新包，新Candidate需自己的PORTFOLIO Study/Evaluation；新Release/包/asof独立，旧包字节、旧Release和旧Candidate完整读取相等，未新增Mission、Approval或Offer。上游科学声明仍是受控fixture，证明事务与版本关联，不宣称真实市场科学或全自动T33/T42验收。

verify-wy9PBE及verify-B47lle均通过编译/格式/Clippy，单项新测试栈溢出失败；将大型研究准备和双包断言分段Box::pin后，verify-eCG9RP通过相同workspace/all-targets编译、格式、严格Clippy及该单项原生数据库测试（7.82秒），未增加线程栈或削减业务断言。源文件在验证期间不变，隔离PostgreSQL已停止。当前仍未push/review/merge/close。


### 2026-09-15：可信Worker的有界再平衡Build

DESIGN补充冻结政策下的确定性构建来源、日程、原Cycle/Runtime/限额、实际cutoff及失败/日额度规则。人工与自动Build复用同一事务准入核心；自动入口不接Actor或人工grant，使用RUNTIME产物来源并在发布后重验原政策/资格/期限。Worker::process_automation接入该阶段及等待Project锁后的未引用文件清理。新不可变portfolio_rebalances表绑定原policy/sourceCandidate/input/Run，原生约束防跨项目/下游/时间关联，project/mandate/downstream/实际cutoff唯一；新policy UUID不清零，已有非失败后继不重复入队。日历使用原Universe日历，不推测交易日。

发现InputSet cutoff只是原数据selection cutoff的上界，修正调度使用与Candidate相同的实际时点，并在核心准入后再次比较。新增真实Store/PGMQ测试从原命令的两个资格、固定间隔Mandate、独立Study/Release及人授权政策开始，覆盖旧数据换新InputSet不推进、写文件失败回滚、双Worker调用只一个Run、原成员/修订/限额、无Operator回执、不可变记录、可领取原生Run且未新增Release/Approval。研究/包准备分段Box::pin，未增线程栈；受控模型/市场响应只证明原生事务，不代替实际数值/真实授权数据或T42。

compile verify-AaU3R3通过；verify-J3pPwH仅测试Cycle Option断言编译失败，修正测试类型；verify-g22syJ新场景被正确拒绝automation_downstream，改为原生命令登记PAPER/LIVE测试下游而未放宽产品Gate。verify-XhxVbv通过两项；实际cutoff修正后verify-1kZckX通过workspace/all-targets、格式、严格Clippy及2项原生数据库检查（23.10秒）。最终verify-f0NSng再次通过同样静态检查和原生组合HTTP/CLI/Worker回归31项（10+21），源不变、隔离PG均停止。

固定间隔正向Store链已验证；实际Worker非MANUAL正向调用、Calendar调度原文件场景、政策失效/UTC配额更多竞态仍需补强。自动Study及自动新Release仍待接入；运行来源在Web/CLI的完整可见性也须随完整T33/T42验收补齐。没有push/review/merge/close，未将本阶段视为完整自动再平衡。


### 2026-09-15：原自动Build的独立Study与新Release

人工/自动Study复用原准入事务；自动入口固定原Candidate、Cycle、Runtime修订和限额，沿用冻结独立研究计划。原自动政策和来源Release必须仍有效/当前；不可变portfolio_rebalance_studies绑定单一后继。正式PORTFOLIO/VALID/PASS之后复用原Release冻结事务，发布后逐字节重读来源和期限，并登记不可变portfolio_rebalance_releases。Worker串行推进三阶段并清理回滚孤立产物，RUNTIME来源不生成Operator回执，也不绕过Paper/Live审批。

扩展原生数据库/PGMQ场景，覆盖未完成阶段不发布、并发单一Study/Release、独立历史输入、原Cycle、发布失败回滚、旧包字节不变、无新Approval/Operator回执、关联不可改写。verify-RiLVyz仅测试传入字节数类型编译失败；改读原artifacts.byte_count后verify-0WHBRB通过workspace/all-targets编译、格式、严格Clippy及两项数据库测试（23.50秒），source_unchanged=true。受控上游数值响应仍不证明真实市场/模型或完整T42。实际非MANUAL Worker正向、日历与更多政策/额度竞态、运行来源可见性及完整验收仍待补齐；尚未push/review/merge/close。

共享异步事务扩展后，verify-67o4Zs/VD9dbb/qd8trH的Live回归出现栈溢出，CLI十项通过但整轮失败。仅Worker分配边界不足以解决；verify-Zk06Bp临时阶段标记定位到资格准备、尚未进入Live Worker。共享Study/Release与Worker阶段使用Box::pin，大型Live测试入口分段后，verify-XnfmbD定向通过；未增加线程栈、未删业务断言，诊断输出已移除。最终同一源码verify-5CiBo1通过workspace/all-targets编译、格式、严格Clippy及31项HTTP/CLI/Worker回归（HTTP21项248.95秒）；verify-uBwbsJ再次通过全部静态检查及2项再平衡数据库用例（25.08秒）。两轮source_unchanged=true，隔离PostgreSQL停止。GitHub最新读取仍为PR63开放草稿、远端37e5713、Issue62开放。

### 2026-09-15：真实Worker再平衡与原政策替换反例

复用已有原命令资格/Release/政策/Forward准备及原生结果完成辅助，新增frozen_policy_worker_rebalance_advances_original_study_release_and_paper。调用真实Worker::process_automation：未探测下游的Paper失败不阻断原Build/Study/Release；双tick及后续普通tick只产生一个原Study，正式PASS后冻结新Release；探测下游能力前无Approval/Offer/额外Operator回执，探测通过后原政策仅向新Release产生一个PAPER Offer，重复tick不重复Build/Study/Release。不声称模拟下游已Claim/ACK或执行订单。

新增两项Store反例：原Build成功后和原Study正式PASS后分别通过原生授权命令更换同内容的政策版本；新版本不能被旧Run借用、不能继续Study/Release，也不重复已有Build。原policy关联及已有Release保留。测试使用真实PostgreSQL/PGMQ和原ArtifactStore；科学结果仍为受控原生协议响应，不是实际数值/市场/T42证据。

verify-1AhZpm静态/原Store通过，新增Worker断言误把SKIP LOCKED正常延后视为必须失败；修正为单次tick验证下游失败、并发允许延后并以随后普通tick验证推进，未放宽交付约束。verify-0ljzur通过。最终verify-lklCit通过workspace/all-targets编译、格式、严格Clippy、4项Store测试（51.63秒）与1项Worker测试（17.28秒），source_unchanged=true且隔离PG已停止。现有CI的store/server全集自动包含这些测试；本地rebalance-focus同步执行两入口。仍待Calendar原文件正向、UTC额度/失效竞态、自动运行来源可见性和完整T01–T42验收，未push/review/merge/close。
