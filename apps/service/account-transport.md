# Target-only account transport

This optional downstream HTTP contract extends the existing configured `connections` URL. It does not expose broker orders or move funds. QuaZonai retains the original native CapitalExit V1 documents; the downstream adapter must connect them to its existing native owner. An installed adapter is **not** implied by the presence of this protocol or the native Rust contracts.

All paths below are relative to the configured connection base. Requests preserve `x-qz-client-id` and the configured bearer environment binding; credentials are never part of persisted research or target documents. No redirects or implicit unsafe POST retries occur. Money/quantities/weights and nanosecond clocks are decimal strings. Native IDs/revisions retain their original formats.

## Existing target/account paths

- `POST targets`: `{id, clientId, accountId, expiresAt, target}`. `target` is the original native `NativeTargetPointV1`, not an order. The response and `GET targets/:id` echo these original envelope identities and return `status` (`PENDING`, `ACKNOWLEDGED`, `REJECTED`, `EXPIRED`, `REVOKED`), optional stable `externalId` and `acknowledgedAt`. ACK means target reception, not trading.
- `GET account`: `{clientId, accountId, sequence, nativeClockNs, currency, equity, cash, positions}`. `positions` contain `{instrumentId, quantity, value, pnl}`; unknown valuation remains null. Sequence and native clock must not regress; the same sequence cannot carry changed facts.
- Existing manual bindings remain manual. Before admitting a **new** manual target, QZ reads authoritative capabilities/capital status and resolves physical-account aliases; unknown reservations or outstanding reserved/fenced funds block admission, including first contact and stale cached-zero records. A readable V1 adapter with verified zero reservations need not support capital-exit initiation. Unsupported/unverified adapters cannot receive new manual targets; historical reads and original-receipt reconciliation remain available.
- Local stop does not revoke a downstream target. Unresolved original/manual receipts block automatic enablement and later offers across aliases even when their local tasks are terminal. A stopped uncertain original receipt can still be reconciled without another POST.

## Optional capabilities and capital status

`GET capabilities` returns the exact configured `{clientId, accountId, mode, currency}` plus:

```json
{
  "capitalExit": {"version": 1, "supported": true, "readable": true},
  "automaticTargets": {"supported": true, "revisionBound": true}
}
```

The envelope fields above are required, not omitted by this fragment. `mode` is `paper`/`live`. `supported:false, readable:true` permits existing reservation/progress reads but not initiation. A missing/unsupported adapter is displayed as unavailable. Absent reservation evidence is unknown, never zero.

`GET capital-exits` returns the same identity/mode/currency envelope plus:

- `accountSourceId`: original native account source ID; it must also match each exit's `account_source_id`.
- `observationId`: authoritative current native source observation ID.
- `controlRevision`: original monotonically ordered account control revision.
- `reserved`: total **outstanding** reservations across the physical account, as an exact nonnegative decimal string, or null if unknown. Funds already withdrawn and reflected in equity must not be deducted a second time.
- `exits`: complete original `CapitalExitViewV1` documents, including owner command, funds, phase, evidence identities, remaining-trading policy and revisions. A paper `SIMULATED` withdrawability fact is never presented as live venue cash availability.
- Optional `blocked:true` / `reason` explaining unavailable account control.

The native account source cannot silently change within an existing binding. For one control revision the known reserve/exit facts cannot contradict one another. QZ initially scopes a record by normalized connection base, account ID and mode, then joins URL/client aliases that advertise the same original native account source, mode and currency. These bindings share local pending reservations immediately; late aliases are joined before sizing and duplicate local enablement is turned off, preserving an already-prepared binding. An existing native source cannot be rebound to a different one. If readable capability disappears, last-known native progress remains visible but the current reserve becomes unknown and sizing stops.

## Original native previews and commands

The source of truth is [`crates/contracts/src/capital_exit.rs`](../../crates/contracts/src/capital_exit.rs). Native `job validate-capital-exit` validates original request/preview/start/action/view DTOs; it is not another owner or transfer engine.

- `POST capital-exits/preview`: `{requestId, clientId, accountId, request: CapitalExitPreviewRequestV1}`. Return the same envelope plus `preview: CapitalExitPreviewV1`. Scope, policy, source observation, account source, environment and control revision must bind exactly. The preview must contain the actual cancellation/reduction plan, costs/limits, validity and available/unavailable funds; do not synthesize missing evidence. QZ persists the complete original owner body, native request/source observation and control revision before its first preview POST; an unknown response replays that exact captured envelope, not newly bound account facts.
- `POST capital-exits`: `{requestId, clientId, accountId, command: CapitalExitStartV1}`.
- `POST capital-exits/:nativeIntentId/actions`: the same envelope with original `CapitalExitActionV1` (`PAUSE`, `CANCEL`, `RESUME`, `RECONCILE_WITHDRAWAL`). Resume retains an approved preview identity. Reconciliation carries the exact externally completed amount/currency and original external transfer reference; QZ itself performs no transfer.
- `GET capital-exits/commands/:originalRequestId`: reconcile the exact original command. Response echoes `requestId`, `clientId`, `accountId`, original `command`, `status` (`ACCEPTED`/`REJECTED`) and `result` (original native view when accepted). An unsettled/unknown command is not a rejection.

