# QuaZonai 产品、领域与架构事实源

> 需求基线：2026-09-05，Issue #62 正文及附录 A（评论 5549224292）、B（评论 5549244417）。
> 所有者修订：2026-09-05，PR #63 的执行要求——**优先 Rust，其次 Python；优先复用，其次造轮子**。
> **状态：Draft 集成实施中。已实现原生适配及合同/领域初始切片；本文的目标合同不代表全量系统、全部测试或受保护验收已完成。**

本文包含完整字段合同、API/CLI/MCP 映射、状态机、故障场景、T01–T42 和交付边界，是唯一完整架构事实源。Issue 是需求出处和验收追溯链接，不是运行时或离线审查必须另行读取的规范依赖；其后续编辑不会自动改变本文。任何新要求必须先通过版本控制更新本文，再实现。不得以摘要、局部绿色 CI、缺失能力清单或语言修订缩小核心范围。

`AGENTS.md` 只定义治理；`OPERATIONS.md`、`CLI.md`、Skill、README 分别展开运行、命令、工作流和入口；`docs/architecture/issue-62-execution.md` 与兼容性矩阵只记录证据，不创造竞争架构。旧代码和过时设计从当前树删除；历史仅由 Git 保存，迁移以只读外部快照为输入。

## 0. 所有者修订：语言与复用的决策顺序

1. 优先寻找仓库已有实现、标准库、平台和成熟外部组件，随后才考虑新写代码。先验证实际接口、安全、许可证、维护性和目标行为，而不是按语言数量评价架构。
2. 在适用实现之间优先 Rust，其次 Python。Rust 是新控制面、领域合同、持久化、CLI/MCP 和网关的默认选择；不是对所有第一方 Python 的绝对禁令。
3. 只有具名目标能力无法由满足合同的Rust组件承接，且已先提交第0.1节要求的证据，才允许采用Python的最小上游适配。能用Rust的组件必须Rust；不要求自研Rust算法替代已有库，也不接受“桥接方便”作为Python理由。
4. 第一方代码只拥有 QZ 的产品规则、权限、证据关联和最小适配。不重建回测、优化器、Agent Harness、OAuth 刷新、消息队列、密码学或容器平台。仅在没有满足需求的成熟能力时自研，并明确缺口与退出条件；不以“adapter”命名隐藏整套自建内核。
5. 不为 Rust 占比增加无意义 FFI/微服务，也不以 Rust 启动器包装全部 Python 然后声称 Rust 优先。一个业务状态机、一个合同源、一个权威数据源；不维持两套永久兼容后端或重复真相。
6. 该修订取代 #62 中“所有第一方后端只能 Rust”“Python 只能是第三方库”“必须因语言删除全部 Python 服务”等绝对措辞。前端 React + TypeScript + **官方 `antd`**、产品范围、安全、数据、测试、CI/Review/合并条件全部不变。

## 0.1 所有者追加修订（2026-09-05，本次执行）

- 第一方目录不使用 `qz-` 前缀：`apps/job`、`apps/server`、`apps/runtime`、`crates/contracts`、`crates/domain`、`crates/store`、`crates/integrations`。包名/构建路径同步改名，不保留旧别名目录。
- 旧代码没有兼容和保留义务。删除旧 Python 服务、旧前端、插件平台、旧专属测试/部署/文档及兼容层；Git 已提供代码历史，不在新树保留 legacy 副本。删除源码不是删除用户数据：不重置用户数据库/数据卷，不删除 LICENSE/NOTICE，迁移、导出及回滚仍是交付项。
- 某组件有满足本项目能力和安全合同的 Rust 实现，就使用该实现。不能以现有桥接方便、旧工具链、版本解析失败、语言占比或赶工为理由选择 Python。
- Python 例外须先提交 `docs/research/reuse.md` 中的具名能力证据：审查的 Rust 候选和具体版本/API、真实缺口/失败复现、采用的 Python API/版本、接口/权限/进程边界、测试和替换条件。检索不到不等于证明不存在；只批准必要范围，可由执行者依据证据自主决定。
- 已确认并实测：Nautilus `nautilus-backtest/model/trading 0.63.0`（官方 `v2.0.0rc4`）、Clarabel 0.11.1、Apache Arrow Rust 56.2.0；使用 Rust 1.98.0 满足上游 MSRV。第一方 job 不再通过 PyO3/CPython 调用这些能力。
- 当前提交只实现受测原生适配与合同/领域基础，不声称完整控制面/UX/研究/交付已就绪。删除旧测试不满足新系统 T01–T42；缺失检查仍阻塞最终合并。

## 1. 当前实现与完整目标

| 部分 | 已有事实 | 必须完成的目标 |
|---|---|---|
| 运行服务 | 旧实现从本分支删除；新系统未完成部署验收 | 按第 0 节重建并验收控制面，完整 Ant Design 产品面；显式切换后移除被替代路径 |
| 原生科学计算 | `apps/job` 固定 FIXTURE 直接调用 Rust Clarabel、Nautilus、Arrow | 隔离真实研究、评估、至少两个 Alpha、完整约束与共享资金模拟 |
| Codex | 无账号 stdio 握手、account/read、完整模型分页、默认与 effort-only Thread 探针 | 真实工具→Job→Evaluation→同 Thread 消费结果、独立 Reviewer、恢复、原生账号和权限隔离 |
| PGMQ | 原生投递/结果/确认事务回滚探针 | 正式领域事务、预算、Run/Attempt 接管、恢复、取消和唯一结果采纳 |
| 交付/迁移/运维/UX | 新系统完整链路尚未实现 | W0–W8、T01–T42；不能把此表当作 Future Work 排除项 |

同一 Draft 集成 PR 承载全部范围。从 `main@941dbcbbaa26293d17b14f733c0d415611035f57` 建立的 #63 不依赖未合并 PR；旧 Issue 不自动关闭，最终覆盖矩阵说明替代和独立保留关系。W0 成功也不能合并骨架或关闭 #62。

## 2. 产品与所有权

QuaZonai 是证据优先、单用户、自托管的自治量化研究工作台。用户提出想法，系统在预算内组织研究、调用专业引擎，交付可解释、可追溯的结论和目标组合。首页回答：研究什么、证据是否可信、组合有什么取舍、哪里需要用户决定。不以聊天条数、Token/Agent 数或动效冒充研究成果。

```text
Idea 与已有证据 → 冻结 Research Brief → 有界 Research Cycle
→ 原生 Codex 提出假设/研究产物/实验请求 → 数据与代码合同验证
→ 原生研究库及远端 Nautilus → 实际结果回到同一 Codex Thread
→ 修正或否定结论 → 独立验证/封存评估 → Qualified Alpha 版本
→ 至少两个合格 Alpha 的原生组合构建 → 共享资金 Nautilus 模拟
→ 冻结 Release/target-only Package → Paper 审批/交付/反馈
→ 冻结政策满足后人工或已授权自动 Live 目标交付
→ Forward Evidence → Degradation Observation → 受限 Wake/新 Cycle
```

QZ 拥有研究意图、版本、预算、权限、证据关联、资格、审批、交付记录及用户体验。Codex 拥有原生模型会话、工具循环、上下文和认证；科学库拥有估计、交叉验证、优化和统计计算；Nautilus 拥有市场目录、市场事件、模拟成交和交易运行语义。

**QZ 不拥有真实 Broker/Exchange 凭据、订单/成交/仓位/账户/NAV、执行风控、下游 heartbeat/recovery/reconciliation 或启停/撤单/平仓。** Live 是目标包交付，不是交易指令。取消计算、暂停研究不是停止下游交易。Nautilus 回测内模拟成交仅是评估证据，不是第二份真实交易账本。

正常人工动作是提出 Idea、审批推荐的交付；暂停/恢复/归档、数据授权、Codex 登录、Mandate/Universe/下游配置和故障处理是低频管理，不得变成每轮必经点击。Brief 确认后预算内不反复要求 Continue。最多澄清 1–3 个真正影响边界的问题；不需要则直接展示可修改 Brief。系统故障、数据不足、研究被否定、等待审批分别显示。没有有效 Alpha 是正常结论，不重试制造赢家。

## 3. 架构、复用与调研落实

```text
React + TypeScript + antd → REST/SSE 生成合同 → qz API/Domain/Worker/CLI/MCP
  ├─ PostgreSQL + PGMQ：领域事实、通知、持久事件和审计
  ├─ 原生 Codex App Server：受信任模型进程
  ├─ Artifact Store：原生不可变对象/版本引用
  └─ runtime：受信任远程计算网关 → 原生 OCI/Docker
       └─ 每任务一个隔离 job 进程/容器 → Rust Nautilus / Clarabel / Arrow
                                                   / 已验证的原生统计与研究组件
```

默认布局（job 与 contracts/domain 已有受测切片，其余完整能力仍为目标，不是一 crate 一微服务）：

```text
Cargo.toml / Cargo.lock / rust-toolchain.toml
apps/server/                 API、Worker、CLI、MCP 入口，共享领域服务
apps/runtime/         远程任务与受限数据访问网关
apps/job/             一次任务一个进程的上游执行器
crates/domain/        无 HTTP/SQLx 的领域规则
crates/contracts/     DTO、错误、事件、政策、OpenAPI/JSON/Arrow 合同源
crates/store/         SQLx、事务、PGMQ 薄适配
crates/integrations/  Codex、OCI、原生存储、科学库和下游适配
frontend/src/features/  research、alphas、portfolios、deliveries、runs、settings
migrations/             新系统显式 SQLx 迁移
contracts/generated/    原生工具/合同源生成，不手改
runtimes/               锁定镜像和依赖，包含经论证的 Python 复用
examples/               明确 provenance 的可重复示例
 tests/                 contract、golden、e2e、security、fault
 deploy/                compose、backup、observability、runbooks
 docs/                  product、architecture、adr、research、protocols、operations
```

| 成熟组件/对标 | 采用能力 | 禁止重复建设 |
|---|---|---|
| 官方 Codex App Server | Thread/Turn/Item、登录、模型目录、工具循环、上下文 | 第二套 LLM Loop、消息历史引擎、OAuth 刷新器 |
| NautilusTrader | BacktestNode/BacktestEngine、ParquetDataCatalog、适配器、执行模拟 | 撮合器、回测引擎、第二份权威 NAV |
| Rust科学组件 / Clarabel | 原生风险/模型/组合求解；Python仅证据批准的缺口 | 自写协方差、优化器或伪指标 |
| Qlib（按需）/scikit-learn | 原生数据/特征/模型流程、估计器 | 强制训练平台或另一套成交账本 |
| Optuna | 预算内有限搜索与试验采样 | 无预算 trial、自研搜索器 |
| PostgreSQL/PGMQ/SQLx | 事务、关系、原生投递/visibility/archive | 自研队列、应用 outbox 搬运平台 |
| Axum/Tokio/Serde/utoipa/schemars/成熟 CLI 库 | 默认 Rust 服务与合同 | 平行 HTTP/schema 框架 |
| 官方 Rust MCP SDK `rmcp` | MCP 协议和工具传输 | 自写 MCP/JSON-RPC 栈 |
| 有证据批准的最小 Python 例外 | 第0.1节审核后才能引入，目前无生产例外 | 通过Python调用已有Rust能力或伪装全后端 |
| Bollard/OCI | 容器生命周期、资源和隔离限制 | 容器平台；向 Agent 暴露 Docker socket |
| 官方 Ant Design/icons、ECharts | 全部基础 UI、主题、表单、反馈和单一图表 | Radix 与 antd 双体系、自写基础组件 |
| RD-Agent | 假设→实现→真实反馈、失败知识组织经验 | 与 Codex 并行的 Agent Harness |
| packaging / pip / npm / Cargo | 原生依赖解析、版本、锁定和完整性 | 自制依赖解析器或业务 hash gate |

PGMQ 只保证至少一次投递场景，外部副作用不是全链路 exactly-once。业务幂等、预算、Attempt 和结果采纳不能外包给消息 visibility。Nautilus Python/Rust 库不是现成官方 HTTP 服务；`/runtime/v1` 是我方适配合同。

调研来源与落地机制：Qlib（arXiv 2009.11189）用于数据/模型/评估分离；R&D-Agent-Quant（2505.15155v2）用于有界假设—产物—反馈及失败记录；AlphaAgent（2502.16789v2）用于研究动机、产物一致性、重复性与复杂度；AlphaPROBE（2602.11917v1）用于血缘和负面证据；TradingAgents（2412.20138）用于独立上下文反方审查；The Probability of Backtest Overfitting 和 The Deflated Sharpe Ratio 用于完整试验集合、选择偏差及统计前提；skfolio（2507.04176）用于成熟估计/优化/验证流程。论文作者在特定市场的结果不是本项目复现或收益承诺；新颖、多 Agent 投票、Sharpe 或一般 CV 不能替代硬性证据 Gate。

DSR/PBO 默认不支持：未确认选定 skfolio 版本具备满足本项目的完整接口。只有接通已审计上游、参考数据验证、完整可比试验集合后才能开启；CPCV 不是 CSCV/PBO。required 指标不支持时 INCONCLUSIVE，不手写常量/近似冒充。基础交付仍必须完成 PIT、时间隔离、真实样本外、试验账本、sealed 防重复消费。

默认不引入 Redis、Kafka、Temporal、向量/图数据库、通用 Workflow DSL、插件市场、第二实验记录平台或自建密钥平台。模块只为真实边界存在，不建立形式化 Repository/Factory/事件总线模板。保持 LICENSE/NOTICE/第三方声明，不擅自换许可证。

## 4. 当前原生适配的准确边界

`job verify-native --output NEW_DIRECTORY` 只接受不存在目录，0700 创建；一个任务一个进程。报告始终 `origin=FIXTURE`、`deliverable=false`，不能产生 Qualification/Release/Handoff。

- `optimization.rs` 直接调用 Clarabel Rust `DefaultSolver`，原生二次锥规划最小方差；两资产协方差 diag(1,4)，long-only、预算1，独立手算参考0.8/0.2，容差1e-5。必须原生 `Solved`、有限权重和正确维度；无 Python 或生产兜底。
- `backtest.rs` 直接调用 Nautilus Rust BacktestEngine 和上游 EmaCross，固定745个 synthetic quote、实际原生事件/订单/持仓计数，成功/失败均 dispose。计数来自引擎，不写死“成交成功”；fixture仍不是 target-weight 多Alpha组合模拟。
- `arrow.rs` 使用 Apache Arrow Rust RecordBatch/FileWriter/FileReader，create_new 写入，回读检查 schema/元数据/每个值和行数；不是 PyArrow。不存在第二套 IPC 协议。
- `report.rs` 完整序列化、换行、sync_all后使用同文件系统 hard_link create-if-absent 发布正式名，不覆盖。任何发布前失败无正式成功报告；不是目录级崩溃一致性或生产 Artifact Store。
- Codex 探针沿用官方 pinned二进制 stdio initialize/initialized/account/read/完整model分页/thread启动；QZ只保留受控适配，不获取隐藏推理/凭据。无真实账号推理和同Thread工具链的测试不能当作T07/T08。
- 原生 PostgreSQL+PGMQ 事务探针保留；临时fixture表不是正式生产Store。

Rust 1.98.0，Nautilus Rust crates0.63.0（Python2.0.0rc4发布族），Clarabel0.11.1，Arrow56.2.0，Codex0.144.4，PGMQ1.10.0；Linux x86_64。Cargo.lock来自原生Cargo，所有验收 locked，不现场生成锁。没有任何生产Python例外被此处批准；旧science requirements/lock/checker随旧桥接删除，供应链改由Cargo原生锁验证。

Nautilus2.0发布族仍为release candidate，不能隐瞒预发行风险或仅因为版本较新宣称稳定；正式目标组合、目录、结算、隔离/资源/取消必须单独验收。原生引擎日志的NaN不能直接当正式指标，正式Metric wire层拒绝非有限值。

## 5. 研究、数据与数值边界

用户只面对研究项目、实验、Alpha、组合、交付、运行六类主对象；Brief/版本/评估放详情，不暴露几十张表的人工操作。`execution_state`、`evidence_status`、`decision` 分离：例如计算成功而数据不完整是 SUCCEEDED + INVALID/INCOMPLETE + INCONCLUSIVE，不是通过。

Brief 冻结问题、假设、经济含义、Universe、数据授权、预测期限、基准、成本/容量、验证分区、选择规则、停止规则和预算。修改创建新版本，不事后改阈值。失败、取消、无效和淘汰试验均保存；重开项目、换 UUID、同赢家改名不重置试验/暴露。

市场源以原生 Nautilus ParquetDataCatalog/不可变 snapshot 为权威；Qlib 等派生缓存可重建，不是第二份源。保留数据/资产定义、授权、版本、质量与当时可得时间。`event_at` 是事件，`available_at` 是当时可用，`decision_at` 是决策；必须 available_at <= decision_at，按事件排序或把 ingest_at 填成 available_at 不构成 PIT。财务重述、成分变更、退市、到期、结算和日历不得以今天状态替换历史。

Discovery/Validation/Sealed/Forward 分权限与挂载。raw/sample/metric/plot/summary 都可能暴露；按根血缘继承，在 evaluator 获得读取能力前事务预约，崩溃/取消不回滚已发生的读取机会。研究者无 sealed raw/preview/日志旁路；evaluator 无 Provider 凭据和研究工作区写权限。是否向后续研究披露由冻结政策决定，不继续把已披露 sealed 当独立。

