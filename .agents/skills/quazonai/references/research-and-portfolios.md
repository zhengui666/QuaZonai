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

研究 Agent 在同一 Pi Conversation 内配置并运行已注册的官方 Rust 策略。当前单标的回测接线为 EMA_CROSS、GRID_MARKET_MAKER（QuoteTick），ORDER_BOOK_IMBALANCE（L2 订单簿）及 HURST_VPIN_DIRECTIONAL（官方 BAR/QuoteTick/TradeTick 订阅）；参数及可选 base 配置按实际原生 schema 原样传递。不是任意 Python Strategy、用户源码动态运行、所有交易所或全部 Nautilus 策略的通用入口。不要用已退役的因子、IC、WASM 拟合或组合评分工具替代。

Hurst/VPIN 的 INTERNAL BAR 由官方 DataEngine 根据原 TradeTick 聚合；EXTERNAL BAR 必须使用原完整 BarType。外部非时间 BAR 若只供策略读取，应明确配置原生 venue.bar_execution=false，并提供用于撮合的原 Quote/Trade 数据；省略保持官方默认 true，不由胶水静默关闭。研究与账户的工厂注册及 capabilities 独立，研究配置不扩大账户执行能力。

先看真实数据类型及覆盖。数据准备不等于策略获利；缺失或未预热统计保留为空，官方统计名、币种和单位保留原意。回测不启动账户，也不把历史目标点当实时策略。

## 同一研究继续和恢复

保存 conversationId 与 submissionId。`watch`/`wait` 超时只退出客户端，不停止远端任务。需要停止时使用 `research stop ID` 并读回。通过 `diagnostics` 确认终止 submission 后：

```sh
quazonai research resume "$research_id" --submission-id "$terminal_submission_id" --instruction "$reply"
```

用 instruction 回答追问、修改研究方向或在已结束/停止的研究中继续；仍在运行的任务不要重建。保留新回执身份。Web 研究页显示同一 Pi 消息并支持回复，CLI 没有另一个聊天发送命令。

## 参数网格与独立原生回测

新网格使用同一 `research start` 或已终止 submission 的 `research resume`，不用已退役的 `parameter-scan options/preview/create/surface`。在 goal/instruction 中写清已安装策略或工厂、每个参数的明确候选值、训练/验证窗口、费用和目的；不要求用户手写工具 JSON，不凭空补充实质假设。

QuaZonai 的 `native_parameter_grid` 按完整原生请求和离散轴值生成笛卡尔组合。路径是现有字段的键数组，轴不能重叠，最后一轴变化最快；原始 JSON 值、十进制字符串保持不变，没有数值范围运算、自动切分或选优。工具分页返回 `requests`、`totalCandidates` 和 `nextOffset`；保持同一输入读完所需各页，不能将第一页冒充全部候选。这是研究 Agent 工具，不是同名 CLI 子命令，也不是 Nautilus 原生优化器。

在数值运行前声明全部候选、用途及独立性假设，验证研究先用 `native_strategy_logic_review` 记录完整原请求。随后将生成的完整请求传给 `native_parameter_batch`：`configured=false` 用官方策略请求，`true` 用已安装 Rust factory 请求。Pi Durable 依次编排独立子任务，Nautilus 执行每组回测并提供原生统计。失败阻止后续组；UNKNOWN 保留原任务待核对，不重发或更换身份。支持的入口还取决于已绑定 runner 的真实 capabilities，不因应用更新自动获得新工厂或导入能力。

```sh
quazonai research native-backtests "$research_id"
# nextCursor 非 null 时原样读取下一页
quazonai research native-backtests "$research_id" --cursor "$next_cursor"
quazonai research evidence "$research_id" "$task_id"
```

结果列表按原始任务顺序分页，包括失败、未完成及同研究的历史运行。每个 task_id 取自实际返回值，逐组核对 input.request、状态及原 Nautilus result；原始任务结果不重算排名或合成组合收益。Web「结果与评价」提供同一分页及原始 JSON 链接。批量完成后默认展示最后一组，不是最优或组合结果。继续阅读原结果后请求研究评价，缺证据保持不可用，不据参数搜索声称独立 OOS 或实盘许可。

旧 `parameter-scan list/get/stop/trials/watch/wait` 仅读取或显式停止历史扫描；旧合同存在不意味着旧生成、评分或排名执行链仍可调用。

## 官方权益统计与收益口径

查看 `equity_performance` 的真实 source、status/reason、币种、快照覆盖、收益日数与频率。它直接读取 Nautilus 官方 portfolio_returns，并调用官方 CAGR/Sharpe 的 252 与 365 两种频率；没有权益证据时保持不可用，不退回持仓收益冒充权益。canonical 默认统计可能使用持仓收益，不能按名称视为日权益收益。官方跨日延续也不等于每天实际观测，短样本年化不是已实现年收益。

通过研究 Agent 声明评价指标时，可明确 `metric_source:"EQUITY_PERFORMANCE"`、`annualization_days:252` 或 `365` 及该频率对应的 `native_key`。不得将旧 WF 工具或历史合同当作当前自动切分能力；各验证窗口需明确声明原生请求。股票常用频率与全天候市场频率只是计算假设，必须与用户目的及报告口径一致。

## 防过拟合与 AI 综合意见

通过研究 Agent 请求逻辑预评估、明确候选/窗口的逐组原生验证、邻近参数及成本敏感性，然后读取实际研究结果。不宣称已执行未提供请求的 walk-forward/CPCV；没有自动折分或独立 CLI 优化子命令。原生回测和统计由 Nautilus 执行；逻辑分析与综合星级是模型判断，不是 Nautilus 官方评分。

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
