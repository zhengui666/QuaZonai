# 原生研究、历史扫描与证据解读

## 自然语言研究

先读取 `quazonai datasets`、`quazonai research models` 和 `quazonai schema show ResearchStart`。使用真实已就绪数据集和模型，不编造目录、数据覆盖或费率。

```sh
quazonai research start --goal "$goal" --dataset "$dataset_id" --model "$provider:$model_id" --request-id "$request_id"
quazonai research get "$research_id"
quazonai research diagnostics "$research_id"
quazonai research wait "$research_id" --timeout 60
```

`research_id` 为受理回执的 conversationId。复杂请求使用 `--file`，不与简单参数混用。模型的推理等级和服务档位取自实际列表。公开文献查询仅使用用户授权外发的内容。

研究 Agent 在同一 Pi Conversation 内配置并运行已注册的官方 Rust 策略。当前单标的回测接线为 EMA_CROSS、GRID_MARKET_MAKER（QuoteTick），ORDER_BOOK_IMBALANCE（L2 订单簿）及 HURST_VPIN_DIRECTIONAL（官方 BAR/QuoteTick/TradeTick 订阅）；参数及可选 base 配置按实际原生 schema 原样传递。不是任意 Python Strategy、用户源码动态运行、所有交易所或全部 Nautilus 策略的通用入口。不要用已退役的因子、IC、WASM 拟合或组合评分工具替代。

Hurst/VPIN 的 INTERNAL BAR 由官方 DataEngine 根据原 TradeTick 聚合；EXTERNAL BAR 必须使用原完整 BarType。外部非时间 BAR 若只供策略读取，应明确配置原生 venue.bar_execution=false，并提供用于撮合的原 Quote/Trade 数据；省略保持官方默认 true，不由胶水静默关闭。当前账户仍仅支持 EMA_CROSS，研究注册这些策略不扩大账户执行能力。

先看真实数据类型及覆盖。数据准备不等于策略获利；缺失或未预热统计保留为空，官方统计名、币种和单位保留原意。回测不启动账户，也不把历史目标点当实时策略。

## 同一研究继续和恢复

保存 conversationId 与 submissionId。`watch`/`wait` 超时只退出客户端，不停止远端任务。需要停止时使用 `research stop ID` 并读回。通过 `diagnostics` 确认终止 submission 后：

```sh
quazonai research resume "$research_id" --submission-id "$terminal_submission_id" --instruction "$reply"
```

用 instruction 回答追问、修改研究方向或在已结束/停止的研究中继续；仍在运行的任务不要重建。保留新回执身份。Web 研究页显示同一 Pi 消息并支持回复，CLI 没有另一个聊天发送命令。

## 历史参数扫描

参数生成、新建扫描、排名与 surface 已退役。CLI 仅保留已有扫描的 list/get/trials/watch/wait 和显式 stop；不要调用 options、preview、create 或按旧 ParameterScanStart 类型构造新请求。旧类型及记录存在不表示仍可执行。

```sh
quazonai parameter-scan list
quazonai parameter-scan get "$scan_id"
quazonai parameter-scan trials "$scan_id"
quazonai parameter-scan wait "$scan_id" --timeout 60
```

仅使用已有扫描 ID；停止须获用户授权，随后读回原状态。保留原请求、失败、STOPPED 和 UNKNOWN，不重新生成参数、重新排名或重发未决执行。历史排名不是样本外证明或实盘许可。新研究使用显式配置的原生 Rust 策略回测；先核实 runner、数据目录和实际可用工具，不以旧扫描替代。

## 官方权益统计与收益口径

查看 `equity_performance` 的真实 source、status/reason、币种、快照覆盖、收益日数与频率。它直接读取 Nautilus 官方 portfolio_returns，并调用官方 CAGR/Sharpe 的 252 与 365 两种频率；没有权益证据时保持不可用，不退回持仓收益冒充权益。canonical 默认统计可能使用持仓收益，不能按名称视为日权益收益。官方跨日延续也不等于每天实际观测，短样本年化不是已实现年收益。

