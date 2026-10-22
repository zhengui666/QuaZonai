# Target-only account transport

This optional downstream HTTP contract extends the existing configured `connections` URL. It does not expose broker orders or move funds. QuaZonai retains the original native CapitalExit V1 documents; the downstream adapter must connect them to its existing native owner. An installed adapter is **not** implied by the presence of this protocol or the native Rust contracts.

All paths below are relative to the configured connection base. Requests preserve `x-qz-client-id` and the configured bearer environment binding; credentials are never part of persisted research or target documents. No redirects or implicit unsafe POST retries occur. Money/quantities/weights and nanosecond clocks are decimal strings. Native IDs/revisions retain their original formats.

## Existing target/account paths

- `POST targets`: `{id, clientId, accountId, expiresAt, target}`. `target` is the original native `NativeTargetPointV1`, not an order. The response and `GET targets/:id` echo these original envelope identities and return `status` (`PENDING`, `ACKNOWLEDGED`, `REJECTED`, `EXPIRED`, `REVOKED`), optional stable `externalId` and `acknowledgedAt`. ACK means target reception, not trading.
- `GET account`: `{clientId, accountId, sequence, nativeClockNs, currency, equity, cash, positions}`. `positions` contain `{instrumentId, quantity, value, pnl}`; unknown valuation remains null. Sequence and native clock must not regress across known physical-account aliases; the same sequence cannot carry changed facts. Shared comparison normalizes only top-level transport `clientId`/`accountId`, preserving source, observation, control, execution and economic evidence. Manual refresh and automatic sizing use the same check, including local same-sequence replay; automatic delivery reads current committed facts and rechecks the original prepared sequence/clock against all known bindings after the awaited uncertainty checkpoint returns, before any offer. A superseding/conflicting manual refresh committed during that await makes the never-offered preparation `NOT_OFFERED` and requires fresh sizing, without querying its nonexistent receipt. A possible POST outcome remains original-ID GET-only reconciliation, even if a newer peer snapshot arrives afterward.
- Existing manual bindings remain manual. Before admitting a **new** manual target, QZ reads authoritative capabilities/capital status and resolves physical-account aliases; unknown reservations or outstanding reserved/fenced funds block admission, including first contact and stale cached-zero records. A readable V1 adapter with verified zero reservations need not support capital-exit initiation. Unsupported/unverified adapters cannot receive new manual targets; historical reads and original-receipt reconciliation remain available.
- Local stop does not revoke a downstream target. Unresolved original/manual receipts block automatic enablement and later offers across aliases even when their local tasks are terminal. A stopped uncertain original receipt can still be reconciled without another POST.

## Optional capabilities and capital status

`GET capabilities` returns the exact configured `{clientId, accountId, mode, currency}` plus:

```json
{
  "capitalExit": {"version": 1, "supported": true, "readable": true},
  "automaticTargets": {"supported": true, "revisionBound": true, "snapshotBound": true}
}
```

The envelope fields above are required, not omitted by this fragment. `mode` is `paper`/`live`. `supported:false, readable:true` permits existing reservation/progress reads but not initiation. A missing/unsupported adapter is displayed as unavailable. Absent reservation evidence is unknown, never zero.

`automaticTargets.snapshotBound` is an optional protocol extension and is required for a **new PM current target**. It declares that the receiver atomically compares the original `accountSourceId`, `observationId`, `snapshotSequence`, `nativeClockNs`, `controlRevision` and reserved funds against its authoritative state before accepting the target. An unchanged control revision alone does not establish that positions are unchanged. A mismatch must reject the original request without applying its target. Older adapters without this declaration retain reads, original-receipt reconciliation and their existing legacy contract; they do not acquire current PM qualification through a default value.

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
- liquidity entries in original native `NativeSizingLiquidityV1` form: `{instrumentId,currency,eventNs,availableNs,notional,lotNotional,minimumNotional,takerFee}`. Used records require `eventNs <= availableNs <= clockNs`; unrelated/out-of-scope records do not constrain sizing. These are exact native account-currency notionals/lot/minimum/fee observations, not invented prices or TS conversions. Absent/non-array positions and unknown holdings valuations block sizing, rather than becoming a known empty account.

