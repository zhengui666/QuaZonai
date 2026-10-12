# 产品、领域与架构

<a id="product"></a>
## 产品边界与最小闭环

QuaZonai 是单人本机、可扩展、无人值守的 AI 因子研究基座。主流程为 **目标与数据 → 因子提案及计算 → 验证反馈 → 记录、迭代或停止**；不要求先创建 Alpha、仓位、组合或逐实验审批。原生模拟、优化和 target-only 交付是可选消费者，QZ 不持有真实券商账户、不发送真实订单。

| 阶段 | 输入与结果 | 职责与验证入口 |
| --- | --- | --- |
| 提交 | 原目标、模型引用、插件数据与政策 → 原生 Conversation/Submission 回执 | [host](../apps/research/src/host.ts)，SQLite/HTTP 原身份重放测试 |
| 提案/计算 | 可证伪假设、模型 → 包含失败与拒绝的完整试验账本 | 官方 Extension/Tool/子 Task；[原生因子测试](../apps/job/tests/factors.rs) |
| 迭代评估 | 原训练/验证时间范围 → 实际样本外验证统计及缺失原因 | Nautilus 数据/指标、Wasmi、ndarray-stats；共享时间切分与可得性回归 |
| 选择/停止 | 完整比较集、一个模型、原标准 → 停止原因或独立最终证据 | [流程](../apps/research/plugins/research.ts)与[最终评估扩展](../apps/research/plugins/heldout-wasm.ts)，暴露/分叉/重启检查 |
| 可选消费 | 原始研究证据 → Alpha、组合、模拟或目标包 | 既有 Rust/Runtime/Nautilus/Clarabel；PG/OCI/交付回归，不是挖因子前置步骤 |

`COMPUTED`、执行成功、有效证据、批准和 ACK 是不同事实。测试合成目录与可控模型只证明真实组件计算和执行协议，不证明市场 PIT、模型质量或盈利。

<a id="modules"></a>
## 模块与原生归属

| 模块 | 责任与边界 | 第一方依赖 |
| --- | --- | --- |
| [contracts](../crates/contracts/src) | Wire、严格科学 DTO、原生生成 schema 与必要历史解码 | 无 |
| [domain](../crates/domain/src) | PIT、缺失、时间隔离、数值/单位和可选消费者的纯规则 | contracts |
| [store](../crates/store/src) | 科学事务、历史读取、回执、防重复采纳与 PostgreSQL/PGMQ | contracts、domain |
| [integrations](../crates/integrations/src) | 文件、产物、严格 JSON、认证与外部协议薄适配，不含 Agent 引擎 | contracts |
| [research](../apps/research) | 官方 Pi Durable 宿主及研究扩展；原生 SQLite、模型、提交、工具、子任务、观察与用量 | 官方 pi-durable/pi-ai/Chord；匹配的科学 job |
| [server](../apps/server/src) | 所有者 HTTP/CLI、durable 薄代理和可选科学 Worker | contracts、domain、store、integrations |
| [cli](../apps/cli) | 复用 Server 的原生 HTTP 客户端；跨平台所有者连接及离线合同查询 | contracts、integrations |
| [runtime](../apps/runtime/src) | 科学 OCI 生命周期、结果、取消与持久 journal；不驱动模型 | contracts、domain、integrations |
| [job](../apps/job/src) | 原生科学、任意有限符号因子、可选模拟/优化/数据工具 | contracts、domain；仅 native-paper 可选依赖 integrations |
| [web](../apps/web/src) | React、官方 UI、原生研究快照/任务/结果/用量及历史/可选消费视图 | 生成的公开 API |

依赖方向及官方来源由 [architecture 测试](../crates/contracts/tests/architecture.rs)检查。Job → integrations 只在普通可选 `native-paper` 入口启用，`native-paper-test` 经该入口启用。扩展是官方 `Extension` 的配置登记，不另造插件框架或核心资产枚举。

Pi Durable 1.1.0 与 pi-ai、Chord 管理模型调用、上下文、Conversation、Submission 去重、Task 图、工具回放、子任务、取消、恢复、状态与用量。QZ 文档只记录原输入、试验、选择和数据暴露事实；不增加第二套 Agent 会话/任务表、调度器或 Rust 模型循环。项目不启动 Codex App Server 或独立 Pi Coding Agent，旧登录、配置、模型目录、Mission/MCP 执行入口已退役。

