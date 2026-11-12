# 账户与资金

## 创建和读取账户观测

先读取 `observation list` 和 `schema show ObservationStart`。观测需要已有的 `connectionId`，模式必须与连接匹配。可从既有观测读取连接身份；不存在时由用户提供或在实例界面配置，CLI 当前没有连接创建/列举命令。

形成请求：

```json
{
  "requestId": "REPLACE_NEW_UNIQUE_REQUEST_ID",
  "purpose": "ACCOUNT_ONLY",
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

## 显式控制已配置的原生策略

```sh
quazonai account-strategy status "$observation_id"
quazonai account-strategy start "$observation_id" --request-id "$request_id" --account-source-id "$account_source_id" --expected-revision "$strategy_revision"
quazonai account-strategy stop "$observation_id" --request-id "$stop_request_id"
quazonai account-strategy receipt "$observation_id" "$request_id"
```

START 的 accountSourceId/strategyRevision 必须来自用户决定启动时已读到的 status。过期拒绝后先报告，不自动获取新修订替旧命令重新授权。STOP 不要求修订；仍须明确用户操作意图。当前账户仅已接入官方 EMA_CROSS；ORDER_BOOK_IMBALANCE、GRID_MARKET_MAKER、HURST_VPIN_DIRECTIONAL 目前只接研究回测，不支持账户运行。连接中的策略配置并非由历史回测自动部署；当前 CLI 不是策略代码上传器或连接配置器。

默认启动宿主不等于启动交易。paper/live 使用独立连接；live START 可能实际下单，必须满足用户明确授权及适用确认要求，不能因回测评级良好自行启动。恢复会重新预热，不承诺无缝恢复策略内部指标状态。STOP 沿用上游配置语义，不保证不撤单或不清仓；观测停止也不是策略 STOP。

命令回执分别报告 REJECTED、UNCERTAIN 与真实原生结果。persisted:false 表示回执未持久化，即使结果显示 STOPPED 也不能称已可靠记账或跨重启防重放。网络不确定时查询原 requestId，不自动重发，不用当前 STATUS 冒充旧命令已完成。取消保护、订单预览和仓位计算按实际能力报告，预览不是下单。

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