原生 WalkForward/CombinatorialPurgedCV 显式 purge/embargo；默认 0 不等于安全。按观测数 purge 仅适用于验证过的固定 horizon；变量区间需支持重叠区间的上游接口，否则 UNSUPPORTED_LABEL_INTERVALS。所有可比试验才可进入统计选择集合，不拼不同市场/频率/区间伪算 PBO/p-value。

Alpha 只发 score/expected_return/uncertainty，不发订单。score 未校准不能冒充收益/仓位；校准/调参仅使用允许训练段。至少两个合格 Alpha 的实际预测经单位、共同期限、币种、资产对齐、覆盖率验证后进入原生 Alpha/Prior/ensemble/optimizer（如 PredictorAlpha、FixedWeightedAlpha、MeanRisk，必须所锁版本确实支持）。固定权重/scale 是配置，不冒充拟合校准。Alpha 混合权重和最终资产权重分别持久化。

风险/协方差优先复用已验证的Rust sample/EWMA/LedoitWolf等接口；优化复用Rust Clarabel等原生solver。skfolio只是待证据审批的具名Python候选，不是默认实现。生产路径真实支持现金、单资产、gross/net、组约束、换手、成本、风险和参与率，保存余量和诊断。不可行返回 INFEASIBLE，无目标，不偷偷等权/单资产100%/放宽约束。ACCEPTABLE_INACCURATE 仅显式政策允许且独立容差验证通过才采纳。

毛净收益、费用、换手定义、年化频率、无风险利率、单位、区间、样本数、方法版本、原生来源均明确；缺值 null + reason，NaN/Infinity 拒绝。成本/滑点/冲击/容量基于版本化费表、流动性及上游模型；缺深度数据不声称精准盘口冲击，capacity 不等于初始资金。最终组合在一个共享资金、统一净额、实际成本 Nautilus 模拟中验证，不平均独立账户曲线或另算权威撮合账本。

市场支持依测试矩阵：保留已使用市场的真实数据路径；venue/data type/到期/结算分别验证。Polymarket/Kalshi 不伪装普通股票；不支持的组合明确 RESEARCH_ONLY/UNSUPPORTED，不能 Paper/Live。

## 6. 原生 Codex、自治与权限

连接合同：`connection_mode=SYSTEM|CUSTOM_PROVIDER`；`profile_origin=MANAGED_VOLUME|OPERATOR_MOUNT`；`use_default_model_settings:bool`；保存 model/effort 可空、fast_mode bool。来源不是第三种 Provider。

SYSTEM 不注入 provider/base URL/API key，不写空值覆盖原生环境，不导入/删除 auth.json，使用明确的 Worker CODEX_HOME。CUSTOM_PROVIDER 只用显式激活路由与凭据，失败不偷用系统订阅。失败时不自动切连接/模型/effort。命名卷不等于宿主 ~/.codex；提供同卷同 UID 原生 login/status，显式挂载 profile 不自动复制/chown/删除。宿主 keyring 容器可用性不能保证，UI/运维明确说明。

由锁定 Codex 二进制生成协议 schema，稳定 stdio initialize → initialized。model/list 遍历全部 cursor，模型 ID、支持 effort、默认值来自原生能力，不硬编码型号或 high/xhigh 集合。default 开关开启时省略 model/effort/Fast 覆盖但保留保存值；关闭时只传实际配置非空项。SYSTEM + model=null + 合法 effort 非空必须可用；unsupported 报错不降档。requested 与原生可观察 actual 分开，未观察到的 actual=unknown。

账号读取、device code 登录/start/cancel/logout、保存/刷新凭据由原生 Codex 管理；QZ 只呈现受控流程，不维护 DB OAuth token 刷新器，不依赖实验 external-token。V1 不以 experimental WebSocket/dynamicTools/project environments 为必需能力。

一个 Mission 对应一个 durable Thread，不使用无限长 Program Thread；真实 Job/Evaluation 结果回该 Thread 后才结论。Reviewer 有独立 Thread、权限和输入清单，不是同聊天换角色。QZ 只编排有限业务阶段，不另造 Agent DAG/规划/记忆/工具循环；并行使用验证过的原生机制或独立受控会话，不固定凑七个角色。

Mission 默认独占临时 Git worktree、独立 App Server child、workspace-write、network disabled、approvalPolicy=never，仅允许 worktree root。Agent 不访问 QZ 源仓库/其他项目/Sealed/Secret/DB/Docker socket，不通过 Git 操作绕过工作区管理。所需数据与实验经 mission-scoped stdio MCP。受信任 App Server 可访问模型服务/Provider 凭据，不等于 Agent shell 可获得该文件系统/环境权限。随机名 sentinel、auth.json、DB、master key、sealed、socket 等真实越界测试是硬要求；过滤 KEY/TOKEN 变量名不是隔离。

初始默认预算：并行 2、Cycle 实验 20、修复 Turn 2、Mission Turn 16、墙钟 3600 秒、容器 2 CPU/4096 MiB、输出 64 MiB、每日自动 Cycle 3；均配置化、冻结并在入队事务预约。Optuna 内部 trial 计入预算。无法精确计费则只显示估算/不可用，不宣称严格美元限额。Agent 不能扩大政策/预算、自评、自批、发包、读 secret、改正式指标或写 SQL。

只保存可观察调用、文件变更、命令/测试、公开总结和 Domain Event，不索取、存储或展示隐藏 chain-of-thought。

## 7. 可靠执行、远端与产物

Run/Attempt/队列/事件的精确字段和状态机见本文 A6、B4–B6。领域写入、预算、事件与 pgmq.send 同一 PostgreSQL 事务；消息仅携带 run_id 等稳定引用。当前 attempt_no/owner_epoch/DB lease 才能采纳结果，queue visibility 不是领域 authority。

远端 stable `(run_id,attempt_no)`，先持久 dispatch intent 再外部 submit。超时/ACK 丢失进入 UNKNOWN/RECONCILING，查询原任务，不马上重复跑。可能接管同一 attempt；只有确认旧任务终止/不存在或安全隔离后才新 attempt。外部调用不持行锁。先验证并持久化不可变产物，再事务唯一采纳结果、事件/评估，最后 archive/ack。旧 Worker 返回 STALE_ATTEMPT，不改终态。

取消先 CANCEL_REQUESTED；远端未确认停止不能显示 CANCELLED。成功/取消按同一行 CAS 唯一终态，不同时发布 success/cancel。浏览器关闭/超时不取消任务。仅可恢复基础设施错误有界重试；研究否定、数据无效、solver 不可行不是重试理由。正式引用产物不可按临时 workspace 规则删除。

远端协议 `/runtime/v1` 是我方网关，不是 Nautilus 官方 HTTP API。默认 Rust，Python 仅按第 0 节有明确复用依据。生产 TLS 校验和明确凭据，Operator 配置 allowlist；拒绝 SSRF、重定向绕过、DNS rebinding、云元数据、任意 URL/宿主路径/环境变量/命令。可信网关独占 OCI socket；不提供任意 docker run。JobKind 映射登记镜像/入口和允许参数。

每 Job 一个非 root、只读 rootfs、capabilities drop、默认无网络的进程/容器；限制 CPU/内存/PID/时限/文件大小/输出字节，不使用长期共享 CPython/BLAS 池承载不受信任任务。API 不嵌入科学解释器；采用 Python 控制面适配不意味着科学执行可回到 API 进程。可信 App Server 与不可信代码不能共享含 Secret 的 filesystem namespace。命令文本/Prompt 不是隔离证明。

Artifact 只能使用服务端登记原生对象/版本；校验 schema/provenance/version/size/access 后采纳。路径存在、exit=0、远端 success 字符串不是资格。使用原生对象存储版本/唯一只读发布目录和数据库约束；Local 发布禁止覆盖。宿主管理员已经控制运行宿主不在不可变威胁边界内。业务不新建 hash/fingerprint 身份或发布 Gate。

## 8. Release、审批、反馈与唤醒

先冻结目标包再审批。审批绑定 release、artifact 原生版本、candidate/mandate/policy、下游、证据和有效期；任何目标/依赖变化产生新 Release/审批。Qualification 仅独立 VALID/PASS/新鲜/合法血缘证据产生；无手工 force PASS。

Approval/Offer/Claim 在事务内重新验证版本、撤销、资格、REAL 数据、授权用途、政策、期限、readiness 配置版本与新鲜度；外部 probe 在事务外执行。Claim/revoke/expire 原子竞争只一结果；下游只领自身 offer。CLAIMED 后 QZ 无停止/撤单/伪撤销权限，只可 advisory 或新版本；旧过期目标不能因重试复活。

Paper/Live 分开审批。MANUAL/AUTO_PAPER/AUTO_HANDOFF 是显式 Operator 授权的不可变政策，不是 Agent 可开启的布尔开关。自动晋级需要完整足量连续且新鲜 Paper、有效未撤销政策、资格/Release/数据新鲜、无活动阻塞劣化、下游兼容；任何缺失分别阻断。停用仅阻止未来授权，不撤销已执行交易。

Forward 按 downstream/external_message_id 去重；保留 stream/sequence/revision/supersedes 和覆盖窗口；迟到、重传、重叠、gap、partial、correction 不重复累计独立样本。完整窗口交原生指标评估形成 HEALTHY/WATCH/DEGRADED/INSUFFICIENT_DATA Observation，再 Wake，再项目状态/冷却/预算校验启动新 Cycle；相同 Observation 不产生两个自动 Cycle。缺数据不等于健康或劣化。PAUSED/ARCHIVED 不开新 Cycle，保留待处理 Wake 和已有风险观察。

确定性再平衡和研究分开：已合格 Alpha 在新 cutoff 计算新 Candidate/Release，所有新包照常校验、授权；不修改已批包，也不强迫 LLM 每次发明策略。Current weights 来源明确 FORWARD_SNAPSHOT/LAST_TARGET/NONE；LAST_TARGET 是假设，不冒称真实账户仓位。

## 9. Ant Design 产品面与浏览器合同

主导航：研究 / Alpha / 组合 / 交付 / 运行 / 设置。React/TypeScript + 官方 antd，不是 Ant Design Vue，不保留 Radix/自制基础组件双体系。ConfigProvider + App 统一 locale/theme/token，官方 icons；默认 ECharts 单图表方案，表格替代、真实单位和证据下载依据。保留合适业务复合组件、React Query 和测试经验，不 fork 基础组件或另建 form/theme。

| 页面 | 用户任务与官方组件 |
|---|---|
| 首页/研究列表 | 目标、阻塞、下一步；Layout/Menu/Table/Card/Alert/Tag，无虚构 KPI |
| 新研究 | Steps/Form/Input/Select/InputNumber/Descriptions；草稿保存不等于启动 |
| 研究详情 | Tabs/Splitter/Timeline/Table/Drawer；版本/证据/分支比较，聊天是证据侧栏 |
| Alpha | Table/Descriptions/Statistic/Alert；预测单位、样本外、资格、限制；缺失不填 0 |
| 组合 | Form/Table/Tabs/ECharts；风险/成本/容量/约束取舍，Alpha 权重和资产权重分开 |
| 交付/审批 | Descriptions/Modal/Alert/Table；准确版本/后果、Paper/Live 分开，不乐观批准 |
| 运行 | Timeline/Progress/Result/Collapse；真实状态、取消、重试、排错，未知进度不编百分比 |
| 设置 | Form/Radio/Select/Slider/Switch；连接、模型/effort/默认互相独立，marks 来自真实能力 |

每页支持 loading、empty、error、stale、permission denied、partial、offline；空列表不是接口故障或合同不兼容。提交禁重复仍依赖服务端幂等；409 展示版本变化与需重载字段，不覆盖。审批/secret 不乐观更新；后端 available_actions 决定可操作项，前端不猜状态权限。生成客户端不得 as any、大量 optional/default0、catch-return-empty 掩盖错误。

390/768/1440 全部核心操作可达；窄屏表格分组详情/横向查看，不隐藏批准/取消/配置。键盘、焦点/嵌套模态、屏幕阅读器、44px 触摸目标、非颜色状态、reduced motion、安全区域和多语言逐项验收。

PWA 只缓存静态 shell；业务 API/认证/证据/审批/产物/SSE NetworkOnly。离线禁止 mutation。新版本由 Service Worker 生命周期检测并提示用户确认；未保存表单/审批对话框不强刷，不循环刷新。浏览器断线不取消运行。

## 10. 身份、安全与运维

### 10.1 浏览器认证的具体实现合同

浏览器 session 复用 tower-sessions 0.14.0 与官方 SQLx PostgreSQL Store 0.15.0；
TOTP 复用 totp-rs 5.7.0（SHA1 / 6 位 / 30 秒），密码学复用 RustCrypto。
Cookie 只承载原生 opaque session ID，Secure（HTTPS）、HttpOnly、SameSite=Strict、
Path=/，不放 TOTP secret、验证码、Provider token 或业务授权。每次请求还必须查询
下述独立授权记录；不能因为会话 middleware 的并发保存而复活已注销/撤销的登录。

- `bootstrap_capabilities`：id、原生 Argon2id verifier、created_at、expires_at（最多15分钟）、
  consumed_at；仅本机特权 CLI 可签发，原始值仅一次输出。start 在锁定 capability 与
  singleton auth state 的事务内消费；同一 capability 不能展示第二份二维码。
- `auth_enrollments`：id、capability_id UNIQUE、secret_ref、browser_binding、expires_at、
  confirmed_at；secret 为成熟 AEAD 加密文件的 UUID 引用，browser_binding 是短期浏览器
  session 内独立随机关联值，不是 Operator 身份。QR/provisioning URI 只在 start 响应
  展示一次，不存在 GET 回读接口；响应丢失需本机重新发证，不能降级为公网无保护初始化。
- `browser_logins`：id、auth_epoch、authenticated_at、expires_at、device_id?、revoked_at。
  login_id 仅保存在原生 server-side session 中；没有把它本身设计成可直接使用的 bearer。
  信任浏览器最多30天，普通登录12小时；设备撤销、session_epoch 变化、到期或注销立即
  使每次权限检查失败。trusted_devices 的 verifier_ref 只引用对应 native-session 授权
  记录，不再保存/实现另一套 browser token。logout 先持久撤销再清空原生 session。
- `auth_rate_windows`：operation（bootstrap/login/reauth）、window_started_at、attempts。
  同一部署的全局60秒窗口最多5次尝试，在验证前短事务原子预约；多 API 实例不因进程
  重启或多 IP 绕过。失败也占用尝试。数据库不可达时拒绝认证，不退回进程内允许状态。

所有表使用A0的UUIDv7/时间/共有字段；各次初始化和TOTP接受锁定同一 auth singleton。
已接受的 step 只能递增；有限时钟宽容为 DB 当前30秒步的±1，匹配由上游 constant-time
TOTP check 计算，QZ 不重写算法。确认初始化与首个登录记录同一事务提交；重放/两个
并发confirm最多一项成功。近期认证为最多300秒；过期必须经独立 reauth 接口重新验证
TOTP，不能仅修改 session 时间。原始 code/token/provisioning URI 不记录到日志、审计或
command receipt。

新增浏览器接口：GET `/auth/session`、POST `/auth/verify`、GET `/auth/devices`、DELETE
`/auth/devices/{id}`（近期认证）。服务端配置明确 public URL；所有浏览器 mutation 验证
精确同源 Origin，拒绝缺失/null/不同 scheme、host 或 port；CORS 不开放通配。
仅显式 loopback development 配置可使用 HTTP，且监听地址也必须为 loopback；不存在
skip-auth 开关。配置错误在启动时失败，不暴露未认证的业务写入口。

Secret 文件格式使用 XChaCha20-Poly1305，随机 nonce 与明确 UUID/purpose AAD，
加密主密钥为仅owner可读的32字节本机文件（不随数据库备份一起存储）。库负责原生
加密/随机/verifier，cap-std 负责受限根目录读写；UUID命名、create_new、同步后只读
发布，禁止任意路径、symlink越界、覆盖旧版本。轮换新建版本，不更改已有引用。
本实现不把原生密码学完整性用作业务资格或内容身份。


浏览器正常登录只输入 Google Authenticator-compatible 6 位 TOTP，不提交 username/password。首次初始化需要本机 CLI 一次性 bootstrap capability 或可信本地入口，不能公网抢绑；二维码/secret 仅受控 enrollment 展示，确认后 CAS 初始化并关闭 setup。TOTP 原生算法、防重放 last step、限流、信任浏览器撤销、注销/session epoch 均测试。

Cookie Secure/HttpOnly/SameSite，同源 Origin/CSRF；机器/CLI 使用独立范围受限可撤销 token，不把浏览器 cookie/TOTP secret/动态码当 API token。Agent MCP 不复用 Operator session。TOTP/session/AEAD/随机 verifier 使用成熟库，依第 0 节选语言，不自制密码学。Secret 仅受信任进程解析；UI 只见 configured/status/last_checked；日志不含 auth 文件、token、完整 Provider/stderr/traceback。

目标 Compose 为 server、worker、PostgreSQL+PGMQ；Codex/远端按 profile 配置，单机也保持权限分区。生产同源 HTTPS，未认证写接口不能暴露。默认不托管在线 wheel 上传/安装/插件市场；受支持集成经显式版本/能力登记，既有使用固定 release。上游 Python import 只在隔离 job/必要受控适配，不长期热加载/卸载不可信插件。

配置至少包括：HTTP bind/public URL、数据库/PGMQ、artifact root/backend、runtime endpoint/credential ref、Codex binary/CODEX_HOME、代理与 egress allowlist、预算/资源限制、session/TOTP secret ref、日志脱敏、backup destination/retention、telemetry opt-in。缺失 fail fast 指明字段，不退到公网无认证。来源显示 SYSTEM/EXPLICIT/DEFAULT 与安全摘要；启动验证镜像、协议、schema、ABI，不等首次真实研究才崩溃。

