# 账户与资金

## 创建和读取账户观测

先读取目标组合、`observation list` 和 `schema show ObservationStart`。观测需要已有的 `connectionId`，模式必须与连接匹配。可从既有观测读取连接身份；不存在时由用户提供或在实例界面配置，CLI 当前没有连接创建/列举命令。

形成请求：

```json
{
  "requestId": "REPLACE_NEW_UNIQUE_REQUEST_ID",
  "portfolioId": "REPLACE_PORTFOLIO_ID",
  "mode": "paper",
  "connectionId": "REPLACE_EXISTING_CONNECTION_ID"
}
```

示例使用 paper；仅按实际用户目标选择 paper/live，不把切换模式当作普通故障恢复。

```sh
quazonai observation start --file observation-request.json
quazonai observation get "$observation_id"
quazonai observation refresh "$observation_id"
```

读取 `status`、`updatedAt`、`positions`、`equity`、`cash`、`receipt`、`error` 及 `provenance`。保留 client/account/sequence/nativeClock 身份和时间依据，`null` 数值不补零。`observation stop ID` 停止观测，不是下单、清仓或撤销券商账户。

## 自动配置

用户要求自动配置时先读 `AutomaticPolicy`。完整请求示例：

```json
{"enabled":true,"cashBuffer":"1000","maximumSnapshotAgeSeconds":60}
```

将现金缓冲和有效期替换为用户所需值。通过 `observation automatic ID --file POLICY.json` 设置，然后读回 `automaticPolicy`、`allocation.status`、`allocation.reason`、targets、controlRevision 与 receipt。关闭时提交完整策略并设置 `enabled:false`，保留当前其余字段；不要假设关闭等于已清仓。

该请求不要求 requestId，不添加统一外壳；配置可能影响后续目标交付，仅在相应操作范围内执行。

## 资金退出：状态 → 预览 → 命令 → 对账

1. 运行 `capital status OBSERVATION_ID`。先检查 `capability`、`reason`、`controlRevision`、`exits` 与 `commands`。`UNSUPPORTED` / `BLOCKED` 时停止该操作并报告原因。
2. 将用户指定金额和观测币种保存为预览请求，使用新的 requestId：

```json
{"requestId":"REPLACE_PREVIEW_REQUEST_ID","amount":"1000","currency":"USD"}
```

省略 policy 时服务使用 `CASH_ONLY`。不要为了完成提现擅自改成减仓策略。金额和币种是示例，替换成用户实际值；更复杂的 policy 必须来自已有合法请求或明确的下游合同。

3. 执行 `capital preview OBSERVATION_ID --file preview-request.json`，读取 `id`、`capability`、`valid_until`、`plan_artifact_id`、预览资金、拟撤销项和减仓腿。预览不会自动成为退出命令。
4. 在用户已授权执行的范围内，使用预览 ID 构造独立的 START 请求：

```json
{"requestId":"REPLACE_COMMAND_REQUEST_ID","action":"START","previewId":"REPLACE_PREVIEW_ID"}
```

5. 执行 `capital command OBSERVATION_ID --file capital-command.json`，再用 `capital status OBSERVATION_ID` 查询同一 requestId 的状态。`PENDING`、`UNCERTAIN`、`ACCEPTED`、`REJECTED` 分别保留原意；`ACCEPTED` 不是已提现。

预览过期或账户修订变化时，先读取状态，按新操作重新预览。未知结果的原命令仍保留原文件和 requestId，不把它改指向新预览。

## 恢复与外部提款对账

非 START 操作绑定实际 `exitId` 和从 `capital status` 返回的 `exit.revision`。这里的 `expectedRevision` 不来自 PM mandate，也不来自账户 controlRevision。

- `RESUME`：提交 `{requestId,action:"RESUME",exitId,expectedRevision,previewId}`，使用有效的预览。
- `RECONCILE_WITHDRAWAL`：仅在用户提供真实外部转账事实时提交 `{requestId,action:"RECONCILE_WITHDRAWAL",exitId,expectedRevision,amount,currency,externalTransferRef}`。金额是字符串，externalTransferRef 使用真实转账引用。
- 其他下游动作仅按已确认的动作合同构造，不从名称猜出 `withdraw`、`sell` 或 `cancel` CLI 子命令。

分别报告 requested、reserved、released cash、verified withdrawable 与 reconciled withdrawal；缺少可提现或已转账证据时，不能依据一个成功退出码宣称资金已到账。
