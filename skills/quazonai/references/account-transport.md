# Target-only account observations

Paper/Live identify an existing configured source binding; these are not launch-trading modes. QuaZonai sends target-only data and observes authorized downstream/account state, never broker orders.

```sh
quazonai settings get
quazonai schema show ObservationStart
quazonai observation start --file original-observation.json
quazonai observation list
quazonai observation get OBSERVATION_ID
quazonai observation refresh OBSERVATION_ID
quazonai observation stop OBSERVATION_ID
```

A start file contains the original `requestId`, `portfolioId`, `mode` (`paper` or `live`) and configured `connectionId`. Preserve that binding and the original portfolio/target identity. Never synthesize an account, swap modes or expand authority to fix a failure. `get` selects the original ID from the actual observation list, so it does not require an optional server get endpoint.

New manual targets require a fresh authoritative capital-status read, including first contact/aliases; unknown reservations or outstanding reserved/fenced funds block delivery. A cached zero or unsupported/unverified adapter is insufficient. This does not prevent historical reads or original-receipt reconciliation.

Read positions, cash/equity, curve, evidence time and target receipt separately. Missing, stale or unpriced values stay null/unavailable; account totals are not a factor's isolated return. ACK is not a fill, and an old/replayed receipt does not renew a target's expiry or snapshot freshness.

`refresh` asks the existing adapter to observe again, not to restart an engine. `stop` stops the observation scope; neither command asserts that a remote account/provider cancelled work. Preserve external source/account/session/event/sequence identities, valuation clocks and explicit dropped-event gaps whenever the adapter supplies them.

Optional `observation automatic ID --file POLICY.json` accepts `{enabled,cashBuffer,maximumSnapshotAgeSeconds}`. This explicitly enables an official Durable background task, not browser polling. It stays within the original selected pool, deducts physical-account outstanding reservations and cash buffer, and uses native joint-capacity/current-holdings/lot/fee sizing. Unknown reserve/equity/capability is blocked; unsupported execution can be projected but not delivered. Expired historical targets are not renewed. Inspect available budget, suggested strategy capital, member scale, clocks/control revision and the separate adjustment receipt; do not call ACK traded. Rejected targets wait for newer authoritative snapshot/control context; uncertainty reconciles the original ID, including unresolved manual receipts. Deterministic expiry before POST is `NOT_OFFERED`, not remote uncertainty. Disabling restores manual snapshot refresh without discarding unsettled receipt reconciliation.

`capital status OBSERVATION_ID` reads original native funds/progress. With compatible advertised V1 capability, `capital preview OBSERVATION_ID --file FILE` uses an original `{requestId,amount,currency,policy?}`; `capital command OBSERVATION_ID --file FILE` explicitly starts the approved preview or submits a native pause/cancel/resume/external-withdrawal-reconciliation action. Preserve original bytes, preview/plan/source observation/control and intent revisions. Preview is not authorization to start. Only initiate the user-requested amount/action; QZ never performs the external transfer. Pause/cancel does not release reserved money. Unknown command outcomes reconcile the same original identity without another unsafe POST.

See the concrete [downstream protocol](../../../apps/service/account-transport.md). Existing reservation/progress can remain readable when initiation is unsupported. Missing adapters and unknown reservation evidence are explicit limitations, not zero funds or success. The retired native-envelope relay is still not exposed; original history identities are not rewritten.
