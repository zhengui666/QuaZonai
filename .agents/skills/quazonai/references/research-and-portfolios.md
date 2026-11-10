# 研究、因子与组合

目录：
- [提交研究](#提交研究)
- [跟踪、分页与恢复](#跟踪分页与恢复)
- [选择因子并构建组合](#选择因子并构建组合)
- [容量分析](#容量分析)
- [Tick 经济回放](#tick-经济回放)

## 提交研究

```sh
quazonai datasets
quazonai research models
quazonai schema show ResearchStart
```

选择真实可用数据集和模型。检查数据集的 `ready`、时间范围、币种、`nativeModality`、`preparation`、`inputCoverage` 和 `researchSettings`；已完成数据准备不代表科学证据合格。模型采用返回的 `provider` 与 `modelId`；可选推理等级和服务档位也从模型返回值选取。

最小文件请求如下。将所有 `REPLACE_*` 替换成实际值后保存为 `research-request.json`：

```json
{
  "requestId": "REPLACE_NEW_UNIQUE_REQUEST_ID",
  "goal": "REPLACE_USER_RESEARCH_QUESTION",
  "datasetId": "REPLACE_DATASET_ID",
  "model": {"provider": "REPLACE_PROVIDER", "modelId": "REPLACE_MODEL_ID"}
}
```

```sh
quazonai research start --file research-request.json
```

也可用简单参数形式：

```sh
quazonai research start --goal "$goal" --dataset "$dataset_id" --model "$provider:$model_id" --request-id "$request_id"
```

不要混用 `--file` 与 `--goal` / `--dataset` / `--model`。需要结构化目标时，在文件中添加 `specification`，先读取 `ResearchSpecification`：

- 使用 `version: 1`，提供非空 `name`、`question`、`rationale`；`goal` 必须与 `question` 完全相同。
- `single-factor` 使用 `pearson_ic`、`maxMembers: 1`；`ensemble` 使用 `Sharpe Ratio (252 days)` 或 `Sharpe Ratio (365 days)` 及正整数 `maxMembers`。
- `evaluation.direction` 为 `maximize`，`evaluation.scheme` 为 `registered-partitions`；`search.stop` 为 `selection` 或 `budget-or-selection`。
- `evaluation.target` 是可选有限数值。仅按用户实际要求设定，不把研究愿望变成已实现的收益承诺。
- 通过 `capital` 添加用户指定的容量假设；通过 `thinkingLevel` / `serviceTier` 选择模型实际支持的选项。
- `publicLiteratureQueries` 仅填写用户明确允许外发的 1–3 条检索文本；每条 1–500 字符。不自动把完整私有目标转成公开检索词。

优先使用 `datasetId` 形式。只有已有完整、合法的科学输入时才使用 `{requestId, input:{goal,model,data,policy,...}}` 形式；不要从展示用的 `researchSettings` 反造完整科学请求。

## 跟踪、分页与恢复

保存受理回执的 `conversationId` 与 `submissionId`。使用 conversation ID：

```sh
quazonai research wait "$research_id" --timeout 60
quazonai research get "$research_id"
quazonai research diagnostics "$research_id"
```

`watch` 返回沿途详情和最后总结，`wait` 仅返回最后总结。判断 `settled`、`submissionStatus`、`research.final`、`criterion`、`trialRecords` 和错误；不能凭 `IDLE`、空 tasks 或存在 final 就断言 submission 已结束。结果同时保留指标分区、单位、状态和缺失原因。

`research list` 返回 `items` 与可选 `cursor`。如果有 cursor，将该对象序列化为单个参数继续：`quazonai research list --cursor "$cursor_json"`，直到没有 cursor。不要自行构造页码或游标。

只有用户需要停止时运行 `research stop ID`，随后读回。需要恢复被中断/失败的研究时，先检查 diagnostics 的原终止 submission，再执行：

```sh
quazonai research resume "$research_id" --submission-id "$terminal_submission_id"
```

保存新回执中的 `submissionId`、`previousSubmissionId`，继续观察同一 conversation。不要对仍在运行或只是客户端超时的研究调用 resume。

## 选择因子并构建组合

```sh
quazonai factor list
quazonai factor get "$factor_id"
quazonai schema show PortfolioStart
```

检查 `evidence`、`combinable`、`reason`、数据集、币种、市场、`criterion` 与 review。`historical`、`component_only`、`validation_only` 和探索性 tick 因子不能被描述为已独立验证的可交易策略。

用户已给定组合资本和配置时，形成 `portfolio-request.json`：

```json
{
  "requestId": "REPLACE_NEW_UNIQUE_REQUEST_ID",
  "name": "REPLACE_PORTFOLIO_NAME",
  "datasetId": "REPLACE_DATASET_ID",
  "capital": "10000",
  "factors": [{"factorId": "REPLACE_FACTOR_ID", "weight": "1"}]
}
```

这里的资金和权重仅演示字段类型，替换成任务实际值。需要独立数据评估时使用真实 `independentDatasetId`。也可按合同使用 `researchIds` 与 `weights`，不要把研究 ID 当因子 ID；不自行假定两种选择方式可以混用。

```sh
quazonai portfolio build --file portfolio-request.json
quazonai portfolio wait "$portfolio_id" --timeout 60
quazonai portfolio get "$portfolio_id"
```

从 build 的回执取得 portfolio ID。读取状态、`progress`、指标、曲线、`performance`、review、容量及 `targetExpired`。生成 targets 不表示已经向账户交付。

## 容量分析

读取 `CapitalAssumptions`，把用户指定的情景保存到文件，例如：

```json
{"scenarios":["10000","50000"],"participation":"0.01","maximumAgeSeconds":60}
```

示例数值不作为默认投资假设。scenarios 必须为正十进制字符串；participation 为 `(0,1]` 的十进制字符串；maximumAgeSeconds 为 `1..4294967295` 的整数。

```sh
quazonai factor capacity "$factor_id"
quazonai factor capacity "$factor_id" --file capacity-assumptions.json
quazonai portfolio capacity "$portfolio_id" --file capacity-assumptions.json
```

容量请求不要求 `requestId`；不要强加统一请求外壳。报告 `status`、`scope`、`basis`、有效时钟、约束、覆盖和 `reason`。流动性容量不等于盈利能力；`economicCapacity.status: UNAVAILABLE` 时保持不可用结论。

## Tick 经济回放

```sh
quazonai schema show TickEconomicReplayStart
quazonai schema show NativeTickEconomicRequest
quazonai schema show NativeTickFactorRequest
```

用真实 tick 因子来源及相符的数据修订构造请求。顶层包含 `requestId`、`datasetId`、`request`，并在单个 `factorId` 与 `members:[{factorId,weight}]` 中二选一。按需要提供真实 `fundingCatalog`。

内层 request 包含 `schema_version:1`、完整 `research`、`scenario`、`fx_instrument_ids` 与 `funding_source`；费用、资本、参与率、延迟和纳秒全部保留字符串。不要把缺失成本补零，不把 EVENT_TIME_PROXY_UNVERIFIED 当 RECEIVED_TIME_OBSERVED，也不为获得更好结果改动已有研究分区或来源。需要额外原生结构时，先在 `schema list --domain` 找真实名称再读取。

```sh
quazonai portfolio replay-ticks --file tick-replay-request.json
quazonai portfolio tick-replay "$replay_id"
```

保留回执中的 replay ID 和 taskId，以有限间隔读取同一 replay；当前没有专属 watch/wait。检查 `COMPUTED` / `FAILED` / `STOPPED`、`progress`、`error`，以及结果的 coverage、methods、limitations、资金费缺口、费用与分区指标。结果的 `UNQUALIFIED_EXPLORATORY` 保持为探索性结论。
