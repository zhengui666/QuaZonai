# Finite research

The core is objective/data → disclosed hypotheses and native trials → validation feedback → selected independent assessment or a recorded stop. Portfolio/backtest and target/account observation are optional consumers, not prerequisites.

Before writing a new mandate, read [research authoring](research-authoring.md) for the original goal, question, rationale, examples and quality checks. This procedure operates an installed service; it does not prepare native data or modify application code.

## Inspect and choose the operation

Use supplied research IDs first. When only a name or idea is supplied, compare the existing title, original question, dataset, model, trial history and final result. `research list --cursor JSON` continues with the returned cursor; preserve it and disclose partial coverage. Similar names or equal scientific inputs alone do not identify the original request.

| User intent / current state | Agent action |
| --- | --- |
| Prepare a plan or request for inspection | Write a local request/proposal and report unresolved choices. Do not call `research start`: it admits work and can invoke the model and scientific tools. |
| Continue observing or explain existing work | Reuse the numeric `conversationId`; read its trials and submission state. Follow [bounded observation](runs.md). |
| Recover an uncertain start | Preserve the original file and `requestId`; reconcile/replay that request, not a newly named study. |
| Continue an eligible stopped/failed submission | Inspect diagnostics and remaining original budget. Use same-conversation [recovery](recovery.md) only within its continuation authorization; do not replace the study. |
| Start a distinct question or an explicitly requested new iteration | Explain what differs from existing work and preserve prior IDs/evidence. Prepare a complete finite mandate and a new original `requestId`; do not use this to clear failures, budgets or held-out exposure. |
| Required model, data, criteria or budget are missing | Continue useful inspection and keep submission pending. Identify the exact missing choice/capability; do not invent data, silently switch models or weaken criteria. |

Resolve ordinary wording from the user's goal and existing settings. Ask only for material choices that are absent and have not been delegated. A request to research does not authorize optional portfolio construction, downstream bindings or capital operations.

```sh
quazonai research models
quazonai datasets
quazonai research list
quazonai research get CONVERSATION_ID
quazonai usage
```

## Select actual model, data and limits

- **Model:** select `model.provider` and `model.modelId` from `research models`, following the user's choice or authorized configured preference. Optional `thinkingLevel` and `serviceTier` must be advertised for that model. Discovery reads availability/auth configuration, not model quality; do not run a paid probe or provider fallback to test availability.
- **Dataset:** choose an actual ready entry from `datasets`. Inspect source, currency, coverage and `researchSettings`: instruments, input family, horizon, partitions and strategy. Registration/readiness alone establishes neither licensing/PIT quality nor suitability for the question. State missing or uncertain coverage instead of inventing historical availability.
- **Policy:** inherit registered instrument/input/horizon/partition/execution settings. The structured mandate can select supported search/evaluation behavior; it cannot override native paths, universe, strategy, simulation or split settings. Report a mismatch rather than silently changing the question.
- **Budget:** set the explicitly authorized `maxTrials` (or native `policy.max_trials`) to a nonnegative safe integer. Counted trial attempts, including committed failures, remain in the original ledger. `usage` is observed usage, not permission to spend more. `maxTrials: 0` is not a draft/preview mode; retain a local proposal when execution was not requested.

## Write a complete original request

A file is a complete `ResearchStart` with an explicit original `requestId`. It may contain `input:{goal,model:{provider,modelId},data,policy}` or the configured-dataset form `{requestId,goal,model,datasetId,maxTrials}`. Native input must match a service-registered dataset; never invent server paths or data. Keep exact bytes and IDs for replay. An optional `--request-id` checks the file's ID; it never replaces it.

Prefer the configured-dataset form for normal submissions; the service captures its registered data/policy. Reuse the native `input` form only from an actual supplied contract, not reconstructed storage paths. Both forms support optional `thinkingLevel` and `serviceTier`, alongside `goal` inside the chosen form.

