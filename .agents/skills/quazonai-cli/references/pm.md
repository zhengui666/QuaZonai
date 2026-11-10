# PM 操作

## 创建与查看 mandate

先运行 `pm capabilities`、`datasets`、`research models` 和 `schema show PmMandateStart`。选择与目标相符的真实能力、数据和模型。

研究情景的请求形状：

```json
{
  "requestId": "REPLACE_NEW_UNIQUE_REQUEST_ID",
  "goal": "REPLACE_USER_GOAL",
  "datasetId": "REPLACE_DATASET_ID",
  "funding": {"kind":"RESEARCH_SCENARIO","capital":{"amount":"10000","currency":"USD"}},
  "model": {"provider":"REPLACE_PROVIDER","modelId":"REPLACE_MODEL_ID"},
  "operatingAuthority": "RESEARCH_ONLY"
}
```

替换示例资本与币种，币种应与数据集一致。账户资金使用 `funding:{"kind":"ACCOUNT","accountBindingId":"实际 observation ID"}`，从 `observation list/get` 确认账户绑定。不要把 connection ID 放到 accountBindingId。

`operatingAuthority` 支持 `RESEARCH_ONLY`、`SHADOW_TARGETS`、`AUTHORIZED_TARGETS`；未指定时服务默认研究。只按用户实际授权选择，不能为了让交付成功而提升权限。`AUTHORIZED_TARGETS` 的运行路径可能自动交付后续符合条件的目标，不只是给当前研究加标签。

可选 `businessPreference` 为 `NET_RETURN`、`DOWNSIDE_FIRST`、`FRONTIER`。保留用户尚未明确的取舍，不自行补成收益/风险保证。

```sh
quazonai pm start --file mandate-request.json
quazonai pm get "$mandate_id"
quazonai pm decision "$mandate_id"
```

从返回的 `mandate.id` 取 ID。完整视图包括 mandate、currentDecision、lastCycle、campaigns、strategies、assessments、capacityEstimates、gaps、targetCommands 和 retirements。当前无 `pm watch/wait`；用 `get`、`decision`、`history`、`events` 有界观察。

## 修订绑定的命令

每次新操作先读取 `pm get ID`，把 `mandate.revision` 原样放进 `expectedRevision` 字符串。不要使用 activePolicy、campaign、strategy 或 decision 自己的 revision。

普通重算或刷新请求：

```json
{"requestId":"REPLACE_NEW_UNIQUE_REQUEST_ID","expectedRevision":"REPLACE_MANDATE_REVISION"}
```

选择一个符合用户目标的命令，不把以下命令全部顺序执行：

| 目的 | 命令 | 后续检查 |
| --- | --- | --- |
| 重新计算 | `pm recompute ID --file COMMAND.json` | lastCycle、currentDecision、gaps |
| 捕获已有来源的新修订 | `pm refresh-data ID --file COMMAND.json` | datasetRevisions；不是导入或下载数据的命令 |
| 结束策略运行 | `pm retire ID --file COMMAND.json` | retirements、残余持仓与未完成目标；可用 `strategyVersionId` 明确版本 |
| 澄清目标 | `pm clarify-intent ID --file CLARIFICATION.json` | ownerBoundaries、unresolvedIntent、最新修订 |

澄清请求读取 `PmIntentClarification`，在通用字段外提供 `statement`、`boundaries:[{kind,sourceStatement,value}]` 和 `resolvedAmbiguities`。保留用户原话，明确的边界才结构化；澄清不删除之前的约束或重新授权已使用的证据。

retire 不是清仓。报告 `positionDisposition`、`outstandingTargetCommandIds` 和 `reconciliationRequired`，不宣称账户已平仓。

## Campaign 与 Strategy

从 `pm get` 取得 campaign、strategy version、cycle 和 capacity estimate 的真实 ID：

```sh
quazonai campaign get "$campaign_id"
quazonai campaign trials "$campaign_id"
quazonai campaign compare "$campaign_id"
quazonai strategy versions "$strategy_id"
quazonai strategy assessments "$strategy_id"
quazonai pm cycle "$cycle_id"
quazonai pm capacity "$capacity_estimate_id"
```

`campaign compare` 可能返回 `null`；保留尚无比较结论的状态。暂停、恢复或结束 campaign 时使用 `campaign pause/resume/close ID --file COMMAND.json`。暂停或恢复策略时使用 `strategy pause/resume CURRENT_VERSION_ID --file COMMAND.json`。所有这些请求的修订号仍来自所属 `mandate.revision`；修改命令返回的可能是完整 PM 视图，之后再读目标 campaign/strategy 状态。

`campaign start` 请求为 `{requestId,mandateId,expectedRevision}`。服务要求前一 campaign 已 CONCLUDED 或 PAUSED，且存在新数据修订；遇到 `campaign_already_active` 或 `new_campaign_requires_new_data_revision` 时报告真实前置条件，不重复提交。不要先暂停、刷新或新建 campaign 来掩盖原研究尚未结束。

## 交付指定决策的目标

仅在账户绑定与用户授权满足时操作。先读最新 mandate 和 decision，核对状态、有效期、账户及是否已有 targetCommandId。请求必须包含指定 decision ID：

```json
{"requestId":"REPLACE_NEW_UNIQUE_REQUEST_ID","expectedRevision":"REPLACE_MANDATE_REVISION","decisionId":"REPLACE_CURRENT_DECISION_ID"}
```

执行 `pm targets MANDATE_ID --file targets-command.json`，然后读 `pm get MANDATE_ID` 的 `targetCommands` 与 decision。不要在文件里自行重写 targets，也不要把 mandate ID 当成 decision ID。服务可能拒绝过期、重复、未绑定账户或不符合执行条件的目标；保留拒绝原因。交付回执与真实成交分别报告。
