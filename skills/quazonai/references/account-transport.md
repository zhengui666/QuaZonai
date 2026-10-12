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

Read positions, cash/equity, curve, evidence time and target receipt separately. Missing, stale or unpriced values stay null/unavailable; account totals are not a factor's isolated return. ACK is not a fill, and an old/replayed receipt does not renew a target's expiry or snapshot freshness.

`refresh` asks the existing adapter to observe again, not to restart an engine. `stop` stops the observation scope; neither command asserts that a remote account/provider cancelled work. Preserve external source/account/session/event/sequence identities, valuation clocks and explicit dropped-event gaps whenever the adapter supplies them.

The compact `/api/v3` client does not expose the retired native-envelope relay or capital-exit routes. Retain original envelope/history files for the host's compatible adapter; do not send them to invented endpoints, rewrite their identities or claim they were migrated. Missing adapters and account access remain explicit limitations.