New structured files add `specification:{version:1,name,question,rationale,exclusions?,evaluation,search,capital?}` beside `goal` (or inside native `input`). The question equals `goal`; the short name is the list title. Supported objectives are `pearson_ic` for `single-factor` and `Sharpe Ratio (252 days)` for `ensemble`, with `direction:"maximize"`, optional finite target and `registered-partitions`; minimizing is unsupported and rejected rather than silently changing the mandate. Search binds `maxMembers` and `stop:"budget-or-selection"` to the original counted trial budget. An unselected `research_finish` is rejected while that budget remains; explicit owner stop is separate. Capital assumptions are exact scenario strings, explicit BAR participation and maximum liquidity age. Dataset instrument/input/horizon/partition/execution settings are inherited, never invented browser overrides. Structured native-input requests must match the registered policy, including its strategy/universe/simulation/partition settings; only the explicit trial budget may differ. Factor provenance resolves through the original submission request identity, not equality with another run's scientific input. A missing objective value is insufficient evidence, not a pass. Legacy prose-only files remain replayable. Capital assumptions can also be supplied later through factor capacity assessment or a combination's own `capacityPolicy`, without rerunning research or altering its original input.

`specification.question` must **exactly equal `goal`**. Do not put a different, broader purpose in `goal`; preserve the original user purpose, source, operational interpretation and evidence basis in `rationale` using [the authoring convention](research-authoring.md). Do not add unsupported fields such as `originalGoal`. An invalid structured request must not be retried as legacy prose to bypass its checks.

For `single-factor`, `search.maxMembers` must be `1`; for `ensemble`, it is an explicit positive safe integer compatible with the registered strategy/adapter. It bounds members per candidate, not the number of counted trials. `evaluation.target`, when supplied, is a finite number rather than a string; omit it when no target was specified instead of inventing one.

Optional `capital` uses positive exact decimal-string `scenarios`, decimal-string BAR `participation` greater than zero and at most one, and a positive integer `maximumAgeSeconds`. These are conditional sizing assumptions, not deposits, observed depth or profitability.

This complete configured-dataset JSON illustrates the file format. Its names, question and two-trial budget are examples, not an authorized study or claims about available data. Before submission, replace the request/model/dataset placeholders and adapt every scientific choice and limit to the actual task. If a prerequisite remains unknown, keep the local file pending.

```json
{
  "requestId": "REPLACE_WITH_ONE_UNIQUE_RETAINED_REQUEST_ID",
  "goal": "在登记的 BTC 现货标的和样本期内，决策前已知的价格变化是否对登记预测期限的收益具有稳定的预测信息？",
  "specification": {
    "version": 1,
    "name": "BTC 现货价格变化的预测信息",
    "question": "在登记的 BTC 现货标的和样本期内，决策前已知的价格变化是否对登记预测期限的收益具有稳定的预测信息？",
    "rationale": "原始目标（示例摘要）：判断短期价格变化是否值得继续验证。来源：示例用户请求。操作化解释：在已登记的数据和期限上研究单因子预测信息。机制猜想：短期信息吸收或订单流持续可能产生相关性，当前没有该样本的实证支持。需检查时间错配、共同趋势、样本挑选和费用影响；预测相关性不等于可交易收益。",
    "exclusions": "本轮不构建组合，不启动账户观察，不声称模拟或实盘盈利。",
    "evaluation": {
      "metric": "pearson_ic",
      "direction": "maximize",
      "scheme": "registered-partitions"
    },
    "search": {
      "scope": "single-factor",
      "maxMembers": 1,
      "stop": "budget-or-selection"
    }
  },
  "model": {
    "provider": "REPLACE_WITH_CONFIGURED_PROVIDER",
    "modelId": "REPLACE_WITH_AVAILABLE_MODEL_ID"
  },
  "datasetId": "REPLACE_WITH_MATCHING_REGISTERED_DATASET_ID",
  "maxTrials": 2
}
```

Save the adapted file as `original-request.json` in an authorized workspace. Check the current schema, question/goal equality, supported objective, actual references, original budget and user intent before submitting:

```sh
quazonai schema show ResearchStart
quazonai research start --file original-request.json
```

There is no research `--preview` or dry-run flag. `schema show` is offline contract discovery, not request-file validation or proof of server readiness. Do not combine `--file` with goal/dataset/model/trial flags. Keep the original file unchanged for replay and retain the returned numeric `conversationId` and `submissionId`, then read `research get CONVERSATION_ID` to check the captured mandate and observed state.

For a legacy prose objective with an already configured dataset, explicit flags are sufficient:

