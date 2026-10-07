# Spot CASH runtime closure candidate

## Scope

This isolated candidate extends the frozen native observer with a dedicated Rust candidate runner. The managed and CLI legacy admission path remains closed until trusted closed-bar inputs and the full research integration are qualified.

- The owned fresh official Nautilus 0.63 engine creates one multi-currency CASH account with no borrowing, 1x leverage and only fresh report-currency funds
- TargetReplay obtains an actual official `Portfolio::build_snapshot` at every sizing point, uses total native cash balances, and records decision snapshots separately from published native snapshots
- Native base inventory, including buy-side base fees, controls subsequent sales; native free quote funds constrain deferred purchases
- A frozen public-fee scenario requires explicit `PUBLIC_RATE_SCENARIO_UNVERIFIED_APPLICABILITY` acceptance. Original instrument fee defaults remain untouched. This is neither historical fee evidence nor account-tier verification
- Read-only fill reconciliation uses official native `Account::calculate_pnls` and `Money` arithmetic to detect dropped/rolled-back native cash postings. Native totals remain valuation authority
- Only the owned fresh runner may produce the no-external-flow receipt. The reusable observer continues returning no such assertion
- Report outputs preserve raw native receipt, original native snapshots and deterministic canonical output separately. Consumer rules bind them without rewriting native evidence
- Exact adjacent complete UTC days use the first native publication at midnight. Decision snapshots support intraday sizing/graphing but never replace daily boundaries. Statistics call official portfolio-return APIs and remain unavailable on gaps

## Verification

The frozen observer v2 separately passed 4 contract, 30 domain and 15 job tests. The consumer candidate separately passed 91 domain/integration tests and Store compilation. Those results are dependency evidence, not a runtime candidate pass.

This candidate adds controlled native buy/sell fee, real full-day return and consumer-binding tests, plus read-only cash audit failure cases. The first runtime compile/test results are recorded in `delivery/runtime/status.json` when executed. Synthetic test inputs do not qualify market data or authorize research admission.

## Unclosed delivery boundaries

Trusted frozen closed-bar source identity; managed/CLI selection of the new runner; joint contract generation; independent final runtime review; and final applicable Rust validation remain required. No real financial order, remote publication or deployment is performed by this candidate.
