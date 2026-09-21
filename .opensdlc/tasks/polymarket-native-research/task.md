# Native-first Polymarket research

## Intent

Implement the owner's request to open and complete a QuaZonai PR based on the native Nautilus reuse conclusion. Related requirement: #100; broader production acceptance remains #62. Starting main: cd4b3e356b768a245358beb5e2b9c6e883f72d3d.

## Specification

Reuse the pinned Nautilus Rust Polymarket public clients, instrument parser, market-data types, Parquet catalog, fee model and simulator wherever they satisfy the required behavior. Do not rebuild a vendor SDK, matching engine, optimizer, plugin marketplace or live execution service. No trading credentials or wallet material are needed for historical data reads.

Data acquisition, native catalog publication, research admission, scientific results and target delivery are separate facts. A native catalog write cannot mark a dataset PIT-verified or make a portfolio deliverable. Preserve event, metadata-observation and import times. Never turn missing historical depth, fees or resolution into zero values or an assumed winner.

## Plan

1. Inspect the pinned upstream APIs, repository governance and actual data/simulation callers.
2. Add bounded native public-history acquisition and original native-record import, with detached source evidence and native catalog round-trip tests.
3. Connect supported prediction-market research and shared-capital simulation without pretending unsupported lifecycle or cost behavior is available.
4. Update canonical design, operations/CLI guidance and coverage using actual implementation evidence.
5. Run the applicable native, contract, browser and repository CI on the final source; fix failures in this branch.
6. Obtain explicit clean read-only Codex review on that same head, resolve threads and merge only after applicable CI succeeds.

## Inspection evidence

- AGENTS.md and DESIGN.md read from the starting main. DEVELOPMENT.md is absent; CONTRIBUTING.md is the development guide.
- CodexPro plugin discovery returned no available plugin. No local workspace or executor run is claimed. Repository DESIGN 0.4 directs GitHub file-tool authorship and Actions verification.
- apps/job pins native Rust crates to 0.63.0, corresponding to upstream v2.0.0rc4.
- The pinned Polymarket Data API client returns partial historical trades at its offset ceiling. Its normalized timestamps include synthetic same-second tie-breakers. Neither behavior establishes complete history or observed subsecond availability.
- The current QZ catalog validator only admits BAR data. Execution assumptions admit CurrencyPair/Margin or Equity; the simulator rejects expiration-bearing instruments. These are integration boundaries, not proof that upstream Nautilus lacks prediction-market data types.

## Verification

NOT_RUN. No production data, credentials, deployment, scientific run or complete research workflow has been executed by this task yet.

## Review and delivery

NOT_REQUESTED. The task is incomplete until the declared implementation scope, applicable final-head CI, explicit clean independent review and merge have actual evidence. Do not close #100 or #62 merely because this task record or an ingestion utility exists.