```sh
quazonai research start --goal 'THE_REQUESTED_OBJECTIVE' \
  --dataset DATASET_ID --model PROVIDER:MODEL_ID \
  --max-trials REQUESTED_LIMIT --request-id ORIGINAL_REQUEST_ID
```

These flags do not express a structured specification. Use the file form for new structured research, and preserve the original form/body when replaying an existing operation. Do not rewrite an old request to attach a specification under the same ID.

## Execute within the original mandate

Use model IDs from actual configured availability. Discovery does not run inference, discover Desktop credentials or establish model quality. The host explicitly persists its capability-supported thinking level; hidden reasoning is not evidence to report.

Retain the original policy, budget, source identity and selection/evaluation criteria. Zero trials allows no computation. Committed failures consume attempts and remain disclosed. Missing inputs or unsupported criteria are not permission to change budgets or create another study until something passes.

## Literature during research

The owner can specify `publicLiteratureQueries` in the original request: place it inside native `input`, or beside `goal` in the configured-dataset form. It is an optional list of 1–3 nonblank public topics, each at most 500 characters with no control characters. In Web, fill “公开论文检索主题（可选）” one topic per line. Use only the owner's intended public topics; do not derive external queries from private study text or add topics merely to enable search. The system does not determine whether entered text is sensitive.

The installed research Agent selects only a `queryIndex` from that frozen list and automatically searches Crossref, arXiv and Semantic Scholar through its bound tool. The trusted tool supplies the query; the model cannot submit arbitrary search text. It can record supporting, contrary or background claims against saved papers. Providers return metadata and available abstracts, not proof that full text was read. No additional paper-provider key is required; the service host still needs access to the public APIs.

Use `research get CONVERSATION_ID` or the research detail's literature tab to inspect saved sources, links, notes, limitations and source failures. Repeated reads reuse the original facts and do not trigger a new search. The CLI has no separate paper-search command; do not invent one or switch credentials to bypass a missing capability.

The bound Agent search tool normally reuses the latest saved receipt for the same normalized query. It can explicitly retry a `partial` or `unavailable` result with `retryFailed:true` in a new tool task, preserving the old failure. An original task replay returns its original receipt; successful results, including empty ones, are not searched again by that flag.

With no configured topics, `public_queries_not_configured` and an empty source list report that no external search capability was configured. The study can continue scientifically; old studies and their receipts remain readable. Do not misreport this as a source outage or quietly expand the original input to enable retrieval. Distinguish that state from no search, no results, partial results and unavailable providers. Limits, absent abstracts and access errors are research limitations, not evidence that literature does not exist. Report only metadata and excerpts actually returned, keep the paper separate from the Agent's interpretation, and never present a citation as independent evaluation, verified PIT or proven profitability.

## Follow the original study

Use `research get` to inspect all trials, failures, comparison and final/stop reason. Native computation is not a qualification. Final held-out assessment is separate from iterative feedback; never tune against it or rename/copy/fork exposed data to call it unseen. The model-code adapter preserves explicit scientific fuel budgets; omitted/null fuel stays unmetered. QZ imposes no process deadline or memory ceiling. Owned child recovery/cancellation and the provider-credential-free execution environment remain.

Keep trial hypotheses, concrete failure modes, parameters, the comparison set, selection and original criteria traceable. Failed or rejected trials remain evidence; do not hide them behind the best result. Interpret the returned criterion precisely: `computed` without a target means the metric is available, not that a threshold was met; `met` / `not_met` compare the actual supported held-out metric with the original target; `insufficient` means the required value or assessment is unavailable. Preserve actual failures rather than fabricating a score when the metric is missing.

## Recover and hand off

After an unknown start response, replay only the same original file or identical explicit flags with the same ID. HTTP 202 means admitted/replayed, not finished. Follow [observation](runs.md) and [recovery](recovery.md), without a replacement key.

Return **original purpose → request/conversation/submission IDs → captured dataset/model/criteria/budget → observed state → evidence and limitations → next authorized action**. State whether the original question is answered, partly answered, rejected or unresolved. Include trial/factor IDs and the actual held-out criterion where available. A negative or incomplete result is valid; do not continue until something passes or start optional consumers without the user's scope.
