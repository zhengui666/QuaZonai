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

默认启动宿主不等于启动交易。paper/live 使用独立连接；live START 可能实际下单，必须满足用户明确授权及适用确认要求，不能因回测评级良好自行启动。恢复会重新预热，不承诺无缝恢复策略内部指标状态。STOP 沿用上游配置语义，可能撤单或清仓，但 STOPPED 不证明仓位已平、资金可提或提现完成；观测停止也不是策略 STOP。

命令回执分别报告 REJECTED、UNCERTAIN 与真实原生结果。persisted:false 表示回执未持久化，即使结果显示 STOPPED 也不能称已可靠记账或跨重启防重放。网络不确定时查询原 requestId，不自动重发，不用当前 STATUS 冒充旧命令已完成。取消保护、订单预览和仓位计算按实际能力报告，预览不是下单。

## 资金退出：仅查询历史记录

当前版本已退役新资金退出规划与执行入口，不支持“指定金额 → 自动减仓 → 提现”的完整流程。不要执行旧版 `capital preview`、START、RESUME 或 RECONCILE_WITHDRAWAL 教程，也不要调用已删除的预览/启动 HTTP 路由。

```sh
quazonai capital status "$observation_id"
quazonai capital get "$observation_id" "$exit_id"
```

从 status 返回的历史记录取得真实 exitId。读取 `capability`、`reason`、历史退出与命令回执；保留原始金额、币种、预留、订单、requestId 和 UNKNOWN / UNCERTAIN，不把读取成功或历史金额当作当前可提现余额。不要删除历史记录、改写未知结果或重新提交原操作来消除阻塞。

`capital command` 只保留旧记录 PAUSE/CANCEL 请求形状，不代表当前下游能够执行。必须检查实际 capability/actions；当前原生账户适配返回 `capital_original_control_evidence_unavailable` 时明确报告不可用，不发送命令，不将“取消退出计划”等同于撤销交易所订单。其他资金动作被 `capital_exit_planning_retired` 拒绝，不以自写脚本或资金计算绕过。

## 策略停止、退出与外部提现的边界

`account-strategy stop` 是独立的显式原生策略控制，不是资金退出或提现替代命令。是否撤单/平仓取决于已配置的上游策略和已接入能力；只据真实订单、持仓与原命令回执报告结果，不把 STOPPED 当作已清仓或已转账。

当前 CLI 不提供自动外部划转或提现。外部操作由用户在平台/钱包完成；当前版本也没有可执行的 RECONCILE_WITHDRAWAL 命令。历史外部引用仅是历史记录，不代表新转账发生。缺少平台流水或到账证据时，明确“未确认”，不宣称已提现；不从命令名、账户余额或成功退出码推断资金到账。