The adapter must make request IDs idempotent and reject changed bodies. QZ checkpoints uncertainty **before** the unsafe POST and thereafter only GETs that identity. Local reservation and admission are one Durable transaction. An accepted start remains locally reserved until authoritative native status includes the resulting intent/revision; receipt acceptance alone never releases funds. Pausing/cancelling retains reservation according to the native owner. The native owner remains responsible for old-target fencing, original observation/control revision checks, inflight orders, bounded reductions, withdrawability evidence and external cash-movement reconciliation.

## Optional unattended target adjustments

New `ObservationStart.automatic` or `POST /api/v3/observations/:id/automatic` explicitly sets `{enabled,cashBuffer,maximumSnapshotAgeSeconds}`. One official Pi Durable task refreshes the authoritative account and capabilities, survives restart and waits through public Durable task lifecycle. Observation polling does not size or deliver targets. Capital-status reads may query the downstream; the unattended adjustment loop does not depend on browser polling.

For executable sizing, `GET account` additionally supplies:

- `mode`, `accountSourceId`, `observationId`, `controlRevision`, matching current capital control;
- fresh `execution:{currency,validUntilNs,liquidity}`;
- liquidity entries in original native `NativeSizingLiquidityV1` form: `{instrumentId,currency,eventNs,availableNs,notional,lotNotional,minimumNotional,takerFee}`. These are exact native account-currency notionals/lot/minimum/fee observations, not invented prices or TS conversions. Absent/non-array positions and unknown holdings valuations block sizing, rather than becoming a known empty account.

The native sizing command deducts outstanding reservations and explicit cash buffer, caps the approved pool by joint capacity, rounds **trade deltas from current holdings**, retains out-of-rebalance-scope holdings at their actual value, enforces causal liquidity/participation/minimum orders and retains fees/cash. If current lot/minimum rules erase an intended buy, sizing is blocked instead of reporting fictional assigned member capital. Zero observed demand liquidity is constrained, not an amount that becomes suitable by increasing capital. No formulas, horizons, leverage or factor membership are retuned. The view separately reports available equity budget, actual suggested strategy capital, capacity, cash, per-member scale and original receipt. Historical capacity is conditional BAR feasibility, not future orderbook depth or promised profitability.

An adjustment uses `POST targets` with a fresh stable `id` (`<observationId>:allocation:<epoch>`) and an additional original `allocation` object binding epoch, snapshot sequence/native clock, source/observation, account control revision/reserve, exact equity/budget/effective capital/buffer/member scales, selected factor weights and capacity assumptions/clocks. Legacy group members retain their original member `weight` and optional original group `poolWeight`; native multiplication determines effective capital without lossy TS reweighting. The downstream must advertise `revisionBound:true`, atomically reject a stale control revision before accepting the target and echo the identical `allocation` object in POST/GET receipts. Its native owner must enforce the capital fence even if a reservation arrives concurrently. Otherwise this path is unavailable, not an ordinary unbound target delivery.

Missing execution capability/fresh market evidence can still yield `PROJECTED`, explicitly **not delivered**. Unknown equity/reservation/capacity blocks sizing. Expired original targets and expired capacity evidence remain expired; the task never manufactures a new forecast or extends TTL. Only an accepted target with unchanged effective equity/capital, members and control/reservation context suppresses another POST; a definitive rejection cannot generate fresh epochs on an identical snapshot/control context and can be reconsidered only after authoritative context advances. An uncertain adjustment reconciles only the original receipt, including after local disable/stop/restart. Explicit refresh prioritizes an unresolved original/manual receipt, otherwise reads the actual allocation receipt rather than the unrelated initial `NOT_OFFERED` identity. Prepared `SIZED`/`PENDING` allocations also block competing admission but are not GET-reconciled as if already offered. Deterministic expiry, disable or abort before the first POST settles `NOT_OFFERED`; actual transport uncertainty retains original-ID GET-only reconciliation. Another same-account binding cannot offer a replacement before uncertainty settles. Disabling restores manual snapshot refresh and is not remote revocation.

## Service/CLI entrypoints

Authenticated service endpoints:

- `GET /api/v3/factors`, `/factors/:id`, `/factors/:id/capacity` (recorded assumptions).
- `POST /api/v3/factors/:id/capacity` with `{scenarios:["1000"],participation:"0.01",maximumAgeSeconds:60}` performs a read-only native assessment using explicit assumptions, including for eligible legacy studies. It never mutates original admission/trials or spends new independent exposure; it is not a financial command and needs no request ID. A new combination's `capacityPolicy` uses the same shared validator and governs its joint assessment even if member assumptions differ.
- `POST /api/v3/portfolios` supports mutually exclusive atomic `{factors:[{factorId,weight}]}` and legacy research-ID requests.
- `GET /api/v3/observations/:id/capital-exits`.
- `POST /api/v3/observations/:id/capital-exits/preview` with `{requestId,amount,currency,policy?}`.
- `POST /api/v3/observations/:id/capital-exits/commands` with original `{requestId,action,previewId? ,exitId?,expectedRevision?,amount?,currency?,externalTransferRef?}` appropriate to that native action.

CLI aliases are `factor list/get/capacity`, `capital status/preview/command OBSERVATION_ID`, and `observation automatic OBSERVATION_ID --file POLICY.json`. Preview/command files retain original bytes and explicit request IDs. Do not initiate a reservation or report external withdrawal without the user's specific operational authorization. General implementation/test authorization is not that request.