The native sizing command deducts outstanding reservations and explicit cash buffer, caps the approved pool by joint capacity, rounds **trade deltas from current holdings**, retains out-of-rebalance-scope holdings at their actual value, enforces causal liquidity/participation/minimum orders and retains fees/cash. If current lot/minimum rules erase an intended buy, sizing is blocked instead of reporting fictional assigned member capital. Zero observed demand liquidity is constrained, not an amount that becomes suitable by increasing capital. No formulas, horizons, leverage or factor membership are retuned. The view separately reports available equity budget, actual suggested strategy capital, capacity, cash, per-member scale and original receipt. Historical capacity is conditional BAR feasibility, not future orderbook depth or promised profitability.

An adjustment uses `POST targets` with a fresh stable `id` (`<observationId>:allocation:<epoch>`) and an additional original `allocation` object binding epoch, snapshot sequence/native clock, source/observation, account control revision/reserve, exact equity/budget/effective capital/buffer/member scales, selected factor weights and capacity assumptions/clocks. It also captures `maximumSnapshotAgeSeconds`, `execution:{validUntilNs,maximumAgeSeconds,liquidity:[{instrumentId,eventNs,availableNs}]}` for the evidence actually used by native sizing, and an aggregate `validUntilNs`: the earliest original target expiry, capacity expiry, captured account-age deadline, execution expiry or used liquidity event-age deadline. Immediately before transport, after the durable delivery checkpoint, QZ rechecks the original target and this captured deadline; current control/reservation, alias and policy checks remain in place. These bounds survive restart and are never extended to rescue a preparation. Legacy group members retain their original member `weight` and optional original group `poolWeight`; native multiplication determines effective capital without lossy TS reweighting. The downstream must advertise `revisionBound:true`, atomically reject a stale control revision before accepting the target and echo the identical `allocation` object in POST/GET receipts. Its native owner must enforce the capital fence even if a reservation arrives concurrently. Otherwise this path is unavailable, not an ordinary unbound target delivery.

Missing execution capability/fresh market evidence can still yield `PROJECTED`, explicitly **not delivered**. Unknown equity/reservation/capacity blocks sizing. Expired original targets and expired capacity evidence remain expired; the task never manufactures a new forecast or extends TTL. Only an accepted target with unchanged effective equity/capital, members and control/reservation context suppresses another POST; a definitive rejection cannot generate fresh epochs when evidence and intended economics/policy are unchanged. Fresh unchanged evidence may support an explicitly changed buffer, account-age policy, member allocation, target or capacity policy; that different intent is reconsidered without extending evidence validity. An uncertain adjustment reconciles only the original receipt, including after local disable/stop/restart. Explicit refresh prioritizes an unresolved original/manual receipt, otherwise reads the actual allocation receipt rather than the unrelated initial `NOT_OFFERED` identity. Prepared `SIZED`/`PENDING` allocations also block competing admission but are not GET-reconciled as if already offered. Deterministic expiry, missing configured credentials, disable or abort before the first POST settles `NOT_OFFERED`; automatic delivery requires a fresh account read/sizing after credentials recover or evidence changes. Older preparations without a captured deadline are likewise never offered and require fresh sizing. Actual transport uncertainty retains original-ID GET-only reconciliation even after the captured evidence expires. Another same-account binding cannot offer a replacement before uncertainty settles. Disabling restores manual snapshot refresh and is not remote revocation.

## Service/CLI entrypoints

Authenticated service endpoints:

- `GET /api/v3/factors`, `/factors/:id`, `/factors/:id/capacity` (recorded assumptions).
- `POST /api/v3/factors/:id/capacity` with `{scenarios:["1000"],participation:"0.01",maximumAgeSeconds:60}` performs a read-only native assessment using explicit assumptions, including for eligible legacy studies. It never mutates original admission/trials or spends new independent exposure; it is not a financial command and needs no request ID. A new combination's `capacityPolicy` uses the same shared validator and governs its joint assessment even if member assumptions differ. Shared HTTP/CLI validation preserves the original positive scenario strings and enforces native `NUMERIC(38,18)` plus the 64-character wire bound before native execution.
- `POST /api/v3/portfolios` supports mutually exclusive atomic `{factors:[{factorId,weight}]}` and legacy research-ID requests. Both require a structured study's requested criterion to be `computed` or `met`, even without a threshold; unavailable metrics remain disqualifying. Goal-only legacy compatibility is unchanged.
- `GET /api/v3/observations/:id/capital-exits`.
- `POST /api/v3/observations/:id/capital-exits/preview` with `{requestId,amount,currency,policy?}`.
- `POST /api/v3/observations/:id/capital-exits/commands` with original `{requestId,action,previewId? ,exitId?,expectedRevision?,amount?,currency?,externalTransferRef?}` appropriate to that native action.