解读已有 WF 结果时，保留原 `metric_source`、`annualization_days` 与 `native_key`，不静默改变原 canonical 语义。历史字段不是当前可创建 WF 的接口承诺。股票常用频率与全天候市场频率只是计算假设，必须与用户目的及报告口径一致。

## 防过拟合与 AI 综合意见

可向研究 Agent 提出逻辑预评估、明确候选/方法的 walk-forward、邻近参数及成本敏感性要求；这些是研究方法要求，不保证当前实例提供自动执行工具。先核实该研究实际暴露的工具及 runner 能力，缺失时明确报告，不能调用已退役扫描或把保留的插件源码当可用能力。原生回测和统计由 Nautilus 执行；逻辑分析与综合星级是模型判断，不是 Nautilus 官方评分。

逻辑预评估应先于对应数值运行，事先说明候选、窗口和优化目标。不得将全区间敏感性当新的样本外测试，不得在看完结果后声称事前注册。检查任务来源、运行状态、窗口、费用、缺失证据和限制；不能只凭任务 COMPLETED 或星级宣称验证通过。证据不足时保持星级不可用，AI 星级不表示获利概率、独立审计或交易授权。

## 既有记录

`portfolio list/get` 保留历史只读结果。CLI 不再提供新建因子、组合 build 或容量评分入口。旧 `portfolio replay-ticks` / `portfolio tick-replay` 的服务路由已移除，不可用于新建或查询回放；使用同一 `research` 的官方原生策略入口，不尝试重新启用旧因子执行链。

## 显式授权的实现源码评审

源码评审不自动扫描本机、下载 URL 或读取凭据。所有者可经已认证的 `POST /api/v3/research/{conversationId}/implementation-sources` 提供准备好的源码文本：

```json
{"requestId":"source-material-1","shareWithResearchModel":true,"model":{"provider":"实际提供方","modelId":"当前研究模型"},"files":[{"id":"strategy","name":"strategy.rs","revision":"所有者声明的版本","content":"经过确认可以交给该模型的源码文本"}]}
```

这明确授权把这些文本交给此研究的指定模型；不要包含密码、密钥、私有账户资料或未授权文件。回复只返回清单，不启动评审/编译/回测/交易。相同 requestId 的原材料不能被替换；材料绑定当时的原生研究请求和模型，修改配置需要重新登记相应材料。没有独立 CLI 上传命令，不要编造命令。

随后让原研究通过 `native_strategy_logic_review` 的 `sourceRequestId` 选择这份材料。Pi 子会话按 `native_assessment_read_source` 分页读取，报告实际读取的文件及 UTF-16 字元范围。未提供的依赖、未读区间、源码与构建产物的对应关系均保持未知；用户给出的版本标签不是构建证明。`SCOPED_SOURCE_LOGIC_REVIEW` 仅表示读过授权片段，不代表全部实现已审查。缺少材料仍是配置逻辑评价。旧 Task、报告和原始请求保留，不重新计算或授权交易。

## 新数据集与单次研究的独立 runner

需要额外的可信 Rust runner 时，先 `native-runner status` 读取当前 revision，再用 `native-runner import --file` 提交一次原请求，随后 `native-runner build-status REQUEST_ID` 确认原任务完成且 configuredRunFactory 可用。导入执行可信代码的 capabilities，不等于数据/策略验收，不会选择全局 runner。

新数据集通过 `datasets registry` 获取 `qz.data.registered` revision，使用 `datasets register --file` 追加服务端已准备的 DatasetConfig 文件。相同 ID 不可覆盖任何现有或保留数据集；保存原 requestId，未知结果只查询/重放原请求，不能另换 ID。登记不启动研究、不运行数据获取，也不改变旧数据源。

验收完成后，新的 `research start --file` 可带 `runnerBuildRequestId` 和当前 `expectedRunnerRevision` 显式绑定该完成构建；无需选择全局默认。原环境变化或 revision 冲突必须先核对，不能自动刷新并重发。已有研究及同 requestId 重放保留原 executor。数据集和 runner 就绪不代表允许实盘。
