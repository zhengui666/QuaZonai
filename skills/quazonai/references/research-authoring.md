# Author a research specification

Read this before creating or materially revising research. Use [research operations](research.md) for exact commands, request examples and submission. These are agent writing standards, not extra API fields or a new approval process. Write user-facing content in the user's language; preserve native field names and enum values.

## Map intent to current fields

For the configured-dataset form of `ResearchStart`, `goal` and `specification` are top-level fields. For native input they are inside `input`. The rules below apply to both forms.

| Meaning | Native location | What the agent writes |
| --- | --- | --- |
| Research name | `specification.name` | A short, single-line subject and comparison, at most 120 characters as checked by the client. Avoid “new research”, “test” or a promised profit. |
| Research question / 研究问题 | `specification.question` and `goal` | The same nonblank string in both places: the testable question this finite study must answer. Unequal values are rejected. |
| Basis / 依据 | `specification.rationale` | Nonblank mechanism, actual motivating observations/sources, assumptions, counter-evidence and limitations. Preserve the original-goal narrative here using the convention below. |
| Original user objective / 原始目标 | Labeled text within `specification.rationale`, with the original request retained | No separate `original_goal` field exists. Preserve user words or a labeled faithful summary separately from the agent's formal question and interpretation. Do not make `goal` differ from `question` to hold this broader narrative. |
| Exclusions | `specification.exclusions` | Explicit boundaries: instruments/periods or approaches excluded, claims not supported, and optional consumers outside this request. |
| Acceptance and search | `specification.evaluation` and `specification.search` | Supported metric/direction, any authorized finite target, registered partitions, search scope/member limit and stopping rule. Prose cannot replace these fields. |
| Data, model and trial budget | `datasetId`, `model`, `maxTrials`, or native `input.data`, `input.model`, `input.policy` | Real configured selections and the authorized limit. Registered settings supply instruments, horizon and partitions. Do not invent native paths or override the dataset to fit an attractive story. |

Use `specification.version: 1`. Do not add unsupported keys such as `original_goal`, `sources`, `hypothesis` or `success_criteria` to the structured specification; the hypothesis and sources belong in the supported prose. Legacy prose-only requests remain replayable; do not rewrite them merely to adopt this convention. New studies should use the structured form when the requested objective is supported.

## Preserve the original goal

Recover the user's words and any relevant existing study before writing. Distinguish quotation, summary, verified fact and agent interpretation. Never invent a quote, citation, budget or threshold. The original goal explains **why the research exists and what decision it should inform**; “run the CLI” is not a substitute for a user's request to evaluate a signal.

Use this text structure within `rationale`; these labels are prose, not additional JSON keys:

```text
原始目标（用户原话或标明“摘要”）：…
来源：实际请求、文档定位或既有研究 ID；不编造链接/日期。
决策与交付物：本研究将帮助决定…；本轮交付到…。
Agent 的操作化解释：将原始目标具体化为 question/goal 中的可检验问题；理由是…。
机制假设：为什么该信息可能尚未被价格吸收，或代表某种风险/行为机制。
已知依据：真实来源、时间/版本、对应观察；支持什么，不支持什么。
待检验假设与替代解释：…；什么结果会否定或削弱机制。
适用边界与未决事项：数据覆盖/PIT、期限、成本、流动性及其他限制。
前序与变更（如有）：真实研究 ID、改变内容及原因，保留原有负面结果。
```

Keep the original-goal narrative stable through the study. A changed user objective is a traceable new request, not an overwrite of admitted history. Preserve the original file and request identity; write a local successor proposal explaining the change before any new submission. Do not copy secrets, private transcripts or unrelated context into the rationale.

## Write a falsifiable question

Use this pattern, then set `goal` to the exact same text:

```text
在 [已登记总体、频率、样本边界与决策时可用信息] 中，
[信号/机制/方法] 对 [目标与期限] 是否提供 [可衡量效果或相对明确基准的增益]，
并在 [登记的成本/执行约束与评估规则] 下满足 [本轮判断标准]？
```

- One main decision per study. Separate unrelated questions while explaining their relationship to the original goal; do not bundle incompatible targets, windows or comparison rules into one question.
- Distinguish information availability, event time, prediction horizon and eventual label. Later metadata or hindsight-selected instruments cannot become contemporaneous features. Describe the actual registered horizon; a market's expiry is not automatically a forecast window.
- Align the question with the supported objective. `single-factor` uses `pearson_ic` and `maxMembers: 1`; `ensemble` uses `Sharpe Ratio (252 days)` and an explicit compatible member limit. Both use `direction: "maximize"`, `scheme: "registered-partitions"` and `stop: "budget-or-selection"`.
- Choose a numerical `target` only when supplied or delegated and finite. Omitted target means computed evidence, not an implicit pass. Keep any broader user aspiration distinct from a criterion the service can evaluate. An IC objective does not establish trading profitability.
- State a benchmark only if the configured study can actually assess that comparison; the specification has no arbitrary benchmark field. If the requested baseline, horizon, metric or minimization objective is unsupported, name the gap and keep the dependent submission pending. Do not silently replace it with an easier objective or fall back to prose to evade validation.
- Separate iterative validation feedback from the final independent assessment. Fix the criteria before seeing held-out results. A failed criterion can reject a candidate; missing coverage or a missing objective value can leave the question unresolved.