NautilusTrader 管理原生 catalog、Instrument/BAR、指标、Strategy、撮合、共享资金模拟、成交、费用和权益。Wasmi 执行模型，WABT 编译 WAT，ndarray-stats 提供真实 Pearson IC，Arrow 保存列式结果，Clarabel 负责可选优化，PostgreSQL/PGMQ 管理既有科学事务和消息。无第三方源码复制、补丁、fork 或替代引擎；许可证与已应用兼容迁移保留。

<a id="contracts"></a>
## 合同与身份

源定义在 [Rust DTO](../crates/contracts/src)。[HTTP](../contracts/generated/api-v2.openapi.json)、[Runtime](../contracts/generated/runtime-v1.openapi.json)、[领域](../contracts/generated/domain-v1.openapi.json)及 Web/CLI 派生文件由原生生成器导出，不手改。原生执行视图保持开放 JSON，不枚举插件的全部任务状态。

durable 的 Conversation/Submission/Task ID 是原生安全整数，不转换成旧 UUID。既有实体 ID、版本 ID、序号和修订号保留原语义；Decimal、大计数与纳秒仍按十进制字符串传递，缺失不补零。相同请求及 `requestId` 返回原身份，改正文不是重试；HTTP 202 不表示完成。未知结果先对账原请求，不另起研究掩盖失败。

<a id="cycle-startup"></a>
## 执行、恢复与历史

新研究入口为 `/api/v2/research`，Rust 使用现有所有者认证后转发到 loopback Node 宿主，不转发 cookie、设备令牌或授权。宿主调用官方 Harness 与持久事务；工具反馈回到同一原生会话。停止调用原生 abort 并包含拥有的后台子任务；停止观察或本地进程退出不冒充远端取消。安全工具沿原身份恢复，不安全工具按上游 interrupted 语义保留中断，不擅自重跑。

科学 Worker 仍使用 PGMQ、Run/Attempt、所有权 fence、远端身份、journal 与结果采纳回执。至少一次投递不等于 exactly-once；输入、预算、原副作用身份和原回执共同约束重复。普通结果 ACK 不隐式创建研究员/审阅员，旧周期唤醒不重建 Codex 配置或 Mission。

旧周期、实验、选择、资格、产物、公开摘要、交付和支出事实保留为历史；不得将 CODEX 来源改标 durable，也不根据引擎退役补写成功/取消。未终止旧工作必须真实对账，不用删除迁移、清库或伪造回执解决。外部科学周期仍是显式、可选操作，不是新的因子主流程。

<a id="data"></a>
## 防过拟合与数据

原问题、数据引用、评价/选择/停止标准和显式预算一并记录。失败与比较过的所有试验保留，分叉采用当前完整事实，不能回到过去擦除选择或恢复试验额度。不存在强制最低预算；零试验额度不运行科学计算，耗尽后结束原生生成而不声称最终证据。

来源许可、原始 snapshot/catalog、Instrument、历史成员、日历、质量及可得性可追踪。`event`、`available`、`decision` 不互换，PIT 必须满足当时可得。目录存在、下载/准备成功、来源名称或当前状态都不是质量、历史可得性、覆盖率或权限证明；公开获取工具见[来源指南](../runtimes/data/source-plugins.md)。

默认因子扩展只在训练/验证段计算与反馈，不读取最终后缀。所有资产共享半开时间切分；预测可得性、未来标签事件期限和标签可得性均须留在所属分区，不按各资产行号切分，不把未来标签给模型。任意有限正负分数不当仓位，不校准成收益，也不凭 IC 数值宣布资格。常数、样本不足和缺失指标为 null 加具体原因。

最终评估在读取前记录一个选择、完整比较集与 session 级暴露窗口。原标准必须一致；崩溃、取消、新根或分叉不退还已经预约的读取机会。暴露身份使用原生 BAR 标识与原事件窗口，不信任可改名的 `datasetId` 或目录；源副本和版本不能重置暴露。最终结果/失败只发给所有者，工具结束而不回传模型作下一轮调参。研究数据与模型 Provider 凭据、独立留出原值隔离。

支持的原生 purge/embargo 和独立评估仍保留；变量标签等缺能力时明确不支持。未实现或未验证的 DSR/PBO 等不填常量，不事后改阈值让旧结果通过。撤销后的历史证据仍能审计，不凭历史标签准入新研究或交付。

<a id="runtime"></a>
## 科学 Runtime 与产物