CLI aliases are `factor list/get/capacity`, `capital status/preview/command OBSERVATION_ID`, and `observation automatic OBSERVATION_ID --file POLICY.json`. Preview/command files retain original bytes and explicit request IDs. Do not initiate a reservation or report external withdrawal without the user's specific operational authorization. General implementation/test authorization is not that request.

## Optional observed spot starting state for PM research

An account-level PM comparison can consume the optional lossless fields below on the existing `GET account` response. The original normalized account identity, sequence, clock, equity, currency, positions and capital-control binding remain required. Old transports remain readable; their summaries cannot reconstruct native balances or prove that no orders are working.

- `nativeObservation`: the complete original `AccountObservationSubmitV1`, with `schema_version`, `binding`, `sequence`, `dropped_events`, `observed_at_ns`, `connection` and `snapshot`. The binding preserves its native account, trader, session, project, environment and native version. The snapshot is the original `NativePortfolioSnapshotV1`, including every exact-decimal total/free/locked currency balance and all original event, clock, valuation and staleness fields.
- `openOrders`: `{schema_version:1, account_id:string, asof_ns:string, available_ns:string, count:number}`. This is an actual producer report, with the same native account and economic `ts_event` as the snapshot. The current observed-spot consumer requires `count:0`; a missing report is unknown, not zero.

The implemented domain is official NautilusTrader `0.63.0`, `CONNECTED`, zero dropped source events, `CASH` with a null native base currency, complete priced state, no margins, no stale assets, and nonnegative balances with `locked=0` and `free=total`. Report currency and the explicit multi-currency spot policy must agree with the original account. Unknown or nonzero capital reservations block this domain. Margin, betting, borrowed funds, locked balances and pending orders need different native state contracts.

QZ records `snapshotReceivedAtNs` locally on the first receipt of a new snapshot. It is not a producer-controlled field. Replaying the same sequence preserves this time. Legacy facts without a receive clock acquire the conservative time of their first local capture. `observed_at_ns` is a producer heartbeat; neither it nor `ts_event` can replace the receive time. A snapshot must have been available before the replay's evaluation start. A current snapshot cannot be inserted into past validation.

The native `study-observed-spot` consumer initializes both HOLD and REBALANCE from the same original currency balances. The allocator reads actual portfolio state at its decision clocks. No artificial fills establish initial holdings. HOLD issues no active rebalance orders; its later valuation, fees and return path still come from the original native engine. Results are conditional historical replays, not realized owner profit, future execution capacity or target-delivery qualification. All attempted scientific legs remain subject to the owner campaign budget.

Retirement requires known empty original orders, no remaining non-report-currency balances or positions, and no remaining target responsibility before declaring the account flat. ACK leaves the target executable through its original deadline. A single original-ID receipt task may track an acknowledged target until expiry or a definitive receipt; it never resends the target. Closure requires a genuinely later authoritative account and zero-order observation after that deadline, or after the retained actual receipt time of a definitive rejection/revocation/expiry. Receiving an older flat snapshot late is insufficient. Missing evidence keeps reconciliation active; an ACK is not a fill or exit confirmation, and a local timer does not rewrite the remote receipt as expired.

The optional `snapshot.accountEvents` carries producer-reported events and explicit completeness. Its shape is `{schemaVersion:1,accountSourceId,nativeAccountId,interval:{startNs,endExclusiveNs},availableAtNs,complete:{cashFlows:boolean,executions:boolean},cashFlows:[],executions:[]}`. `cashFlows` entries are `{id,eventAtNs,availableAtNs,currency,amount,kind}` with signed decimal amount and `DEPOSIT|WITHDRAWAL|TRANSFER|CORRECTION`. `executions` entries are `{id,eventAtNs,availableAtNs,instrumentId,orderId,clientTargetId,side,quantity,price,currency,fee}`; `clientTargetId` and `fee` may be null, `side` is `BUY|SELL`, and a known fee is `{amount,currency}`. All clocks are nanosecond strings and monetary values remain decimal strings.

