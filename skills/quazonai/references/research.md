# Finite research

The core is objective/data → disclosed hypotheses and native trials → validation feedback → selected independent assessment or a recorded stop. Portfolio/backtest and target/account observation are optional consumers, not prerequisites.

```sh
quazonai research models
quazonai datasets
quazonai research start --file original-request.json
quazonai research list
quazonai research get CONVERSATION_ID
quazonai usage
```

A file is a complete `ResearchStart` with an explicit original `requestId`. It may contain `input:{goal,model:{provider,modelId},data,policy}` or the configured-dataset form `{requestId,goal,model,datasetId,maxTrials}`. Native input must match a service-registered dataset; never invent server paths or data. Keep exact bytes and IDs for replay. An optional `--request-id` checks the file's ID; it never replaces it.

New structured files add `specification:{version:1,name,question,rationale,exclusions?,evaluation,search,capital?}` beside `goal` (or inside native `input`). The question equals `goal`; the short name is the list title. Supported objectives are `pearson_ic` for `single-factor` and `Sharpe Ratio (252 days)` for `ensemble`, with `direction:"maximize"`, optional finite target and `registered-partitions`; minimizing is unsupported and rejected rather than silently changing the mandate. Search binds `maxMembers` and `stop:"budget-or-selection"` to the original counted trial budget. An unselected `research_finish` is rejected while that budget remains; explicit owner stop is separate. Capital assumptions are exact scenario strings, explicit BAR participation and maximum liquidity age. Dataset instrument/input/horizon/partition/execution settings are inherited, never invented browser overrides. Structured native-input requests must match the registered policy, including its strategy/universe/simulation/partition settings; only the explicit trial budget may differ. Factor provenance resolves through the original submission request identity, not equality with another run's scientific input. A missing objective value is insufficient evidence, not a pass. Legacy prose-only files remain replayable. Capital assumptions can also be supplied later through factor capacity assessment or a combination's own `capacityPolicy`, without rerunning research or altering its original input.

For a legacy prose objective with an already configured dataset, explicit flags are sufficient:

```sh
quazonai research start --goal 'THE_REQUESTED_OBJECTIVE' \
  --dataset DATASET_ID --model PROVIDER:MODEL_ID \
  --max-trials REQUESTED_LIMIT --request-id ORIGINAL_REQUEST_ID
```

Use model IDs from actual configured availability. Discovery does not run inference, discover Desktop credentials or establish model quality. The host explicitly persists its capability-supported thinking level; hidden reasoning is not evidence to report.

Retain the original policy, budget, source identity and selection/evaluation criteria. Zero trials allows no computation. Committed failures consume attempts and remain disclosed. Missing inputs or unsupported criteria are not permission to change budgets or create another study until something passes.

## Literature during research

The installed research Agent can search Crossref, arXiv and Semantic Scholar through its bound literature tools and record supporting, contrary or background claims against saved paper records. These providers return metadata and available abstracts; they do not establish that full text was read. Search uses public academic keywords, never private datasets, account information, credentials or the complete research request. No additional paper-provider key is required; the service host still needs access to the public APIs.

Use `research get CONVERSATION_ID` or the research detail's literature tab to inspect saved sources, links, notes, limitations and source failures. Repeated reads reuse the original facts and do not trigger a new search. The CLI has no separate paper-search command; do not invent one or switch credentials to bypass a missing capability.

The bound Agent search tool normally reuses the latest saved receipt for the same normalized query. It can explicitly retry a `partial` or `unavailable` result with `retryFailed:true` in a new tool task, preserving the old failure. An original task replay returns its original receipt; successful results, including empty ones, are not searched again by that flag.

Distinguish no search, no results, partial results and unavailable providers. Limits, absent abstracts and access errors are research limitations, not evidence that literature does not exist. Report only metadata and excerpts actually returned, keep the paper separate from the Agent's interpretation, and never present a citation as independent evaluation, verified PIT or proven profitability.

## Follow the original study

Use `research get` to inspect all trials, failures, comparison and final/stop reason. Native computation is not a qualification. Final held-out assessment is separate from iterative feedback; never tune against it or rename/copy/fork exposed data to call it unseen. The model-code adapter preserves explicit scientific fuel budgets; omitted/null fuel stays unmetered. QZ imposes no process deadline or memory ceiling. Owned child recovery/cancellation and the provider-credential-free execution environment remain.

After an unknown start response, replay only the same original file or identical explicit flags with the same ID. HTTP 202 means admitted/replayed, not finished. Follow [observation](runs.md) and [recovery](recovery.md), without a replacement key.
