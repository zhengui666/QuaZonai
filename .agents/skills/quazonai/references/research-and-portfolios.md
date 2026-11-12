# 原生研究、参数扫描与证据评级

## 自然语言研究

先读取 `quazonai datasets`、`quazonai research models` 和 `quazonai schema show ResearchStart`。使用真实已就绪数据集和模型，不编造目录、数据覆盖或费率。

```sh
quazonai research start --goal "$goal" --dataset "$dataset_id" --model "$provider:$model_id" --request-id "$request_id"
quazonai research get "$research_id"
quazonai research diagnostics "$research_id"
quazonai research wait "$research_id" --timeout 60
```

`research_id` 为受理回执的 conversationId。复杂请求使用 `--file`，不与简单参数混用。模型的推理等级和服务档位取自实际列表。公开文献查询仅使用用户授权外发的内容。

研究 Agent 在同一 Pi Conversation 内配置并运行已注册的官方 Rust 策略。当前回测接线为 EMA_CROSS（QuoteTick）及 ORDER_BOOK_IMBALANCE（L2 订单簿）；参数及可选 base 配置按实际原生 schema 原样传递。不是任意 Python Strategy、用户源码动态运行、所有交易所或全部 Nautilus 策略的通用入口。不要用已退役的因子、IC、WASM 拟合或组合评分工具替代。

先看真实数据类型及覆盖。数据准备不等于策略获利；缺失或未预热统计保留为空，官方统计名、币种和单位保留原意。回测不启动账户，也不把历史目标点当实时策略。

## 同一研究继续和恢复

保存 conversationId 与 submissionId。`watch`/`wait` 超时只退出客户端，不停止远端任务。需要停止时使用 `research stop ID` 并读回。通过 `diagnostics` 确认终止 submission 后：

```sh
quazonai research resume "$research_id" --submission-id "$terminal_submission_id" --instruction "$reply"
```

用 instruction 回答追问、修改研究方向或在已结束/停止的研究中继续；仍在运行的任务不要重建。保留新回执身份。Web 研究页显示同一 Pi 消息并支持回复，CLI 没有另一个聊天发送命令。

## 参数扫描

先请求来源的实际选项，将 `{ "sourceKind":"research", "researchId":实际整数ID }` 保存为 query.json：

```sh
quazonai parameter-scan options --file query.json
quazonai schema show ParameterScanStart
quazonai parameter-scan preview --file scan.json
quazonai parameter-scan create --file scan.json
quazonai parameter-scan wait "$scan_id" --timeout 60
quazonai parameter-scan get "$scan_id"
quazonai parameter-scan trials "$scan_id"
```

scan.json 按合同提供 requestId、name、source={kind:"research",researchId}、mode:"STRATEGY"、axes、objective、execution={concurrency}。axis.key 仅使用 options 返回的现有数值路径；domain 使用 values 字符串数组或 range 的 start/end/step 字符串。不是新策略 DSL，非扫描字段继续沿用原请求。

objective 使用真实官方 native_key、partition:"FULL_SELECTION" 和用户选择的 maximize/minimize。候选统计列表只是建议，不按指标名字猜方向或改变数值符号。当前合同没有 batchSize。读回缺失原因、失败、未决和 comparable 数量，不把缺失值补零。排名标记 UNQUALIFIED_EXPLORATORY，不是样本外证明，也不是实盘许可。

## 防过拟合与 AI 综合意见

通过研究 Agent 请求逻辑预评估、明确候选/方法的 walk-forward、邻近参数及成本敏感性，然后读取实际研究结果。它们是研究工具，不是独立 CLI 子命令。原生回测和统计由 Nautilus 执行；逻辑分析与综合星级是模型判断，不是 Nautilus 官方评分。

逻辑预评估应先于对应数值运行，事先说明候选、窗口和优化目标。不得将全区间敏感性当新的样本外测试，不得在看完结果后声称事前注册。检查任务来源、运行状态、窗口、费用、缺失证据和限制；不能只凭任务 COMPLETED 或星级宣称验证通过。证据不足时保持星级不可用，AI 星级不表示获利概率、独立审计或交易授权。

## 既有记录

`portfolio list/get` 保留历史只读结果。CLI 不再提供新建因子、组合 build 或容量评分入口。现有 `portfolio replay-ticks` 只按实际 schema 和真实来源操作，不把它当作通用原生策略部署。