The envelope is bound to the original native account, source, sequence, economic snapshot clock, and local first receipt. Source publication time never replaces receiver availability. A missing envelope means unknown coverage; an explicitly complete empty interval is a source assertion of no events in that interval. Exact event-ID retries are deduplicated; a changed body under an existing event identity is rejected and retained as a source-integrity gap.

Observed attribution uses only actual source events. With complete cash-flow coverage and matched reported-equity currency, QZ can display closing equity minus opening equity minus external net cash flow as an exact account-level diagnostic. It does not identify strategy profit, compute a time-weighted return, convert missing currencies, or infer a fill from a target ACK. Partial executions and unknown fees remain partial or unknown. Historical HOLD/REBALANCE simulations never enter this actual-event stream.

## PM current inference and target receipts

One physical account has one authorized target controller. Strategies that share
its capital are combined inside that mandate. PM admission, legacy manual and
automatic admission, and delivery preparation read the original PM/Application
documents in the same Durable transaction and resolve aliases with the original
account-control bindings. Concurrent authorized mandates, an active legacy
automatic controller or an unsettled competing target block new offers. Research
and shadow mandates may coexist without claiming account control. Pause retains
the controller; retirement releases it only after the latest strategy is retired
and all original residual responsibilities are resolved. Existing uncertain
commands continue GET-only reconciliation through these changes.

Current inference revalues the original observed currency quantities at marks actually available at the decision cutoff. It retains the account snapshot's economic and receiver clocks. `AccountBasis.valuation.timeBasis=DECISION_CUTOFF` identifies this derived price basis; reported equity remains null when the source reported no common-currency equity. Continuous model weights are target intentions. Original native evidence separately records rounded trade quantities, resulting inventory, funding and modeled effects. Source entry/exit checks use the rounded quantities and notionals.

Continuous target and delta notionals retain the exact NAV × weight product, up
to the native derived type's 40 integer and 36 fractional digits. They do not
round actual money facts into a database scale. Original NAV, native weights,
balances, lot quantities and rounded execution estimates keep their existing
native decimal constraints.

The original authenticated capabilities response is retained with its first retained receiver clock. Polling identical content never renews that clock. A changed capability response changes the captured account basis even when sequence, balances and control revision are unchanged. New PM delivery requires `automaticTargets.supported=true`, `revisionBound=true` and `snapshotBound=true`. Its `allocation` retains `snapshotSequence`, `nativeClockNs`, `accountSourceId`, `observationId`, `controlRevision`, reserved funds and original reported equity, together with decision/policy/basis references. An inventory valuation is a separate `valuation`, never a replacement for reported equity.

The target preserves the original native decision cutoff and weights. Delivery can shorten its expiration to the earliest original source deadline; it cannot extend the original target. The receiver must atomically enforce the original allocation revision and echo the identical complete `allocation` in both POST and GET receipts, as required for unattended adjustments. A missing or changed echo remains uncertain. Acknowledging a target does not establish an order, fill, position or profit. A command already uncertain before this PM envelope was introduced keeps its original GET-only reconciliation path; upgrade does not retrofit a new request or new source facts onto a possible remote effect.

Paper shadow operation requires the separate `PAPER` assessment with requirements `qz.pm.paper-shadow-source-target-qualification/1`. It preserves the original method's SHADOW research evidence and cannot qualify live investment. Live target operation additionally requires an original independent protocol explicitly authorizing that stage, qualified historical availability, and complete current execution source evidence. The currently registered paired economic falsification protocol authorizes SHADOW only.

## Frozen-method current spot inference

A PM policy registers its operating rule before research and independent evaluation. After a method survives the registered paired independent check, one current `build-observed-spot` call reuses its original models, calibrations, ensemble coefficients, optimizer, cost policy and selected capital-to-NAV fraction. It values the unchanged original account quantities at cutoff-available marks, derives current asset weights and solves against that actual current state. Original source snapshot and receiver clocks remain unchanged. This is `FORWARD_MODEL_ESTIMATE`, not an observed return or another historical replay. The actual operating comparison uses native lot-rounded holdings and linear costs; the continuous solution remains available as evidence.