统一 request_id/project_id/cycle_id/run_id/attempt，tracing/OpenTelemetry 兼容；指标含队列等待/重投/lease loss、时长、未确认取消、孤儿任务、数据失败、预算耗尽、审批过期、反馈迟到。readiness 分 research/sealed/portfolio/paper/live，含组件、状态、reason、checked_at/valid_until；健康检查不每次启动 Codex/付费调用。检测连接是显式有总超时动作。

复用 PostgreSQL 原生备份/pgBackRest、restic 等，不建备份平台。备份数据库、引用 artifacts、配置和受保护原生 Codex profile；市场目录由原所有者按版本备份，密钥与数据分离。恢复先暂停 admission，恢复一致版本、查悬空引用、reconcile 未完成远端任务，再恢复消费；不盲目重放 Live。重置/恢复明确处理旧 session/设备/凭据。RPO 24h/RTO 60min 是待演练目标，只有实际记录才声称达到。

升级检查版本、磁盘、备份与兼容矩阵；不可逆 schema 用备份恢复回滚，不声称旧二进制任意读新 schema。磁盘满/DB断连/runtime离线停止接新任务并明确告警。恢复报告包含备份时点、DB/产物验证、reconcile 清单、未重复 Handoff、凭据处理、耗时和损失区间。

## 11. 迁移、删除与 README

升级的写入切换点：先暂停新 HTTP/CLI/MCP 命令与 Worker 调度，结束旧事务，再用 `cargo run --locked -p server -- migrate`。Store 在专用连接上先取得 SQLx 原生迁移 advisory lock，随后开启 READ COMMITTED 外层事务，从原生 catalog 读取现有 app 普通/分区表，先认证状态、后其余表按名称稳定顺序取得 SHARE ROW EXCLUSIVE 锁；取得全部锁后才运行 SQLx Migrator。SQLx 原样校验已应用 checksum、用原生嵌套 savepoint 执行所有待应用文件，外层提交同时公开整个批次及迁移记录；禁止 no-transaction 迁移。新库无 app 表时仍由原生迁移锁串行化。锁等待超时或校验失败整体回滚，专用连接关闭以释放 session advisory lock，不返回运行连接池；不能用独立 SQLx CLI/逐条 SQL 对活跃实例升级。本合同不声称零停机升级；锁获取之前已提交的旧事实必须被新的回填/检查看见，不能宣称锁请求一发出旧事务就已停止。

对曾部署 0005 的实例，0006 是独立、原样可核验的修复迁移：第一步锁相关表，补齐旧窗口漏掉的 evaluation_publications，再检查全部 Degradation 精确关联。非法历史不删除、不改标签，迁移失败并保留。已初始化认证强制令 session_epoch 大于当前值及全部历史 browser_logins/trusted_devices/operator_command_grants 的 epoch 最大值，避免“先回退、再加一”误复活旧会话；超过 bigint 范围则整个升级失败，不回绕。以 command_receipts 的 SYSTEM_MIGRATOR/AUTH_UPGRADE_INVALIDATE/固定迁移版本记录原、新 epoch 和原因，resource_id 绑定真实 auth_state.id，不记录秘密。已有登录、信任设备和一次性授权失效，用户重新 TOTP 登录；未初始化新库不做无意义撤销。已应用迁移重跑只验证 checksum，不重复撤销。后续每次升级继续使用同一个外层写入隔离入口。

新数据库/数据卷/API v2，不不可恢复重置原库：冻结旧写入 → 一致性备份/导出 → 新 schema → 导入映射/校验 → 只读对照 → 全链路验收 → 显式切换 → 观察/回滚窗口。不长期双写，不为语言删除有依据的合格复用；被替代的旧入口/架构/重复真相必须移除。

旧 Research/Run/Artifact 保留追溯；不能证明等价的 Strategy 为 LEGACY_REVALIDATION_REQUIRED，旧 PASS 不自动变新 qualification；旧审批/Handoff 只读不自动触发 Live。认证迁移独立，默认保留旧凭据，选择新原生 profile 则显式登录，不偷读/删除宿主 auth.json。导入报告包含 ID 映射、逐类行数、关系完整性、时间/精度、产物可读率、失败/人工决策/legacy 重验及未继承权限/审批/凭据，不能只看脚本无异常。

旧 API/db/Alembic/jobs/harness/auth/science/portfolio/remote/plugin/前端路径逐项标记复用或替换；保留已发布旧迁移语义和只读导出，Git 保存代码历史。删除自研投递、重复 LLM/OAuth、伪指标、错误 execution-control、在线插件市场、Radix/重复图表及过时永久 PASS 文档；删除旧代码同时删除仅适用于旧系统的测试；不能把剩余测试绿色当完整新系统验收。

README 对标 uv 的清晰定位/快速使用、Nautilus 的架构与支持边界、Qlib 的数据准备/实际流程、RD-Agent 的可运行研究示例、Ant Design 的文档/生态导航；不借用上游性能/收益/全部功能当本项目已交付。中文为主，英文状态同步。最终结构：一句话是什么/不是什么；真实 E2E 截图/短演示；已验证能力与限制；真实架构图；无付费凭据 Demo；原生登录/数据/远端/预算真实启动；流程与证据；开发测试；部署备份升级故障安全；路线图贡献许可证/第三方。

Demo 一条文档命令启动，synthetic/fixture 明显且不能生产领取；真实模式不依赖测试 seed/手工 SQL。所有 Quickstart、CLI Help、配置/Skill 示例和生成合同进入 smoke；不存在命令就不能写“一键可用”。截图来自真实界面，不用概念图冒充。README/Skill 不复制领域状态机，实际 CI/Review 链接替代永久 RELEASE READY 声明。

## 12. 工作包与完成边界

| 工作包 | 必须输出 | 证明 |
|---|---|---|
| W0 | 原生版本/ABI/科学镜像、Codex 协议、PGMQ 事务、Nautilus 目标适配和复用登记 | 真实依赖与最小任务，不是 mock |
| W1 | Brief/Cycle/Run/Evidence/Alpha/Candidate/Release/Delivery，字段模型和导入 | 新库/真实旧快照、约束/关系报告 |
| W2 | Rust 优先控制面 API/Worker/CLI/MCP、事件、预算、幂等、鉴权 | 状态机/API/并发与复用取舍证据 |
| W3 | 原生 Codex、远端、隔离 job、取消/恢复 | 真实 stdio/工具/进程/断连故障 |
| W4 | PIT、分区、科学调用、trial ledger、独立评估 | 时间泄漏/缺数据/过拟合/非泄漏 golden |
| W5 | 多 Alpha 原生优化、共享资金、Paper/Live、Forward/Wake | 数值参考、竞态、去重与自动闭环 |
| W6 | 六域官方 antd、移动/PWA/状态/配置 | 三视口 Playwright/axe、截图 |
| W7 | 备份恢复、升级/切换、迁移、README/CLI/Skill、清理 | 冷启动/恢复演练、docs smoke、残留检查 |
| W8 | 全部 T01–T42、检查族、Review、合并后证据 | 最新 Head/merge/main 可复核 |

新增选择先说明消除哪些第一方代码、增加哪些运维成本。核心缺口在同一 PR 解决，不用空实现、永久关闭 Feature Flag、缩小范围或 Future Work 跳过。覆盖率不是正确性；相同 fixture 可共享但不能空断言。

结束顺序：完整实现同一 PR → 最新 Head 所有适用 CI 通过、所有 review threads 解决且 `@codex review` **明确无问题** → 才允许 merged → main 检查、迁移/完整链路/隔离/恢复/文档证据回填 → 才关闭 #62。更新 Head 必须重新满足；缺失、失败、取消、应运行却跳过、额度不足、未回复、旧 Head 或仅 emoji 都不算通过。创建 Issue/方案/空页面/PR/mock 不算完成。

**GitHub 上 Codex 只承担 review，禁止要求其修复、实现、提交或自动处理。执行者自行分析、修改、补测、push 后再请求 review。** 普通 PR 不携带生产密钥；真实账号/许可数据/远端只在经过审查、锁定待交付 Head、最小权限的受保护环境验收。缺账号/数据/额度/权限为 BLOCKED，不是 skipped pass。禁止 pull_request_target 等把未审查代码放进 secret-bearing 环境；不用真实下单证明代码正确。

# 附录 A：完整字段级数据模型

以下是正式目标合同，不声称数据库已经实现。逻辑记录不意味着同等数量的服务/页面/框架。字段可在一致迁移中统一命名，但语义、必填性、约束和权限不得缺失。所有本地引用为真实 FK，未标 `?` 的字段必填；类型别名和共有字段按 A0。本文包含源附录 B 的 SQL 补充，不需到外部 Issue 补全。

## A0. 类型、共有字段与写入

- `Id=UUIDv7`，JSON 标准 UUID 字符串；外部 ID 单独保存，不冒充本地 FK。
- `Time=timestamptz`，JSON UTC RFC3339；市场 ns 为 Arrow timestamp(ns,UTC)，JSON 十进制字符串，不经 JS Number 截断。
- `Rev=bigint>=1`，JSON 十进制字符串；可变对象 `expected_revision` CAS。计数依字段范围，涉及 bigint 的 wire 值用字符串。
- `Decimal=numeric(38,18)`，JSON 十进制字符串；线格式为1–64个ASCII字符、可选正负号、非指数普通小数。允许前导/尾随零及`.5`/`1.`，但规范化后整数最多20位、有效小数最多18位，不截断或舍入；JSON Schema同时限制词法、长度和可表示范围。Money 带 ISO currency。tick/lot/price precision 复用 Nautilus，不另造算法；是否允许负权重由 mandate 决定。
- `Metric=finite f64|null`；不接受 NaN/Infinity，null 带 status/reason。bool 只接受 bool，不混淆省略/null/false。
- 每表 `id:Id PK, created_at:Time`。可变表另有 `updated_at:Time, revision:Rev`；immutable 禁 UPDATE/DELETE，撤销/修订追加；append-only 没有伪 mutable revision。
- 默认归档，不级联删除引用的研究/评估/审批/交付。封闭 enum 与合同/DB CHECK 一致；状态迁移带当前 state/revision，不接受客户端终态赋值。
- JSONB 仅存版本化上游配置/政策，schema_version、严格字段校验、unknown-field 拒绝或明确兼容；不用 dict[str,any] 隐藏领域。
- Ref 只能服务端登记的原生对象，不能用户/Agent 提交 file:///etc/passwd、任意公网/内网 URL/bucket 路径。
- Web/CLI/MCP 同一领域服务/权限合同，Rust 优先按第 0 节，Agent/job 无 DB 凭据。
- 发布引用环用 nullable draft pointer 或同事务分配 ID + DEFERRABLE FK，不禁 FK/跨事务半发布。owned FK 优先 `(id,project_id)` 复合唯一/外键，血缘无环等由事务校验并发测试。

## A1. 项目、Brief、周期与预算

```text
projects [mutable]
  root_lineage_id: Id FK research_lineages
  name: varchar(120)
  description: text default ''
  state: DRAFT|ACTIVE|PAUSED|ARCHIVED
  current_brief_id: Id? FK research_briefs
  current_automation_policy_id: Id? FK automation_policies
  created_by: OPERATOR|IMPORT
  archived_at: Time?

research_lineages [append-only]
  origin: NEW|FORK|LEGACY_IMPORT
  parent_lineage_id: Id? FK research_lineages
  legacy_reference: text?
  reason: text

research_briefs [DRAFT mutable; FROZEN immutable]
  project_id: Id FK projects
  version: int >= 1
  hypothesis: text
  economic_rationale: text
  universe_version_id: Id FK universe_versions
  target_kind: SCORE|EXPECTED_RETURN
  horizon_kind: FIXED_BARS|FIXED_DURATION|VARIABLE_INTERVAL
  horizon_value: bigint? > 0
  base_currency: char(3)
  benchmark_ref: Id? FK benchmark_versions
  evaluation_policy_id: Id FK evaluation_policies
  execution_assumptions_id: Id FK execution_assumptions
  budget: BudgetV1
  stop_rule: StopRuleV1
  state: DRAFT|FROZEN
  frozen_at: Time?
  supersedes_id: Id? FK research_briefs
  revision: Rev  # only DRAFT updates

brief_data_bindings [append-only after Brief freeze]
  brief_id: Id FK research_briefs
  dataset_revision_id: Id FK dataset_revisions
  role: DISCOVERY|VALIDATION|SEALED|FORWARD
  access_policy: METADATA_ONLY|RESEARCH_READ|EVALUATOR_ONLY

research_cycles [mutable]
  project_id: Id FK projects
  brief_id: Id FK research_briefs
  ordinal: int >= 1
  trigger: OPERATOR|SCHEDULE|DEGRADATION|NEW_DATA
  wake_id: Id? FK wake_events
  state: QUEUED|RUNNING|WAITING_INPUT|PAUSING|PAUSED|COMPLETED|CANCELLED|FAILED
  outcome: QUALIFIED_CANDIDATES|NO_SUPPORTED_CANDIDATE|BUDGET_EXHAUSTED|INCONCLUSIVE|null
  budget_snapshot: BudgetV1
  reserved_experiments: int >= 0
  used_experiments: int >= 0
  reserved_cpu_seconds: bigint >= 0
  reserved_tokens: bigint >= 0
  used_tokens: bigint >= 0
  reserved_model_cost: Decimal? >= 0
  used_model_cost: Decimal? >= 0
  model_cost_currency: char(3)?
  started_at: Time?
  ended_at: Time?
  next_action: text?
```

`unique(research_briefs.project_id,version)`、`unique(research_cycles.project_id,ordinal)`；current_brief 同项目且 FROZEN。Brief 冻结后内容/数据绑定不可改。预算预约与入队同事务，多 Worker 不可读剩余额度后各自超发。fork 接父血缘；legacy exposure unknown 不得换 UUID 获全新 sealed。

```text
BudgetV1:
  schema_version: 1
  max_experiments: u32
  max_parallel_runs: u16
  max_turns_per_mission: u16
  max_repair_turns: u16
  max_wall_seconds: u32
  max_cpu_seconds: u64
  max_memory_mib: u32
  max_output_bytes: u64
  max_cycles_per_day: u16
  min_cycle_interval_seconds: u32
  max_tokens: u64?
  max_cost_decimal: decimal-string?
  cost_currency: string?
  cost_enforcement: UNAVAILABLE|ESTIMATED|EXACT
StopRuleV1:
  schema_version: 1
  stop_on_qualified_count: u16
  stop_on_budget: bool default true
  stop_on_no_improvement_trials: u16?
  stop_on_invalid_data: bool default true
```

无准确计费能力不得接受 EXACT；费用值/币种配对。 模型预算准入必须在同一Cycle锁下读取已经消耗和仍在预约中的Token/费用，比较 `used + reserved + requested <= frozen_limit` 后才提交预约和入队。费用使用BigDecimal精确运算，所有加法检查PostgreSQL bigint及NUMERIC边界；不同币种不能相加或自动兑换。配置成本上限时，三个成本字段必须完整已知且币种与政策一致；缺用量或缺新请求估算报不可用，不将null当0。新模型请求必须预约正Token上限，估算成本允许有可信依据的显式0；只有受信任分派器认定的非模型任务才可不带模型预约。此区分和预算不能由Agent自报。

每次原生模型请求、工具后的后续Turn、修复和重试均纳入预约，不因沿用Thread、失败或新Attempt清零。消耗结算必须绑定精确的原生请求/Attempt并幂等转移预约到已用计数；结果未知保留预约，不能因断线/取消请求提前退款。实际用量超过预约也要如实记录并阻断后续准入，不能将账本裁到上限。ESTIMATED只对估算预算作准入，不承诺Provider最终账单严格不超过金额，缺准确计费继续拒绝EXACT。纯领域函数的这部分检查不等于正式Worker与数据库结算链路已经交付。

模型轮数与 token/费用在相同准入事务内预约，但轮数属于精确 Mission（run_id），不能把整个 Cycle 的多个 Mission 合并计数。每个模型请求必须给出可信调度器绑定的 mission_id 和 turn_kind=RESEARCH|REPAIR；每次恰好预约一轮，REPAIR 同时占总轮数和修复轮数。Mission 从不可变逐轮账本投影 used_turns/reserved_turns/used_repair_turns/reserved_repair_turns（u16，JSON整数，数据库非负约束），不另存可被重置的权威计数，repair 分别不超过相应 total；准入使用 used+reserved+1 与冻结 max_turns_per_mission/max_repair_turns 比较，checked_add 溢出必须拒绝。研究者不能自行创建新 Mission 或更改 turn kind 来重置/扩大预算。续轮/工具后续轮使用同一 Mission 的单独 model-turn 准入，不重复占用实验数、运行并发槽或 job CPU；真正新任务仍要完整预约。不同 Mission 的轮数隔离，Cycle 的 token/费用仍全局累计。缺失或身份不一致的 Mission 账目报错，不默认为零。

模型发送前先在同一事务持久化轮数/token/费用预约与 pgmq.send；原生分派器在独立短事务持久化唯一发送意图，再做外部 I/O。命令幂等绑定与最终实际用量 receipt 分开，准确阶段见 A6.1。ACK 丢失不释放预约/重新开轮，必须先按原生 Thread/Turn 对账；已发送轮即使失败/取消也计已用，重试和修复同样占额。只有确认从未发送才释放未用预约。首次 Mission 的零账目只能由可信服务和新的 run 在同事务创建；独立 Reviewer 是独立受控 Mission，不用重置研究者计数冒充隔离。

