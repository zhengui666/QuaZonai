# Polymarket external execution boundary

## Approved architecture and this slice

QuaZonai remains target-only. A separate official Nautilus execution owner is
responsible for accounts, balances, inventory, orders and reconciliation. The
present work permits architecture development and order-free verification only;
it does not authorize credentials, persistent account access or live orders.

This slice implements a pure library preflight, not a host that always reports
not-ready. It parses an explicit format request, constructs an actual official
native `OrderAny` and calls the pinned Polymarket public static validator. A
valid request produces `FORMAT_VALIDATED` plus an in-memory native order for a
future execution owner. An invalid request produces the real failure stage and
reason. Neither outcome is a target receipt, account observation, authorization,
exchange acknowledgement or trade result. There is no HTTP/CLI route, identity
provisioning, execution client, signer, environment lookup or network operation.

The format request has an explicit scope, native identities, initialization UUID
and nanosecond timestamp, BUY/SELL side, pUSD or outcome-share amount, order kind,
time-in-force, reduce-only and (for LIMIT) price/post-only. GTD includes its exact
expiry. Missing/extra fields, unknown units and non-string decimals fail. The
native representation must preserve the exact declared amount and price; no
funding amount, tick, price, TIF, risk parameter or currency is inferred.

`FreshPaperCashV1`, `TargetPackageV2`, the simulation order shape and every Paper
admission check remain unchanged. No source parser is presented as a Live claim
validator. This preflight does not accept a release/claim/account-balance field.

## Existing paths to reuse

- QZ release creation, approval, offer, authenticated downstream claim, ACK and
  revocation stay in the existing API/Store transaction boundary. An execution
  owner obtains an original claim through that boundary; local JSON is not proof
  of server-side authority.
- `DownstreamTransport` presently implements only GET
  `/downstream/v1/capabilities`. It does not push targets. The existing job-side
  control protocol provides `/targets`, `/status` and `/stop` under that prefix.
  A future Live owner may implement that protocol independently of Paper.
- Capability probe observations bind integration revision and freshness. The
  existing `TARGET_ONLY` mode describes the QZ delivery contract, not brokerage
  connection health or permission to trade.
- The control-service bearer and QZ downstream machine identity have different
  destinations and purposes. Reuse authorized credentials only in their own
  services; this slice provisions and consumes neither.
- `NativeNodeObserver` can attach to an actual official execution client/account
  in a Live node. It derives connection state from the native node/client, not
  merely from snapshot arrival. Existing account-observation intake preserves
  exact money, clocks, session/cursor identity, gaps and stale/unpriced flags.
  Those observations are read-side projections, not QZ's trading ledger.

## Required real Live target source (design only)

The current strategy current-decision request/outcome both carry
`FreshPaperCashV1`. Its release is explicitly Paper. Changing its environment,
using a V1 forecast envelope to disguise it, or copying its target into another
JSON document cannot produce a Live-authorized strategy target.

Introduce a distinct current-live-decision request and outcome only in a later
coordinated producer/consumer change. It must use the already accepted immutable
strategy Alpha versions and new Forward-purpose feature inputs with the same
feature dictionary and provenance meaning. Reuse all existing source
permissions, PIT/evidence qualification, original fees, universe, current input
age, model/fuel and constraint checks. Keep research capital assumptions labeled
as assumptions, never observed cash. Do not initialize or restore an account in
QZ to manufacture a Live decision.

A new target-only package version, rather than an overloaded Paper V2, must bind:

- Original project, candidate, mandate and newly persisted release identities
- Native decision run, accepted attempt and original report artifact
- Immutable Alpha version IDs and original input/source/feature provenance
- Original input revision/artifact references and native component versions
- As-of, effective and expiry timestamps; target/cash weights, currencies,
  frozen constraints/tolerance and original execution-cost evidence references
- An explicit LIVE delivery environment and named authorized downstream owner
- Any native observation/current-weight reference genuinely used by the
  decision, including its original account/source/session/cursor/freshness

There is no `account_start`. Retain simulation settings as research evidence
where needed; they are not a live fee, fill or latency promise. The external
owner derives current inventory and spendable cash from its own reconciled
native account/cache. A QZ account-observation snapshot must not silently replace
that authoritative state. A new current-weight reference, if needed for a
turnover constraint, must retain its original source and observation identity;
do not synthesize zero positions from an absent observation.