The version-2 PM allocation method registers `constraints.cash_funding=PRESERVE_MIN_CASH_AFTER_LINEAR_TURNOVER_V1` for observed spot CASH, long-only, mean-variance allocation. Its single native QP funds the original linear turnover costs inside the selected deployment budget while preserving the original minimum cash amount. Historical selection, independent evaluation and current inference carry the same frozen field; no new fee, buffer, allocation fraction or second optimization is introduced. Older methods and seeds do not acquire this behavior or its qualification retrospectively.

The registered rule checks hard-bound feasibility before comparing objectives. A drifting HOLD that violates the original long-only, asset, cash, net or gross exposure limits cannot defeat the sole feasible native proposal on modeled return/risk. The consumer measures the original cutoff-NAV bounds with the original weight tolerance, then conservatively checks rounded inventory, turnover and cash against NAV reduced by the stated linear fee scenario. Unknown constraint families or an unproven repair require evidence. These post-trade quantities and fees are estimates, not actual fills or balances. `AssetTarget.executionEstimate` exposes the original rounded quantities and notionals separately from the continuous target intention. A proven repair still needs every current source and authority qualification before delivery.

The original schedule anchor and TTL define at most one logical solve per slot. Late trustworthy receipts move the actual computation cutoff forward; they never move receipt availability backward. New parameter, member, coefficient or capital-fraction searches return to the shared research ledger. Fixed-method operational computation retains its native resource receipt separately and does not consume another search trial. Checkpoints preserve original UUID objects, binary identity, full input basis and original fit-prefix proof. Replayed computation does not reopen independent data. Source facts arriving within an already admitted slot invalidate incompatible target preparation and update a bounded pending observation; they do not generate another search or skip-decision history entry.

Current delivery additionally requires the original `GET account` response to contain `execution.pmEvidence`:

```text
{
  schemaVersion: 1,
  id, sourceRevision, methodRef, sourceEvidenceRef,
  sourceKind: "PAPER_EXECUTION_MODEL" | "LIVE_EXECUTION_SOURCE",
  accountSourceId, nativeAccountId, observationId, sequence, controlRevision, currency,
  asOfNs, availableAtNs, validUntilNs,
  instruments: [{
    instrumentId, rulesRef,
    costs: {commissionRate, slippageRate, impactRate, evidenceRef},
    entry: {maximumBuyNotional, maximumSellNotional, evidenceRef},
    exit: {maximumNotional, evidenceRef}
  }]
}
```

All clocks and amounts are exact strings. Each cost component may be null, which leaves execution qualification unavailable; missing impact is not zero. The proof carries the execution source's own economic and availability clocks, which may be later than the position snapshot, and must be inside the actual received account bundle. It binds the original physical account, sequence and control revision. `rulesRef` is `sha256:` followed by the canonical JSON SHA-256 of the original native instrument-definition object returned by the build. The source must supply its actual rule identity; QZ does not invent one on its behalf. The original `execution.liquidity` entries remain required.

Entry limits cover the actual rounded buy or sell amount, and exit limits cover the complete rounded post-trade inventory. Rates must agree with the native model's original all-in linear cost rate. BAR traded notional cannot establish these executable limits. The complete original native inventory, rule definitions, source receipts, immutable catalog and original training-prefix proof remain separately checked. Current source limits do not become a historical capital curve or an economic capacity ceiling.

The effective deadline is the earliest original native target expiry, execution-source expiry, liquidity expiry, and the frozen maximum-age deadlines of the original account, forecasts and price marks. Retries cannot extend any deadline. Capability responses and their first local receipt are retained as `capitalCapabilities`; `automaticTargets.supported`, `revisionBound` and `snapshotBound` must all be true. PM delivery preserves the native cutoff and original continuous target weights, optionally shortens its expiry, and uses the same `allocation` envelope with atomic snapshot and capital-control checks. The downstream must echo that exact envelope in both POST and GET receipts.

Paper observations require a separate `PAPER` assessment under `qz.pm.paper-shadow-source-target-qualification/1`, an actual paper execution source, and explicit owner target authority. They remain additional shadow observations of the frozen method. A prior `SHADOW` research result does not authorize LIVE operation: a live consumer additionally requires an original target-level independent qualification protocol, verified historical availability and actual live execution evidence. The current research protocol stops at SHADOW. Missing method or source qualification leaves an explicit gap, even when the current native solve succeeds. Authorized, fully qualified paper decisions enter the same official Durable target task automatically; uncertain delivery reconciles only the original ID.