Optuna 内部 trial 使用预分配预算，不能藏在一次 job 无限搜索。资源/turn/并行上限必须有效正值且符合 runtime capability；修复 turn 不超过总 turn。停止规则由用户冻结，Agent 不能扩大。

## A2. 数据、Universe、基准与执行假设

```text
data_sources [operator mutable]
  name: varchar(120)
  runtime_id: Id FK runtime_integrations
  native_catalog_ref: registered text
  provider_kind: registered adapter enum
  enabled: bool default true

universe_versions [immutable]
  name: text
  membership_artifact_id: Id FK artifacts
  instrument_definition_artifact_id: Id FK artifacts
  calendar_ref: text
  calendar_version: text
  selection_asof: Time
  has_historical_membership: bool
  coverage_start: Time
  coverage_end: Time

benchmark_versions [immutable]
  name: text
  dataset_revision_id: Id FK dataset_revisions
  return_kind: TOTAL_RETURN|PRICE_RETURN|CASH
  currency: char(3)
  frequency: text

dataset_revisions [immutable published metadata]
  source_id: Id FK data_sources
  data_use_grant_id: Id FK data_use_grants
  native_snapshot_ref: text
  native_storage_version: text
  universe_version_id: Id FK universe_versions
  schema_version: text
  data_kind: BAR|QUOTE|TRADE|ORDER_BOOK|FUNDAMENTAL|EVENT|DERIVED_FEATURE
  partition_role: DISCOVERY|VALIDATION|SEALED|FORWARD
  event_start: Time
  event_end: Time
  available_through: Time
  row_count: bigint >= 0
  timezone: text
  quality_artifact_id: Id FK artifacts
  pit_status: VERIFIED|UNVERIFIED|INVALID
  revision_policy: AS_KNOWN_THEN|RESTATED|UNKNOWN
  origin: REAL|SYNTHETIC|FIXTURE|LEGACY_UNKNOWN

execution_assumptions [immutable]
  venue_capability_ref: text
  engine_image_ref: pinned native OCI reference
  price_type: MID|BID_ASK|TRADE|BAR
  starting_capital: Decimal > 0
  base_currency: char(3)
  fee_schedule_artifact_id: Id FK artifacts
  slippage_model: NativeModelRefV1
  fill_model: NativeModelRefV1
  latency_model: NativeModelRefV1?
  liquidity_artifact_id: Id? FK artifacts
  cost_assumption_status: DATA_BACKED|CONSERVATIVE_ASSUMPTION|INSUFFICIENT
  participation_limit: Decimal? in (0,1]
  calendar_version: text
  settlement_rule_ref: text
```

`event_start < event_end`；发布 snapshot 不原地覆盖，更新新目录/版本；许可与用途匹配。PIT 报告证明 available_at 来源，不用 ingest_at 替代。Universe 含退市/到期；静态今日成分明确有偏，不能称完整历史池。

`NativeModelRefV1={schema_version,adapter_kind,upstream_class,upstream_version,parameters}`。class/adapter 来自服务端 allowlist 和实际 capability；parameters 为对应锁定适配器的严格 schema。未知项拒绝，不映成 GENERIC/DEFAULT；禁止任意 Python import/path/exec 越界。

### A2.1 不可变数据授权与原生身份

将可变 `data_sources.license_reference/allowed_uses` 移除；仅 name/enabled 可变。runtime_id/native_catalog_ref/provider_kind 一经引用不可变。`dataset_revisions` 增加 `data_use_grant_id:Id FK data_use_grants`，授权属于同 source（复合FK）。

```text
data_use_grants [immutable]
  source_id: Id FK data_sources
  version: int >= 1
  license_reference: nonempty text
  evidence_artifact_id: Id FK artifacts
  allowed_uses: RESEARCH|RESEARCH_AND_PAPER|RESEARCH_PAPER_LIVE
  valid_from: Time
  valid_until: Time?
  authorized_by: OPERATOR
data_use_revocations [append-only]
  grant_id: Id FK data_use_grants
  effective_at: Time
  reason: nonempty text
```

`unique(source_id,version)`；grant.valid_until 为空或晚于valid_from。每次新消费/发布/审批/Claim按DB时间检查精确grant和撤销，历史保留当时授权；升级不自动扩张旧dataset用途，撤销不回写历史。换授权需要明确新原生snapshot发布及新证据，不允许仅换UUID洗旧sealed。

必须 `unique(dataset_revisions.source_id,native_snapshot_ref,native_storage_version)` 和 `unique(data_sources.runtime_id,native_catalog_ref)`；相同原生身份同请求返回已有记录，不同partition/授权等409。服务端registry规范化来源；迁移/别名映射已有身份并继承暴露，无法证明独立时LEGACY_UNKNOWN，不能获得sealed资格。此项不得以应用内容hash实现。

## A3. 实验、产物、Alpha 与校准

```text
experiment_families [immutable identity]
  project_id: Id FK projects
  root_lineage_id: Id FK research_lineages
  question: text
  selection_policy_id: Id FK evaluation_policies

experiments [immutable proposal; mutable execution pointer until consumed]
  cycle_id: Id FK research_cycles
  family_id: Id FK experiment_families
  parent_experiment_id: Id? FK experiments
  ordinal: int >= 1
  hypothesis: text
  expected_failure_modes: text
  proposal_artifact_id: Id FK artifacts
  code_artifact_id: Id? FK artifacts
  parameter_artifact_id: Id? FK artifacts
  trial_source: CODEX|OPTUNA|OPERATOR
  native_study_ref: text?
  native_trial_id: text?
  run_id: Id? FK runs
  outcome: PENDING|SUPPORTED|REJECTED|INVALID|INCONCLUSIVE
  outcome_reason: text?
  conclusion_artifact_id: Id? FK artifacts

artifacts [immutable]
  project_id: Id? FK projects
  producer_run_id: Id? FK runs
  producer_attempt_id: Id? FK run_attempts
  kind: CODE|PARAMETERS|SIGNALS|TARGETS|REPORT|METRICS|DATA_QUALITY|MODEL|PACKAGE|LOG|MIGRATION
  media_type: text
  schema_name: text
  schema_version: text
  storage_backend: LOCAL|OBJECT_STORE|NATIVE_CATALOG
  storage_object_ref: text
  storage_version: text
  byte_count: bigint >= 0
  access_class: OPERATOR|RESEARCH|EVALUATOR_ONLY|DELIVERY
  origin: REAL|SYNTHETIC|FIXTURE|LEGACY_UNKNOWN
  created_by: OPERATOR|RUNTIME|AGENT|IMPORT
  retention_class: REFERENCED|TEMPORARY|AUDIT

alphas [mutable display/active pointer]
  project_id: Id FK projects
  name: text
  lifecycle: RESEARCH|QUALIFIED|SUSPENDED|RETIRED
  active_version_id: Id? FK alpha_versions

alpha_versions [immutable]
  alpha_id: Id FK alphas
  version: int >= 1
  experiment_id: Id FK experiments
  root_lineage_id: Id FK research_lineages
  code_artifact_id: Id FK artifacts
  model_artifact_id: Id? FK artifacts
  signal_contract_version: text
  signal_kind: SCORE|EXPECTED_RETURN
  horizon_kind: FIXED_BARS|FIXED_DURATION|VARIABLE_INTERVAL
  horizon_value: bigint?
  forecast_unit: RETURN_PER_HORIZON|RESIDUAL_RETURN_PER_HORIZON|UNITLESS_SCORE
  calibration_id: Id? FK calibrations
  runtime_image_ref: text

calibrations [immutable]
  estimator_kind: registered native estimator
  estimator_version: text
  model_artifact_id: Id FK artifacts
  train_input_set_id: Id FK input_sets
  fit_end_available_at: Time
  output_unit: text
  horizon_kind: text
  horizon_value: bigint?
  validation_evaluation_id: Id FK evaluations
```

`unique(cycle_id,ordinal)`、`unique(alpha_id,version)`；parent experiment 无环，root lineage 不可由 Agent 改。保留失败实验/试验次数。草稿可未绑定必要产物，但一经 evaluator/qualification/Release 引用即冻结，发布事务检查完整输入版本；不能改正在评估的实验。storage_version 是原生存储/发布目录版本，不是自研内容 hash ID。

Arrow `qz.alpha_signal.v1`：

```text
instrument_id: utf8 non-null  # Nautilus native ID
asof_ns: timestamp(ns,UTC) non-null
available_at_ns: timestamp(ns,UTC) non-null
horizon_end_ns: timestamp(ns,UTC) non-null
score: float64 nullable
expected_return: float64 nullable
uncertainty: float64 nullable
coverage_status: utf8 non-null
alpha_version_id: utf8 non-null
```

`available_at <= asof < horizon_end`；score/expected_return 按 signal_kind 校验；uncertainty 未估计为 null，不能填 confidence=1；唯一 `(alpha_version_id,instrument_id,asof_ns,horizon_end_ns)`。元数据含币种、单位、horizon、dataset revision。预测表不含 broker_key/order_id/quantity/真实 account/position。

原生产物身份必须唯一：`UNIQUE(storage_backend,storage_object_ref,storage_version)`。登记前由受信任存储适配器解析规范原生引用，不允许路径、bucket或挂载别名产生新身份。相同原生身份及完整不可变元数据的重试返回原artifact_id；origin、access_class、schema、project或其他不可变字段冲突返回409，不能借新UUID将FIXTURE/已暴露证据改标REAL/DELIVERY。不能以修改UUID或自建内容hash替代这一约束。

## A4. 输入、政策、评估、资格与暴露

```text
input_sets [DRAFT mutable; FROZEN immutable]
  project_id: Id FK projects
  purpose: DISCOVERY|VALIDATION|SEALED|PORTFOLIO|FORWARD
  decision_cutoff: Time
  frozen_at: Time?  # null only while its membership is assembled
  revision: Rev

input_set_items [immutable]
  input_set_id: Id FK input_sets
  dataset_revision_id: Id? FK dataset_revisions
  artifact_id: Id? FK artifacts
  role: registered contract enum
  ordinal: int >= 0
  CHECK exactly one of dataset_revision_id/artifact_id

evaluation_policies [immutable]
  project_id: Id FK projects
  version: int >= 1
  selection_rule: SelectionRuleV1
  split_policy: SplitPolicyV1
  metric_requirements: MetricRequirementV1[]
  minimum_observations: int > 0
  maximum_missing_fraction: Decimal in [0,1]
  require_real_data: bool default true
  required_capabilities: text[]
  maximum_sealed_uses_per_lineage: int >= 1
  validity_seconds: bigint > 0

evaluations [immutable completed record]
  project_id: Id FK projects
  subject_alpha_version_id: Id? FK alpha_versions
  subject_candidate_id: Id? FK portfolio_candidates
  CHECK exactly one subject
  input_set_id: Id FK input_sets
  policy_id: Id FK evaluation_policies
  run_id: Id FK runs
  evaluation_kind: DISCOVERY|WALK_FORWARD|SEALED|PORTFOLIO|FORWARD
  execution_status: SUCCEEDED|FAILED|CANCELLED
  evidence_status: VALID|INVALID|INCOMPLETE|UNSUPPORTED
  decision: PASS|REJECT|INCONCLUSIVE
  report_artifact_id: Id FK artifacts
  method_versions_artifact_id: Id FK artifacts
  concluded_at: Time
  valid_until: Time?

metric_values [immutable]
  evaluation_id: Id FK evaluations
  metric_code: text
  scope: text  # e.g. total/fold:2/regime:bear
  value: Metric
  status: OK|INSUFFICIENT_DATA|UNSUPPORTED|INVALID_INPUT|FAILED
  reason_code: text?
  unit: text
  period_start: Time
  period_end: Time
  observation_count: bigint >= 0
  frequency: text
  annualization_factor: Metric
  method_id: text
  method_version: text
  source_artifact_id: Id FK artifacts
  higher_is_better: bool?

qualifications [immutable]
  alpha_version_id: Id FK alpha_versions
  policy_id: Id FK evaluation_policies
  qualifying_evaluation_id: Id FK evaluations
  granted_at: Time
  valid_until: Time

qualification_revocations [append-only]
  qualification_id: Id FK qualifications
  reason_code: text
  evidence_evaluation_id: Id? FK evaluations
  effective_at: Time

evidence_exposures [append-only]
  root_lineage_id: Id FK research_lineages
  dataset_revision_id: Id FK dataset_revisions
  evaluation_id: Id? FK evaluations
  actor_kind: OPERATOR|RESEARCH_AGENT|EVALUATOR|IMPORT
  actor_session_ref: text?
  exposure_kind: RAW|SAMPLE|METRIC|PLOT|SUMMARY|LEGACY_UNKNOWN
  exposed_at: Time
  purpose: text
```

输入项要求 `UNIQUE(input_set_id,ordinal)`，并对非null的 `(input_set_id,dataset_revision_id)` 和 `(input_set_id,artifact_id)` 分别建立唯一部分索引。相同位置及同一不可变引用/role重试幂等返回原项；位置冲突或相同对象重复位置返回409，不自动改ordinal、拼接或重复消费。原生任务输入严格按ordinal升序；整个InputSet及其项目引用在同事务完整发布并冻结。

评估头及指标组成一个原子不可变聚合：创建 `evaluations` 与全部 `metric_values` 必须在同一事务内完成。内部 `evaluation_publications(evaluation_id PRIMARY KEY FK evaluations)` 记录封口，不增加公开状态、hash、事务 ID 或时间戳身份；延迟约束触发器在创建事务提交前写入标记。指标插入先锁对应 evaluation，已封口则拒绝，不能通过晚到指标改写旧 Qualification/Release 的证据。Qualification、Release、Calibration、Exposure、Forward、Degradation 等引用在同事务内先封口评估；封口后同一事务也不能追加指标。迁移为所有已有完成评估补齐封口标记，保留既有头及指标原值。Candidate 的 allocation_evaluation_id 与被评估 Candidate 的循环引用保留原生 deferred FK，允许同一事务先创建 Candidate 头、再创建评估与指标并完整提交，不能用过早的引用守卫破坏这个顺序。指标更正需要新的评估身份及新的下游决定；发布标记只保证组成不可变，不替代完整政策验证或 PASS 判定。

Qualification必须绑定被评估的精确Alpha版本和政策：评估表提供 `UNIQUE(id,subject_alpha_version_id,policy_id)`，qualification的 `(qualifying_evaluation_id,alpha_version_id,policy_id)` 复合FK引用它；qualification另外提供 `UNIQUE(id,alpha_version_id)`，candidate_alphas的 `(qualification_id,alpha_version_id)` 必须使用复合FK而不是两条互不关联的FK。授予与使用时仍要事务检查同项目、VALID/PASS、新鲜度、撤销及Mandate政策，不允许未合格版本借用其他版本资格。

sealed 使用预约先提交再授予 evaluator 能力，失败/取消不抹去机会。Exposure 包括原始行、样本、指标、图、摘要和 legacy unknown。后续反馈按冻结披露政策，不能洗白相同 sealed。缺 required metric、实现不支持、样本不足、方法不适用或过期均 INCONCLUSIVE；无“全部 Gate 缺值自动跳过”。 对INVALID_INPUT亦必须先验证原生方法/版本/单位/频率与冻结allowlist；来源未登记或过期归UNSUPPORTED，不得驱动stop_on_invalid_data。仅可信且合同匹配的INVALID_INPUT保留INVALID。

```text
MetricRequirementV1:
  metric_code, scope
  comparator: GT|GE|LT|LE|BETWEEN
  threshold_low: Decimal?
  threshold_high: Decimal?
  required: bool
  minimum_observations: nonnegative integer
  method_allowlist: string[]
SplitPolicyV1:
  kind: WALK_FORWARD|CPCV_FIXED_HORIZON
  train_size, test_size
  step_size?, group_count?, test_group_count?
  purge_observations, embargo_observations
  label_horizon_observations?
  interval_validation_required: true
  sealed_revision_id: Id FK dataset_revisions
```

这些政策 JSON 同样有 schema_version=1；阈值按 comparator 校验数量/次序，原生方法/单位一致，不能仅比较数值。SelectionRuleV1 是冻结的候选选择合同，必须明确可比试验范围、选择指标/方向、候选数量和确定性平手规则；全部失败/淘汰仍在 trial ledger，不允许事后重定义集合。原生适配器具体可选参数由锁定版本的严格注册 schema 提供，不用任意 blob。固定 horizon 切分不能用于未支持 VARIABLE_INTERVAL。DSR/PBO 维持 UNSUPPORTED，直至真实上游与参考数据验收。

### A4.1 SelectionRuleV1 的严格线协议

```text
schema_version: 1
comparable_scope: FAMILY_LINEAGE
root_lineage_id: Id FK research_lineages
family_id: Id FK experiment_families
comparison_input_set_id: Id FK input_sets
execution_assumptions_id: Id FK execution_assumptions
evaluation_kind: WALK_FORWARD|SEALED
metric_code: nonempty string
metric_scope: nonempty string
method_id: nonempty string
method_version: nonempty string
unit: nonempty string
frequency: nonempty string
direction: MAXIMIZE|MINIMIZE
candidate_count: u16 >= 1
tie_break: EXPERIMENT_ID_ASC
missing_required_metric: INCONCLUSIVE
```