The real publisher must reconstruct that exact package from the accepted
request/report inside existing release transactions and reread original bytes
and current authority at approval/claim, as V2 does. New schema-version support
must be carried through the settings/capabilities enums, generated schemas,
release view/envelope, SQL version/environment constraints and original claim
path together. This slice implements none of those source records or migrations
and advertises no new accepted package version.

## Ownership and lifecycle

QZ owns immutable research evidence, release identity, qualification, downstream
scope/revision, approval policy, expiry, offer/claim/ACK history and revocation.
The external owner owns admission of the original claim, native account/client
binding, authoritative balances/inventory/open orders, target-to-order policy,
order journal, submission outcome and restart reconciliation. Credentials and
wallet operations stay entirely with that independently authorized owner.

A future owner must durably reserve the original claim identity before any native
work is scheduled. The journal key binds project, downstream, environment,
handoff and external claim identity. Equal replay returns the original receipt
without extending validity or reexecuting; conflicting content under that
identity fails. A new output directory does not reset claim history. Crashes or
unknown submission outcomes require native reconciliation, not a new claim ID
or blind resend. The current format function is pure: repeat calls return the
same native identity, but it does not pretend to provide durable idempotency.

Target consumption, format validation, native order submission, venue acceptance,
partial/final fills and reconciliation are distinct observations. QZ ACK records
only target delivery. An original live order report/fill and account correlation
would be required for actual execution evidence; a mock, ACK or format success
is insufficient.

At claim/admission and before each future child-order submission or retry, the
owner must respect the original target/approval/source expiry and freshest
applicable revocation. QZ revocation/supersede does not retroactively undo an
already claimed fact or cancel a venue order. `/stop` can stop scheduling; any
cancel/close action needs separate authorized execution semantics and real
terminal evidence. Restart must retain the journal and reconcile pending native
orders before admitting another target. Market data connectivity alone cannot
prove account readiness.

## Native reuse and remaining readiness checks

The returned in-memory order is an actual official `OrderAny`, not a mock or a
parallel order model. A later owner can pass it to the ordinary Nautilus strategy
and execution-engine flow after independent authority/account checks; the
PolymarketExecutionClientFactory remains the unmodified official gateway. This
library does not instantiate that gateway and does not call `submit_order`.

The two public static validators check only part of the official path. Full
venue price-range/instrument/tick validation, current GTD safety horizon, market liquidity,
fees/allowances, account balance/inventory, private connection, reconciliation,
signing and venue acceptance are not established here. This slice makes no
`execution_ready` claim and supplies no constant-false demonstration host.

## Validation plan and state

Source and static review precede the sole Cargo slot. Run the existing native
contracts generator in this independent tree; never edit schema JSON by hand:

`cargo run --locked --offline -q -p contracts --example generate > contracts/generated/domain-v1.openapi.json`

Then run:

`cargo test --locked --offline -p job --test polymarket_execution_preflight`

The tests cover strict/duplicate/unknown fields, unsupported scope and units,
exact decimal/nanosecond strings, native identities, nonrepresentable decimals,
zero/negative size, real native order preservation, official denomination/TIF/
reduce-only/post-only failures, explicit expiry, deterministic repeated calls and
exact agreement between the recorded schema and the live native schema export.
All samples are explicitly synthetic order formats, not original Live releases.
An independent review found that the enum-derive output lacked closed variant
objects despite strict Serde parsing. The four tagged unions now have explicit
source-defined native schemas. A regression compares every variant's declared,
required and serialized field sets, asserts `additionalProperties: false` and
checks that Serde rejects extras. Generated JSON is still produced only through
the native exporter, never patched by hand. Each referenced type is collected
along with its dependencies; a separate Contracts unit test exports Request and
Report as individual roots and proves that every local schema reference resolves.

On 2026-10-07 both commands completed offline in the independent candidate tree.
Native generation added exactly the nine declared preflight schemas, changed no
existing schema and removed none. The final focused Job target compiled and
passed all 19 tests with zero failures or ignored tests. The single-root Contracts
unit test passed separately. The installed JSON Schema 2020-12 validator also
passed ten assertions covering valid variants and rejected extra fields against
the generated schemas; it needed no installation or network. Standalone rustfmt
check also passed.
An existing unrelated unreachable-pattern warning in `spot_fees.rs` remains.

Independent review and final integration checks are still required. This
candidate is separate from the already validated V5 static format-test patch;
it does not modify or invalidate the V5 source archive. When integrated, add this
pure test target to the existing routine Job test selector alongside the static
Polymarket format-contract target; do not replace its original cases.