Runtime 接收登记的科学镜像、原输入引用和明确任务，不拥有模型、预算政策或交付资格。原 OCI launch/cancel/结果/journal 关联及重启恢复保留，未知旧远端身份先对账。缺原生能力/目录/结果是失败，不以文件存在或 exit 0 当正式成功。科学代码改变后用重新构建镜像执行适用[OCI 测试](../apps/runtime/tests)，不是只查版本。

默认因子扩展通过官方 NodeExecutionEnv 调用匹配 `job evaluate-factor`；只接收模型源与已登记数据，进程不继承 Provider 凭据。模型生成代码使用有限原生 fuel、官方进程超时和主机地址空间限制，真实失败仍入账；可信直接 CLI 的可选不计量语义保留。提交为 FAILED 的试验在 safe 回放时只重抛原错误，不免费重算。Artifact 与既有历史导入保留原字节、类型、来源和生产者关联；原 Agent 会话/profile/轮次/用量/唤醒通过只读 `run history` 和 Web 运行详情展示，不重新执行。更详细的配置与扩展方式见[research 宿主](../apps/research/README.md)。

<a id="portfolio"></a>
## 可选组合与原生模拟

因子证据不要求资格、仓位或组合。选择可选 Forecast/target-weight 消费者时，原有单位、校准、原预测/资金来源、许可与资格关联继续按该消费者合同解释，不把其专用限制施加到普通因子研究。Clarabel 无解不返回备用权重；Nautilus 的成交、费用和权益是唯一权威模拟结果，不加总独立 Alpha 曲线或重复扣费。

官方 `nautilus-backtest 0.63.0` 不公开回测内原位更新合约定义接口；共享模拟仍拒绝规格变更窗口，不删历史、预载未来定义、拼接净值或自建撮合器绕过限制。[Arrow 历史](../crates/contracts/src/portfolio_history.rs)绑定原 manifest/请求与行列，不补零或另算权威净值。

<a id="polymarket"></a>
## 可选市场与来源适配

Polymarket、现货和其他受支持资产的费用、币种、结算与交易期限留在所选适配器内，不构成核心唯一资产分支。历史准备复用 Nautilus 原生适配器与 Parquet catalog；来源/结算/PIT 不从今天的状态、末价或本次下载时间推断。原公开获取、EVM/Alloy 采集、目录准备、质量测量与 source registry 继续是独立数据能力，见[历史数据指南](../runtimes/data/README.md)。

<a id="handoff"></a>
## 可选目标交付

Release、批准、Offer、Claim、ACK、撤销和 supersede 保留原事实；批准不等于领取，ACK 不代表 QZ 交易或下游撤单。原外部身份重放原结果，不刷新期限，领取时重验当前来源与合同。此消费者不会自动从一个 COMPUTED 因子创建交付授权。

<a id="web"></a>
## Web、CLI 与认证

默认页面提交并观察持久研究，展示原生状态/任务/用量、全部试验与失败、最终选择和证据；不在浏览器另存权威 Agent 状态。历史项目、Alpha、组合、运行和交付作为历史/可选消费者保留。使用成熟官方 UI、生成客户端与既有 PWA/主题；未知、离线、HTTP 202 或过期缓存不展示为成功。

实例密码、Argon2id、浏览器 session 与独立 CLI owner-device 登录/撤销复用既有能力。密码/设备令牌/Provider 凭据不进模型上下文。CLI 复用 [service_http](../apps/server/src/service_http.rs) 的无重定向、无自动写重试传输和原文回执，研究命令不要求另发 Operator grant。HTTP 仅用于可信网络或现有 loopback，公网 HTTPS；旧历史受限凭据不会获得新研究所有者权限。

<a id="container-release"></a>
## 部署与持久状态

应用镜像携带 Web/API/Caddy、官方 Node research 宿主及匹配科学入口；PostgreSQL/PGMQ 独立。科学 Worker/Runtime 原生入口和同源发布、安装、备份、更新与恢复边界保留。Node SQLite、配置/扩展与模型的私有环境由本安装持久保存；备份前停止其所有者，恢复同一状态，不重生任务身份。旧 Codex home/凭据/历史不删除，也不再作为新服务依赖。

实际安装入口为 [Bash manage.sh](../deploy/docker/manage.sh)，不是仅开发/CI 用的 Python 镜像。版本清单、Git 分支/tag 正式策略不因执行器切换而改变；本任务交付到 `dev`，不部署用户服务。操作见[部署手册](../deploy/docker/README.md)，实测范围与最终交付见[任务](tasks/durable-factor-platform/task.md)。