拒绝unknown字段；所有FK同project且family/root一致，candidate_count不超过冻结max_experiments，所选方法/单位/频率为真实native capability。policy/family引用环在同事务分配ID+DEFERRABLE FK。实验后规则不可改。比较集合包含同family/lineage/输入/执行假设/评估类型的全部试验，失败/取消/无效/淘汰留账本及排除理由，不以0填入排名。冻结实际experiment/evaluation ID清单；仅VALID/required指标完整且同方法口径参与finite值排序，direction优先，相等按UUID原生16字节升序。每experiment只一次；不足返回实际数量+INCONCLUSIVE，不复制赢家、不扩大政策、不重置sealed。

### A4.2 不经 f64 舍入的冻结阈值

阈值和 BETWEEN 的两端始终保留 A0 Decimal 精度，由 BigDecimal 比较；禁止先转
f64 再判断区间次序或 PASS。Metric 仍是原生 finite f64：比较语义以已锁定 Serde JSON
实际序列化出的最短 round-trip 数字作为报告值，由 BigDecimal 解析该数字后与精确
阈值比较。统计值没有因此变成数学上的精确估计，更不能拿它保存金额/权重。
例如报告值0.1不满足GE 0.10000000000000001；两个会舍入到同一f64的反向Decimal
上下界仍必须拒绝。相等的报告十进制在GE/LE闭边界通过，在GT/LT开边界拒绝。
所有u16/u32字段的生成合同同时声明本机类型上界65535/4294967295；业务最小值
和数据库更窄限制仍由相应领域校验，不以wire类型可解析替代可执行性。

## A5. Mandate、Candidate、目标与 Release

```text
portfolio_mandates [immutable versions]
  project_id: Id FK projects
  version: int >= 1
  objective: MIN_RISK|MAX_UTILITY|RISK_BUDGETING
  risk_measure: VARIANCE|CVAR
  base_currency: char(3)
  capital_assumption: Decimal > 0
  universe_version_id: Id FK universe_versions
  covariance_estimator: NativeModelRefV1
  alpha_ensemble: NativeModelRefV1
  optimizer: NativeModelRefV1
  constraints: PortfolioConstraintsV1
  rebalance_schedule: RebalanceScheduleV1
  required_evaluation_policy_id: Id FK evaluation_policies
  execution_assumptions_id: Id FK execution_assumptions
  exposure_tolerance: Decimal > 0

portfolio_candidates [immutable after result]
  project_id: Id FK projects
  mandate_id: Id FK portfolio_mandates
  input_set_id: Id FK input_sets
  decision_asof: Time
  run_id: Id FK runs
  solver_status: OPTIMAL|ACCEPTABLE_INACCURATE|INFEASIBLE|UNBOUNDED|FAILED
  evidence_status: VALID|INCOMPLETE|INVALID
  reason_code: text?
  forecast_artifact_id: Id? FK artifacts
  covariance_artifact_id: Id? FK artifacts
  diagnostics_artifact_id: Id FK artifacts
  target_artifact_id: Id? FK artifacts
  allocation_evaluation_id: Id? FK evaluations
  cash_weight: Decimal?
  current_weights_source: FORWARD_SNAPSHOT|LAST_TARGET|NONE
  current_weights_artifact_id: Id? FK artifacts

candidate_alphas [immutable]
  candidate_id: Id FK portfolio_candidates
  alpha_version_id: Id FK alpha_versions
  qualification_id: Id FK qualifications
  ensemble_weight: Decimal
  calibration_id: Id? FK calibrations
  forecast_unit: text
  coverage_fraction: Decimal in [0,1]

candidate_targets [immutable compact publish-time snapshot]
  candidate_id: Id FK portfolio_candidates
  instrument_id: text
  target_weight: Decimal
  currency: char(3)
  asof: Time
  valid_until: Time

releases [immutable]
  candidate_id: Id FK portfolio_candidates
  package_artifact_id: Id FK artifacts
  package_schema_version: text
  mandate_id: Id FK portfolio_mandates
  evaluation_id: Id FK evaluations
  market_capability_version: text
  asof: Time
  valid_from: Time
  valid_until: Time
  environment: DEMO|REAL
```

Release 必须以复合 FK `(evaluation_id,candidate_id)` 引用
`evaluations(id,subject_candidate_id)` 的唯一键；Alpha 评估、其他 Candidate 的
PASS 都不能借用。另以 `(candidate_id,mandate_id)` 绑定 Candidate 的精确 Mandate。
非空关联不足以授权：服务仍须验证独立组合模拟类型、VALID/PASS、有效期、数据用途、
资格及不可变 Package；复合 FK 不替代这些 Gate。

历史目标序列存 Arrow/Parquet，不每 bar 建业务对象。不可行 cash/targets 均 null；LAST_TARGET 是假设，真实权重输入来自下游签发 snapshot，QZ 不建真实账户账本。`sum(asset_weights)+cash_weight=1` 在 mandate tolerance 内，现金字段/保留代码明确；gross/net、组、成本、参与率原生计算，领域层独立合同/容差验证。

```text
PortfolioConstraintsV1:
  schema_version: 1
  long_only: bool
  min_cash_weight, max_cash_weight: Decimal
  min_asset_weight, max_asset_weight: Decimal
  max_gross_exposure: Decimal
  min_net_exposure, max_net_exposure: Decimal
  max_turnover_per_rebalance: Decimal
  max_participation: Decimal?
  max_ex_ante_risk: Decimal?
  group_bounds: [{group_id, min:Decimal, max:Decimal}]
  asset_overrides: [{instrument_id, min:Decimal, max:Decimal}]
  transaction_costs_ref: Id
  liquidity_ref: Id?
RebalanceScheduleV1:
  schema_version: 1
  kind: MANUAL|FIXED_INTERVAL|CALENDAR_SESSION
  interval_seconds: u32?
  calendar_ref: string?
  timezone: IANA timezone
  session_offset_seconds: i32?
  max_input_age_seconds: u32
  target_ttl_seconds: u32
```

不用的约束明确 null/empty，不能默认放宽；min<=max，与 long_only/现金/净敞口一致；原生 solver 实际不支持就报 capability 错误。日历/定时复用库不另造 Cron 平台。新 cutoff 必须新 Candidate/Release；ACCEPTABLE_INACCURATE 不能冒充 OPTIMAL。

### A5.1 候选子对象唯一性

`unique(candidate_alphas.candidate_id,alpha_version_id)`、`unique(candidate_targets.candidate_id,instrument_id)` 是数据库约束，不是普通索引。重复相同请求幂等，冲突409；至少两个不同alpha_id的合格版本才满足多Alpha，不以同Alpha多个版本或重复条目凑数。发布验证每资产唯一权重，再校验sum/gross/net/cash/约束。

## A6. Run、Attempt、事件和原生会话

```text
runs [mutable]
  project_id: Id FK projects
  cycle_id: Id? FK research_cycles
  kind: AGENT_RESEARCH|DATA_VALIDATE|ALPHA_EVALUATE|PORTFOLIO_BUILD|PORTFOLIO_SIMULATE|FORWARD_EVALUATE|EXPORT|IMPORT
  input_set_id: Id FK input_sets
  state: QUEUED|DISPATCHING|RUNNING|RECONCILING|CANCEL_REQUESTED|SUCCEEDED|FAILED|CANCELLED
  current_attempt_no: int >= 0
  active_attempt_id: Id? FK run_attempts
  last_event_seq: bigint >= 0
  deadline_at: Time
  cancellation_requested_at: Time?
  terminal_reason_code: text?
  queued_at: Time
  started_at: Time?
  finished_at: Time?

run_attempts [mutable until terminal]
  run_id: Id FK runs
  attempt_no: int >= 1
  worker_owner_id: text
  owner_epoch: bigint >= 1
  lease_expires_at: Time
  runtime_id: Id? FK runtime_integrations
  external_job_id: text?
  dispatch_state: NOT_SENT|SENT_UNKNOWN|ACKNOWLEDGED|TERMINAL
  runtime_state: UNKNOWN|PENDING|RUNNING|SUCCEEDED|FAILED|CANCELLED
  result_manifest_artifact_id: Id? FK artifacts
  accepted_at: Time?
  error_class: RETRYABLE_INFRA|PERMANENT_CONFIG|INVALID_INPUT|CANCELLED|RESOURCE_LIMIT|null
  error_code: text?

run_events [append-only]
  run_id: Id FK runs
  seq: bigint >= 1
  attempt_id: Id? FK run_attempts
  event_type: registered event type
  schema_version: int >= 1
  payload: TypedEventPayload
  occurred_at: Time

codex_sessions [mutable native references, not copied chat store]
  project_id: Id FK projects
  cycle_id: Id FK research_cycles
  run_id: Id FK runs
  role: RESEARCHER|INDEPENDENT_REVIEWER
  profile_id: Id FK codex_profiles
  profile_revision: Rev  # immutable settings snapshot, not mutable profile cache
  thread_id: text
  active_turn_id: text?
  used_turns: int >= 0
  reserved_turns: int >= 0
  used_repair_turns: int >= 0
  reserved_repair_turns: int >= 0
  codex_version: text
  protocol_schema_version: text
  requested_settings: EffectiveCodexRequestV1
  observed_model: text?
  observed_effort: text?
  observed_provider: text?
  native_history_ref: text
  public_summary_artifact_id: Id? FK artifacts
```

`unique(run_id,attempt_no)`、`unique(run_id,seq)`；external_job_id 在 runtime 唯一。lease 使用 DB 时间；接管同一次外部任务可增 owner_epoch，不因超时直接建新 attempt。持 run 行锁分配 seq、插事件、更新 last_event_seq，同一 run 已提交顺序一致；全局自增 ID 分配顺序不是事务提交顺序。

EffectiveCodexRequestV1 记录 schema_version、profile/connection 来源和实际发送的非秘密可选 model/effort/Fast 覆盖；default 开启的省略与 saved 配置区分，observed 只能来自协议可观察事实。TypedEventPayload 为注册事件的封闭版本化 union，不记录秘密、任意原始 traceback 或隐藏推理。

### A6.1 单一 Mission 会话与原生 Turn 明细

`unique(codex_sessions.run_id)` 将 Mission（AGENT_RESEARCH Run）绑定到一个会话；
`unique(profile_id,thread_id)` 防止两个 Mission 共享同一原生 Thread。重试同一
run/profile/thread/role 返回原会话，不同绑定409；role/profile/thread/run/project
绑定不可修改或删除，接管 Worker 不创建新会话。Reviewer 必须是不同 Run/Thread。
`active_turn_id` 仅可作为非权威投影，不得用于重置轮数或辨认丢失的请求。

以下五类记录是预算和原生发送的权威只追加账本，不复制聊天正文/工具循环/隐藏推理。
每表仍有 A0 的 id/created_at。所有关联以复合 FK 保证 session、run、cycle、project
属于同一个 Mission；标量均采用 A0 的 bigint 字符串/精确 Decimal。

```text
model_turn_reservations [immutable]
  project_id: Id FK projects
  cycle_id: Id FK research_cycles
  run_id: Id FK runs
  session_id: Id FK codex_sessions
  attempt_id: Id FK run_attempts
  owner_epoch: Rev  # reservation-time owner, never changed on takeover
  profile_revision: Rev  # exact settings revision frozen in the Session
  ordinal: int in [1,65535]
  command_key: nonempty text <= 200 bytes
  turn_kind: RESEARCH|REPAIR
  reserved_tokens: bigint > 0
  reserved_cost: Decimal? >= 0
  cost_currency: char(3)?
  request_artifact_id: Id FK artifacts  # immutable nonsecret structured request
  deadline_at: Time
  UNIQUE(session_id, command_key), UNIQUE(run_id,ordinal), UNIQUE(session_id,ordinal)

model_turn_dispatches [immutable; at most one per reservation]
  reservation_id: Id FK model_turn_reservations UNIQUE
  owner_epoch: bigint >= 1
  rpc_request_id: nonempty text <= 200 bytes
  UNIQUE(reservation_id, rpc_request_id)

model_turn_bindings [immutable; only native-observed acknowledgements]
  reservation_id: Id FK model_turn_reservations UNIQUE
  session_id: Id FK codex_sessions
  native_turn_id: nonempty text <= 200 bytes
  UNIQUE(session_id, native_turn_id)

model_turn_terminals [immutable; at most one per reservation]
  reservation_id: Id FK model_turn_reservations UNIQUE
  native_turn_id: text?  # null only for NOT_SENT
  outcome: SUCCEEDED|FAILED|CANCELLED|NOT_SENT
  reason_code: nonempty text <= 120 bytes
  observed_at: Time
  UNIQUE(reservation_id,outcome)
  FK(reservation_id,native_turn_id) -> model_turn_bindings

model_turn_receipts [immutable; at most one per reservation]
  reservation_id: Id FK model_turn_reservations UNIQUE
  outcome: SUCCEEDED|FAILED|CANCELLED|NOT_SENT
  actual_tokens: bigint >= 0
  actual_cost: Decimal? >= 0
  cost_currency: char(3)?
  usage_source: NATIVE_REPORT|CONFIRMED_NOT_SENT
  reason_code: nonempty text <= 120 bytes
```

模型四个轮数计数、已用/预约 token 和费用由这些不可变明细和唯一 receipt 在同一
Cycle/Mission 锁内通过 SQL 聚合投影；不另外维护一套可被重置的权威聚合缓存。
未有 receipt 的条目继续占用所有预约。NATIVE_REPORT 的任何 outcome 都计一轮已用，
REPAIR 同时计总轮和修复轮；NOT_SENT 不计已用且 actual_tokens/cost 必须为0。
已发但缺原生用量不写虚构零 receipt，不提前释放预约。一次会话只允许一个尚未
settle 的预约，完成工具后的续轮沿用同一会话；跨会话仍可按 Cycle 预算并发。

创建 reservation、预约校验及原生 `pgmq.send('model_turns', {reservation_id})`
同事务。command_key 重放必须比较 kind/请求产物/资源/期限/原始attempt等精确字段，
相同返回原条目且不重复发送，不同409；只保存非秘密字段，不引入请求hash。
新发送按 project→cycle→run→session 固定锁序，验证项目/周期状态、当前attempt、
owner epoch、数据库时钟 lease 与 deadline，然后先提交唯一 dispatch intent。
首次成功插入 intent 的 Worker 才得到 Send 一次的许可；已存在 intent 一律 Reconcile，
绝不能把 JSON-RPC id 当成原生幂等保证再次 turn/start。

若 intent 提交后在写管道前崩溃，仍按 UNKNOWN 保留占用；只有原生证据能唯一识别
该请求时才绑定 Turn。不能证明未发送就不得退额/重发；不能用相邻 Turn 的位置猜。
原生 binding 必须引用该 reservation 的相同 session，已绑定后不能换 native ID。
可信适配器可先调用 `observe_turn_terminal` 持久化原生终态，稍后用原生 usage 结算；
终态本身不退还任何预约，也不确认队列消息。缺用量时不得只存在内存或强造零 receipt。
相同终态（包括原生 ID、原因、观察时间）重传幂等，不同事实409。
`settle_turn` 要求 receipt 的 (reservation_id,outcome) 精确引用终态，原因一致；
同一原生事件若同时含终态与用量，两条记录可在同一事务产生。单纯绑定 ACK 不是完成事实。
已结算相同事实返回原 receipt，不同事实409。
当前 owner 在 lease 内才能新增绑定/结算，陈旧 Worker 不可采纳；重试时既有
相同 receipt 可读但不产生第二副作用。NOT_SENT 只允许尚无 dispatch 的条目，
一旦存在 intent 即保守拒绝。真实消耗超过预约/预算仍如实入账，后续准入阻断；
若总量无法表示为A0范围，整事务失败并保持预约，绝不 wrap/截断或称成功。

这是一段正式持久化合同；其实现与原生模型发送/同Thread结果消费、账号隔离的
验收分别记证据，不能以数据库测试冒充已接通模型。

实现命名与先前 C2–C4 规范的对应关系：Mission 即 `run_id`；不可变
reservation `id` 同时是请求身份，`command_key` 提供同 Session 的幂等命令身份，
不另造第二个重复 request UUID；`model_turn_bindings` 即原生 ACK，
`model_turn_receipts` 即用量 settlement。Profile/Thread 由不可变 Session 复合关联取得，
Session 冻结 `profile_revision`，预约引用相同 (session_id,profile_revision)。
`ordinal` 在 Session 锁内递增，NOT_SENT 不回收序号；原始 owner_epoch 只记录事实，
后续接管必须验证同 Attempt 的当前 DB owner_epoch。所有历史关联保留，不能换记录清零。

## A7. 自动化、审批、交付、Forward 与 Wake