Avoid “find a good strategy”, “use AI to predict BTC”, “maximize profits” or “prove this works”. They do not specify a checkable question. A useful question permits a negative answer.

## Write an honest basis

Prefer primary sources, recorded dataset metadata and actual study/trial/factor IDs. For a paper or external observation, provide a real locator and the specific relevant claim. For previous evidence, identify its data window, costs and whether it was iterative validation or independently assessed. A citation or a real-data label alone proves neither PIT suitability nor effectiveness in this study.

When the only basis is an idea, say “机制猜想，尚无该市场的实证支持”. This can motivate an authorized finite exploratory study. Do not write “已验证” without evidence, invent confidence percentages, or borrow a result from another period/horizon/cost model. Include competing explanations such as common-factor exposure, time leakage, selection bias, sparse observations and omitted costs, together with the evidence that would distinguish them.

## Coherent writing example

The following is illustrative, not a claim about available datasets or an authorized computation. Suppose the user asks: “研究 BTC 现货的短期变化是否对下一时段收益有预测信息，先判断是否值得进一步验证。” After inspecting a compatible registered dataset and its horizon, the agent could write:

**Name:** `BTC 短期价格变化的单因子预测信息`

**Question and identical goal:** “在选定已登记 BTC 数据集及其固定预测期限内，仅使用决策时刻已可得的短期价格变化构造单因子，其收益预测 Pearson IC 在登记分区的独立评估中表现如何，是否有足够证据支持进一步验证？”

**Rationale:**

```text
原始目标（用户原话）：研究 BTC 现货的短期变化是否对下一时段收益有预测信息，先判断是否值得进一步验证。
来源：本次用户请求。
决策与交付物：决定是否继续投入；交付全部试验、独立评估值及覆盖/失效限制。
Agent 的操作化解释：使用已登记预测期限和单因子 Pearson IC 衡量预测信息；不把它等同于净收益。
机制假设：短期信息传播可能产生持续性，但也可能因微观结构噪声产生反转。
已知依据：目前只有用户提出的机制方向，没有本样本上的实证支持。
反证与替代解释：独立样本无预测信息会削弱该假设；时钟错配、样本选择或价格噪声也可能造成伪相关。
适用边界：结论仅对应实际登记数据、期限及分区；费用后可交易性不由 IC 单独回答。
```

**Structured choices:** `single-factor`, `maxMembers: 1`, `pearson_ic`, `maximize`, `registered-partitions`, `budget-or-selection`. With no authorized numeric target, omit `target` and report the actual evidence as computed; do not claim a preset threshold was met. Use real dataset/model IDs and the separately authorized trial budget before submission. Replace generic dataset/horizon prose with the actual observed settings when preparing the real request.

| Weak entry | Problem | Correction |
| --- | --- | --- |
| Original goal: “跑通 CLI 和回测” when the user asked if a signal is useful | Substitutes tool execution for the user's decision | Preserve the user's goal in the rationale and put CLI work in the operational plan. |
| Question: “找到年化 100% 的策略” without a supplied threshold | Invents a target and encourages selection bias | Use a supported, bounded evaluation; record an aspiration separately only if actually supplied. |
| Basis: “真实数据，所以有效” | Confuses provenance with predictive evidence | Explain data suitability separately from the untested mechanism. |
| Basis: “Sharpe 很高，已验证” without inputs/costs/partition | Cannot be checked and may be in-sample evidence | Cite the actual study/trial and recorded independent result with limits. |
| `goal` holds a broad quote while `question` holds a different sentence | Violates the current request contract | Keep `goal === question`; preserve the quote and interpretation in `rationale`. |
| Failed result leads to a lower target or a fresh ID retry | Changes the comparison or repeats uncertain work | Retain the failure, reconcile the original operation and describe any authorized successor separately. |

## Quality checks and handoff

**Before preparing the file:** another agent should understand the user intent, formal question, rationale, sources, exclusions, intended evidence and unresolved choices without the chat. Missing scientific decisions or registered selections remain visible in a local draft; do not submit invented IDs or budgets. There is no remote Project/Brief draft command on this CLI, and `research start` can begin computation.

**Before submitting:** verify exact `goal`/`question` equality, supported metric/scope/member combinations, actual data/model selections, compatible registered partitions/horizon/strategy, explicit trial budget and requested endpoint. Optional capital assumptions belong only to a requested capacity question; they do not authorize portfolio creation or account controls. Use [research operations](research.md) and [connection](connection.md) for native discovery and request handling.

**At handoff/completion:** retain the original request file/ID, Conversation/Submission IDs, recorded input, all trial results, selected factor/independent evidence, criterion status and limitations. State whether the original goal is answered, partly answered, rejected or unresolved. Distinguish proposed follow-up from already submitted work; do not rewrite a study after observing its result.
