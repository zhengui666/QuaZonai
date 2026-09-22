# Native-first Polymarket research

## Intent

Implement the owner's request to open and complete a QuaZonai PR based on the native Nautilus reuse conclusion. Implementation PR: [#101](https://github.com/zhengui666/QuaZonai/pull/101). Related requirement: #100; product-completion criteria are defined in [DESIGN](../../../DESIGN.md#delivery-completion). Starting main: cd4b3e356b768a245358beb5e2b9c6e883f72d3d.

## Specification

Reuse the pinned Nautilus Rust Polymarket public clients, instrument parser, market-data types, Parquet catalog, fee model, simulator and portfolio statistics. Do not rebuild a vendor SDK, matching engine, optimizer, plugin marketplace or live execution service. No trading credentials or wallet material are needed for historical data reads.

Data acquisition, native catalog publication, research admission, scientific results and target delivery are separate facts. A native catalog write cannot mark a dataset PIT-verified or make a portfolio deliverable. Preserve event, metadata-observation and import times. Never turn missing historical depth, fees or resolution into zero values or an assumed winner.

## Implemented scope

1. Operator-only public-history acquisition and bounded original native-record import, with detached source evidence and native catalog round-trip tests. Instrument and complete BarType identities have separate native Parquet partitions.
2. Existing BAR Alpha and shared-capital portfolio research accept native POLYMARKET BinaryOption, CASH, original collateral and the native price-dependent fee model. Original close events drive 0/1 and 50/50 settlement; delayed or missing availability is not replaced with scheduled expiry or last price.
3. Original USDC, USDC.e and pUSD remain distinct research units through API, database, execution assumptions, study results and web forms. Model-billing currency remains ISO-only.
4. Published Polymarket portfolio statistics use native 365-day configuration and account snapshots; existing non-prediction paths retain 252 days. Canonical upstream output is preserved separately.
5. Runtime capability/image binding, native CLI tests, real OCI study regression, web contract tests and three-viewport collateral form tests cover the integration. Synthetic preview history without simulation explicitly has no equity curve.
6. Canonical design, CLI and operations guidance describe actual usage and remaining data limitations.

## Inspection basis

- AGENTS.md and DESIGN.md were read. DEVELOPMENT.md is absent; CONTRIBUTING.md is the development guide.
- CodexPro plugin discovery returned no available plugin. No local workspace or executor run is claimed. DESIGN 0.4 directs GitHub file-tool authorship and Actions verification; GitHub Codex is a read-only reviewer, not a source author.
- Native Rust crates are pinned to 0.63.0, corresponding to upstream v2.0.0rc4.
- The pinned Data API client can return partial trades at its offset ceiling and includes synthetic same-second ordering. Neither behavior proves complete history or observed subsecond availability.
- On the starting main, research accepted BAR data, execution assumptions excluded BinaryOption, and simulation rejected expiration-bearing assets. This PR extends native BinaryOption consumption; it does not introduce a new tick/L2 scientific runtime.

## Verification evidence

[Preparation run 35674382220](https://github.com/zhengui666/QuaZonai/actions/runs/35674382220) compiled/linted Job and the Runtime OCI target, passed all Contracts/Domain/Job tests, generated API/web contracts, and passed web type and unit checks on source 779b2973a522c3398eb655fb084a44bc5587c72d. These are scoped preparation results, not a substitute for final-head repository CI or completed real OCI/browser execution.

The final applicable CI, clean independent review, resolved threads, expected-head merge and main verification are recorded in PR #101's native checks and discussion. Those records, not a self-authored status field here, determine delivery. Any source change requires new applicable checks and review.

## Acceptance boundaries

The research path remains validated LAST/EXTERNAL BAR. Stored TradeTick/QuoteTick/L2 records are not automatically BAR inputs; arbitrary vendor CSV/Parquet normalization, complete archive coverage, historical fee/rule/PIT evidence and real-market research acceptance remain separate #100 work. Dataset registration and existing scientific admission are not bypassed.

No full real historical archive, trading account, wallet, production deployment, live order or real-market Alpha/portfolio profitability is claimed. Synthetic native computations and their regressions do not establish those facts. Settlement availability does not prove unobserved on-chain redemption or gas costs.

Completion requires: (1) the implementation PR, (2) all applicable CI passing and explicit clean read-only Codex review on its final head, (3) merge after (2), followed by main verification. Do not close #100 or #62 merely because this native integration or an ingestion utility is merged.

## First review corrections

The first independent review identified unbound close rows, target TTL beyond binary
expiry, incoherent sibling payouts and inconsistent annualization documentation.
Corrections freeze complete source payout vectors in registered quality metadata and
native task contracts, recheck native close records before replay, and enforce the
original contract lifetime at build/adoption boundaries. No source scan grants its own
scientific authority. The importer also preserves original arrival order for equal
reception times. Native, domain and real-journal regressions cover these cases.

These changes require final-head CI and another explicit read-only clean review in
PR #101; their authorship alone is not a successful executor or acceptance result.
The detailed remediation specification is recorded in Issue #100 comment 5770031108.