```text
automation_policies [immutable, operator only]
  project_id: Id FK projects
  mode: MANUAL|AUTO_PAPER|AUTO_HANDOFF
  mandate_id: Id FK portfolio_mandates
  downstream_id: Id FK downstream_integrations
  required_paper_observations: int > 0
  minimum_paper_elapsed_seconds: bigint > 0
  max_feedback_age_seconds: bigint > 0
  promotion_metric_requirements: MetricRequirementV1[]
  degradation_metric_requirements: MetricRequirementV1[]
  authorized_at: Time
  valid_until: Time
  enabled_for_new_rebalances: bool
  max_rebalances_per_day: int >= 1

policy_revocations [append-only]
  automation_policy_id: Id FK automation_policies
  effective_at: Time
  reason: text

approvals [immutable]
  release_id: Id FK releases
  environment: PAPER|LIVE
  downstream_id: Id FK downstream_integrations
  authority_kind: OPERATOR|FROZEN_POLICY
  automation_policy_id: Id? FK automation_policies
  evidence_set_id: Id FK input_sets
  granted_at: Time
  valid_until: Time
  CHECK FROZEN_POLICY requires policy FK; AGENT is never an authority

approval_revocations [append-only]
  approval_id: Id FK approvals
  effective_at: Time
  reason: text

handoff_offers [mutable state; immutable bindings]
  release_id: Id FK releases
  approval_id: Id FK approvals
  downstream_id: Id FK downstream_integrations
  environment: PAPER|LIVE
  delivery_sequence: bigint >= 1
  state: OFFERED|CLAIMED|ACKNOWLEDGED|REJECTED|REVOKED|EXPIRED
  external_claim_id: text?
  offered_at: Time
  expires_at: Time
  claimed_at: Time?
  acknowledged_at: Time?

forward_messages [append-only]
  downstream_id: Id FK downstream_integrations
  external_message_id: text
  handoff_id: Id FK handoff_offers
  stream_id: text
  sequence: bigint >= 0
  message_revision: int >= 1
  supersedes_message_id: Id? FK forward_messages
  window_start: Time
  window_end: Time
  coverage_status: COMPLETE|PARTIAL|CORRECTION
  observation_count: bigint >= 0
  report_artifact_id: Id FK artifacts
  issued_at: Time
  received_at: Time

forward_evidence_windows [immutable evaluated snapshot]
  release_id: Id FK releases
  input_set_id: Id FK input_sets
  evaluation_id: Id FK evaluations
  window_start: Time
  window_end: Time
  complete_observations: bigint >= 0
  is_contiguous: bool
  freshness_deadline: Time

degradation_observations [append-only]
  project_id: Id FK projects
  release_id: Id FK releases
  evaluation_id: Id FK evaluations
  policy_id: Id FK automation_policies
  classification: HEALTHY|WATCH|DEGRADED|INSUFFICIENT_DATA
  reason_codes: text[]
  observed_at: Time

wake_events [mutable delivery state]
  project_id: Id FK projects
  observation_id: Id? FK degradation_observations
  trigger: DEGRADATION|DATA_AVAILABLE|OPERATOR|SCHEDULE
  state: PENDING|SUPPRESSED|CONSUMED|CANCELLED
  not_before: Time
  consumed_cycle_id: Id? FK research_cycles
  reason: text
```

唯一 `(downstream_id,environment,delivery_sequence)`、`(downstream_id,external_message_id)`；同 observation 不重复同类自动 Wake。Correction 追加替代引用，不覆盖旧消息；重叠窗口不能加总 observation_count。自动晋级事务验证 ACTIVE、Operator 政策有效未撤销、Release/资格/数据新鲜、完整足量新鲜 Paper、无阻塞观察、readiness/合同通过、无重复交付。Agent 文本不满足这些条件。

`approvals(id,release_id,downstream_id,environment)` 必须有 UNIQUE；
`handoff_offers(approval_id,release_id,downstream_id,environment)` 用一个复合 FK
引用完整授权 tuple，不用四个独立 FK 代替。Paper 的批准不能作为 Live 或其他
Release/下游的授权；不同元组409/约束失败。撤销/期限/人工拒绝/Readiness 仍在
每次 Offer/Claim 的领域事务重查。

Degradation 的 `(project_id,release_id,evaluation_id,policy_id)` 必须整体绑定：项目与 AutomationPolicy、Release 对应 Candidate、Evaluation 相同；政策的 mandate 与 Release 相同；Evaluation 的 subject 必须是该 Release 的精确 Candidate、kind=FORWARD，且输入是同项目已冻结的 FORWARD InputSet。必须存在精确 `(release_id,evaluation_id,input_set_id)` 的 Forward evidence window，不允许另一个项目、Alpha、Discovery、Candidate 或输入快照借出证据。新增观测不满足关联返回23503；升级发现旧关联违规则明确失败，不能删历史或重贴标签。关联有效不代表当前授权/新鲜度有效，Wake 领域事务仍检查期限、撤销、退化阈值和配额。

### A7.1 逻辑消息与人工拒绝

除了external_message_id，必须 `unique(forward_messages.handoff_id,stream_id,sequence,message_revision)`。换external ID重传不新增逻辑记录：字段及不可变report版本相同返回已有记录，冲突409。Correction必须同handoff/stream/sequence且revision递增、supersedes指向前版；缺前版/分叉待对齐不进观察窗口。只计已采纳最新版；重叠窗口不能简单加样本数。

```text
release_decisions [append-only; operator only]
  release_id: Id FK releases
  candidate_id: Id FK portfolio_candidates
  downstream_id: Id FK downstream_integrations
  environment: PAPER|LIVE
  ordinal: int >= 1
  decision: REJECT|REOPEN
  supersedes_decision_id: Id? FK release_decisions
  reason_code: nonempty text
  reason: nonempty text
  decided_at: Time
  decided_by: OPERATOR
```

unique(candidate_id,downstream_id,environment,ordinal)；release属于candidate。锁candidate并按expected_latest_decision_id CAS追加，首次只REJECT；REOPEN引用最新REJECT且近期Operator认证，不能自批。活动REJECT阻断相同candidate/downstream/environment的新推荐、审批、offer、claim/自动授权，另建Release UUID不绕过；REOPEN不恢复旧审批。Claim后的拒绝仅限制未来操作，无撤单权限。人工拒绝与downstream REJECTED分离并有审计。

## A8. 集成、身份与幂等

```text
runtime_integrations [operator mutable]
  name: text
  endpoint: text
  tls_policy: SYSTEM_CA|PINNED_CA
  credential_ref: text
  allowed_capabilities: text[]
  protocol_version: text
  last_capability_snapshot_artifact_id: Id? FK artifacts
  enabled: bool

downstream_integrations [operator mutable]
  name: text
  endpoint: text
  credential_ref: text
  accepted_package_versions: text[]
  environments: PAPER|LIVE|BOTH
  enabled: bool

codex_profiles [operator mutable]
  name: text
  connection_mode: SYSTEM|CUSTOM_PROVIDER
  profile_origin: MANAGED_VOLUME|OPERATOR_MOUNT
  codex_home_ref: text
  custom_base_url: text?
  custom_api_key_ref: text?
  custom_provider_options: StrictProviderOptionsV1?
  use_default_model_settings: bool default true
  saved_model: text?
  saved_reasoning_effort: text?
  saved_fast_mode: bool default false

operator_auth_state [singleton mutable]
  initialized: bool
  totp_secret_ref: text?
  last_accepted_totp_step: bigint?
  session_epoch: bigint >= 1
  setup_completed_at: Time?

trusted_devices [mutable]
  token_verifier_ref: text  # mature opaque-session/crypto verifier
  label: text
  last_used_at: Time?
  expires_at: Time
  revoked_at: Time?
  auth_epoch: bigint

command_receipts [immutable result binding]
  principal_scope: text
  operation: text
  idempotency_key: text
  normalized_nonsecret_request: StrictCommandV1
  resource_id: Id
  response_status: int
  response_nonsecret_body: StrictResponseV1?
  expires_at: Time?
```

`operator_auth_state.session_epoch` 是全局撤销代数，只能保持或增加；禁止减小、归零、bigint 溢出回绕。相同 epoch 的正常认证状态更新可以继续；已全局失效但未单独撤销的旧 BrowserLogin/TrustedDevice，不能因误写旧 epoch 恢复权限。该不变量由数据库更新守卫执行，锁等待之后仍以实际 OLD 行比较。

幂等唯一 `(principal_scope,operation,idempotency_key)`；同规范化非敏感请求返回原结果，不同请求409。长期不可重复操作另有领域唯一约束，receipt 过期不能再次 Live handoff。secret 操作用原生凭据存储/版本，不把 secret/token/auth JSON/可还原秘密请求存 receipt，也不自制请求哈希 Gate。StrictCommandV1 是各真实命令的严格版本化 union，不是任意 JSON；StrictProviderOptionsV1 来自 pinned provider 允许参数的严格 schema，不让配置指定任意命令、环境泄漏或认证模式兜底。

credential_ref 只被可信进程解析；API 仅 configured/status/last_checked，Agent 不得读。ChatGPT native token 留在 Codex profile，QZ DB 不设 access_token/refresh_token 列。

前端模型目录最低字段：`id,model,display_name,hidden,default_reasoning_effort,supported_reasoning_efforts[{reasoning_effort,description}],is_default,fetched_at,profile_revision`。遍历 cursor，未知能力不补默认值。

Readiness snapshot 至少：`integration_id,integration_revision,capability_version,scope,status,reason_code,checked_at,valid_until`。事务外 probe，事务内只采纳配置 revision 一致且未过期的快照；不持锁等待 HTTP。

### A8.1 持久机器主体与权限

控制面 wire/事务细化：机器令牌仅通过单个 Authorization Bearer 头传输，固定 `qz2.<UUIDv7 public_token_id>.<43字符原生随机capability>`；拒绝 query/body 令牌、多个头、Cookie+Bearer 混合、错误Bearer回退Cookie。只有首次签发返回完整token，公开CredentialView不含verifier_ref；重试返回原credential metadata且token=null/replayed=true。随机数/Argon2id/SecretVault复用既有组件，单个请求的密码学验证不代替领域事务的期满/撤销/epoch/归属检查。普通机器事务按 project→Mission run→principal→credential 锁顺序复核，写命令使用 principal FOR UPDATE 串行化同一主体。credential_epoch只能保持/增加；enabled变化必须严格增加，重启/重新启用不能复活旧证。

Operator业务写命令统一先锁单一 operator_auth_state FOR UPDATE，再锁真实BrowserLogin或已验证CLI credential；这是本系统单Operator合同下的原生串行化，不新增intent/队列/锁服务。在该锁下检查command_receipts同scope/operation/key，执行领域变更，再一次INSERT完整不可变receipt并同事务提交；不用先插入后UPDATE不可变receipt，也不新增事务identity。receipt增加 `response_nonsecret_body: StrictResponseV1?`，历史行为原样保留，新控制面命令必须在插入时完整保存非秘密原响应。重试返回原响应快照而不是资源后来的状态；同key不同规范化请求409，失败不留下receipt。机器写命令在主体锁下复用同一幂等机制。Idempotency-Key为1–200字节，不含控制字符或首尾空白。

人工CLI授权进一步绑定完整非秘密命令：`OperatorGrantRequest(schema_version, command: OperatorCommandV1, target_id?, code)`；command是按operation标记的封闭union，request为该真实端点的严格DTO，不能任意JSON。credential_id由已验证的CLI Bearer派生，创建operation的target_id必须null并由服务器分配；更新/撤销的target必须准确。grant增加 `normalized_nonsecret_request: StrictCommandV1?`，历史空值grant不能被新服务消费，不补造授权。新grant的operation/target/完整非秘密request/credential/auth_epoch/到期必须全匹配，防止更换下游、环境、scope或其他参数。TOTP仍走原生限流/重放防护，code永不进入receipt/grant。有效性与消费在提交事务内再核对；完全相同已消费grant+key仅可读原receipt，不续期、不重复操作。

控制面认证重试与密钥生命周期：人工CLI grant在真实机器认证、当前epoch和完整非秘密命令绑定检查后先读幂等回执；已有回执不重新验证TOTP、不消耗REAUTH配额、不续期。仅创建新grant需要新TOTP，正式提交事务再读一次回执。Verifier签发在持有现有Operator命令事务并确认无回执后才写加密文件；并发重试不生成另一份Verifier。数据库失败/提交不明后，重新取得同一authority行锁并在主库确认无任何machine_credentials.verifier_ref引用，才允许按UUID删除已通过MACHINE_VERIFIER用途认证的文件并同步目录；无法判定则保留待对账。进程中断遗留物由本地prune-unpublished-verifiers命令在相同锁序下回收。禁止删除TOTP、SESSION_KEY或外部凭据；没有任意路径/HTTP删除接口。文件写入失败只清理本次成功create_new的对象。

机器认证限流复用PostgreSQL原生原子窗口，不靠单进程内存。machine_auth_rate_windows的credential_id为nullable FK machine_credentials、UNIQUE NULLS NOT DISTINCT，NULL唯一全局窗口；window_started_at为Time、attempts为非负整数，全局上限32、每凭据上限5、窗口60秒。昂贵Argon2之前按全局→凭据顺序预约，任一超限全事务回滚并429/Retry-After；成功仅归还原窗口时间对应的一个占用，失败/取消保留到窗口重置。未知public_token_id不建立窗口。机器密码校验使用独立2槽，不占用TOTP/人工认证的2槽；该限制针对失败及在途计算，不限制持续成功的普通请求总量。

首批OperatorCommandV1变体：PROJECT_CREATE(ProjectCreate)、PROJECT_UPDATE(ProjectUpdate)、PRINCIPAL_CREATE(PrincipalCreate)、PRINCIPAL_UPDATE(PrincipalUpdate)、CREDENTIAL_ISSUE(CredentialIssue)、CREDENTIAL_REVOKE(CredentialRevoke)。已记录的Release/Policy历史操作保留枚举，未提供真实端点前不允许新grant签发。后续B2命令以具体DTO扩展同一封闭union。CLI普通机器scope（含只读doctor）不会改变；单次grant是用户这次输入TOTP的人工授权，不是Doctor或Agent取得持久Operator权限。MISSION/AUTOMATION/DOWNSTREAM不能取得该授权。

ProjectCreate(schema_version,name[1..120],description[0..8000],fork_from_project_id?)只允许Operator；服务端建立NEW/FORK谱系及DRAFT项目，不接id/root_lineage/current_brief/revision。ProjectUpdate(schema_version,expected_revision,name,description,state)不接不可变谱系/批准政策；ACTIVE必须已绑定同项目FROZEN Brief，归档需无未终态Run，ARCHIVED不得原地复活。ProjectView明确列出id/root_lineage/name/description/state/current_brief/current_automation_policy/created_by/archived_at/created_at/updated_at/revision，不输出其他表字段。所有列表limit默认50、1..100，按UUIDv7 id倒序，cursor为上一页末尾Id；机器查询只返回其授权项目，跨项目返回404。

PrincipalCreate(schema_version,name[1..120],kind=CLI|DOWNSTREAM|AUTOMATION,project_id?,downstream_id?,enabled)，PrincipalUpdate(schema_version,expected_revision,name,enabled)，CredentialIssue(schema_version,scope_codes:MachineScopeV1[1..10]非空唯一,expires_at)，CredentialRevoke(schema_version,reason[1..2000])。公开入口不接受MISSION/run_id/epoch/issuer/时间等服务事实。签发由服务器固定epoch/issuer/issued_at，期限须晚于数据库当前时刻且不超出主体限制；disabled/epoch切换、per-credential撤销与正在执行的命令使用相同原生锁顺序。

```text
machine_principals [operator mutable]
  name: nonempty text
  kind: CLI|DOWNSTREAM|AUTOMATION|MISSION
  project_id: Id? FK projects
  downstream_id: Id? FK downstream_integrations
  run_id: Id? FK runs
  enabled: bool
  credential_epoch: bigint >= 1
machine_credentials [immutable issuance]
  principal_id: Id FK machine_principals
  public_token_id: text UNIQUE
  verifier_ref: text
  principal_epoch: bigint >= 1
  scope_codes: MachineScopeV1[]  # nonempty, unique
  issued_at: Time
  expires_at: Time
  issued_by: OPERATOR|MISSION_SERVICE
machine_credential_revocations [append-only]
  credential_id: Id FK machine_credentials
  effective_at: Time
  reason: nonempty text
```

MachineScopeV1闭合集合：RESEARCH_READ、EXPERIMENT_SUBMIT、ARTIFACT_SUBMIT、EVIDENCE_READ、RUN_READ、RUN_CANCEL、DOWNSTREAM_CLAIM、DOWNSTREAM_ACK、FORWARD_SUBMIT、DOCTOR_READ。无wildcard/SQL/Secret/Operator管理能力。除只读doctor主体外project绑定必填；DOWNSTREAM绑定下游且仅自身offer；MISSION绑定活动同项目run、expires<=deadline，不能拥有downstream或其他run权限。主体绑定发行后不扩大，改范围须新主体+撤销旧证；enabled/epoch可控制撤销。每次请求验证native opaque verifier/期满/撤销/epoch/归属，命令事务重查；只发证时显示token一次，不入receipt/日志。Secret/密码学复用成熟库，不自制hash gate。MISSION_SERVICE仅内部为已授权run派生更窄证，不能产生CLI/Operator身份。

Operator-only CLI操作仍是人类动作，使用近期TOTP获取绑定CLI主体、命令、target的单次授权（独立于普通machine scope）：`operator_command_grants [immutable]` 包含 credential_id FK、operation（API命令封闭枚举）、target_id、auth_epoch、authenticated_at、expires_at（<=300秒）；`operator_command_consumptions [append-only]` 包含grant_id UNIQUE FK、command_receipt_id UNIQUE FK、operation（与grant一致的命令）、target_id（与grant一致的目标）。grant的(id,operation,target_id)、receipt的(id,operation,resource_id)各自UNIQUE，consumption以两个复合FK绑定同一命令及目标；不得把一次人类授权用于另一个资源或多个回执。该授权只能近期人类认证发出，Agent/Automation/Downstream不能获取，消费与命令同事务；幂等重试仅返回已执行receipt。管理权限不得放入普通scope来绕过近期认证。

## A9. 索引、保留与迁移核对

