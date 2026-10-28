# Route standalone research without campaign prerequisites

<a id="task"></a>
## Task

The first actual Hyperliquid study stopped with zero trials after campaign-only intake and hypothesis calls returned campaign_binding_required. Global authoring guidance incorrectly required every experiment to register a hypothesis, although the factor tool makes campaign IDs optional and the ledger intentionally admits standalone research without a binding.

<a id="spec"></a>
## Change

Make guidance branch on research_brief.campaign. Standalone research calls factor_evaluate with hypothesis and model directly; bound campaigns retain intake, registered hypothesis, lineage, comparison and budget rules. Scope the affected tool descriptions and README consistently. No campaign is fabricated, no existing study or budget is reset, and no tool schema or ledger safeguard changes.

<a id="verification"></a>
## Verification

Actual local checks passed: research typechecking and all 32 existing standalone host/campaign protocol tests. No new test infrastructure or native build was added. Current-head CI, independent review and GitHub review remain pending; this patch alone does not prove a successful live model run. The owner controls any temporary read-only instruction overlay and same-study continuation.
