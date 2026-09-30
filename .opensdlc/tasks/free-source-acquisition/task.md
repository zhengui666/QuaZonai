# Free source acquisition foundation

## Scope

Part of the owner's 2026-09-30 autonomous `dev` iteration. First establish a
single bounded source-plugin acquisition boundary with anonymous free Polymarket
and crypto APIs. Full native conversion, service/UI orchestration and admitted
research remain subsequent deliveries; acquisition alone is not the requested
complete end-to-end plugin outcome.

The [provider guide](../../../runtimes/data/providers.md) is the maintained design,
capability contract, usage, evidence and integration backlog. Provider planning
and decoding are separate from shared transport, byte/request limits, immutable
publication and offline reproduction. Distinguish sampled marks from original
OHLCV; preserve missing rows, source clocks, exact decimal text and source terms.
No paid API, account, credential, order, synthetic BAR or source qualification is
introduced. Existing archive/EVM acquisition and native Polymarket paths remain
unchanged.

## Acceptance and verification

- Run every `runtimes/data` unittest, including prior acquisition boundaries
- Verify CLI help and deterministic planning without network dependencies
- Independently review bounded I/O, provenance, precision, unsafe paths,
  interrupted publication, duplicate/malformed rows and false qualification
- Keep live public reads distinct from fixture tests: Polymarket observed marks
  succeeded; Coinbase returned an environment-generated HTML unavailable page
  and is not live-verified from this executor
- Require exact-head hosted CI and independent native review before merging to
  `dev`; neither a local test nor a public fetch establishes research admission

Independent review requested preservation of a bounded failing raw response and
strict UTC publication/response chronology checks. These are acquisition
integrity checks, not upstream authenticity or historical PIT attestation.

## Remaining integration

The existing native Polymarket importer only accepts original `BinaryOption`
definitions on `POLYMARKET`. Crypto OHLCV needs a generic native BAR conversion
extension using the pinned Nautilus API and original definition/precision/clock
evidence, followed by the existing catalog preparation, data grants, partition
isolation and fresh `DATA_VALIDATE`. Polymarket price marks cannot be relabelled
as BARs; retain the existing original trade/archive/capture route for scientific
input. The in-app interface must expose unsupported steps without fake ready
states. See the provider guide for the complete evidence-backed backlog.