必要索引：projects(state,updated_at)；research_cycles(project_id,ordinal DESC)；experiments(family_id,ordinal)；runs(project_id,state,queued_at)；run_attempts(run_id,attempt_no)、活动 lease_expires_at partial index；run_events(run_id,seq)；artifacts(producer_run_id)；input_set_items(input_set_id)；evidence_exposures(root_lineage_id,dataset_revision_id)；evaluations(subject_alpha_version_id,concluded_at DESC)、evaluations(subject_candidate_id)；metric_values(evaluation_id,metric_code,scope)；candidate_alphas(candidate_id,alpha_version_id)；candidate_targets(candidate_id,instrument_id)；releases(candidate_id)；handoff_offers(downstream_id,environment,state,delivery_sequence)；forward_messages(handoff_id,stream_id,sequence,message_revision)；wake_events(state,not_before)。所有 owned FK 有适用 `(id,project_id)` 唯一及复合 FK。

被正式评估/审批/交付引用的 artifacts 默认保留，临时日志/未采纳产物先查引用再清理；审计、trial ledger、sealed exposure 不因清空历史删除。市场目录/缓存由原生工具管理。迁移核对逐类旧新 ID/行数、悬空 FK=0、产物可读率、时间/精度、失败/人工决策/legacy revalidation、未继承权限/审批/凭据；旧 PASS 不是新 qualification；旧实现不在当前源树保留。

# 附录 B：完整接口、状态机、故障测试与 CI

所有路径均为目标合同，不声称旧主干已实现。每行都需要 request/response schema、权限、状态转换、正负测试及代码/CI 证据。服务器生成字段不可由客户端赋值。

## B0. 合同源、版本与持久化

`contracts` 为默认 Rust 的 HTTP/MCP 共用 DTO、错误、事件、政策、产物源，生成 OpenAPI/JSON Schema/TypeScript；批准的 Python 适配消费同一合同，不复制平行真相。Codex 协议从 pinned 原生二进制生成，不发明近似 DTO。HTTP `/api/v2`，远端 `/runtime/v1`，产物 `qz.*.v1`；不兼容改主版本，可选字段按明确兼容策略，生成物提交且 CI diff。引用环、candidate cash/current weights、草稿冻结、readiness、sealed 预约和允许清单规则已完整纳入 A0–A8。

## B1. 通用 wire、权限与错误

浏览器同源 secure/httpOnly/SameSite session + Origin/CSRF；机器独立 scoped/revocable credential；Agent 不能复用 Operator。创建/命令 `Idempotency-Key`，可变资源 `expected_revision`，二者不可互相替代。GET 无副作用，202 仅接受长任务。

```http
POST /api/v2/projects/{project_id}/cycles
Idempotency-Key: <client-generated-key>
Content-Type: application/json

{"brief_id":"<frozen-brief-uuid>","expected_revision":"7"}
```

```json
{"cycle_id":"<uuid>","run_id":"<uuid>","state":"QUEUED","revision":"1","links":{"run":"/api/v2/runs/<uuid>","events":"/api/v2/runs/<uuid>/events"}}
```

统一 Problem Details：

```json
{"type":"urn:quazonai:problem:revision-conflict","title":"对象已被修改","status":409,"code":"REVISION_CONFLICT","detail":"请重新载入后提交，不会覆盖新版本。","request_id":"<uuid>","retryable":false,"current_revision":"8","field_errors":[],"safe_next_actions":["RELOAD"]}
```

最低 code：VALIDATION_ERROR、AUTH_REQUIRED、SETUP_ALREADY_COMPLETED、TOTP_REPLAY、FORBIDDEN_CAPABILITY、REVISION_CONFLICT、IDEMPOTENCY_CONFLICT、BUDGET_EXHAUSTED、UNSUPPORTED_MODEL_EFFORT、INTEGRATION_UNAVAILABLE、CAPABILITY_STALE、CONTRACT_VERSION_UNSUPPORTED、DATA_NOT_POINT_IN_TIME、SEALED_ACCESS_DENIED、SEALED_ALREADY_EXPOSED、UNSUPPORTED_LABEL_INTERVALS、INSUFFICIENT_EVIDENCE、SOLVER_INFEASIBLE、STALE_ATTEMPT、CANCEL_NOT_CONFIRMED、RELEASE_EXPIRED、APPROVAL_REVOKED、ALREADY_CLAIMED、DEMO_NOT_DELIVERABLE、EVENT_CURSOR_EXPIRED。

HTTP 400/422 输入、401认证、403权限、404不存在/需隐藏、409版本/状态、410过期cursor/不可续用能力、429配额/限流、503暂时依赖故障。内部分类留受控日志，响应不泄漏路径/secret/Provider原文/堆栈。

## B2. HTTP 与 CLI 完整映射

| API（均为 /api/v2 下） | 输入/结果要点 | 角色 / CLI |
|---|---|---|
| GET /bootstrap/status | initialized/setup_allowed，无 secret | 未认证；qz auth status |
| POST /bootstrap/start | 一次性本机 capability → 短期 enrollment_id/二维码 | Bootstrap；qz auth bootstrap |
| POST /bootstrap/confirm | enrollment_id/TOTP/可选 device label，CAS 初始化 | Bootstrap |
| POST /auth/login | TOTP/trust_device/label，限速防重放 | 未认证 |
| POST /auth/logout | 撤销当前 session | Operator |
| GET/DELETE /auth/devices/{id} | 列表/撤销，敏感动作近期认证 | Operator |
| GET/POST /projects | 筛选分页/新建，Agent 不得新建洗血缘 | Operator；qz project list/create |
| GET/PATCH /projects/{id} | 展示/名称等可变字段，expected_revision | Operator；qz project show/update |
| POST /projects/{id}/pause、/resume | 控制新研究，不操作下游交易 | Operator；qz project pause/resume |
| POST /projects/{id}/briefs | 草稿/解析/缺失字段 | Operator；qz brief create |
| POST /briefs/{id}/freeze | 数据政策预算验证冻结；422完整问题 | Operator；qz brief freeze |
| POST /projects/{id}/cycles | frozen Brief → 202 cycle/run | Operator/受控调度；qz cycle start |
| GET /cycles/{id} | 阶段/预算/实验/结论/available_actions | Operator；qz cycle show |
| GET /experiments、/{id} | 可比条件过滤，不默认隐藏失败/取消 | Operator；qz experiment list/show |
| GET /data/sources、/data/revisions/{id} | 可用性/许可/PIT/coverage，无任意读路径 | Operator；qz data list/describe |
| POST /data/validate | 已登记 ref → 202 validation run | Operator；qz data validate |
| GET /alphas、/alphas/{id}/versions/{version} | 资格/版本/单位/血缘/证据/限制 | Operator；qz alpha list/show |
| POST /alpha-versions/{id}/evaluations | policy/input refs；sealed专门 evaluator | Operator/限权服务；qz alpha evaluate |
| GET /evaluations/{id} | 三层状态/方法/指标/证据 | Operator；qz evidence show |
| POST /portfolio-mandates | immutable版本，验证原生solver能力 | Operator；qz portfolio mandate |
| POST /portfolio-candidates | mandate/alpha_version_ids/decision_asof/input_set_id → 202 | Operator/受限Agent建议；qz portfolio build |
| GET /portfolio-candidates/{id} | Alpha/资产/cash、风险成本容量余量诊断 | Operator；qz portfolio show |
| POST /releases | candidate_id → freeze包，独立组合模拟/政策PASS | Operator/受控服务；qz release create |
| GET /releases/{id} | exact版本/asof/expiry/证据/市场 | Operator；qz release show |
| POST /releases/{id}/approvals | environment/downstream_id/expiry/近期验证 | Operator；qz release approve |
| POST /approvals/{id}/revoke | reason/expected context，claimed不伪撤销 | Operator；qz approval revoke |
| POST /handoffs | release_id/approval_id → OFFERED，不是 executed | Operator/政策服务；qz handoff offer |
| POST /handoffs/{id}/claim | 下游身份/external_claim_id/支持版本，原子重查 | Downstream |
| POST /handoffs/{id}/ack | external receipt/状态，不代表知道全部成交 | Downstream |
| POST /forward/messages | message/stream/sequence/revision/窗口/report ref，去重 | Downstream |
| GET /projects/{id}/forward | 完整/缺失/迟到窗口与晋级/劣化原因 | Operator；qz forward show |
| POST /projects/{id}/automation-policies | 显式授权、冻结阈值/期限/范围 | Operator；qz automation authorize |
| POST /automation-policies/{id}/revoke | 阻止未来授权，不停止已执行交易 | Operator；qz automation revoke |
| GET /runs、/runs/{id} | 分页/状态/attempt/原因/下一动作 | Operator；qz run list/show |
| POST /runs/{id}/cancel、/retry | 限定转换，202或409 | Operator；qz run cancel/retry |
| GET /runs/{id}/events | 持久SSE/恢复cursor | Operator；qz run watch |
| GET /artifacts/{id}、/content | 元数据/受限下载，敏感访问先 exposure | 限权；qz artifact show/export |
| GET/PATCH /settings/codex | 正交配置，secret仅状态 | Operator；qz codex config |
| GET /codex/models | 全分页/支持effort/profile_revision | Operator；qz codex models |
| POST /codex/login/start、/cancel、/logout | 原生account RPC，UI只展示受控流程 | Operator；qz codex login/logout |
| GET /codex/account | 原生认证类型/status，不读回token | Operator；qz codex status |
| GET /readiness、POST /integrations/{id}/probe | 分场景能力/期限，probe总超时 | Operator；qz doctor |
| POST /migrations/import | 受信任 export ref/dry_run，202/report | Operator；qz migrate import --dry-run |

补充的管理入口同属 `/api/v2`，所有写入近期Operator认证、幂等键；可变PATCH加expected_revision。机器普通token不可调用。

| API | 严格输入与规则 | CLI |
|---|---|---|
| POST /data/sources；PATCH /data/sources/{id} | 创建name/runtime_id/native_catalog_ref/provider_kind/enabled；更新仅name/enabled，已引用身份不能改 | qz data source create/update/disable |
| POST /data/sources/{id}/grants；POST /data/grants/{id}/revoke | A2.1授权字段/撤销reason和有效期，服务端发行版本 | qz data grant create/revoke |
| POST /data/revisions | 已登记source/grant/native snapshot/version，native metadata受信任读取并校验；原生身份重试不新建 | qz data register |
| GET/POST /integrations/runtimes；GET/PATCH /integrations/runtimes/{id} | A8字段；credential只引用服务端已登记ID，禁任意Secret路径；enabled=false停新任务 | qz runtime list/create/show/update/disable |
| GET/POST /integrations/downstreams；GET/PATCH /integrations/downstreams/{id} | A8字段；native合同版本/环境明确；停用不终止已领交易 | qz downstream list/create/show/update/disable |
| POST /credentials；POST /credentials/{id}/rotate | name/kind=RUNTIME或DOWNSTREAM或CUSTOM_PROVIDER/secret；近期认证，原生secret store，不打印/回读；改revision失效旧readiness | qz credential create/rotate |
| POST /releases/{id}/rejections | environment/downstream_id/reason_code/reason/expected_latest_decision_id；追加REJECT | qz release reject |
| POST /release-decisions/{id}/reopen | reason/expected_latest_decision_id；A7.1，仅追加，不自动审批 | qz release reconsider |
| GET/POST /machine-principals；PATCH /machine-principals/{id} | name/kind/bindings；PATCH仅name/enabled，权限不能扩张 | qz token principal list/create/disable |
| GET/POST /machine-principals/{id}/credentials；POST /machine-credentials/{id}/revoke | scopes/expires_at/reason，A8.1上限；只首次发行回token | qz token issue/list/revoke |
| POST /auth/operator-command-grants | CLI credential/operation/target/TOTP，防重放/限流，单次300秒 | 敏感CLI命令的人类确认 |

服务端字段id/version/revision/snapshot/issuer/epoch不能由客户端指定。集成endpoint/credential/协议/能力变化使readiness失效并重新probe。被停用记录仍供历史引用；权限/授权事件不删除。

表中 `/{id}` 等简写沿同一行资源前缀，不是根路由。列表 opaque cursor、服务端 limit 上限、稳定排序和项目/权限过滤。CLI 用生成客户端和同一服务器命令，不直写 SQL；唯一本地特权入口为受限 bootstrap/备份恢复等运维。

## B3. MCP 白名单与真实闭环

官方 rmcp；服务端绑定 project_id/cycle_id/run_id/role/capabilities/budget/deadline，模型不能自报 authority。

| 工具 | 请求字段 | 返回/边界 |
|---|---|---|
| research.get_brief | brief_id | frozen目标/允许动作，无secret |
| data.describe | dataset_revision_id | schema/coverage/PIT/用途；sealed只metadata |
| research.search_history | query/family_id?/limit | 受限项目历史、失败和证据，暴露过滤 |
| experiment.propose | family_id/hypothesis/rationale/parameters_ref/idempotency_key | experiment_id，不能自定PASS |
| artifact.submit | declared_kind/schema_version/workspace_relative_path | 校验artifact_id，拒绝绝对/软链越界/超额 |
| experiment.validate | experiment_id/input_set_id | 202/run_id，可见合同错误 |
| experiment.run | experiment_id/registered_runtime_kind/input_set_id/idempotency_key | 先预算预约，202/run_id；无任意command/URL |
| run.get | run_id | 真实state/attempt/产物/安全错误 |
| evidence.read | evaluation_id | 允许披露级别/限制，exposure生效 |
| portfolio.propose | mandate_id/alpha_version_ids/input_set_id/decision_asof/idempotency_key | proposal/candidate run，无approval |
| research.conclude | experiment_ids/evaluation_ids/decision/explanation | 引用真实性验证，不生成qualification |

approve/publish/handoff.claim/policy.update/db.query/secret.read/http.fetch_any 不存在于工具集。Reviewer 不是 Operator，研究者不能把别的 service token 带回 shell。最小真实闭环：原生 thread/start或resume → turn/start → 实际 tool请求 → 真实外部job → artifact/evaluation → 同thread后续turn引用实际evaluation_id → 结论。空工具列表、启动日志、漂亮解释/fake metrics不算。

## B4. Runtime 协议

```text
GET  /runtime/v1/capabilities
GET  /runtime/v1/catalogs/{registered_ref}/metadata
POST /runtime/v1/jobs
GET  /runtime/v1/jobs/{external_job_id}
POST /runtime/v1/jobs/{external_job_id}/cancel
GET  /runtime/v1/jobs/{external_job_id}/result
```

Capabilities：protocol_versions、runtime_version、engine_versions、image_refs、job_kinds、artifact_schemas、data_kinds、venues、label_interval_support、solver_capabilities、max_cpu/max_memory/max_output、isolation_profile、checked_at。缺能力拒绝，不选“差不多”executor。

```json
{"schema_version":1,"run_id":"<uuid>","attempt_no":1,"owner_epoch":"3","external_job_id":"<run-id>/1","job_kind":"PORTFOLIO_SIMULATE","image_ref":"<registered-pinned-native-image>","input_set_id":"<uuid>","inputs":[{"kind":"DATASET","revision_id":"<uuid>","registered_ref":"<immutable-catalog-version>"},{"kind":"ARTIFACT","artifact_id":"<uuid>","storage_version":"<immutable-version>"}],"parameters_artifact_id":"<uuid>","limits":{"cpu":2,"memory_mib":4096,"wall_seconds":3600,"output_bytes":67108864},"deadline_at":"<RFC3339>","requested_output_schemas":["qz.evaluation.v1","qz.portfolio_targets.v1"]}
```

JobSpecV1 使用固定登记入口/镜像/参数，不接受任意 docker 参数。输入能力只经可信通道/挂载授予，JobSpec/日志/Agent响应无secret。Nautilus对象用原生序列化/配置，不自己的价格/订单模拟。

流程：验证合同/版本 → 只读输入挂载 → Rust直接调用pinned native；仅已证据批准的缺口调用独立Python适配 → 原生导出 → Arrow/schema/资源验证 → 原子发布manifest → 退出。该层不审批/改政策。

ResultManifestV1：`schema_version,run_id,attempt_no,external_job_id,state,engine_versions,started_at,finished_at,resource_usage,artifacts[{kind,schema,storage_ref,storage_version,byte_count}],error{class,code,safe_message}?`。控制面再次验证，不因 JSON 写 PASSED 授资格。

同 external_job_id + 同 JobSpec 返回已有任务，不同409；使用 OCI身份/状态，不内存锁做唯一事实。terminal identity/tombstone 保留至确认采纳和重试窗口结束，清理不让旧请求立即重建。少量网关映射/tombstone 可复用嵌入式数据库/原子文件，不再建业务库/队列。实际隔离按第7节/T34/T35验证，不用 Prompt 替代。

## B5. 事务与状态机

### B5.1 入队

```text
BEGIN
  lock project/cycle; validate state, frozen Brief, budget, receipt
  reserve budget; create run(QUEUED)
  lock run; allocate seq; insert run.created
  pgmq.send(queue, {run_id})
  write command receipt
COMMIT
```

任一步失败整体回滚，不另建outbox搬到其他broker。PGMQ extension安装/升级显式部署/迁移。

### B5.2 接管与采纳

```text
PGMQ read → short transaction, lock run/attempt
  terminal: only safe ack, no reexecution
  dispatch unknown: adopt owner_epoch, RECONCILING
  dispatch allowed: persist attempt and intent
commit → external submit/query without DB locks

real terminal result → validate manifest and immutable artifacts
→ short transaction, lock run/attempt
→ check attempt_no/owner_epoch/state/input versions
→ accept result, business records/evaluation and event
→ commit → archive/ack
```

crash-after-submit-before-save 以 external_job_id 查询；crash-after-result-before-ack 不重复qualification/Release/Handoff；旧owner STALE_ATTEMPT，未采纳输出隔离用于诊断。外部至少一次，正式结果唯一采纳。

### B5.3 取消与重试

```text
QUEUED → CANCELLED  # only no external side effect
DISPATCHING/RUNNING/RECONCILING → CANCEL_REQUESTED
CANCEL_REQUESTED → CANCELLED  # remote confirmed stopped/nonexistent
CANCEL_REQUESTED → FAILED    # genuine native failure before confirmed cancellation
RUNNING/RECONCILING → SUCCEEDED # success adoption wins before cancellation intent
terminal → immutable; rerun creates new run or explicit safe new attempt
```

同一行CAS决定唯一终态，不能取消同时公布成功。取消意图先提交后原生成功只能作为诊断，确认终止后CANCELLED；原生真实失败保留FAILED/error_class/error_code，不伪称取消。成功先采纳则取消不能改已定终态。无法确认停止保留CANCEL_REQUESTED+reason。重试前确认旧attempt停止/不存在/安全隔离；研究否定/数据无效/求解不可行不自动重试。

### B5.4 Qualification/Release/Approval/Claim

Qualification 仅独立VALID/PASS、未过期、合法数据/血缘；无强制PASS。Release检查原生solver可采纳、约束、共享资金模拟PASS、非demo/许可适用、资格新鲜、引用完整。文件先不可变发布再Release再审批。

Approval/Offer/Claim锁相关对象，按DB时间重查政策撤销/到期/版本、Release、readiness和重复领取；最多一方领到能力，不持锁等HTTP。新rebalance同project/mandate/environment明确supersession，旧过期目标不能重试复活。

### B5.5 自动化

Forward真实消息 → 去重/对齐 → native指标 → Observation → Wake → 状态/冷却/预算 → 新Cycle；同Observation单事务不能两次开Cycle。PAUSED/ARCHIVED不启动；暂停现有任务按用户明确选择继续或申请取消，保留结果。缺数据INSUFFICIENT_DATA不假称健康/劣化。自动Live全部条件依第8节/A7逐项事务复核。

## B6. SSE 与恢复

GET run 同一快照返回 state/revision/last_event_seq；随后 `Last-Event-ID=<run_id>:<seq>` 从持久表读 seq>cursor。

```text
id: <run_id>:42
event: run.state_changed
data: {"schema_version":1,"run_id":"...","seq":"42","state":"RECONCILING","reason_code":"REMOTE_RESPONSE_UNKNOWN"}
```

run行锁保证已提交序列，持久查询补快照后订阅前窗口；通知丢失仍轮询。客户端seq去重，不按到达顺序覆盖较新状态。过期cursor在流开始前410+snapshot地址；已连接发送reset-required。未知兼容事件保留envelope并记录，未知不兼容版本提示升级/刷新不清空。heartbeat不编进度，断线不cancel。

## B7. Target Package

`qz.target_package.v1` 最少：

```text
release_id, package_schema_version, environment_origin,
project_id, candidate_id, mandate_id, qualification_refs,
evaluation_refs, input_revision_refs, engine_versions,
asof, valid_from, valid_until, base_currency,
capital_assumption, current_weights_source,
targets[{instrument_id,target_weight,currency}], cash_weight,
constraints_summary, cost_assumption_ref, compatible_market_capabilities,
limitations, provenance_artifact_refs
```

全部对应不可变记录；无brokercredential、真实订单、quantity指令、账户写入口、实时止损/撤单/平仓。再平衡新包，不下游偷跑未经批准新研究代码。下游独立风险/账户/执行治理。下载只针对精确artifact/storage_version，不跳到相邻私有目录；claim失败不扩大权限重试。

## B8. 完整自动化验收矩阵 T01–T42

全部是本次交付项；共享基础fixture不等于空断言。每项输出CI日志、输入版本、产物/截图。真实收益不是预设必须出现的结果。

| ID | 场景 | 必须证明 |
|---|---|---|
| T01 | Rust/Nautilus/Clarabel/Arrow冷启动 | pinned镜像实际安装/导入/运行，真实版本/结果；科学解释器不在API进程，语言取舍按第0节 |
| T02 | 无凭据Demo | 一条文档命令完整UI演示；synthetic/fixture明显且不能生产领取 |
| T03 | 原生Codex SYSTEM | 空QZ URL/key不覆盖native配置；真实stdio使用既有profile，无自动删/复制auth.json |
| T04 | SYSTEM+effort | model=null、合法非空effort生效，来源不变，default开关保留保存值 |
| T05 | 自定义Provider | 显式route/key，失败不偷用订阅，inactive凭据不注入 |
| T06 | 动态模型目录 | 全分页，Slider marks来自supported efforts；未知报错不降档 |
| T07 | 官方订阅原生登录 | 受保护环境实测device code/start/cancel/logout/status；token不进QZ DB/日志 |
| T08 | Agent真闭环 | tool→真实job/evaluation→同thread消费结果→引用真实证据结论，不scripted UI假成功 |
| T09 | 独立Reviewer | Thread/权限/输入隔离，研究者不能改Reviewer指标/冒用approval |
| T10 | 并发预算 | 多Worker/Agent实验/CPU/并行预约不超发，Optuna trial计总预算 |
| T11 | PIT | event已发生但available_at晚于decision拒绝，重述/退市不穿越 |
| T12 | sealed不泄漏 | raw/preview/图/摘要/workspace/日志均不能绕读，fork继承暴露 |
| T13 | sealed中途crash | 已授读取但失败仍记录消费，不能重领独立资格 |
| T14 | purge/embargo | 固定horizon正确原生CV；未支持变量区间明确拒绝 |
| T15 | 缺指标/NaN/试验不完整 | null+reason，required缺值INCONCLUSIVE，不填Sharpe0/DSR1/伪p-value |
| T16 | 科学数值golden | 原生库与独立参考/手工小例一致，容差/solver状态/单位有依据 |
| T17 | 至少两个Alpha | 两份真实预测经校准/混合/native优化到资产目标，不平均两条NAV |
| T18 | 不可行约束 | INFEASIBLE，无发布权重，不fallback单资产100% |
| T19 | 成本/换手/容量 | 原生费率/滑点/流动性输入产生可解释变化；超参与率拒绝，缺输入不补常数 |
| T20 | 共享资金模拟 | 真Nautilus净额/资金/费用与目标序列一致，不重复扣费/平均独立账户 |
| T21 | 市场/到期/结算 | 支持venue/data path原生场景；不支持预测市场不得READY |
| T22 | DB事务故障 | domain/event/PGMQ任一步失败无半入队，不同会话验证 |
| T23 | 重投/lease过期 | 同run不重复结果，过期owner终态被拒，current attempt可恢复 |
| T24 | crash-after-submit | ACK丢失/Worker crash查询原任务，不盲目重复job |
| T25 | result-before-ack | 重投不再发qualification/Release/Handoff，唯一约束/业务幂等生效 |
| T26 | 取消/完成竞态 | 唯一终态，远端未停不CANCELLED，取消不触发下游撤单 |
| T27 | SSE断线/通知丢失/并发 | 不漏事件/重复倒退，正确cursor/410/reconnect，关浏览器不取消 |
| T28 | 包/审批绑定 | 变权重/数据/mandate必须新Release/审批，不能改已批文件 |
| T29 | claim/revoke/expiry竞态 | 双领一次成功，撤销领取同步重查，过期目标不复活 |
| T30 | Forward去重/纠正 | 重传不加样本，重叠/迟到/gap/correction正确处理 |
| T31 | 自动Live | 缺Paper/过期政策/劣化/readiness分别阻断，仅完整条件交付target |
| T32 | Degradation→Wake→Cycle | 真实观测，冷却/预算/PAUSED/去重无循环风暴 |
| T33 | 确定性再平衡 | 新cutoff新Candidate/Release，不覆盖旧包、不让LLM绕审批 |
| T34 | Secret/文件系统隔离 | 随机变量名sentinel、auth.json、DB、master key、Docker socket实际不可达 |
| T35 | 恶意产物 | symlink/traversal/压缩炸弹/出网/sealed/fork bomb/超输出受限且安全错误可见 |
| T36 | 初始化/TOTP | 无本机capability不能公网抢绑，并发confirm只一成功，重放/限速/设备撤销/注销有效 |
| T37 | antd桌面/移动 | 390/768/1440全部核心动作，无Radix残留/嵌套模态焦点丢失 |
| T38 | a11y/PWA/离线/更新 | 键盘/标签/非颜色/触摸，API不缓存，离线不写，未保存表单不被强刷 |
| T39 | 旧数据迁移 | dry-run报告/真实旧快照/FK完整，旧PASS不晋级，原数据不毁 |
| T40 | 恢复/磁盘满/依赖离线 | 版本一致、admission先暂停、无盲目Live重发，磁盘满拒新任务并告警 |
| T41 | README/CLI/Skill | 全部quickstart/help/示例实际执行，真实截图，能力矩阵与测试相符 |
| T42 | Web/CLI双入口完整闭环 | 新实例分别达研究/Alpha/组合/Paper/Forward/晋级或劣化唤醒，无手工SQL |

T16可用两资产独立手算最小方差/费用前后差，不维护第二生产优化器。T31/T42可用专门非交易验收下游接target，**不用真实下单**。Demo/fixture不能通过测试开关变生产可批；真实路径用可追溯有权数据，生产制品无跳过Gate后门。

## B9. Required checks 与受保护验收

| Check | 必须内容 |
|---|---|
| rust | cargo fmt --check、clippy deny warnings、locked build、nextest/unit/proptest |
| db-domain | 真PostgreSQL+PGMQ、SQLx offline、新库迁移/约束/事务/并发 |
| contracts | OpenAPI/TS/JSON/Arrow diff、Codex原生schema比对、兼容性 |
| frontend | locked install、lint、typecheck、Vitest、production build |
| e2e | Web/CLI、三视口/PWA、Playwright/axe/截图 |
| native-runtime | 真Rust Nautilus/Clarabel/Arrow，以及有证据批准的必要科学适配、市场/数值golden |
| codex-contract | 真pinned App Server+本地可控Provider fixture；protocol/model/list/工具循环/环境 |
| security-isolation | 真实隔离越界、依赖漏洞/secret扫描、安全配置 |
| recovery | kill/restart、ACK丢失、lease、cancel race、SSE、恢复/迁移 |
| docs-smoke | README命令/CLI/Skill/链接/生成文档一致与真实能力矩阵 |
| supply-chain | Cargo/前端/科学锁、镜像/action原生版本固定、许可证/SBOM |
| protected-acceptance | 授权系统Codex/官方订阅/custom Provider、真实remote和数据完整链路 |
| rewrite-complete | 汇总所有结果和交付矩阵，失败/取消/缺失/应运行而skip必须失败 |

普通PR CI无生产密钥，本地Provider fixture不能代替T07或受保护真实链路。真实账号由Operator在同一受保护profile登录，经审查锁定Head最小权限执行。额度/账号/数据/权限缺失为待处理/阻塞，不视通过；真实订阅集成不证明收益或Codex review。复用现成测试/coverage/license/SBOM工具，不另建Gate平台；不能删测试/全skip/关闭核心功能造绿。

## B10. 交付映射与恢复证据

最终PR提供W0–W8/T01–T42逐项代码模块、实际测试、CI run/artifact映射；标明复用/替换/删除的旧Python服务、自研数值/Agent/队列、Radix/重复图表和过时文档。语言按第0节，不以第一方Python本身判失败，也不以目录名掩盖未论证重建。运行配置、fail-fast、ABI/readiness和备份恢复演练字段完整见第10节；迁移核对见第11节/A9，不以新文档充当运行证据。

## B11. 关闭证据

```text
Implementation PR:
Reviewed Head:
Merge Commit:
GitHub CI Runs:
Final Codex Review (explicit no issues):
Unresolved Threads: 0
W0 Compatibility / Native Runtime Evidence:
Fresh-install Web + CLI E2E:
Multi-Alpha / Numerical Golden:
Paper / Live / Degradation / Wake Evidence:
Isolation / Recovery / Backup Restore:
Migration Report / Rollback Evidence:
Ant Design Desktop + Mobile + PWA + Accessibility:
README Commands / Screenshots / Docs Checks:
Post-merge main Checks:
Known Limitations / Residual Risks:
```

填有链接的完整证据不是授权跳过任何检查。第12节顺序不可降低：完整实现 → 最新Head CI全绿且Codex明确无问题 → merged → main复核/证据回填；不满足即部分完成，不关闭Issue。


## B12. PostgreSQL 初始持久化与逐轮 Store 实施合同

`migrations/202609050001_domain.sql` 在新数据库建立领域关系；`202609050002_model_turns.sql` 建立逐轮阶段约束与计数投影。仅使用 SQLx 原生 migration runner/事务/测试数据库管理与 PostgreSQL/PGMQ；不是自研迁移、队列、工作流或 Agent Harness。首次运行不能指向旧业务库，迁移角色与运行角色分离；运行角色不得拥有 schema、DDL、TRUNCATE 或超级用户权限。当前 DDL 的版本化 JSON CHECK 只验证容器/版本，不能替代领域服务的完整参数、权限和资格检查；创建表不是开放相应 API。

InputSet 与 Brief 的关联成员在草稿期组装，在相同事务中冻结。冻结后禁止改写/删除父记录以及新增/改写/删除成员；登记相同成员命令需在服务层先幂等读取，而不是用 `ON CONFLICT` 绕过冻结检查。发布的 Run 必须引用冻结 InputSet；执行服务在启动/采纳时重查，不能把未完成草稿交给远端。

数据库时间为有限 PostgreSQL timestamptz（微秒精度），逐轮命令拒绝非零的亚微秒部分，防止首次写入静默截断后重试变成不同命令。市场纳秒仍由 Arrow 保存，不经数据库 Time 丢精度。UUIDv7 校验对未知 RFC 变体返回 NULL 的情况也明确拒绝，只有可选字段的真实 NULL 允许；钱/权重使用 native numeric domain 检查有效 scale/range，避免 typmod 先舍入再通过 CHECK。

Store 的模型续轮入口为 `reserve_turn` → `claim_turn_dispatch` → `bind_native_turn` → `observe_turn_terminal` → `settle_turn`。它们需要可信服务绑定的当前 Attempt/OwnerEpoch 与 DB 租约，研究代码不能获取 Store/数据库凭据。事务锁序为 project、cycle、run、attempt、session；锁等待后使用 `clock_timestamp()`，而不是事务开始时间。Cycle token/费用从其所有不可变轮次投影；未决预约、实际失败、实际超额都不能清零。新发起阶段要求 ACTIVE/RUNNING 和 deadline，新授发送能力前再核验一次；暂停/取消不妨碍已发送轮的真实结果对账。

发送意图首次提交者才得到 Send，所有后续调用只得到 Reconcile 或 Settled；原生 JSON-RPC ID 不是上游幂等键。相同 Session 只允许一个未结算预约；无法确定之前的真实 Turn 时保持未决，不猜测重新调用。已发出的轮只有在可信原生适配器观察到 Turn identity 和终态用量后结算。实际费用仍是 ESTIMATED，而非声称精确账单。全新 Attempt 不得接管旧预约的采纳权限；同一 Attempt 增长 owner_epoch 后可以对账。未决旧任务必须先安全处理，不通过重开 Attempt 洗白预算。

`acknowledge_settled_turn_message` 仅接受精确 reservation_id 对应的已提交不可变 receipt 与原生消息内容；其原生 archive 幂等，错误消息引用返回冲突。它不授予结果采纳/发送权限，旧 lease 过期不妨碍安全清理已结算通知；未结算不能归档。结算与 ACK 之间崩溃会产生重投，但不会再次产生原生调用或再次累计已用。

新建库、约束和 Store 集成测试可以使用独立 ephemeral fixture 写入来构造故障，不能把它们标成 T42 的 Web/CLI 完整流程或受保护真实账号验收。所有新 Store 测试在单独 PostgreSQL/PGMQ CI job 中执行，foundation 汇总必须依赖它；未设置 DATABASE_URL 必须失败，不能变 skipped green。

## C. 已落实到 A4/A6/A7 的精确数值与关联补充

本节保留先前 C1–C4 的语义，不建立第二套表名或状态机。指标的有限 f64 使用
Serde 原生 JSON 的可往返十进制表示，再由 BigDecimal 原生解析，与冻结阈值
精确比较；不得先把阈值转 f64，也不把未通过 wire 暴露的二进制尾数当额外有效位。
0.1 与 0.1 相等，但不满足 GE 0.10000000000000001。BETWEEN 按完整 Decimal
验证上下界；NaN/Infinity 拒绝。u16/u32 wire 最大值分别为65535/4294967295。

A6.1 的五类明细落实 Mission 单一会话、请求身份、原始 owner_epoch、Profile
配置版本、确定性序号、发送意图、ACK、独立终态与延后结算；终态而用量未知时，
保留占用与待对账，不盲目重发或退款。它们不实现 Codex 工具循环。

Release 的 (evaluation_id,candidate_id) 必须引用同一 Candidate 的评估；
Alpha 或其他 Candidate 的 PASS 不可借用。Offer 的
(approval_id,release_id,downstream_id,environment) 必须完整引用审批元组，
Paper 审批不可转用 Live 或别的下游。相关复合 FK 是最低关联约束，不取代
事务内新鲜度、撤销、项目、人工拒绝与资格检查。
