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

## 0.2 编译器补丁基线（2026-09-07）

正式构建和本地验证固定 Rust **1.98.1**，不使用浮动 stable、不降低上游 MSRV。Rust 官方于 2026-09-03 发布该补丁，修复 1.98.0 的 trait-object vtable 错误生成及其未定义行为：<https://blog.rust-lang.org/2026/09/03/Rust-1.98.1/>。更新 `rust-toolchain.toml`、workspace `rust-version` 与 CI 安装/选择；依赖版本和 Cargo.lock 不因工具链升级重新解析。

历史 1.98.0 的已执行证据原样保留且仅作为历史；本补丁基线需重新执行原生合同、领域/数据库/HTTP、Clippy 与科学探针后才能记录通过。宿主可能用发行版 cargo 或覆盖变量绕过 rustup，不能只看文件内容推定实际编译器：验证入口使用明确的 `rustup run 1.98.1`，记录实际 `rustc -Vv`、Cargo/rustfmt 版本。安装工具链是本机执行器的环境操作，不是让其修改源码；失败返回真实诊断，由网页作者处理。

## 0.3 所有者接管与个人项目范围修订（2026-09-11）

所有者授权当前本机 Codex 直接完成 #62/#63 的全部开发、修复、验证和 Git/GitHub 操作，覆盖网页唯一作者及本机仅执行的旧分工。现有工作应核对并接续，不重复实现；GitHub Codex 仍仅做独立 review。全部开发完成、最新 Head 适用 CI 通过且 Codex 明确无问题后直接合并 main。

本项目为个人项目，安全工作的范围是防止账户密码、钱包、支付/API 凭证等敏感信息进入 LLM、源码和日志。不新增除此之外的网络安全专项、渗透测试、供应链安全审查或额外安全合并门禁；本文旧的专项安全要求以此修订为准。保留现有原生保护与回归测试；该范围调整不改变预算、科学有效性、Sealed 评估独立性、审批、target-only 交付、数据保留与故障恢复等产品合同，也不允许用假结果满足验收。

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

Rust 1.98.1，Nautilus Rust crates0.63.0（Python2.0.0rc4发布族），Clarabel0.11.1，Arrow56.2.0，Codex0.144.4，PGMQ1.10.0；Linux x86_64。Cargo.lock来自原生Cargo，所有验收 locked，不现场生成锁。没有任何生产Python例外被此处批准；旧science requirements/lock/checker随旧桥接删除，供应链改由Cargo原生锁验证。

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

Mission复用锁定版本的原生named permissions及stdio MCP，不依赖dynamicTools。沿用已选模型/认证来源，Mission工具边界由受信任启动器覆盖：只允许专用工作区读写、原生最小系统文件及已锁定Codex二进制的只读访问；shell不继承服务环境，MCP能力只传入已绑定Run/Attempt的专属子进程。原生config/read只投影MCP名称并禁用其他服务器，不持久化或展示原始配置/环境值；启动与恢复重新应用同一边界。不加载个人插件、记忆、浏览器、跨Agent、无限Goal或登录shell能力。0.144.4的全局AGENTS由host独立加载，不受project_doc_max_bytes控制且无stdio关闭开关；Mission因此要求专用profile：若CODEX_HOME存在AGENTS.md/AGENTS.override.md或配置含个人instructions/developer_instructions/model_instructions_file，在发送Thread请求前明确拒绝，不读取文件内容、修改/删除用户文件、复制认证或暗换profile。Managed volume和显式Operator mount均可用，但后者也须满足这个已验证边界。项目文档自动注入关闭，任务材料由冻结Brief和受限MCP提供。原生发行版的权限/stdio运行必须实际验证，不用新版文档中而锁定协议没有的字段冒充已生效。

取消或到期后的Mission恢复只允许重连已登记Thread，对账原已发送Turn；不创建Thread、
签发Mission凭据、启用MCP或准备新Turn。使用原资源上限及至多110秒的独立清理窗口，
不是延长研究预算；用量/终态证据缺失仍保留原预约和CANCEL_REQUESTED，不假造取消成功。

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

### 原生持久化的交付边界（2026-09-06 审查修订）

Release 的 `package_artifact_id` 必须引用同 Candidate 项目的独立不可变 `PACKAGE`，
`media_type=application/json`、`schema_name=qz.target_package`、`schema_version=1`，
Release 的 `package_schema_version` 同为 `1`，且 `byte_count>0`。不能引用 PARAMETERS、
别的项目或不同版本的对象。REAL Release 要求该 Package 的 origin=REAL 且
access_class=DELIVERY；DEMO 也要使用真实的 PACKAGE 类型记录，而不能用任意测试参数充数。
**DEMO Release 永远不能产生 PAPER/LIVE Approval 或 Handoff Offer。** 这些原生元数据
约束不替代产物内容校验、真实输入来源、多 Alpha、独立评估和完整交付授权。

首次 Claim 在获得原生行锁后按 `clock_timestamp()` 检查 offered_at<=当前时间<expires_at，
不能通过客户端回填过期前的 claimed_at 复活目标。该次正式 claimed_at 由数据库写入实际
采纳时间。后续已领取记录的幂等读取不重新领取，不因 TTL 过去改写或撤回已转移的事实。
Forward 新消息必须持有对应 Handoff 的共享行锁，并且 Handoff 在该次准入时为
CLAIMED 或 ACKNOWLEDGED；OFFERED/REVOKED/EXPIRED/REJECTED 均不能新增消息。
对历史数据的升级检查允许确实曾领取、后来被下游拒绝的合法历史报告，不能以当前拒绝
状态抹去旧证据；从未领取的历史消息则升级失败，保留原行供显式处理。

Research lineage 的 parent 不可修改，且必须无环。使用 PostgreSQL 原生递归查询的
CYCLE 检测和 AFTER INSERT 约束触发器，覆盖自身引用及同一语句多行相互引用；
历史环在升级时使整个事务失败，不能偷偷改 parent、删除或赋新 UUID 洗白暴露。



先冻结目标包再审批。审批绑定 release、artifact 原生版本、candidate/mandate/policy、下游、证据和有效期；任何目标/依赖变化产生新 Release/审批。Qualification 仅独立 VALID/PASS/新鲜/合法血缘证据产生；无手工 force PASS。

Approval/Offer/Claim 在事务内重新验证版本、撤销、资格、REAL 数据、授权用途、政策、期限、readiness 配置版本与新鲜度；外部 probe 在事务外执行。Claim/revoke/expire 原子竞争只一结果；下游只领自身 offer。CLAIMED 后 QZ 无停止/撤单/伪撤销权限，只可 advisory 或新版本；旧过期目标不能因重试复活。

下游原生探测固定 GET `/downstream/v1/capabilities`，使用登记的 DOWNSTREAM 凭据及部署允许的精确 origin/socket，SYSTEM_CA 原生 TLS；仅部署显式允许的 literal loopback 开发端点可 HTTP。禁止重定向、环境代理、自动重试；连接3秒、整个请求及读取10秒，响应最多64KiB。响应 `DownstreamCapabilitiesV1` 严格字段为 schema_version=1、delivery_mode=TARGET_ONLY、accepted_package_versions（当前唯一版本字符串1）、environments（不重复的PAPER/LIVE，1–2项）、market_capability_versions（不重复非空字符串，1–64项，每项1–200字符）、accepting_targets（布尔）、checked_at（UTC时间）。不接收账户、订单、仓位或执行权限字段。checked_at 不得晚于本机5秒或早于本机60秒，采纳观察时还须绑定本次探测开始时间及精确配置revision；自报接受目标不构成QZ审批。接受版本、环境与市场合同必须同时匹配登记配置和原Package，accepting_targets=false阻止新交付。响应不保存或展示凭据反射、任意错误正文。观察持久化、期限和事务消费另按上述readiness门禁执行；仅网络方法存在不表示已实现审批准入。

人工下游探测为 POST `/api/v2/integrations/downstreams/{id}/probe`，请求 `DownstreamProbeRequestV1`（schema_version=1、expected_revision），需要近期Operator认证或精确DOWNSTREAM_PROBE单次grant。200只表示记录了AVAILABLE/UNAVAILABLE观察，不能视为交付成功。准备和完成分别使用短事务，网络在事务外；完成时重新核对身份、grant、配置revision、enabled和20秒总采纳期限。本次checked_at不得早于探测开始5秒；不可变qz.downstream_probe/1产物和观察行、原命令回执原子关联，失败回滚并精确回收未引用文件。观察有效期固定为探测开始后60秒，重放不能延长。最近观察按服务端started_at排序，较早开始的迟到响应不得覆盖较新探测失败。GET `/api/v2/integrations/downstreams/{id}/readiness`只读当前revision和最近观察，返回NOT_CHECKED/DISABLED/STALE/UNAVAILABLE/AVAILABLE、available_package_versions、available_environments；有效范围取登记配置与真实观察交集，accepting_targets=false或空交集为UNAVAILABLE。真正配置更新使旧revision观察失效，记录探测不修改配置revision。审批仍须匹配原Package市场合同，readiness不是审批。部署DOWNSTREAM_TARGETS与RUNTIME_TARGETS使用同一严格格式但独立允许列表，默认[]。


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

浏览器验收分离两种证据：三视口/axe/PWA故障展示用受控HTTP fixture；真实入口验收必须启动当前 `server` 原生二进制、PostgreSQL18/PGMQ1.10的新库和独立非owner应用角色，执行原生迁移、一次性bootstrap、真实 `/bootstrap/confirm` TOTP绑定、项目写入、丢ACK同键重放、跨源拒绝和确认退出。不能用页面文案或mock响应代替数据库事务。原生Playwright使用单独配置，不混入fixture测试；原始error-context等输出只放本次私有临时目录并清理，公开证据仅包含脱敏摘要。浏览器/Vite子进程只获得环境白名单，禁止继承管理员URL、数据库密码和GitHub/模型令牌；Vite关闭隐式.env加载。收到终止信号后先终止并等待本次子进程，再清理本次库/角色，脱敏清单失败不得阻止资源清理或发布原始日志。Web CI必须与Rust基线一致：固定1.98.1、仓库实际server包、固定PG18/PGMQ镜像、精确PR Head和生成合同无差异。此验收覆盖认证及研究组织入口，不冒充T42的Alpha/组合/交付全链路。

### 9.1 已知不可提交选项与开发文件边界

费用字段按原生域合同联合校验：UNAVAILABLE 要求金额/币种均为空；ESTIMATED 要求正的精确十进制金额和所锁 iso_currency 0.7.0 接受的币种。可选空字符串只在线协议转换处变 null，不 trim/浮点转换非空金额。浏览器使用从同一 Rust 原生币种表生成的 Ajv 字段校验器，不维护第二份 ISO 清单或把三字母正则当币种目录。用户明确切换到 UNAVAILABLE 时清空金额/币种；载入的历史不一致值不可静默删除，须提示并拒绝提交。只读冻结记录不因表单 effect 被改写。

BUDGET_EXHAUSTED 作为独立 HTTP429 Problem 保留，field_errors 仅使用封闭的资源字段映射，未知内部标记不反射。冻结额度耗尽不标 retryable，也不编造 Retry-After；AUTH_RATE_LIMITED 的原有限流重试语义不变。客户端成功响应依据实际 OpenAPI 的 operation/status/media type 选择原生 JSON/Ajv、无内容或 binary/stream 处理；仅声明过的二进制操作可透传未消费的 Response。服务器返回错误媒体类型、未知成功状态或 JSON DTO 违约仍失败，不能以 parseAs=blob 或任意非JSON绕过响应合同。二进制错误响应继续按 Problem 处理与认证失效，不吞成下载成功。

长 Brief 抽屉的下拉菜单通过官方 ConfigProvider/getPopupContainer 锚定到可滚动表单内、相对定位的字段容器，而非固定到锁定滚动的 body。三视口验收使用真实鼠标操作字段和可见选项，不使用 force/DOM click/键盘绕过不可点击菜单来制造通过；费用、数据角色和访问边界须在滚动后仍可操作。依据：Ant Design Select 的 getPopupContainer 与 FAQ（https://ant.design/components/select/）。

复合资源选择器通过官方ConfigProvider.useConfig继承Form的disabled上下文；请求在途、离线、首次选项未加载时不能打开选择器或改变原请求引用。菜单容器回调身份保持稳定。系统减少动态效果偏好通过matchMedia订阅映射至Ant Design原生theme.token.motion；reduce时motion=false，其余情况保持原生动画，运行中偏好变化不重建表单或清空选中值。不用全局0.01ms CSS覆盖组件的原生动画生命周期，不增加固定延时、手工坐标、强制点击或关闭无障碍规则。三视口重复验收覆盖reduce/no-preference、偏好变化后选项点击和在途表单不可编辑。依据：官方主题motion配置（https://ant.design/docs/react/customize-theme）与ConfigProvider.useConfig（https://ant.design/components/config-provider/）。

表单不能把当前服务端必定拒绝的值当作可操作能力。费用目前仅有 UNAVAILABLE/ESTIMATED；EXACT 尚未接通，草稿编辑不提供该选项，已载入的不支持值必须先由用户明确修改而非自动替换。项目为 ARCHIVED 时只允许保留 ARCHIVED；ACTIVE 选项须取得该项目 current_brief_id 对应的真实 Brief，校验精确项目/编号、FROZEN 及 frozen_at，加载/错误/缺失时不可用。这只是基于服务端事实的字段约束，不授予权限，不替代提交事务对状态、活动 Run、近期认证与 revision 的再次检查。SEALED 的访问候选仅 METADATA_ONLY/EVALUATOR_ONLY；其他分区仅 METADATA_ONLY/RESEARCH_READ。分区变化使旧选择不兼容时清空该字段并要求用户重选，不能自动升级权限；依赖校验同时拒绝程序化或残留的不合法组合。

PWA 生命周期 fixture 的宿主与 CI 限 Linux；浏览器产品不受此限制。静态内容读取使用 Node FileHandle 与 Linux `/proc/self/fd` 的已打开目录句柄，逐个单路径组件以 O_DIRECTORY/O_NOFOLLOW 打开中间目录、以 O_NOFOLLOW 打开最终普通文件；构建根和任意子目录软链均拒绝。单请求最多32层，每个目录锚点保持打开到读取结束，不因路径被 rename/软链替换而重新解析旧路径。最后只在同一个文件句柄上 fstat/readFile，所有已取得句柄在成功/错误路径都关闭；缺少原生能力时失败，不降级成先 realpath/lstat 再按路径读取。该最小 fixture 适配不用于产品 Artifact 服务，不声称防止可信构建目录拥有者原地改写文件或进行特权 mount。

公开 HTTP `Idempotency-Key` 为单个头值，1–200个可打印ASCII字节；首尾不得为空格，内部空格允许，控制字符、非ASCII、重复头均拒绝。HTTP原生HeaderValue的可见ASCII检查与既有运行时校验不变；实际HTTP OpenAPI统一用原生utoipa字符串 minLength/maxLength/pattern发布同一可表达范围。CredentialIssue.scope_codes继续使用Vec和原生domain/DB拒绝重复；其生成Schema必须uniqueItems=true，不能改成反序列化Set悄悄删除重复值。

### 9.2 失败合同、联合约束与验收资源（Review 5144502684）

客户端Ajv standalone使用原生inlineRefs=false复用组件校验函数，不在每个操作展开重复Problem/Brief代码；保持仓库既有Workbox 3MiB单文件预缓存上限，本次不提高上限，也不能省略校验来掩盖生成代码膨胀。

所有 HTTP 响应（包括非2xx）按已生成 operation/status/media/schema 组合核验；Problem 必须使用 application/problem+json、有效 UUIDv7 request_id 与精确十进制 revision，body.status 必须等于 HTTP status。格式错误响应不能触发认证事件、重试或业务错误展示。只移除手写的宽松 Problem 形状检查，不放宽 binary/SSE 的成功响应边界。responseFailure 的 schemaPath/method 必填；手工 fetch 的 RunEvents 也绑定 GET /api/v2/runs/{id}/events，不提供省略操作上下文的宽松路径。SSE 只有生成合同认可的成功状态/媒体才能连接；格式错误401不触发退出，只有已验证 AUTH_REQUIRED 可以。

费用表单与 isDecimal 复用从原生 DecimalValue、BudgetV1 正金额字段生成的 Ajv standalone 校验器，不维护第二套较窄语法。+000.0100、.1、1. 等原生可表示的正金额保留原文提交；零/负数、溢出、尾随控制字符仍拒绝。共享 decimal-wire/cost-tuples 同时覆盖Rust、Schema和实际表单派发，schema-valid 不等于当前 EXACT 能力可用。

BudgetV1 继续保留既有 Rust/serde 字段，原生 OpenAPI 用公共字段与费用模式 oneOf 联合表达：UNAVAILABLE 金额/币种省略或 null；有费用上限的模式必须显式正金额和原生币种。EXACT 的结构有效不表示该能力已实现，仍由现有 domain/能力检查拒绝；不能用 schema 伪造准确账单。基础币种复用同一 iso_currency0.7.0 成员表。Brief 表单对 dataset_revision_id 去重只做拒绝，不删除或合并用户绑定；修复轮数≤总轮数、合格目标数≤实验数按完整路径双向依赖重验，不自动扩大预算。

真实浏览器验收在发出 CREATE ROLE/CREATE DATABASE 前登记本次随机名字的创建意图，终止或 ACK 丢失后在子进程退出后仍对这些精确名字执行 DROP IF EXISTS；不凭收到创建 ACK 的布尔值决定是否清理。随机名字冲突必须预检拒绝，不能清理已有对象；不扩展到名字前缀匹配或其他实例。清理失败保留失败回执，不声称资源已删除。真实原生 PostgreSQL 的丢 ACK 与 SIGTERM 回归必须确认无遗留本次资源。

### 9.3 数据与原生集成管理界面

设置页分为浏览器安全、原生集成、数据管理三个任务区，保留默认安全页；仅挂载当前任务区，未访问的页面不主动请求数据。Runtime/Downstream 使用已有严格配置接口：保存不意味着可用，Runtime 探测必须显式发送新的命令并显示真实版本、时限、能力交集与资源上限。数据管理按 Source→许可授权→原生版本登记的流程组织，不让用户在表单自报来源、PIT、样本数或原始质量数据。登记时固定打开表单时的 Source/Runtime revision；409展示真实冲突，不自动提高版本重试。

关联选择器复用官方 Select 与 React Query 的原生分页；显示已载入条数、显式载入更多和刷新，搜索只匹配已经载入的记录，不把前50项当完整数据。Runtime、许可证明、Grant、Universe均选择已有对象；不可用选项只按已知真实条件禁用，最终授权仍在服务器事务复核。缺接口/响应不兼容/权限失败与真实空列表分别显示。数据版本展示真实 origin/PIT/许可核对时间和原生证据引用；非REAL/未验证/缺登记历史不得伪装有效数据。

集成凭据只在独立瞬态表单字段和 write-only 请求中存在，不进入 Query/Mutation缓存、URL、日志、localStorage或持久表单。成功后清空明文，仅保存原生对象引用供配置提交；更新可保持既有引用但绝不回读秘密。Runtime、Downstream和TLS_CA使用共享Rust生成的目的相关范围；原生PEM仍由服务器校验。凭据请求未结束时阻止关闭或提交父配置，结果未知保留同一意图键重放；放弃配置不擅自删除已发表原生凭据。

所有管理操作保持离线禁写、真实错误、原生幂等、CAS、不乐观批准、键盘/触摸/窄屏可达和PWA未保存保护；测试分别覆盖真实后端和受控三视口展示，后者不替代生产数据或完整T42证据。

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

运行数据库身份检查包含 PostgreSQL 原生 ADMIN OPTION 委派闭包（即使 INHERIT/SET
暂为 false），并拒绝可达的服务器文件读写/程序执行预定义角色，不能仅核对
`rolsuper` 或单张表的 ACL。只有成员身份但无 INHERIT/SET/ADMIN 的边不产生权限。

Cookie Secure/HttpOnly/SameSite，同源 Origin/CSRF；机器/CLI 使用独立范围受限可撤销 token，不把浏览器 cookie/TOTP secret/动态码当 API token。Agent MCP 不复用 Operator session。TOTP/session/AEAD/随机 verifier 使用成熟库，依第 0 节选语言，不自制密码学。Secret 仅受信任进程解析；UI 只见 configured/status/last_checked；日志不含 auth 文件、token、完整 Provider/stderr/traceback。

目标 Compose 为 server、worker、PostgreSQL+PGMQ；Codex/远端按 profile 配置，单机也保持权限分区。生产同源 HTTPS，未认证写接口不能暴露。默认不托管在线 wheel 上传/安装/插件市场；受支持集成经显式版本/能力登记，既有使用固定 release。上游 Python import 只在隔离 job/必要受控适配，不长期热加载/卸载不可信插件。

配置至少包括：HTTP bind/public URL、数据库/PGMQ、artifact root/backend、runtime endpoint/credential ref、Codex binary/CODEX_HOME、代理与 egress allowlist、预算/资源限制、session/TOTP secret ref、日志脱敏、backup destination/retention、telemetry opt-in。缺失 fail fast 指明字段，不退到公网无认证。来源显示 SYSTEM/EXPLICIT/DEFAULT 与安全摘要；启动验证镜像、协议、schema、ABI，不等首次真实研究才崩溃。

统一 request_id/project_id/cycle_id/run_id/attempt，tracing/OpenTelemetry 兼容；指标含队列等待/重投/lease loss、时长、未确认取消、孤儿任务、数据失败、预算耗尽、审批过期、反馈迟到。readiness 分 research/sealed/portfolio/paper/live，含组件、状态、reason、checked_at/valid_until；健康检查不每次启动 Codex/付费调用。检测连接是显式有总超时动作。

复用 PostgreSQL 原生备份/pgBackRest、restic 等，不建备份平台。备份数据库、引用 artifacts、配置和受保护原生 Codex profile；市场目录由原所有者按版本备份，密钥与数据分离。恢复先暂停 admission，恢复一致版本、查悬空引用、reconcile 未完成远端任务，再恢复消费；不盲目重放 Live。重置/恢复明确处理旧 session/设备/凭据。RPO 24h/RTO 60min 是待演练目标，只有实际记录才声称达到。

升级检查版本、磁盘、备份与兼容矩阵；不可逆 schema 用备份恢复回滚，不声称旧二进制任意读新 schema。磁盘满/DB断连/runtime离线停止接新任务并明确告警。恢复报告包含备份时点、DB/产物验证、reconcile 清单、未重复 Handoff、凭据处理、耗时和损失区间。

## 11. 迁移、删除与 README

运行数据库身份的无 owner/DDL/TRUNCATE/TRIGGER 边界覆盖 `app`、`tower_sessions` 与 `pgmq` 三个服务 schema；缺任一 schema 也拒绝启动。原生 current_user/session_user 的可继承、可 SET ROLE 权限均须检查；普通会话/队列 DML（含删除）不被误拒。部署只授权 DML，不把外部组件的 schema 排除在安全边界外。

原生会话表兼容性同时约束结构与行为：必须是普通 logged heap 表；三列、原生默认 collation/未缩减类型精度、NOT NULL 与立即生效 id 主键；不得含默认/生成/identity、RLS/策略、继承/分区、自定义规则/触发器、额外唯一性/CHECK/FK/exclusion 约束。PostgreSQL18 的原生 NOT NULL catalog 记录是合法结构。仅允许使用原生默认 opclass/collation 的简单非唯一 B-tree 性能索引，不接受表达式/条件索引或额外 UNIQUE。检查真实 catalog 记录而非可能滞后的 relhas* 提示位。失败沿用 native_session_schema_incompatible，并回滚整个部署事务；绝不通过清表或删除用户定义静默修复。

升级的写入切换点：先暂停新 HTTP/CLI/MCP 命令与 Worker 调度，结束旧事务，再用 `cargo run --locked -p server -- migrate`。Store 在专用连接上先取得 SQLx 原生迁移 advisory lock，随后开启 READ COMMITTED 外层事务，从原生 catalog 读取现有 app 普通/分区表，先认证状态、后其余表按名称稳定顺序取得 SHARE ROW EXCLUSIVE 锁；取得全部锁后才运行 SQLx Migrator。SQLx 原样校验已应用 checksum、用原生嵌套 savepoint 执行所有待应用文件，外层提交同时公开整个批次及迁移记录；禁止 no-transaction 迁移。新库无 app 表时仍由原生迁移锁串行化。锁等待超时或校验失败整体回滚，专用连接关闭以释放 session advisory lock，不返回运行连接池；不能用独立 SQLx CLI/逐条 SQL 对活跃实例升级。本合同不声称零停机升级；锁获取之前已提交的旧事实必须被新的回填/检查看见，不能宣称锁请求一发出旧事务就已停止。

完整 CLI 命令的原子性覆盖领域迁移、原生 session 表、可选运行角色的存在性检查与 DML 授权；这些步骤必须共享上述唯一 PgConnection/外层事务，全部成功后仅一次 COMMIT。不得先提交 Store 再从 pool 执行 `PostgresStore::migrate` 或 GRANT，也不能让另一连接的 advisory guard 冒充同事务。锁定 tower-sessions-sqlx-store 0.15.0 的 migrate API 不接受调用方事务；新增 `202609060009_native_sessions.sql` 原样复用该版本的默认 schema/table DDL，注明上游许可，交由 SQLx 版本管理。原生 PostgresStore 继续处理序列化及全部 session 操作，不 fork 或重写。每次正式迁移均在提交前核对 session 三列类型/非空与主键合同；已有不兼容对象明确失败而不删除/重建。锁定范围包含已有 `tower_sessions` 普通/分区表。运行角色名通过参数核对及原生 quote_ident，GRANT 不另开连接；会话合同、角色或任一权限操作失败，整个批次和 epoch 撤销均回滚。连接在异常/取消后关闭以释放原生锁；COMMIT 应答丢失只能记录提交结果未知，不能声称服务器一定回滚。上游今后支持外部事务时可替换适配，但已发布迁移字节保留。原生迁移参考表的 catalog 对比和原生 PostgresStore CRUD 是必需合同测试。

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

沿用Issue62第6.3–6.4节的原生Thread/Turn：预算的一轮是一次App Server `turn/start`及其完整原生工具循环，不是Provider HTTP请求数。以下“模型请求”指QZ发起的原生Turn；其内部工具后续模型请求全部占用该Turn的原预约，按原生累计用量结算，不另建代理或接管工具循环。max_turns_per_mission不声称限制内部HTTP请求次数。QZ发起的每个新Turn、修复和重试均需新预约，不因沿用Thread、失败或新Attempt清零。消耗结算绑定精确原生Turn/Attempt并幂等转移预约到已用；结果未知保留预约，不因断线/取消请求退款。实际超额如实记录并阻断后续准入，不将账本裁到上限。ESTIMATED只对估算预算作准入，不承诺Provider最终账单严格不超金额，缺准确计费继续拒绝EXACT。

模型轮数与token/费用在相同准入事务内预约，但轮数属于精确Mission（run_id），不能把整个Cycle的多个Mission合并计数。每个新原生Turn必须给出可信调度器绑定的mission_id和turn_kind=RESEARCH|REPAIR；每次恰好预约一轮，REPAIR同时占总轮数和修复轮数。Mission从不可变逐轮账本投影used_turns/reserved_turns/used_repair_turns/reserved_repair_turns（u16，JSON整数，数据库非负约束），不另存可被重置的权威计数，repair分别不超过相应total；准入使用used+reserved+1与冻结max_turns_per_mission/max_repair_turns比较，checked_add溢出拒绝。研究者不能自行创建新Mission或更改turn kind来重置/扩大预算。同Thread的新Turn（包括科学结果回送和修复）单独准入，不重复占用实验数、运行并发槽或job CPU；同一Turn内部原生工具续请求保持原预约，真正新任务仍完整预约。不同Mission的轮数隔离，Cycle的token/费用仍全局累计。缺失或身份不一致的Mission账目报错，不默认为零。

模型发送前先在同一事务持久化轮数/token/费用预约与 pgmq.send；原生分派器在独立短事务持久化唯一发送意图，再做外部 I/O。命令幂等绑定与最终实际用量 receipt 分开，准确阶段见 A6.1。ACK 丢失不释放预约/重新开轮，必须先按原生 Thread/Turn 对账；已发送轮即使失败/取消也计已用，重试和修复同样占额。只有确认从未发送才释放未用预约。首次 Mission 的零账目只能由可信服务和新的 run 在同事务创建；独立 Reviewer 是独立受控 Mission，不用重置研究者计数冒充隔离。

新轮预约及首次发送均在锁定 Session 对应的当前 Profile 后重新检查精确版本、无进行中账号操作以及数据库时间下的 Attempt lease/deadline；等待配置锁不能复活过期执行者。Profile 修改只阻断新发送，不阻断旧预约/发送意图的准确读取、原生 Turn 绑定、对账与真实用量结算。已经发送但回执未知的调用不能因版本改变就退款或改用新账号重发。

原生实际token或估算费用超过本轮原预约时，即使Cycle总额尚未超限，也阻断该Cycle的新模型预约及尚未发送预约的首次派发；不是仅检查Cycle总和。使用同Cycle锁下的不可变reservation/receipt直接比较，不保存可重置的overrun标记。既有回执重放、已发送对账和确认未发送的结算继续可用，真实用量不能因超预约而回滚、裁剪或补零。

Optuna 内部 trial 使用预分配预算，不能藏在一次 job 无限搜索。资源/turn/并行上限必须有效正值且符合 runtime capability；修复 turn 不超过总 turn。停止规则由用户冻结，Agent 不能扩大。

### A1.1 Brief 草稿作者事务

`POST /projects/{id}/briefs` 只接受 `BriefCreate={schema_version,content:BriefContentV1,bindings:BriefBindingV1[1..64],supersedes_id?}`；project 从路由派生，人工CLI的 `BriefCreateIntent={project_id,request}` 绑定该完整路由身份与请求，不能借给另一项目。`BriefContentV1` 精确包含 hypothesis/economic_rationale（各1..8000字符）、universe_version_id、target_kind、horizon_kind/horizon_value、ISO base_currency、benchmark_ref?、evaluation_policy_id、execution_assumptions_id、BudgetV1、StopRuleV1；`BriefBindingV1={dataset_revision_id,role,access_policy}`。所有ID已登记，服务端分配Brief ID/version/revision，初始DRAFT；不接受root_lineage/current_brief/frozen_at/终态。

`PATCH /briefs/{id}` 接受 `schema_version,expected_revision,content,bindings`，原子全量替换DRAFT内容和绑定；不可修改project/version/supersedes身份。保存验证预算/停止规则、固定或可变horizon形状、同项目政策、政策执行假设一致、币种与数据Universe一致、成员唯一和原生partition角色。SEALED禁止RESEARCH_READ；非SEALED禁止EVALUATOR_ONLY；METADATA_ONLY不等于执行授权。FROZEN不得保存。保存不宣称当前许可、PIT、原生能力、Sealed机会或资格已经通过；正式freeze另做当前事实检查。

复用既有Operator命令回执与项目行锁；同项目版本checked递增，同key回原响应、换请求409，CAS错误返回真实current_revision且无半套删除或孤儿回执。ARCHIVED禁止新建或修改。014增量迁移使DRAFT子成员可替换；INSERT/UPDATE/DELETE一律锁父Brief，FROZEN一律拒绝，禁止跨父移动与改成员身份。冻结和子写入竞争由同一父锁决定；既有迁移和历史数据不改。

`GET /projects/{id}/briefs` 使用现有limit1..100/UUID倒序cursor；`GET /briefs/{id}` 返回公开BriefView和绑定，无Secret/URL/宿主路径/raw数据。机器只读需要有效RESEARCH_READ且精确project。修改必须近期Operator或完整请求绑定的单次人工CLI grant，增加封闭BRIEF_CREATE/BRIEF_UPDATE操作，不增机器scope。

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

原生模拟必须显式冻结费用、填充/滑点和延迟模型。NativeSimulationSettingsV1的
fee_model、fill_model、latency_model均为NativeModelRefV1，不使用隐式默认模型或
旧顶层insert_latency_ns。NAUTILUS_MAKER_TAKER对应MakerTakerFeeModel，参数为空；
费率仍逐资产绑定原生instrument的maker/taker。NAUTILUS_DEFAULT_FILL对应
DefaultFillModel，参数为prob_fill_on_limit、prob_slippage（Decimal [0,1]）及必填
random_seed（DbCounter），滑点由这个原生填充模型拥有，不另造QZ滑点算法。
NAUTILUS_STATIC_LATENCY对应StaticLatencyModel，参数为base/insert/update/cancel
latency_ns；每个合计不得溢出，插入总延迟必须大于零。三者锁定nautilus-execution
0.63.0及完整Rust类名，角色/版本/参数不匹配拒绝，不降级到默认值。
这些冻结模型必须实际进入Nautilus venue配置；目标因果/到期检查使用同一插入总延迟。
锁定Nautilus 0.63.0的单基础币种账户中，CurrencyPair只支持MARGIN，Equity可用
CASH或MARGIN。执行假设登记、原来源重验、原生组合构建和模拟共用此账户/资产类
约束；不自动改写CASH配置、不声称支持多币种现金账户。旧不可变假设保留原值，
不受支持的组合在新消费时明确拒绝。
它们不证明原费用证据、许可或组合资格，正式执行假设创建仍须绑定原项目/产物。

ExecutionAssumptionsCreateV1通过原Operator命令创建不可变执行假设，包含project_id、
runtime_id/expected_runtime_revision、input_set_id、dataset_revision_id、完整settings
及settlement_rule_ref。InputSet必须已冻结且属于项目，不读取Sealed；原目录登记、
许可、Runtime探测/PORTFOLIO_SIMULATE镜像与simulation-models/1须在创建事务重验。
settings费率和币种必须逐资产精确匹配原登记instrument definitions，不能手填替代。
instrument definitions保留锁定Rust Nautilus InstrumentAny的原生Serde结构
（例如{"CurrencyPair":{"id":...,"quote_currency":...,"maker_fee":...}}），
不转换为Python式顶层type字段。登记、能力探测和费用绑定共用此结构；类别唯一、
原生id唯一且覆盖质量报告资产，未知/缺失费用拒绝，旧数据不重写或补默认值。
完整settings保存为同项目不可变PARAMETERS（qz.native_simulation_settings/1），
也是fee_schedule_artifact_id的确切内容；原生来源关系另以execution_assumption_sources
绑定原InputSet、Dataset、Runtime探测、project和settings。模型fill/slippage字段
同时引用同一原生DefaultFillModel配置，不生成另一套滑点算法。
声明参数的Artifact origin为SYNTHETIC，不是市场数据来源。组合准入按该原始类型
读取原始字节并验证settings及来源关系，不能要求声明参数冒充REAL；Forward市场数据
与Alpha资格各自的REAL/PIT和授权要求保持不变。
当前这个声明式入口只产生CONSERVATIVE_ASSUMPTION、BAR；可选的历史参与率假设
仅使用下述原生来源，不声称实时流动性；
DATA_BACKED等完整来源能力仍须单独接通，不能由请求标签冒充。旧数据保持原值，
未绑定原生来源的历史行不能伪装成此入口的新版本。原请求/原响应支持精确重放，
创建失败回滚数据库并回收本次未发布对象；不授予Alpha资格或交付权限。

原生DATA_VALIDATE的每份Dataset质量结果可带last_bar_notionals；null/缺省表示
没有测量该项，不表示零成交额。支持bar-notional/1的job从本次原目录选择中逐资产
读取最后一根已完成、已可用的BAR，保持selection资产顺序，记录instrument_id、
currency、event_ns、available_ns、close_price、traded_volume、notional_value。
名义金额复用Nautilus 0.63.0 Instrument::try_calculate_notional_value，使用原数量、
收盘价、合约乘数与原生币种/舍入，不自行重写线性、反向或quanto合约公式；
明确use_quote_for_inverse=false，数值溢出或原生计算失败不补零。
close_price必须正，数量与名义金额非负，真实零成交量允许为零；资产、顺序与时间
必须绑定原质量结果及原始selection。此值是“最后一根历史BAR按收盘价估值的成交量”，
不是该BAR真实逐笔成交金额、未来可用盘口、可成交保证或精准冲击模型，不能因测量
存在自动把执行假设改成DATA_BACKED。后续参与率适配必须绑定原生结果产物、原选择、
决策时可用性、币种与明确期限；未接通这一来源链前不能解除现有参与率准入拒绝。
SEALED分区不输出此明细，原生job按冻结Dataset输入角色返回null；Sealed目录元数据
也拒绝携带此明细，不能借质量报告向研究侧暴露封存价格或成交量。

ExecutionAssumptionsCreateV1可带bar_liquidity:{schema_version:1,report_artifact_id:Id,
maximum_age_seconds:u32>0,participation_limit:Decimal in (0,1]}。这是明确的历史
单BAR、每次再平衡的规划上限，不按预测horizon、经过时间或资金规模放大历史量。
报告必须是同项目/Runtime、同原InputSet中该Dataset的已成功采纳原生DATA_VALIDATE
产物；原任务/Attempt/manifest/输出映射、原选择、当前许可和非Sealed用途都要核验。
仅有qz.data_quality schema、目录登记副本或手填数值不能作为该原生来源。
全部资产必须有原last_bar_notionals，币种等于settings.base_currency；每资产事件
时点到决策时点的年龄均必须小于maximum_age_seconds，available不得晚于决策。
创建时也按数据库当前时刻核对年龄。此期限专属该历史假设，不修改Mandate的预测/
权重max_input_age_seconds；有效期内可复用原假设，过期必须重新创建完整假设/政策，
不能改写旧政策或通过复制资格恢复有效性。不存在自动刷新或默许无期限使用。
源配置与执行假设不可变保存，liquidity_artifact_id与participation_limit绑定原值；
bar_liquidity_valid_until取最早原事件加该期限，向下截至数据库微秒精度；到期边界
不再有效，不能向未来舍入延长。读取返回此原期限，历史行不补造来源或有效期。
该来源不自动提升cost_assumption_status。Build与Candidate发布仍须完成原报告
复核，原生求解以每资产原notional_value/capital乘参与率约束绝对权重变动。
NativePortfolioBuildRequestV1.bar_liquidity可选冻结{schema_version:1,assumption:
BarLiquidityAssumptionV1,source:NativeDatasetSelectionV1}。来源Dataset可不同于
本次Forward Dataset，但必须是执行假设的原Dataset/选择。有绑定时Mandate的
liquidity_ref/最大参与率必须与原配置相等；全部assets.available_notional精确等于
原报告对应资产的notional_value，无绑定时三者均为空。报告作为唯一该用途的
DATA_QUALITY原产物输入，job读取原字节核对完整报告、选择、币种、年龄和数值，
不能只信任冻结副本或提供手填数组。来源校验不替代Store的采纳/许可/资格检查。
Store在Build准入及Candidate发布时重新读取执行假设原来源，核对当前许可、
独立有效期及原数值；原配置/字节不一致为可重试Integrity，真实到期不再具备资格。
最终无文件回调的数据库期限快照同时检查该原有效期。报告origin按已有规则合并，
不能提升FIXTURE/SYNTHETIC；绑定消费需要portfolio-liquidity/1镜像能力。
非零滑点规划适配见A5.2；DATA_BACKED仍需完整来源适配，不因模型概率或历史量升级。

离线STUDY_PORTFOLIO的滚动BAR政策与上述有期限的单次快照是不同输入，不延长或
复用已过期快照。可选rolling_liquidity冻结NativeRollingBarLiquidityPolicyV1
{schema_version:1,maximum_age_seconds:u32>0,participation_limit:Decimal in (0,1]}；
Mandate.liquidity_ref引用该政策的原PARAMETERS产物，max_participation必须等于
政策原值，job重读字节核对。此原生计算合同不替代正式Store政策登记/准入。
只测量研究本身的原FORWARD目录；逐cutoff读取同一原选择的已知前缀，复用
DATA_VALIDATE的Nautilus try_calculate_notional_value，不输入手填available_notional、
不创建每BAR业务对象、不借最后全样本量回填历史。每帧报告保留原NativeBarNotional
值及时间，资产顺序/币种/截止/零量均核对；在实际引擎调仓时再次检查专属年龄。
原生优化使用该历史notional/当时模拟权益乘原参与率限制绝对权重变化，不按经过
天数或horizon放大量，不二次扣成本或推断未来深度。无政策时引用/参与率/测量均空。
真实零量是零约束，缺值、未来、过期或不一致必须失败，不补常数或提升DATA_BACKED。

滚动政策登记复用ExecutionAssumptionsCreateV1.rolling_liquidity，可空且与
bar_liquidity互斥；两者都为空表示没有参与率假设，不补默认政策。登记要求当前
portfolio-rolling-liquidity/1原生能力，沿用原InputSet/Dataset/Runtime/费用/许可
绑定。与费用同事务发表qz.rolling_bar_liquidity/1 PARAMETERS，RESEARCH访问、
SYNTHETIC声明来源；execution_assumptions.liquidity_artifact_id及participation_limit
引用原政策，execution_assumption_sources.rolling_liquidity保存原内容。原政策没有
bar_liquidity_valid_until，因为它不是某根历史BAR的观测；年龄仍在每个实际cutoff
检查。读取返回原政策及rolling_liquidity_artifact_id，旧行保持null。发表失败沿用
原对象清理与同键回执重试，不允许留下半个执行假设；不授予科学PASS。正式Study
准入仍须独立实现和验证，不由登记成功推定可用。

原生Build的rolling_liquidity与单快照bar_liquidity互斥，绑定Mandate原政策PARAMETERS
文件，输入assets.available_notional必须为空。Job重读原政策并逐值比较，从原selection
读取最后已知BAR，复用Study/DATA_VALIDATE的Nautilus测量；bar_notionals原值进入
qz.native_portfolio报告。原事件必须等于本次forecast_asof，位于原选择内且当时已可用；
按原政策年龄与币种核对，再将原名义量赋予同一资产输入，与原费用/滑点适配合并后
交既有求解器。结果采纳重建相同资产并逐值核对；无政策时测量数组为空，不能混入
手填额度、别的时点或币种。需要portfolio-build-rolling/1原生能力。
Store在原Build事务重读已登记政策PARAMETERS、原InputSet/Runtime/当前许可并逐值
核对保存配置，原政策作为唯一该用途输入冻结，不把声明SYNTHETIC合并成市场来源。
原生成功后仍须按原manifest能力、完整报告、政策原文件和所有资格/费用/权重来源
重验；按数据库当前时间核对原BAR年龄，最早event+maximum_age向下取整到微秒。
此截止与权重/目标期限共同进入发表文件后最后一个无文件回调的数据库时限快照，
不能因为发表耗时跨过失效边界。政策或报告损坏为可重试Integrity；真实过期保留
原solver_status但Candidate标INVALID且无目标；发表中途过期回滚并允许重试封口。
该发布不创建PORTFOLIO Evaluation或Release，独立组合评估仍不可省略。

原生Universe membership每条带可选groups（最多64个唯一、1..120字符的组标识）。
null/未提供表示分类未知，[]表示来源明确声明没有组；不自动按名称、币种或证券
类型猜分类。组集合随该条valid_from/valid_until/available_at生效与可用，沿原
不可变qz.native_catalog_metadata和qz.universe_membership发表，不新增分类引擎。
组合有group_bounds时，必须从本次原Forward Dataset/Universe来源解析：决策时点
在Universe覆盖范围内且不早于selection_asof；每个资产恰有一条valid_from≤决策、
决策<valid_until（或无终点）且available_at≤决策的记录，groups必须已知。
未知、缺失、重叠歧义或请求的组不属于任何参与资产，均在Run准入前拒绝。
源组集合进入冻结assets.groups；原生求解与结果绑定不得删改，Candidate发表前
按原决策时点重新读取原InputSet登记证据复核集合；原不可变分类不一致报完整性
错误并保留发布重试，不把来源损坏封口成最终INVALID候选。无组约束时不要求组分类，
不把未读取分类当作来源已声明空集合。组来源不提升数据origin/PIT或成本状态。

`event_start < event_end`；发布 snapshot 不原地覆盖，更新新目录/版本；许可与用途匹配。PIT 报告证明 available_at 来源，不用 ingest_at 替代。Universe 含退市/到期；静态今日成分明确有偏，不能称完整历史池。

`NativeModelRefV1={schema_version,adapter_kind,upstream_class,upstream_version,parameters}`。class/adapter 来自服务端 allowlist 和实际 capability；parameters 为对应锁定适配器的严格 schema。未知项拒绝，不映成 GENERIC/DEFAULT；禁止任意 Python import/path/exec 越界。

已接通的组合适配使用CLARABEL_QP（clarabel::solver::DefaultSolver，0.11.1，
parameters为AllocatorSettingsV1）与FIXED_WEIGHTED_FORECAST（ndarray::ArrayBase::dot，
0.17.1，parameters为空对象，原混合权重仅在forecasts中）。原生AllocationInputV1
必须显式带optimizer和alpha_ensemble，不再另带settings或按缺省版本选择实现；
适配角色、类名、版本、参数均匹配才执行。更多模型须按实际能力单独接通，不能
用这两个模型冒充协方差估计、完整Mandate或其他目标已支持。

协方差原生引用为SAMPLE_COVARIANCE / ndarray_stats::CorrelationExt::cov / 0.7.0，
parameters仅含ddof=1。显式模型引用进入现有样本协方差入口，错误角色、类名、
版本或ddof拒绝，不切换总体估计、年化、正则化或补缺值。它不证明传入收益序列
已由可信目录/许可/决策时点绑定；这仍是完整Mandate和Candidate编排的前提。

### A2.1 不可变数据授权与原生身份

将可变 `data_sources.license_reference/allowed_uses` 移除；仅 name/enabled 可变。runtime_id/native_catalog_ref/provider_kind 创建后不可变，原生数据库守卫与正式管理入口共同阻止改写；需要不同来源时新建来源而不重绑历史许可/数据。`dataset_revisions` 增加 `data_use_grant_id:Id FK data_use_grants`，授权属于同 source（复合FK）。

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

### A2.2 正式数据管理与原生登记

Source、许可、撤销、Dataset、Universe沿用A2既有表，不新增业务数据源、内容hash或通用CRUD平台。`POST /data/sources`使用DataSourceCreate(schema_version,name,runtime_id,native_catalog_ref,provider_kind=NAUTILUS_CATALOG,enabled)，PATCH只接schema_version/expected_revision/name/enabled；Runtime、原生登记key、provider身份不可变。native_catalog_ref是已配置Runtime的精确registry key，不是URL、绝对/相对宿主路径、SQL或调用方自选fetch。保存配置没有网络副作用，不等于数据或引擎可用。已登记原生Source在不同命令键下重复创建明确409，不把原生唯一约束冲突报告成503。

DataGrantCreate显式包含schema_version/source_id/license_reference/evidence_artifact_id/allowed_uses/valid_from/valid_until；完整Source绑定同时进入规范化OperatorCommand意图，不能只在可更换的HTTP父路径里。证据必须引用已发布非空、OPERATOR作者、非Sealed的本地REPORT。Source锁下分配单调version，授权者/时间由服务端生成。DataGrantRevoke(schema_version,effective_at?,reason_code,reason)追加记录；省略生效时间用锁后的数据库实时钟，显式值只能未来生效，不能回溯改写发生过的读取机会。许可读取同时返回原始字段与checked_at时的ACTIVE/NOT_YET_VALID/EXPIRED/REVOKED，read-time状态不构成新的授权或续期。

`POST /data/revisions`使用DatasetRegister(schema_version,source_id,grant_id,expected_source_revision,expected_runtime_revision,native_storage_version,existing_universe_version_id?)，不接受origin、PIT、计数、质量、原生快照正文、URL或宿主路径。该命令与RuntimeProbe相同，作用目标是已有Source，返回原生登记的Dataset及完整命令回执，HTTP统一200；目标不是预先由CLI grant分配的新Dataset。这样相同原生身份可在另一个幂等键下返回原Dataset，不被一次性人工授权强迫制造第二个UUID。SourceCreate和GrantCreate仍是服务端分配新资源的201创建命令。

原生登记先在短Operator事务读取原回执并按Source→Runtime→Grant共享锁顺序验证精确配置版本、enabled、许可和撤销；锁后实际数据库时间产生20秒票据。事务外复用已验证的RuntimeTransport读取固定 `/runtime/v1/catalogs/{registered_ref}/metadata?storage_version=...`：原生URL段/query编码、部署origin/SocketAddr/TLS、无代理/重定向/自动重试、1MiB累计响应、原始重复字段及解码凭据反射拒绝。RuntimeCatalogMetadataV1必须精确匹配登记Source/key/version/provider，保留真实native_snapshot_ref、来源、PIT说明、原生quality及Universe。结构与时间排序校验不是历史PIT证明，测试数据不会升级REAL。连接或集成暂不可用返回503/INTEGRATION_UNAVAILABLE，不伪装成字段冲突或空数据集。

完成事务重取原命令锁并优先处理回执，复核票据/配置/许可/授权，只有命令所有者才能发布Store分配的原生对象批次。发表qz.native_catalog_metadata/1原始JSON、qz.data_quality/1、qz.universe_membership/1、qz.instrument_definitions/1，然后在同一事务提交只读Artifact元数据、必要的新Universe、Dataset、dataset_registration_evidence和完整回执；网络I/O不在此事务。全局原生管理证据为OPERATOR访问级别、RUNTIME作者、保留真实origin，不作为Agent提交的RESEARCH报告或正式评估PASS。

原生唯一(source_id,native_snapshot_ref,storage_version)存在时，只有原Grant、指定Universe及原metadata的完整内容一致才重放；两侧比较均使用实际接收的JSON值，不把一侧DateTime重新序列化后与另一侧原始RFC3339字符串比较。已保存metadata先按原Dataset的origin核对，新文档改变origin属于409/NATIVE_IDENTITY_CONFLICT而非422。任何原生身份冲突均不能通过更换许可、来源、UUID或原生别名洗掉历史。旧Dataset缺正式registration evidence时保留历史并拒绝推断填补。DatasetRegistrationEvidence仅增加dataset_revision_id PK/FK、native_metadata_artifact_id UNIQUE/FK、source_revision、runtime_revision、observed_at、created_at，不改旧不可变记录。native版本的登记仅授予可供受信任验证的引用，首个DATA_VALIDATE仍必须实际打开原生快照。

Dataset可显式重用既有Universe，前提是名称/calendar/version/selection_asof/覆盖/历史成员及原始instrument definitions全部一致，且origin一致；否则409。由此Discovery/Validation/Sealed可共享同一冻结Universe，不靠复制相同成员为三个新身份。已发布原生对象而DB失败时，原事务结束后重新取得原Operator锁并逐个确认精确对象无正式引用才回收；不确定则保留，不扫描或删除其他产物，复用现有原生Operator发表恢复机制。

Universe查询派生registration_state=NATIVE_METADATA|LEGACY_UNVERIFIED：只有被正式dataset_registration_evidence关联的数据版本引用时为NATIVE_METADATA；没有证据的既有Universe保留并显式标记LEGACY_UNVERIFIED，不回填、改写或描述成真实原生登记。该状态不代替数据版本的REAL/FIXTURE/PIT/资格。Source原生registry-key的既有语法由Rust发布到OpenAPI，Ant Design使用该原生生成字段验证器，不另维护一套URL/路径规则。

管理写入仅近期Operator浏览器或完整意图单次CLI人工grant。全局管理元数据读只允许Operator或严格只读DOCTOR_READ CLI，不因此赋予Mission/Automation/Downstream数据管理、Sealed raw、Secret、SQL能力；Mission仍通过项目授权InputSet的数据工具。列表保持UUID cursor、默认50/上限100；Source/许可/撤销/Dataset/Universe真实API与同一CLI/Ant Design管理界面逐项回归。新鲜授权、原生事务/文件/真实TLS、故障回滚及同键/原生身份并发重放必须实测，不能以DTO或页面存在替代。

### Runtime 实际可用性与调度边界

Runtime 配置中的 enabled、allowed_capabilities 仅表达 Operator 意图，不是实际可用性。Cycle 与 standalone Run 的共同准入事务，以及 Attempt 首次由 NOT_SENT 转为 SENT_UNKNOWN 前，必须读取当前配置 revision 对应、尚未过期的成功原生探测，核对 job kind 和实际 max_wall_seconds / max_memory_mib / max_output_bytes。预算上限不能代替执行环境上限；cpu_seconds 是累计记账预算，不得错误解释为申请的 CPU 核数。拒绝必须与 Run、预算预留、事件、PGMQ 消息、幂等回执一起回滚。已经发出但结果未知的稳定远端身份仍须查询、取消和对账，不能因当前 Runtime 不健康而跳过恢复，也不能重建新 Attempt 重跑研究。

探测网络 I/O 在事务外执行。网络返回后，只有取得命令幂等回执所有权的一方可通过有界本地存储回调发布 Store 分配的快照 ID 与精确字节；并发重放不调用回调、不留下额外快照。发布后再次检查授权和探测有效期，再原子提交快照元数据、观测与回执。数据库提交结果未知时保留可能已被引用的对象，不以猜测为依据删除。探测只证明集成可用性，不构成科学评估证据。

Runtime bearer 凭据的线缆形状为 32–8192 字节可打印且不含空白的 ASCII；这是最小形状约束而非熵证明。注册和原生传输构造使用同一规则，旧短凭据在发送请求前返回认证不可用，避免短字符串与固定协议字段相撞而被误判为响应泄密。Downstream / Custom Provider 保留各自上游兼容的 1–8192 字节边界；TLS CA 保留 1–65536 字节 ASCII 形状并继续由原生证书解析器验证。响应中的解码后凭据反射检测不得移除。

Rust 单一契约生成请求依赖关系：Runtime Create 的 PINNED_CA 必须附非空 CA 引用且 development_http=false；SYSTEM_CA 只允许省略或 null CA。Runtime Update 的 PINNED_CA 允许省略/null CA 以保留已存引用，转换配置时仍由领域校验与 Store 检查真实旧状态。engine_versions 必须有 1–64 个条目，键和值均为 1–120 字符的非空、非控制文本。JSON Schema / OpenAPI / TypeScript / Ajv 必须由同一 Rust 源生生成，新增回归同时覆盖长度、映射键、TLS 依赖、并发快照以及首次派发与已发出恢复的区别。

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

### A3.1 研究产物提交与本地原生对象

`POST /api/v2/artifacts` 接受严格 `ArtifactCreate={schema_version:1,project_id:Id,kind:CODE|PARAMETERS|REPORT,content:string}`，另带 Idempotency-Key。内容最多 2 MiB UTF-8；JSON 传输最多 12 MiB+16 KiB（覆盖合法转义），每进程最多四个并行产物请求。CODE 是待隔离验证的 Rust 源码，不在 API 进程编译/执行；PARAMETERS/REPORT 必须是 schema_version=1 的 JSON object，保存原始字节，不把报告文字或其中的 PASS 当证据。调用方不能设置 origin、access_class、storage_ref、producer、Run/Attempt、id 或创建时间。

此提交路径一律保存 SYNTHETIC+RESEARCH，kind 决定 media_type 和 schema_name；它只接收研究输入/说明，不提供 REAL、METRICS、DATA_QUALITY、MODEL、SIGNALS、TARGETS 或 PACKAGE 的自助登记后门。真正的市场数据/native evaluator 输出必须由后续受信任运行适配器单独登记，不能把本接口提交结果升格成资格。浏览器需近期 Operator；机器仅 CLI/AUTOMATION/MISSION 且精确项目 ARTIFACT_SUBMIT，普通权限、Downstream、过期/撤销的 Mission 均拒绝。Mission 的 producer_run_id/producer_attempt_id 从服务端当前 Run 和 active Attempt 派生；同一 Run 历史产物字节总和与本次字节合计不得超过 run_admissions 冻结 output_bytes，重试或新 Attempt 不清零。无真实准入账目不能默认为无限额度。Mission 凭据的 `issuer_attempt_id: Id?` 由数据库签发触发器在 project→run→principal 锁序下绑定签发当时的 active Attempt，非 Mission 必须为 null；上传与最终发表都要求它等于当前 Attempt，不能把旧进程的输出记到新 Attempt。018 后续增量迁移不猜测旧凭据的历史 Attempt：旧行保留 null 作为审计记录，但统一鉴权拒绝其所有 Mission scope（包括读取和机器身份自省），需由受信任 Mission 服务按当前 Attempt 重新签发；其他非 Mission 身份的既有权限不因此改变，历史签发内容不能扩大或伪造。

复用现有 command_receipts，保存完整非内容元数据和首次公开响应；不复制算法内容到命令账本，不新增内容 hash。相同 key 的重放还必须与首次原生对象逐字节比较，内容不同（即使长度相同）409；重放不重复写对象、不重复计量。上传授权、Run 输出额度和数据库发表通过已有 PostgreSQL 锁及事务串行化，机器写入按 project→run→principal→credential 顺序取得写锁，避免两次产物上传持共享 Run 锁再互相升级。DB 提交失败或 ACK 丢失不删除可能已经被引用的对象。

目录能力的生产依赖明确固定 cap-std 3.4.6，沿用其已修补的 cap-primitives 3.4.6，不以主机存在 openat2 代替回退路径验收。GHSA-hp8f-xmx4-4qrg 涉及多层软链与尾斜杠的手工路径解析；本次检查时既有 Cargo.lock 的底层 cap-primitives 已为3.4.6，故不宣称当时运行的底层仍有该漏洞。提升 facade 最低版本以避免重新解析依赖时退回旧底层，并对真实默认路径、强制 ENOSYS、强制 EPERM 三种环境运行相同目录边界。仅测试使用成熟 seccompiler 在全新子进程屏蔽 openat2；不写自制 BPF，不更改生产进程/系统配置。所有越界哨兵只位于同一个私有临时目录内的兄弟测试目录，同时验证合法根内软链和真实 ArtifactStore/SecretVault 仍可工作。这些证据不是完整 Agent/OCI/Sealed 隔离验收的替代。来源：https://github.com/bytecodealliance/cap-std/security/advisories/GHSA-hp8f-xmx4-4qrg 。

本地存储复用 cap-std 的目录能力与 OS create_new/hard_link/fsync：服务端生成 UUID 对象键，私有 pending 文件完整写入并 fsync、设只读并再次 fsync 后，原子 create-if-absent 发布，再 fsync 目录。对象键、LOCAL 和固定本地存储版本 1 共同标识一次不可覆盖写入；不是应用内容散列。私有 artifacts 目录拒绝软链/宽权限；对象读拒绝软链、非普通文件、尺寸变化和可写文件，不接受客户端路径。不经 API 授权不能由 UUID 直接读取磁盘。

`GET /api/v2/artifacts?project_id=...`、`GET /api/v2/artifacts/{id}` 返回公开元数据，无原生地址/凭据；稳定 UUID cursor，limit1..100。机器 RESEARCH_READ 只见同项目 RESEARCH；浏览器可见 RESEARCH/OPERATOR/DELIVERY。EVALUATOR_ONLY 对上述路径一律不可见，不能绕过未来的 evidence disclosure/exposure 服务。`GET /api/v2/artifacts/{id}/content` 先走相同权限，再限额读取本地原生对象；下载使用服务端生成文件名的 attachment 和 nosniff/no-store，不内联执行用户 HTML/脚本。成功体为 application/octet-stream 原始字节，OpenAPI 必须使用 native utoipa 的 string/binary 合同而非 Vec<u8> 默认的 JSON 整数数组；生成合同回归同时断言该媒体类型和二进制形状，实际下载仍由原生字节比较回归验证。原生 Catalog/Object Store 产物的内容不会误用宿主文件路径，未接通的后端返回明确不可用。此切片不宣称已实现受信任市场数据登记、原生计算或全部 T01–T42。

生产 serve 显式打开 state-dir/artifacts；旧私有 state-dir 首次升级可创建不存在的空目录，不覆盖已有对象，不调整用户其他目录权限。未提交/中断上传可能留下无 DB 引用的私有对象，保留而不自动删未知提交；回收必须在停止写入的维护窗口核验原生对象与数据库引用，不能通过 git clean 或在线猜测删除。

### A3.2 原生目录与共享资金模拟适配

原生科学任务从 `nautilus-persistence 0.63.0::ParquetDataCatalog` 读取运行时已登记、只读挂载的精确快照，不接收Agent指定路径、SQL、URI、存储选项或凭据。任务的NativeBarSelectionV1仅包含完整bar_type列表、event_start_ns/event_end_ns（左闭右开）、decision_cutoff_ns和maximum_rows；Nanos保留既有bigint字符串。调用方把不可变source/snapshot/storage_version与受限挂载关联。原生目录可能按路径子串发现文件，因此读取结果必须再次严格检查完整BarType/InstrumentId集合、唯一版本、数据行边界和类型，不能以宽匹配授权更多数据。每资产时间严格递增、OHLC精度/价格步长/非负volume、event_at<=available_at<=cutoff；未来创建的instrument定义不得回填到早期数据。

Nautilus的ts_init只有在已登记的原生采集来源证明它代表当时可用时间时才可当available_at；历史批量导入的ingest/创建时间、手工令ts_init=ts_event均不自动获得VERIFIED/PASS。科学加载器的顺序校验只验证数值关系，不替代此来源证明。只读快照、许可和PIT证明由运行时/Store受信任登记验证，源管理员修改宿主本身不在Agent威胁边界内。

NativeSimulationRequestV1用同一账户、NETTING、固定Nautilus0.63.0和明确账户/资本/费率/latency/快照周期，消费各资产真实bar和已冻结的整组target/cash。目标的asof必须不晚于本次决策可见时间，valid_until仍有效；所有资产当前价格必须来自同一已完整到达的时点，缺价/未完成指令/原生拒单均明确失败，不补零。权重转原生数量使用同一原生equity、Instrument数量步长及当前原生net_position；交易按减仓优先，且必须等全部原生减仓成交确认后才提交增仓；仅先发送卖单不能视为原生保证金已释放。跨零目标分成明确reduce-only平旧仓与随后开新仓，空头回补同样属于减仓。两阶段保留同一决策equity与目标数量，部分成交/拒单/过期不提前完成该目标点；量化余量保留，不另建账户账本。目标不是基于未来bar的成交指令；至少1ns的原生插入延迟阻止在生成目标的bar内部获得事后价格。

最终订单、成交、持仓、equity snapshots和收益均直接来自原生BacktestResult/CanonicalBacktestResult。原生统计缺值/非有限值转为带原因的INSUFFICIENT_DATA而不是0/PASS；方法版本、原生统计键、频率、样本数与年化约定明确，未知口径不能登记为支持的方法。原生撮合内部的模拟账户不是QZ真实账户；没有交易网关或真实券商凭据。FIXTURE/SYNTHETIC测试数据永远保留其来源，加载/求解成功不升级为REAL或Qualification。

共享资金结果的 `returns` 与 `statistics[group=RETURNS]` 只使用原生 PortfolioAnalyzer 从该唯一账户的真实 PortfolioSnapshot 计算的 UTC 日频权益收益；显式 `returns_kind=PORTFOLIO_DAILY`、`returns_status` 和 `returns_reason` 标明可用性。不得直接将 BacktestResult.returns_series 的仓位收益回退当组合收益：上游在没有跨日权益样本时会退回 position_returns，而仓位收益与账户资本/杠杆口径不同。适配调用原生 set_portfolio_returns_from_snapshots，不自行重采样、拟合或年化；PnL/General 与完整 canonical_result 仍保留原生结果，canonical 中的统计不自动获得方法资格。没有可用日收益时数组为空、状态INSUFFICIENT_DATA、原因PORTFOLIO_DAILY_RETURNS_UNAVAILABLE，不填0；真实跨日全现金的0收益则为原生有效观测。回归同时覆盖20分钟不足样本、日内平仓有position收益却无portfolio收益、跨两个UTC日及跨日全现金；收益可用仍不代表满足独立评估的最小样本数。来源：Nautilus0.63.0锁定analyzer.rs与 https://nautilustrader.io/docs/latest/concepts/portfolio/#returns-position-vs-portfolio 。

原生模拟的集成与并发验收必须执行真实 `job simulate` 子进程，与一任务一进程的生产合同一致；不得在共享进程池反复创建多个研究内核。2026-09-09回归观察到同进程测试组的全现金权益序列缺失，但精确单独执行通过，故保留原断言并新增四个同时运行的独立job（两组全现金、两组持仓）验证原生账户/权益曲线不互扰，不使用测试串行锁、提高重试次数或补零。该子进程测试只证明进程生命周期与真实JSON入口，文件系统/网络/cgroup隔离仍须独立验收。代码中的公共Rust函数供该单次job入口使用，不允许API/Worker嵌入科学内核。

### A3.3 原生有界预测 ABI

预测按经目录校验的每个instrument单独构造原生EMA和Wasm实例；warmup行明确forecast=null/INDICATOR_WARMUP，不填零。未来收益标签只在该行预测调用完成后计算，记录label_available_ns；没有完整固定horizon的尾部标签为null/LABEL_NOT_COMPLETE，不删除真实观测。标签是受限科学证据，不能因位于同一产物就披露Sealed。任务只有一个total_fuel，实例化及预测实际消费累计扣减，不能每个instrument重新获得总额度。每个独立训练/验证折必须重新调用并使用自己的允许输入，通用预测文件本身不代表已完成分折或独立验证。

第一条可执行研究代码路径使用Rust编译到 `wasm32-unknown-unknown` 的纯计算模块，复用Wasmi2.0.0解释执行；不新增脚本语言或Agent框架。唯一预测入口固定为 `predict(f64,f64,f64,f64,f64,f64,f64,f64)->f64`，依次接收完整已可用观测的 close、previous_close、原生EMA快值、原生EMA慢值、volume、open、high、low。指标由Nautilus原生组件从当前及过去记录计算，模块拿不到未来标签；Score/ExpectedReturn单位仍由冻结Alpha合同和独立校准决定，不把分数直接当收益。

模块必须是合法Wasm二进制，大小不超过2MiB，无任何导入、无start函数，无WASI、宿主文件/环境/时钟/网络函数。Wasmi显式启用stable与portable-dispatch，避免关闭default-features后在未优化构建中依赖宿主尾调用消除；同一生产/测试配置保留原生校验、deterministic、extra-checks、严格编译结构限制和fuel。2026-09-09原生无限循环回归暴露了旧配置的宿主栈溢出，不能通过增加线程栈、降低测试fuel或只测release绕过。采用上游已有portable loop dispatch，不修改解释器；其行为见 https://docs.rs/wasmi/2.0.0/wasmi/#crate-features 。每实例最多一个16MiB线性内存、一个4096项表、有限栈和调用深度；每次预测与整次任务有分开的原生fuel上限。trap、非有限输入/输出、超额或ABI不符直接失败，实例失效，不返回零信号或重新置零预算继续调用。每个instrument/fold/受隔离评估使用独立实例，不能复用一个带历史状态的实例跨验证边界。

编译用户Rust同样是执行不可信输入：由原生运行时的既有进程/文件系统隔离执行固定rustc参数，只读标准工具链、当前代码目录及本次独立输出目录；不挂载Codex home、DB、SecretVault、Docker socket、其他任务或Sealed数据。Wasmi的内存/fuel只保护预测执行，不替代编译、解析和整个job的原生cgroup/CPU/墙钟/输出限制。Wasm MODEL制品只能由绑定Run/Attempt的原生编译结果产生，不能以用户上传的标记自行声称可信执行或REAL数据来源；独立JSON校准MODEL沿用A4.4的原生拟合及正式Validation发布关联。此ABI适配不拥有资格/审批/交付权限。

### A3.4 实验提案、作者绑定与受限结果投影

实验提案使用现有 `experiments`、Operator/机器权限、项目/周期行锁和 `command_receipts`，不增加工作流引擎。`POST /api/v2/experiments` 的 `ExperimentProposalV1` 只接 schema_version、cycle_id、family_id、可空parent_experiment_id、hypothesis、expected_failure_modes、proposal_artifact_id、parameter_artifact_id和可空code_artifact_id；请求最多64KiB。id/project/root/ordinal/trial_source/run/outcome/qualification一律由可信服务确定，客户端不能给PASS或扩大预算。报告/参数/代码引用必须是同项目、RESEARCH访问级别、真实已发布且非空的对应REPORT/PARAMETERS/CODE，不能引用Sealed、Reviewer报告或他项目对象。

近期认证的Operator浏览器，以及精确项目EXPERIMENT_SUBMIT的CLI/MISSION可提交；AUTOMATION/DOWNSTREAM不能借此获得研究作者身份。MISSION还必须属于同Cycle的AGENT_RESEARCH、持当前issuer_attempt绑定且状态DISPATCHING/RUNNING、未过期。复用现有authority取得项目写锁后，锁Cycle；项目行作为跨命令共同串行屏障，所有后续锁等待结束再检查身份和时限。新提案要求项目ACTIVE、周期RUNNING、Brief FROZEN；family必须等于该Brief冻结政策的family且同根血缘，parent只能引用同family的已有实验。原始幂等请求经过当前身份检查后返回首次资源，不能更换字段或命令键复制已用预算。

每Cycle持锁检查全部已登记实验数量不超过冻结max_experiments，分配单调ordinal；失败/无效/已淘汰提案也占此不可删除的试验账本上限。这是提案数量约束，不再次更新run_admission的reserved_experiments/used_experiments；实际科学执行仍在原生Run准入事务预约，避免把一次试验双重计费。实验创建、作者记录与原始命令回执共同提交；任一步失败全部回滚。

`experiment_authorship` 是不可变的精确来源关联，不是新的用户主对象：experiment_id/project_id、actor_kind=OPERATOR|CLI|MISSION、credential_id?、author_run_id?、author_attempt_id?、created_at。OPERATOR不带机器字段；CLI只带精确credential；MISSION必须同时关联credential、同项目/同Cycle的原生Mission Run与issuer Attempt。历史实验没有此记录时保留未知来源，不回填猜测身份或授予跨Run读取权限；实验的run_id继续仅代表该实验科学执行，不混作作者Mission。

`GET /api/v2/experiments` 与 `/{id}` 使用项目RESEARCH_READ或Operator身份、原生游标和1–100分页。返回提案及原生作者Run/Attempt元数据，不返回机器秘密/存储路径。结果投影显式 `result_visibility=PENDING|RESEARCH|RESTRICTED`：未产生结果的PENDING可显示；已知DISCOVERY/VALIDATION运行且输入不含Sealed、结论为同项目RESEARCH报告时才显示实际outcome/公开reason/conclusion_ref。未知来源、Sealed或EVALUATOR_ONLY结果全部字段置null并标RESTRICTED，不伪装PENDING，不因Operator/RESEARCH_READ就自动披露摘要。专门证据披露仍须冻结政策和暴露预约，不能由此GET绕过。此提案入口不启动科学任务、不制造Alpha/资格，也不替代正式冻结/Cycle/Worker服务。

### A3.5 提案到原生编译的事务关联

可信研究Mission可为同项目/同Cycle的正式PENDING提案准备一次原生CompileModel。
`experiment_compilations`仅保存不可变关联：experiment_id主键、project_id/cycle_id、
mission_run_id、compile_run_id唯一、code_artifact_id、parameter_artifact_id、created_at。
它不拥有新的任务状态；执行、重投、预算、取消和结果仍由原Run/Attempt/PGMQ驱动。
关系引用必须指向原提案、同Cycle研究者Mission及DATA_VALIDATE原生任务；没有
experiment_authorship的历史提案不推断作者或自动执行。首次关联冻结提案代码/参数
指针，后续修正使用有父血缘的新提案，不替换已执行输入或抹去失败记录。

编译只带原CODE和可信服务生成的CompileModel参数，不挂载Dataset或Sealed；输入
集合仍引用冻结Discovery上下文供准入重验。编译是原实验首阶段，experiments=1，CPU/内存/
输出/墙钟仍在既有预算事务预约，并且期限不能超过所属Mission。调用者只能是持
有效fence的可信Worker，不增加Agent通用执行接口。产物发表、Run/原生定义、编译
关联及消息同事务提交；相同提案重放返回原Run，不换模型或身份。编译产生的MODEL
仅是后续预测输入，SYNTHETIC编译来源不冒充市场数据或Alpha资格。

### A3.6 编译后的原生 Discovery 预测

Wasm预测提案的PARAMETERS文档采用严格字段`schema_version=1`、
`dataset_revision_id`和`parameters:NativeForecastParametersV1`；后者为schema_version、
fast_period、slow_period、label_horizon_observations、total_fuel。数据版本必须明确选自
本Brief冻结Discovery输入，不默认第一个版本；当前原生适配要求FIXED_BARS且label
horizon等于Brief，其他horizon明确返回能力不支持，不能偷换为固定观察数。
选择的bar types、事件区间、可得时间上限和行数来自既有原生登记元数据适配，不由
Agent提供路径、任意查询或扩大时间范围。参数文件不接受MODEL ID；模型只取该
提案编译Run真实SUCCEEDED终态所采纳的唯一qz.wasm_model及原生产者映射。

预测使用既有EvaluateAlpha任务，只有明确选中的Discovery Dataset、原MODEL和
服务生成参数作为输入。原编译已预约并结算同一试验，预测experiments=0，代码/参数/模型/
Dataset/科学Run的不可变关联与消息同事务提交；同提案重放不重复试验或换生产者。
该科学Run才写入experiments.run_id，编译Run不占用此指针。预测结果仍是Discovery
研究反馈，不等于验证分折、独立Reviewer、Sealed评估或Alpha资格。

此处统一首阶段记账与B1原生编译合同：编译失败/取消仍保留该次试验，后续预测/
独立验证不能再次收费。只有已核对原编译及其一次试验账目的可信阶段准入可使用0；
普通ALPHA_EVALUATE准入仍必须预约正试验数，DATA_VALIDATE管理任务仍为0。
新关联以数据库约束同时核对编译1/预测0；历史账目和已有成功回执不修改、不退款。
未按首阶段记账的历史编译不能新增未收费的预测，须明确报告而不是猜测或补写历史。

### A3.7 Mission驱动科学任务

常驻Worker在本Mission最新原生Turn已确认结算后，按原提案ordinal选择一个已就绪
的编译、预测或正式验证步骤：尚未编译的正式PENDING且有CODE提案进入编译；原编译
SUCCEEDED且未有预测关联的提案进入预测；预测成功后登记原RESEARCH Alpha，再按
A4.7准备正式Validation。已排队/运行/未知的任务不重复创建，失败编译不
替换模型或自动进入预测。每次消费最多准备一个步骤；已有PGMQ消息继续负责恢复，
科学Worker独立执行任务，不让原生Codex会话等待科学进程或接管它的执行循环。
首轮公开请求包含准确Mission、冻结Brief、Cycle和冻结政策的family ID及当前原生
ABI/参数合同，不让Agent猜测必填提案身份；这不授予修改family或政策的权限。

每步以原Mission冻结的JobLimits为资源分配请求，按编译1/预测0/验证0设置experiments；
现有Cycle累计预算、Runtime能力与Mission剩余墙钟仍可拒绝准入。消息重投保留
原Thread、原Turn结算和全部原生Run身份。没有就绪步骤不等于Mission完成；任务
终态、失败说明和后续研究反馈必须由后续结论/同Thread接续处理，不能提前ack。
模型Turn可能超过60秒的Runtime探测有效期。准备科学步骤前复用现有有fence的
Runtime原生探测：当前活跃Researcher Mission亦可刷新其冻结Runtime，不要求
Mission处于NOT_SENT；准备与发表均重验原租约、Runtime版本、状态和期限。实际
HTTP在事务外执行，仍保留原60秒有效期，探测不可用不得假造可执行能力。

### A3.8 同Thread科学反馈

可信Worker只在最新模型Turn有完整结算后，为原Mission关联的已采纳科学终态准备
一次公开反馈。编译或Discovery预测失败/取消进入REPAIR；预测成功直接进入正式
验证，不为中间观察抽样额外消耗模型Turn。Validation须先按A4.8发表完整Evaluation，
再按执行成功RESEARCH、失败/取消REPAIR接续；指标拒绝不伪装成进程失败。
使用现有model_turn_reservations的原Session及唯一command_key
`mission/result/{scientific_run_id}`，不新增反馈队列或状态表。未知科学任务、未结算
模型Turn不触发接续；并发重投不重复预约。反馈文件、预约、PGMQ同事务，仍检查
原fence/Profile/预算/截止时间；下一次消费恢复原Thread，绝不新建Thread洗账。

反馈只含精确实验/任务/可空Attempt/终态/公开原因。正式验证仅从已封口Evaluation
和MetricValue投影：Alpha/Policy/InputSet、执行/证据/decision、原完成报告引用、来源、
concluded_at/valid_until及数据库观测时是否过期；唯一选择指标按冻结政策的code/
scope/method/version/unit/frequency精确选择，保留原值/null、状态、样本数、区间和
生产者引用，缺失明确null。不存在跨试验排名或资格结论。实验元数据可披露这条
精确Validation关联的首次裁决及报告ID，但不授予原报告读取权限；Sealed与未知
评估来源仍受限。反馈不读取任何EVALUATOR_ONLY报告字节、市场行、预测、标签、
校准系数、原生私有历史、凭据或任意诊断正文。失败明确当前无详细
编译器诊断，不能凭空解释错误；修复需新建parent_experiment_id指向原试验的提案。
反馈送达仍不等于Mission完成，结论与终态另行确认。

### A3.9 原生公开回答记录

研究者公开回答只通过锁定Codex的`thread/turns/list {itemsView:"summary"}`读取。
0.144.4原生实现只保留首条userMessage及最后一条agentMessage；QZ忽略userMessage，
不请求full/items列表、不启用推理通知、不读取rollout文件。每页一轮，仍有限页数/
帧大小和原Mission墙钟；只选择原Session中精确native_turn_id。原生列表状态不能
代替先前真实turn/completed终态观察，公开回答也不替代完整usage回执或领域结论。
依据为锁定版本thread_processor.rs的apply_thread_turns_items_view及官方
https://learn.chatgpt.com/docs/app-server 的agentMessage/item生命周期合同。

`model_turn_summaries`按reservation_id保存唯一不可变的REPORT引用及原生item ID；
REPORT为`qz.mission_summary`v1、RESEARCH/SYNTHETIC，内容只有原Turn/item ID、
原生phase（缺失保留null）和最多64KiB公开text。它是可观察回答摘要，不是另一套
聊天数据库或可信科学报告。当前fence、精确成功终态及完整usage结算检查后，在既有输出预算内
发表；相同原文重放返回原产物，不同原文冲突。文件/元数据/关联同事务，失败不能
伪造摘要或阻止已观察真实用量保留。收束仍须最新Turn完整结算与所有科学任务对账，
不能只因存在公开回答就宣布Mission/Cycle成功或授予资格。
Worker停止会同步发送原cgroup.kill，但systemd回收scope是异步的。下次原Mission
启动前用原生systemctl仅查询精确Run scope的LoadState，最多等3秒且不超过剩余
墙钟；未回收则拒绝启动，不停止/替换仍活跃的旧owner，不换scope名绕过资源边界。

### A3.10 有界研究会话收束

Researcher Mission的执行收束复用原Run/Attempt、run_terminal_receipts及PGMQ归档，
不建立另一套完成状态。最新Turn须有原成功终态、完整usage及已发表公开回答；
所有此前Turn也须结算，成功回答不得缺失。尚待编译的可执行提案、本Mission的
未完整提案、未终结科学任务或未在原Thread消费并回答的科学反馈均阻止收束。
所有编译/预测须有精确原Attempt的真实终态；未派发取消保留null Attempt，不能伪造。
编译成功还须完成对应预测；预测成功必须有原Alpha的正式Validation、已封口评估
及原Thread反馈回答。科学任务成功但评估发布失败仍阻止收束与Mission ACK。
没有提出实验的会话可凭公开限制说明结束，但不制造实验或“无有效Alpha”的科学裁决。

在同一project/Cycle/Run锁定事务中，当前fence重验后将原Attempt的结果引用指向
最终qz.mission_summary，终态回执关联原Session/预约/摘要；这不是Runtime的
qz.job_result，也不重复复制回答。沿用原终态CAS和预算结转：先提交的取消意图
不能再被成功覆盖；未知模型/科学任务不因本地连接关闭而变成CANCELLED。
提交后才归档Mission消息，终态后丢失ACK可重放归档。暂停或Profile变更不抹去
已完成的真实执行证据，不为收束重新准入/发送模型。Mission的SUCCEEDED仅表示
有界会话已执行并报告；收束本身不改Cycle、Experiment outcome、Evaluation或Qualification；
正式评估/独立Reviewer及Cycle结论仍由对应可信服务完成。

取消或真实期限到达后，收束不再要求执行尚未准入的编译/预测/验证、发表新反馈或
补一轮公开回答。已经准入的科学Run仍须有精确原Attempt（含未派发null）的终态，
不能由Mission停止推定远端停止；不自动取消其他Run或下游。已有Validation的评估
发布消息仍独立保留，Mission取消不删除它或将未发表评估写成PUBLISHED。

取消的模型部分必须全部对账。仅当原预约没有model_turn_dispatches发送意图时，
可通过既有逐轮结算逻辑记录NOT_SENT/CONFIRMED_NOT_SENT，并只释放该原预约；
同一个Run事务锁覆盖取消、证明和收束，不新增退款账本。任何发送意图、未知原生
Turn或缺失最终用量均保留预约，不能根据空列表、已关闭连接或取消应答补零。
零预约表示从未获准发送模型Turn，取消可不创建Thread/公开回答；这不声称删除原生
Thread或取消QZ未授权的外部会话。已有成功/失败/取消回执均保留原值，公开回答
缺失可在取消结果中如实为null，不能捏造总结；没有结果产物时accepted_at也保持null，
真实收束时间仍由Run终态记录。正常成功收束仍要求全部成功回答。
Worker在任何新科学步骤/结果反馈和新原生连接之前先尝试这个已结算收束边界。

### A3.11 未授资格的研究版本与实验初次裁决

正式评估先引用不可变Alpha版本，不能要求实验先自报SUPPORTED才能取得评估主体。
可信Mission可从原成功Discovery预测建立一次RESEARCH Alpha/首版本；只采用原
experiment_forecasts的MODEL、原提案CODE、原生预测镜像、family根血缘及冻结Brief
的signal kind/horizon。SCORE的单位为UNITLESS_SCORE，EXPECTED_RETURN为
RETURN_PER_HORIZON；未独立校准时calibration_id保持null，不填常数估计器。
版本创建不改PENDING、不授资格、不把编译SYNTHETIC或预测FIXTURE重标为REAL。
重复准备复用现有command_receipts的精确实验身份，不新增版本队列或状态表。

被Alpha版本引用后，提案CODE/PARAMETERS/科学Run仍不可变；尚为PENDING的初次
结果只能在同事务创建精确Alpha版本、冻结Brief政策的正式Evaluation及全部指标时
发表，conclusion引用该评估的原REPORT，outcome由执行/证据/decision映射。这个
评估尚未封口；事务提交会原生封口，因此不能事后补结果或修改已有评估/资格。
已有非PENDING裁决不可改写，已授资格的旧实验也不因初次裁决规则获得修改许可。
这修正了“先建评估主体便把PENDING永久冻结”的顺序冲突，不放宽输入或既有证据
不可变性；数据库关联不代替可信服务的数值、政策、暴露及完整试验账本验证。

### A3.12 Alpha 版本与正式 Validation 的只读操作面

Alpha与完整Validation指标供Operator浏览器，或具有精确项目RESEARCH_READ的CLI
读取；不向Mission/Automation/Downstream开放这组操作面。Mission继续使用B3的
受限工具及冻结政策允许的反馈，不能借此扩大证据披露。全部查询复用现有授权、
原生UUID cursor及limit1..100，不创建第二套业务记录，不触发计算/探测/模型请求。

`GET /alphas?project_id=...`返回Page<AlphaView>，含原id/project/name/lifecycle、
active_version_id及其准确十进制版本、revision/创建和更新时间。登记的lifecycle不是
当前可交付资格。`GET /alphas/{id}/versions`分页列出不可变AlphaVersionView；
`GET /alphas/{id}/versions/{version}`按原Alpha和十进制正版本定位，不以当前版本替代。
版本返回A3的原实验/血缘/CODE/MODEL/信号合同/单位/horizon/calibration/镜像和创建
时间；origin只从原已绑定Discovery任务来源读取，无证据保留null，不从生成CODE的
SYNTHETIC推断市场数据来源，也不将null calibration变成已校准。

`GET /alpha-versions/{id}/evaluations`只列出该版本已正式发表的原WALK_FORWARD
Validation；`GET /evaluations/{id}`返回相同EvaluationView。必须精确匹配原
experiment_validations的Run/Alpha/Policy/输入、原qz.alpha_evaluation生产者和
evaluation_publications；未发表、Sealed或不属于此正式路径的证据不经该入口披露。
返回A4的三层状态、政策/输入/Run/版本及报告引用、真实来源和原concluded_at/
valid_until，附DB检查时间及unexpired_at_read；未过期不等于资格或允许交付。
`GET /evaluations/{id}/metrics`对同一授权和正式生产者检查后返回Page<MetricValueV1>，
使用数据库原metric行UUID分页，完整保留value/null/status/reason/方法版本/单位/
频率/样本数/期间/来源；不计算新分数、不补零、不重新评估。两类报告字节、原生
目录、参数、标签及逐折索引均不读取。原EVALUATOR_ONLY产物GET继续拒绝。

CLI alpha list/versions/show/evaluations及evidence show/metrics使用相同HTTP/DTO；
附加校准的alpha calibration及只读来源详情见A4.4，不挪用源版本的评估或资格。
React/Ant Design页面明确区分请求失败、无可披露Validation、研究登记、科学PASS、
过期和资格。版本/指标翻页不得跨Alpha或评估，切换项目清空旧选择；缺值显示原因，
不用0或“通过”占位。该只读操作面不替代后续独立Reviewer/Sealed/校准/资格与
主动评估准入合同。

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
  sealed_metric_requirements: MetricRequirementV1[]? # null only for historical policies without frozen Sealed criteria
  portfolio_metric_requirements: MetricRequirementV1[]? # null means no portfolio criteria; never inherit Alpha/Sealed thresholds
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

评估产物必须与 `evaluations.project_id/run_id` 精确绑定：`report_artifact_id` 和
`method_versions_artifact_id` 都只能引用同项目、由该 Run 产生的 `REPORT`。
一个原生报告同时包含结果和方法版本时可供两者引用；不能用纯参数、日志、他项目或他
Run 的报告代替。`metric_values.source_artifact_id` 同样必须属于该评估的项目和 Run，
且类型是 `REPORT|METRICS`。这些是不可变来源关联，不构成 PASS；实际产物 schema、
方法能力、独立评估资格和数值仍由可信服务验证。迁移不重标历史来源，遇到不合法旧
关联明确失败，保留原始数据供审计。

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

### A4.3 不可变研究输入与评估政策登记

Create/View.maximum_missing_fraction 继续采用精确 decimal-string；生成合同以原生 allOf
组合 DecimalValue 精度/长度格式与 [0,1] 数值字符串约束，保留前导零、正号和负零等原有合法
表示，运行时仍由 BigDecimal/is_fraction 判定。required_capabilities 为 0–64 个不重复的
1–120 字符字符串，Create/View 生成合同相同；Rust 和 JavaScript 使用同一组边界语料验证。

#### 2026-09-06 独立审查修订：用途、关系列和诊断

数据许可的用途不是仅存在于元数据。登记时在已锁定grant上读取封闭
`DataUse=RESEARCH|RESEARCH_AND_PAPER|RESEARCH_PAPER_LIVE`：DISCOVERY/VALIDATION/SEALED
接受三种许可，PORTFOLIO/FORWARD至少要求RESEARCH_AND_PAPER。最高许可包含前两级，
但此登记不授权Live，实际Live审批/Claim仍须重查RESEARCH_PAPER_LIVE。
额外Sealed选择样本按研究用途核对；不足在对应dataset字段返回
`DATA_USE_PURPOSE_NOT_AUTHORIZED`。期限/撤销在锁等待后复查；准确旧回执只读，不是新消费。

Policy/Family在关系层一一对应：policy的`family_id`和`root_lineage_id`是从已冻结
selection_rule提取的PostgreSQL STORED generated UUIDv7 NOT NULL列，不允许另一份
独立客户端赋值。Policy的`(family_id,project_id,id,root_lineage_id)`与Family的
`(id,project_id,selection_policy_id,root_lineage_id)`双向复合外键均DEFERRABLE
INITIALLY DEFERRED；两侧原生复合唯一约束支持精确匹配。同一事务可以按任一顺序
建立完整配对，孤立/错误项目/错误血缘/额外Family在提交时拒绝。迁移015先锁两表，
由原生生成列和外键校验全部历史；坏历史使整批升级回滚，绝不修改旧selection或洗新UUID。

MetricRequirementV1的metric_code/scope为1..120字符，method_allowlist为1..64项且
每项1..120字符、无重复；原生生成Schema同时约束数组和items。冻结Selection及PolicyView
维持相同可表达边界。阈值比较继续使用精确Decimal函数，字段级错误code保持
EXACT_THRESHOLD_BOUNDS：GT/GE缺low或意外high分别定位对应字段；LT/LE缺high或意外low
分别定位对应字段；BETWEEN缺端点定位该端点、逆序同时返回两个端点。诊断不回显输入值。


本节是研究准备命令，不是实际评估、算法支持或资格判定。Policy 可以登记尚待原生
能力确认的意图；**Brief 冻结和任务准入仍必须核对实际原生方法/版本/单位/频率、
固定 horizon、数据许可/PIT、预算与暴露**，不能把登记成功视作 SUPPORTED 或 PASS。
未知方法不自动替换，已有政策不改写，已消耗的研究机会不因创建政策或 family 重置。

`POST /api/v2/input-sets` 接受严格 `InputSetCreate`：`schema_version=1`、
`project_id:Id`、`purpose:DISCOVERY|VALIDATION|SEALED|PORTFOLIO|FORWARD`、
`decision_cutoff:Time`、`items:InputItemV1[1..256]`。`InputItemV1` 是带 `kind`
判别字段的封闭联合：`DATASET(dataset_revision_id:Id,role:DISCOVERY|VALIDATION|SEALED|FORWARD)`
或 `ARTIFACT(artifact_id:Id,role:CODE|PARAMETERS|SIGNALS|TARGETS|MODEL|REPORT|METRICS|DATA_QUALITY)`。
不得同时给两个引用、传入 id/ordinal、宿主路径或 URL，未知字段拒绝；重复原生本地
引用拒绝，不自动去重改变输入。服务端按请求数组顺序分配0起连续 ordinal。整个
头、成员及 frozen_at 和公开原始命令回执同事务提交；没有公开的草稿或追加接口。

非归档项目才能登记。DATASET 的 role 必须等于 immutable partition_role，purpose
必须匹配该 role；PORTFOLIO 只允许 DISCOVERY/VALIDATION。SEALED 原始数据不能进入
其他目的的输入。托管STUDY_PORTFOLIO的portfolio-study/6同样仅接受原登记
DISCOVERY或VALIDATION目录，明确拒绝FORWARD和SEALED；原目录partition与任务role
仍须相同，不重标目录来满足用途。Build及候选HOLD保持其独立FORWARD限制。
这是原生输入绑定，不替代正式PORTFOLIO准入的许可、模型可用时间和政策检查。
Store原目录读取器以InputPurpose检查调用方明确允许的用途，再按同一领域规则核对
成员DataPartition；二者不是同一个枚举。PORTFOLIO头不改写其DISCOVERY/VALIDATION
成员分区，其他调用方不因共用读取器自动接受PORTFOLIO。仍重读原元数据并核对
版本、来源、事件/可用范围、行数及原分区，复用原许可重验，不增加另一个数据入口。
登记时 data source/runtime 仍启用，数据许可在数据库当前时间生效
且未被已生效撤销；dataset.available_through 不晚于 decision_cutoff。cutoff 不能是
未来时间，非零亚微秒部分拒绝，不能先截断改变point-in-time边界。INVALID PIT 拒绝；UNVERIFIED/PIT 和非 REAL 来源可作为明确标记的研究准备
输入，但不授予投产/资格，`require_real_data` 或验证政策在后续准入强制落实。
ARTIFACT 必须属于同项目，role 等于实际 kind；EVALUATOR_ONLY 只可进入 SEALED。
Secret、LOG、PACKAGE、MIGRATION 等不是可由该命令伪装的数据输入。

`InputSetView` 返回 `header:InputSetSummary` 与有序 `items:InputItemView[]`。
header 含 id/project_id/purpose/decision_cutoff/frozen_at/revision/created_at；item 含
id/ordinal/item/origin/pit_status（artifact 时null）。没有 storage_object_ref、原生路径或
原始字节。`GET /api/v2/input-sets?project_id=...&cursor=...&limit=1..100` 返回有界
`Page<InputSetSummary>`；`GET /api/v2/input-sets/{id}` 返回详情。只能读已冻结输入，
未知/跨项目均404；机器必须 RESEARCH_READ 和精确 project，能看 SEALED 引用元数据
并不等于可读取其字节。全部读取使用同一事务进行当前授权复核。

`POST /api/v2/evaluation-policies` 接受严格 `EvaluationPolicyCreate`：
`schema_version/project_id`、`question:text[1..8000]`、`comparison_input_set_id:Id`、
`execution_assumptions_id:Id`、`selection:SelectionParametersV1`、`split_policy:SplitPolicyV1`、
`metric_requirements:MetricRequirementV1[1..64]`、`minimum_observations:u32[1..2147483647]`、
`maximum_missing_fraction:Decimal[0,1]`、`require_real_data:bool`、
`required_capabilities:text[1..120][0..64]`（非空字符串、不重复）、
`maximum_sealed_uses_per_lineage:u32[1..2147483647]`、`validity_seconds:DbCounter>0`。
validity_seconds 必须可由原生时间库从数据库当前时间表示为有限未来时刻。

新政策同时明确 `sealed_metric_requirements:MetricRequirementV1[1..64]`，至少一项
required，复用相同精确阈值、方法白名单和重复检查。`metric_requirements` 用于
Validation；`sealed_metric_requirements` 用于实际 Sealed，不能将分折阈值隐式改名、
丢弃或套用到整段封存样本。selection 按 evaluation_kind 在对应数组中绑定 required
指标。原生 Sealed 每资产的完整可评估区间使用 `asset:N` scope，Validation 保持
`asset:N/fold:F`；Sealed 不构造训练折、不重新拟合。两组要求均须在开始实验之前
冻结，后续资格只使用实际 Sealed 评估的精确版本和该组要求。历史政策保留原值，
读取时未定义的 Sealed 要求为 null；不得自动复制 Validation 要求、补写历史政策或
允许其进入 Sealed，须另行创建完整政策及新的研究周期。

SelectionParameters 只包含 A4.1 的 evaluation_kind、metric_code、metric_scope、method_id、
method_version、unit、frequency、direction、candidate_count；各文本1..120且无控制字符，
candidate_count是1..65535。服务端在项目行锁下分配policy.version和同项目root的新
experiment_family，question保存于family，形成完整 SelectionRuleV1。comparable_scope固定
FAMILY_LINEAGE，tie_break固定EXPERIMENT_ID_ASC，missing_required_metric固定INCONCLUSIVE；
root/family不得由请求指定。政策、family、精确选择引用与原始命令回执同事务提交。
comparison input须同项目且FROZEN；SEALED选择必须指向SEALED purpose，WALK_FORWARD
必须指向VALIDATION purpose。execution assumptions的费用/流动性产物只能属于该项目
或部署级共享（project_id=null）配置，不能借用另一项目的产物。

SplitPolicy 各字段具有schema_version=1：train_size/test_size为正DbCounter，
step_size为可空DbCounter，purge_observations/embargo_observations为DbCounter；
group_count/test_group_count为可空u16，label_horizon_observations为可空正DbCounter；
interval_validation_required必须true，sealed_revision_id为真实SEALED分区且仍有当前许可的Id。
WALK_FORWARD要求正step_size，group_count/test_group_count均null；可空label_horizon，
有值时必须正数。CPCV_FIXED_HORIZON要求step_size=null、group_count>=2、
1<=test_group_count<group_count、正label_horizon。观察数相加必须不溢出bigint；
这些校验不自行执行切分或证明区间无泄漏。sealed_revision不可出现在WALK_FORWARD
comparison input；SEALED选择则comparison input必须包含这一精确sealed_revision，不能
以另一个SEALED引用代替。任何超出原生能力的split在实际准入仍失败。

浏览器“组合”的“评估政策”按所选项目分页读取、查看原完整政策并创建新不可变版本。
结构化表单分别填写selection、split、Validation/Sealed指标要求、可选独立组合要求及
样本/缺失/许可能力/有效期限制，不根据数据生成阈值或方法。组合要求未启用时提交null，
不继承另外两组。Decimal与DbCounter保留原字符串；空可选端点提交null，比较器与精确
端点关系仍由既有领域校验。保存复用近期浏览器认证和完整原请求的幂等键；结果未知时
保留输入并原键重试，关闭编辑器不宣称撤销。页面不编辑原政策、不读取Sealed字节、
不启动实验或授予资格，原生方法是否可运行仍须实际准入核查。

required metric至少一项；(metric_code,scope)不重复；code/scope/method_allowlist元素
1..120，allowlist非空不重复且最多64项。比较器和Decimal端点复用A4.2精确规则。
selection须对应required项，其method_id在该项allowlist中。没有根据候选数据自动
选择政策/提高阈值，未登记原生方法不会被标为已支持。

`EvaluationPolicyView` 含 id/project_id/version/created_at、question、完整selection_rule、
split_policy和其余上述政策字段。GET列表要求project_id，稳定UUID倒序cursor和1..100
分页；详情按同项目授权，没有PUT/DELETE或原地更改。遇到无法解析的历史不完整合同
返回明确服务完整性失败，不静默将旧证据转换成新政策或从列表消失。

上述两种写命令复用现有OperatorCommand与原始响应幂等回执，新增封闭操作
INPUT_SET_CREATE/EVALUATION_POLICY_CREATE。浏览器仍需近期真实认证；人工CLI只经绑定
完整非秘密请求的一次性TOTP grant。RESEARCH_READ机器、Mission与Automation不能创建。
规范请求不含密码/secret，失败无头记录、成员、family或回执；准确重试返回原始成功
响应，不重新验证当前数据许可以改写历史成功，也不执行第二次登记。后续新消费必须
再次核对当前许可。数据库错误不向客户端暴露；已知字段拒绝返回422及字段路径/原因码。

锁序复用Operator auth→project；随后按稳定UUID顺序锁source、runtime和grant。
source的runtime/native_catalog/provider绑定本来不可变；source/runtime采用FOR SHARE，
授权grant采用FOR SHARE。data_use_revocations的插入取得同grant FOR UPDATE，确保
撤销先取得锁时后续消费重新核对并拒绝；消费先取得锁时在撤销前线性化。未来新任务
准入必须再次核对而非沿用本次成功。输入成员的数据库原生封口触发器保持不变。通用revision触发器允许零个额外绑定参数（Runtime/Downstream）：将原生NULL TG_ARGV视为空数组，仍强制id/created_at不变、revision递增及溢出拒绝，不拒绝合法停用。

HTTP请求体上限64KiB用于这两个研究准备POST及承载相同完整请求的operator-command-grants；其他接口仍使用既有更小上限；不因
有界数组而取消字节上限。metadata分页不读取原始市场字节。原生回归覆盖事务回滚、
幂等并发、项目/Sealed隔离、许可撤销与停用锁等待、字段/时间边界和真实HTTP认证；
生成OpenAPI从同一Rust DTO和实际处理器导出，不手抄平行schema。

### A4.4 固定期限原生分折与估计适配

固定 `solow-cv 0.7.3` 的原生索引输出。WALK_FORWARD 的 train_size/test_size 是固定窗口大小，step_size 是相邻测试窗口起点距离；每个截止前缀用原生 TimeSeriesSplit(test_size,gap=purge+embargo,max_train_size=train_size)产生最后一折，不从其余折拼接训练集。入口要求至少三个训练样本，train_size>test_size+gap（锁定上游双折 API 的实际前提），不满足给明确能力/样本错误，不填充假样本。起点按冻结step推进；结果只含已完成标签的观测。CPCV_FIXED_HORIZON使用原生CombinatorialPurgedKFold的全部测试块组合，train_size/test_size分别是每折最低有效训练/测试观测数；group_count限制2..16且组合折数最多256，不能因预算不足只留下赢家折。所有分折均在分配前检查累计索引上限800万、观测上限100万，固定label_horizon已验证且purge_observations不小于它；未支持的VARIABLE_INTERVAL仍明确拒绝。

独立结果检查要求每折训练/测试索引有序、无重复、在范围内且不相交。WALK_FORWARD训练标签严格早于测试；CPCV允许非相邻训练块但剔除每个测试块两边purge以及后侧embargo，并不等于PIT已经成立。训练/校准只能使用各折训练索引；重叠测试窗口的样本不能重复计为独立观测。公开元数据只能披露政策允许的折统计；sealed索引、预测与标签同样属于受限证据。

样本协方差复用ndarray-stats0.7.0(ddof=1)，不隐式年化、不丢失/补零；SCORE校准复用linregress0.5.4的固定一元含截距OLS。拟合输入由可信调用方按折及数据许可提供，不能把该折测试标签或封存标签混入。模型与数据来源单独冻结，预测应用同一原生模型；缺失、常数、非有限、秩不足或样本不足不产生伪校准。原生返回的系数/协方差再做有限性与维度检查，参数不是手工给定的scale冒充拟合。上述适配不产生Qualification，也不替代完整独立评估发布、许可、血缘和新鲜度检查。

#### 冻结最终 SCORE 校准

每折原生报告记录训练样本最后一个完整标签的实际 `training_end_available_ns`；
它来自原目录的 available 时间，不从 event、测试起点或执行完成时间猜测。
WALK_FORWARD 严格早于首次测试预测；全部折不超过原请求 cutoff，并核对报告中
同一原观测的时间。当前固定最终规则为每资产最后一个原生折（LAST_NATIVE_FOLD），
不是按指标挑最好折，也不额外用测试/Sealed标签拟合。所有资产的最后折都须有
OK 的原生 linregress 0.5.4 含截距 OLS；失败不回退较早折，EXPECTED_RETURN 不伪造校准。

可信 Store 在原 Validation 的 SUCCEEDED + VALID Evaluation 发布事务内同步保存
`qz.alpha_calibration/1` MODEL 及不可变 calibration，关联原报告、Evaluation 和原
Validation InputSet；模型明确保存原资产/bar_type、折、训练 ordinal 子集、真实训练
截止时间和原生系数。train_input_set_id 标识原授权输入全集，精确训练子集以模型中
的 ordinal 为准，不能声称使用了整个 Validation 区间。DB 时间向上取整到微秒，模型
保留精确纳秒；整个模型的可用时间为各资产训练标签截止时间最大值。
文件或事务失败不 ACK、不重做拟合；精确重放返回原模型，历史无模型不补写/升级。
这是冻结可复用的拟合结果，不是候选选择、Qualification 或 Reviewer 批准；科学 REJECT
仍为 REJECT。原 AlphaVersion 和试验身份不改变；附加校准在同一发布事务创建原
Alpha 的下一不可变版本，分配新 id/version/calibration_id 并记录本次真实创建时间，保留原实验、血缘、CODE、
Wasm MODEL、horizon、信号合同、单位及镜像。原信号仍为 SCORE/UNITLESS_SCORE，
使用时必须实际应用校准，不能只改名为收益。一个校准只关联一个原 Alpha 的派生
版本；相同发布重放返回原记录。只有 Alpha 仍是 RESEARCH 且活动指针仍指向该源
版本时才推进指针，不覆盖人工改动、恢复 SUSPENDED/RETIRED 或产生资格。
ExpectedReturn、不可用拟合或未达 VALID 不创建伪派生版本。历史已封口记录不自动
补写；原试验选择仍绑定源版本及其原 Validation，不能借新版本 UUID 重置试验账本。

`GET /api/v2/alpha-versions/{id}/calibration`、人工 `alpha calibration <version-id>`
及 Alpha 版本详情只读附加校准的元数据、原输入和源版本的正式 Validation。
沿用 Operator/精确项目 RESEARCH_READ CLI 权限，不对 Mission/Automation/Downstream
开放。未附加或无法证实原生来源返回404；不下载模型字节、系数、训练索引或标签。
fit_end_available_at 明示为向上取整的微秒时间，原纳秒保留在受限模型中。
源 Validation 的 subject/version、决定和有效期保持原值；新版本的评估列表不借用
源版本评估，后续 Sealed/Qualification 必须实际评估并绑定该新版本。

锁定 linregress 没有模型反序列化/从系数重建 API；持久模型仅保存原生拟合系数，
使用已锁定 ndarray 的逐元素乘加应用同一固定仿射模型，须与原 RegressionModel.predict
实际比对，不重新估计或另写回归器。仅接受相同资产/bar规格和晚于整个模型训练截止
时间的预测；此薄应用器不授予数据读取能力，也不替代 Sealed 预约、许可或实际执行。

### A4.5 独立原生Alpha分折执行

#### 原生封存计算

`NativeAlphaSealedRequestV1`含原生forecast请求、target_kind和
`research_available_through_ns`。后者由可信准入绑定此前研究可见数据的实际可用时间
上界，不使用执行墙钟或只用较早的校准训练截止冒充全部研究信息截止。每个实际
预测必须严格晚于此上界。SCORE必须使用原冻结校准，相同完整资产顺序/bar规格/
horizon，且校准训练截止不晚于该研究上界；EXPECTED_RETURN不接受额外校准。

计算复用原forecast目录/因果EMA/每资产新Wasm及整任务fuel，再应用已冻结OLS，
不调用fit、不创建假训练折。结果保留原NativeForecastResultV1和逐行对齐的
expected_returns（预热为null、尾部无标签的预测仍保留），不能覆盖原始SCORE。
每资产以完整标签配对分别计算原始信号Pearson IC和校准收益RMSE，复用原
ndarray-stats0.7.0方法；无完整标签为INSUFFICIENT_DATA/NO_COMPLETE_LABELS，
常数相关性沿用CORRELATION_VARIATION_REQUIRED，不补0。方法/校准来源和真实
训练截止保留；不存在校准时相应来源字段为null。该原生数值入口不授予Sealed
读取能力，不替代先提交的暴露预约、实际来源绑定、正式Evaluation或资格。

受管操作EVALUATE_SEALED_ALPHA使用ALPHA_EVALUATE，固定原dataset_revision_id、
Wasm model_artifact_id、可空calibration_artifact_id与上述请求；只接受一个SEALED
目录、原Wasm MODEL、原冻结校准MODEL和PARAMETERS。SCORE必须引用校准且不同于
Wasm/参数身份；EXPECTED_RETURN不得附加校准。PARAMETERS不复制系数配置。
Job从原挂载对象读取校准，可信Store采纳从原JobSpec绑定的同一MODEL重新读取、
解析并核对结果；通用输出结构检查不能代替该原始输入关联。校准JSON最多8MiB。
输出qz.alpha_sealed.v1为受限REPORT；Runtime原生镜像能力必须实际包含alpha-sealed/1。
该操作本身不提供公开研究者准入，Sealed读取机会仍须由可信准入在授予能力前预约。

NativeAlphaValidationRequestV1绑定原NativeForecastRequestV1、冻结SplitPolicyV1和
TargetKind，不接受手填预测、标签、系数或折索引。仅固定bars，预测label horizon
须精确等于split horizon。本入口仅用于验证分区CV；SEALED评估须另用已冻结训练/
校准模型，不能在封存分区内拟合。每资产从同一已授权目录按原event/available cutoff加载；
原EMA特征代码复用，始终只计算当前及过去价格。分折只覆盖预热完成且label已完成
的连续原观测，索引保留目录内ordinal，不把不同资产或缺失行拼成一个时间轴。

每资产、每折、训练和测试各自使用新Wasmi实例；CPCV不连续块也重新实例化，
不会携带其他块、训练或Discovery的模型状态。所有实例共享整项任务的剩余fuel。
训练和测试的原预测分别产生后才读取相应labels；WalkForward再核对最后训练
label的实际available时间严格早于首次测试预测。SCORE只用该折训练分数/标签
运行原linregress含截距OLS，然后应用于测试分数；EXPECTED_RETURN保留模型原值，
不伪造校准。常数训练分数或拟合失败保留明确状态及null系数/收益预测。

每折保留训练ordinal、测试原预测/独立label/校准收益及全部指标。Pearson IC直接
调用ndarray-stats0.7.0 pearson_correlation，单位CORRELATION；收益RMSE调用
root_mean_sq_err，单位RETURN_PER_HORIZON；方法ID分别为
ndarray-stats.pearson_correlation和ndarray-stats.root_mean_sq_err。不年化，周期由
原bar_type及horizon确定。常数、不足两项的相关性、不可用校准或非有限原生结果
保留status/reason/null，不作为0或PASS。折指标不平均成全局指标；重叠测试行的
unique_test_observations按资产/ordinal去重，它不是独立同分布样本数或PBO。
全部折必须执行，整任务至多256折、累计800万训练/测试索引，不能预算不足时仅
保留赢家折。结果含真实方法版本，仍是受限数值输入；上层正式Evaluation发布、
政策判定、暴露/资格与Reviewer不能由本地计算入口自行授予。

原生分折的有界薄适配统一位于domain::execution::validation；Job执行与受信任
结果采纳复用相同锁定solow-cv接口及限制。控制面不另写CV/组合枚举算法；采纳时
用原冻结政策、原报告的逐资产源行数及已完成label范围核对全部原生train/test
索引，不能只验证数组可解析就接受漏折或被替换的训练集。这是结果合同检查，
不在控制面执行模型、重新拟合或授予资格。

受管操作VALIDATE_ALPHA使用ALPHA_EVALUATE Run，参数只有schema_version、原
dataset_revision_id、model_artifact_id及NativeAlphaValidationRequestV1。输入仅原
VALIDATION分区、MODEL和PARAMETERS；不能把Discovery预测结果或Sealed训练
伪装成独立评估。成功输出唯一qz.alpha_validation.v1 REPORT，沿用原生封口索引/
输出限额/Run与Attempt生产者，不另建队列。每折source_row_count记录该资产源
目录行数；与原warmup/horizon一起重建原生分折并逐索引比较。所有资产必须与原
selection同序且完整，源行数累计不超过原上限；逐折原生指标有限性与状态/单位
关联仍需校验。这些关联不授予Evaluation/Qualification或数据来源真实性。

### A4.6 原生分折到评估指标记录

Sealed结果先与原请求和原冻结校准完整核对，再转换为同一MetricValueV1；scope为
原资产顺序的`asset:N`，方法/单位/频率沿用下述原生映射，不附加假fold或平均。
完整标签数量同时作为原始信号IC与校准收益RMSE的配对数；缺标签的尾部不计数。
有配对时期间为首个完整配对预测event至末个label_available；没有完整标签时
期间仅表示实际检查的源行首event至末available，observation_count=0且指标
INSUFFICIENT_DATA，不能声称该期间已观测收益。时间仍分别向下/向上取整到微秒。
Sealed原生政策检查仅接受实际资产的规范asset:N、已实现方法和原bar/horizon，
required缺失或方法不支持明确拒绝；不把Validation分折要求挪用为Sealed要求。

可信发布适配先以原请求验证完整qz.alpha_validation报告，再逐项转换为既有
MetricValueV1；不重新计算指标或自行授予PASS。scope固定为`asset:{a}/fold:{f}`，
a是原selection中的0起资产序号，f为该资产原生0起fold_index。报告完整保留序号与
instrument_id/bar_type的关联；不能按指标值重排、丢折或生成total平均值。

metric_code为PEARSON_IC/RETURN_RMSE，方法ID与单位沿用A4.5，method_version
为锁定ndarray-stats的0.7.0。frequency为原生bar_type去掉instrument前缀后的完整
四段规格加`;horizon={固定bars数}`，例如`1-MINUTE-LAST-EXTERNAL;horizon=5`。
不同bar规格或horizon不是相同口径，annualization_factor保持null。value/status/
reason沿用原生结果；IC的observation_count是该折完整预测/标签配对数，RMSE为
该折非空校准收益/标签配对数，不能把缺校准计为有效收益预测。

period_start取首次测试event，period_end取最后测试label_available；为适配既有
微秒Time边界，前者向下、后者向上取整到微秒，形成覆盖所有原始观测的区间，原
纳秒值仍完整保留在REPORT。这只转换记录精度，不改cutoff/数据可见性或数值。
同一可信转换同时提供这些方法/版本/单位/频率的能力记录供既有evaluate_metrics
核对冻结allowlist和精确Decimal阈值；请求方不能上传能力记录来批准自身指标。
该适配不代表数据库Evaluation、试验选择、sealed消费或Qualification已发表。

Brief冻结与Cycle准入使用同一原生能力检查：读取已登记且获授权的Validation目录
元数据，不读取市场行或Sealed内容。当前VALIDATE_ALPHA合同精确绑定一个目录版本
（可含多资产）；多版本输入明确报能力不支持，不能只选首个版本或拼接不同时钟。
Selection的资产/折scope、方法/版本/单位及bar规格+horizon必须匹配该原目录，全部
required指标也须有可执行方法和合法scope。scope不代表样本已足够；实际全部分折、
warmup后样本及缺值仍在Job与原请求采纳时核验，不将登记行数冒充实际执行证据。
分折参数使用同一原生适配边界检查；旧意图政策仍可登记和审计，但不因此获得冻结/
执行资格。当前受管Validation只支持固定bars与WALK_FORWARD选择类别（包括原生
CPCV分折），不把Sealed选择意图悄悄降为普通验证。
同一冻结检查还要求政策显式包含Sealed要求，并以登记Sealed元数据核对单目录版本、
资产/bar原顺序与Validation一致及规范asset:N方法。缺少要求报SEALED_POLICY_NOT_DEFINED，
不补历史阈值、不读取市场行、不预约或启动封存任务。元数据适配由调用方明确指定分区；
普通DATA_VALIDATE仍只接受Discovery/Validation，不能借此增加Sealed访问。

### A4.7 原试验的正式Validation任务

可信Mission服务从同一试验已采纳的编译/Discovery预测及唯一RESEARCH Alpha版本
准备VALIDATE_ALPHA；Agent不能提交Evaluation、切换MODEL或选择另一政策。沿用
原提案的特征参数/固定horizon，输入改为冻结Validation目录，split/target来自原
Brief和Policy。experiment_validations只记录不可变experiment/alpha_version/run/
policy/dataset关联，不另建任务队列；Run仍由原PGMQ/Attempt/Runtime执行。该阶段
要求原编译已计一次试验，本阶段计零，但继续预约CPU、墙钟、内存和输出额度。
并发重放只返回同一Run，失败不清除原trial，experiments.run_id仍指原Discovery。
编译及两个Alpha阶段先以原Mission剩余墙钟约束本次分配，再推导实际所需CPU数；
超过Runtime容量明确拒绝，不能用缩短前的墙钟生成无法执行的JobSpec。

所有Alpha科学阶段选择的实际镜像须等于冻结ExecutionAssumptions；正式Validation
还须等于原Alpha版本镜像。Runtime连接修订未变不代表原生镜像未变，不能借新探测
把旧政策换到新引擎。Validation参数/原始分折REPORT为EVALUATOR_ONLY，不通过普通
研究产物GET披露；后续可信Evaluation发布和受控反馈独立处理，排队不授资格。

### A4.8 正式Validation评估的原子发布

原生Worker在原Run终态采纳后、PGMQ ACK前完成正式Validation评估。终态重放走同一
收尾入口：读取原experiment_validations、冻结Policy/InputSet、原Attempt的已采纳
报告及参数，不重跑模型、不依赖Mission仍在线，不建立第二队列。Evaluation头、
全部原生逐折MetricValue、评估报告及实验首次结论在一个事务封口；文件或事务失败
保留原消息供重试，同一正式Run只可发表一次。无对应正式Validation关联的Run不
进入此发布器。资格、Sealed和Reviewer仍是独立后续步骤。

成功进程先按A4.5–A4.6复核原完整报告，再使用原政策required指标和精确Decimal
阈值。minimum_observations检查实际去重测试观测数，不使用源行数代替；按资产
首折去重的source_row_count核对同一登记目录的row_count。maximum_missing_fraction
约束登记行在实际载入中缺失的比例，以整数/Decimal交叉比较，不能用浮点四舍五入
放宽阈值；它不声称重建交易日历中从未被源登记的行。超出登记数量为INVALID，
缺失超限或有效样本不足为INCOMPLETE。require_real_data同时要求原登记REAL、
VERIFIED、AS_KNOWN_THEN；不满足不授PASS，不改写数据来源。缺方法/指标、取消、
失败、无可用原生报告均INCONCLUSIVE，不生成零指标。有效期从原生完成时间起算，
发布时已到期亦INCONCLUSIVE；重放不刷新期限。

可信qz.alpha_evaluation.v1 REPORT记录精确evaluation/experiment/alpha/run/input/
policy、执行/证据/决策、静态原因、原生报告/manifest引用、实际已核验方法版本及
观测计数；失败没有实际方法记录时明确null。该报告不含市场行、预测、标签、校准
系数、宿主路径或任意上游诊断，与原始Validation报告一样为EVALUATOR_ONLY。
它同时作为本Evaluation的report和method_versions引用；MetricValue仍指向原始
逐折REPORT。原试验结论依既有规则映射SUPPORTED/REJECTED/INVALID/INCONCLUSIVE，
不可改原输入、Discovery Run、试验计数或旧结论。普通研究反馈仅可另行投影明确
允许的元数据，不因本次内部发布自动披露受限报告。该控制面完成报告独立限64KiB，
不占用或扩张原生科学payload的output_bytes；与固定manifest封口开销一样单独有界。

### A4.9 原 Mission 收尾的冻结试验选择

可信服务在RESEARCHER Mission的原PGMQ消息确认前，按A4.1冻结本Cycle的一次选择。
复用acknowledge_run的project→cycle→run锁、原终态回执和同一事务，不开第二队列、
新Run或模型轮次；原Mission终态先前已经持久化，快照失败只保留待确认消息，不撤销
真实完成状态。已准入的本Cycle科学任务须有精确原Attempt终态；正式Validation须已
完整发表，取消也不能因缺少这一独立发表步骤而冻结成“没有评估”。无模型轮次或
无提案的取消可产生空快照，但不补造研究或候选。原队列重放只返回原快照。

`cycle_selections`以原cycle_id为身份，绑定project、原research_run、冻结policy及
数据库形成时间；它同时封口本次成员集合。`cycle_selection_trials`在同事务先插入，
通过deferred FK引用最后写入的快照头。头存在后禁止追加成员，头/成员均不可更新
或删除。不复制实验、评估或Metric身份，不新增hash、发布队列或通用Workflow。
原Cycle此后不得接收新提案；精确已提交命令重放仍返回原回执。快照不完成Cycle、
不授资格、不给Sealed读取机会，也不代替后续独立Reviewer和资格裁决。

成员的可空review_alpha_version_id冻结后续审阅目标，不替换alpha_version_id所指的
原Validation版本。仅排名入选且原正式Validation为SUCCEEDED/VALID/PASS、形成时
未到期的成员可有目标：EXPECTED_RETURN保留原版本；SCORE使用原Validation真正
附加校准的下一版本，沿不可变校准/原生生产者关系确定，不读取active_version_id。
验证REJECT、缺校准、未入选或历史缺证据保持null，不补造目标。此字段在取消快照中
也仅为审计事实；启动Reviewer仍须原Researcher成功、选择COMPLETE及当前预算/证据
准入，后续到期或撤销必须重验，不能将非空引用当资格或读取Sealed原始数据的许可。

快照包含形成时同项目、同Family/根血缘的全部已登记试验，跨Cycle保留历史；每个
experiment仅一行，保留原Cycle/编译/Discovery/Validation/Alpha/Evaluation引用、
形成时执行状态及排除理由。未执行、失败、取消、无原正式评估、不可比、无效或缺
选择指标均留记录，数值不填0。其他Cycle尚未结束的可比试验明确标记未完成并使
本快照INCONCLUSIVE；后续Cycle可形成新快照，但不得回写或补齐旧集合。
execution_run_id绑定所显示状态的原Run；历史试验即使没有正式流水线关联，也保留
原Run引用和真实状态，不误称未执行。未终态历史Run保持未完成，不能授排名。

可排序记录必须绑定原正式WALK_FORWARD Validation和完整发布标记，来源关联沿用
A3.12；原Brief/Policy的comparison_input_set_id与execution_assumptions_id须匹配
本次冻结选择。每个科学Run的物化输入另含自己的MODEL/PARAMETERS，不能拿不同
物化InputSet UUID误判市场比较集合，也不能仅比较数据角色而忽略原冻结输入。
仅SUCCEEDED、VALID且原required指标完整的评估参与比较；选择指标必须为OK、
finite，并匹配冻结的code/scope/method/version/unit/frequency。VALID的科学REJECT
仍可在同口径比较中排序，但不是PASS或后续Sealed/资格准入。当前未接通的Sealed
选择类别明确拒绝，不降为WALK_FORWARD，不读取受限报告字节来临时重算。

排序使用PostgreSQL原生数值比较与UUID次序：MAXIMIZE降序/MINIMIZE升序；相等
（包括正负零）按原experiment UUID升序，前candidate_count为selected。排名保留
所有可比记录，未选中记录不删。头记录真实trial/eligible/selected/unfinished计数；
候选不足或存在未完成可比试验为INCONCLUSIVE，否则为COMPLETE。这里COMPLETE
只说明冻结比较集合完整且数量满足，不是科学PASS、当前新鲜度或可交付资格。

Operator与精确项目RESEARCH_READ CLI可使用`GET /api/v2/cycles/{id}/selection`
读取原选择头/冻结规则，以及`GET /api/v2/cycles/{id}/selection/trials`分页读取原
成员、排名、排除理由和原选择MetricValue；未形成返回404，不伪造空完成快照。
成员按原experiment UUID升序，原生UUID cursor，limit1..100。Mission/Automation/
Downstream不能用这两个接口扩大证据读取；Reviewer的受控输入另行按角色准备。
CLI为`cycle selection <id>`与`cycle trials <id>`；界面从原Cycle查看，不提供客户端
选择赢家、替换成员、刷新旧排名或手填评估的写入口。

### A4.10 Sealed读取机会的精确预约

`sealed_evaluation_tasks`只记录既有Run的不可变alpha_version_id、policy_id、
validation_evaluation_id和dataset_revision_id，不是新队列或用户可写评估。
源Validation必须是原试验真实受管验证的SUCCEEDED/VALID/PASS且预约时未过期；
SCORE目标必须是引用该原验证校准的精确附加版本，EXPECTED_RETURN不附加校准。
原Alpha、源验证、政策、Run与输入同项目/根血缘，镜像、Wasm和校准MODEL精确匹配。
任务origin继承Discovery、Validation训练和Sealed输入中最受限的来源：
LEGACY_UNKNOWN、FIXTURE、SYNTHETIC、REAL依次优先；真实封存数据不能洗白旧测试训练。
新政策仍可引用既有模型，但必须显式冻结自己的Sealed要求和精确Sealed版本。

`sealed_opportunities(attempt_id PRIMARY KEY, exposure_id UNIQUE)`连接原
run_native_attempts与append-only evidence_exposures。首次native_job事务在冻结
JobSpec后、返回任何读取/上传/执行能力前预约；失败时整个首次spec事务回滚。
同一Attempt重放只使用原预约，取消、崩溃、未知发送或未实际读到行均不退款；
新的Attempt需要新的机会。已存在的Sealed spec没有预约不能补造历史机会。
普通DATA_VALIDATE、未知Sealed操作或仅自报EVALUATOR_ONLY均不能取得这种能力。

持既有project→cycle→Run锁后锁根research_lineages行，所有封存预约/披露写入
遵循同一顺序。maximum_sealed_uses_per_lineage对该根下所有Sealed EVALUATOR/RAW
机会累计，不按当前项目、政策、Alpha或Dataset UUID清零。已有IMPORT/Operator/
Research Agent的raw/sample/metric/plot/summary暴露使相同原生目录快照不再独立；
按登记runtime/native_catalog_ref/native_storage_version/native_snapshot_ref关联，
不是只比较Dataset UUID。根下LEGACY_UNKNOWN暴露无法证明独立，明确拒绝新机会。
Reviewer的独立受控输入属于EVALUATOR，不自动转成后续研究者可见反馈。

预约记录RAW表示已授予读取机会，不声称已成功观察市场行；purpose明确为
NATIVE_SEALED_CAPABILITY_RESERVED，actor_session_ref保留原Attempt引用。
独立性按原预约时已知暴露判断，不能事后改写该机会或把后续披露洗成全新证据。
此记录本身不产生Evaluation、资格或Reviewer批准。

已耗尽根血缘封存机会或已有不相容披露时，尚未授予能力且NOT_SENT的Run由既有
未发送结算器结束为FAILED/SEALED_OPPORTUNITY_UNAVAILABLE，不重试到超时。
同一根锁和机会检查用于预约及拒绝；等待锁后重验租约和期限。已有能力保留原机会，
已发送任务仍须远端对账，不据此假称停止或退款；失败评估仍须发表后才ACK。

### A4.11 原封存结果的正式发表

可信科学Worker在原消息ACK前发表SEALED评估；与WALK_FORWARD共用原终态回执、
原生对象读取和不可变Evaluation/Metric封口机制，不新建队列或重新执行模型。
成功只接受当前原Attempt已采纳的manifest、qz.alpha_sealed报告、原PARAMETERS和
精确校准MODEL，并要求该Attempt已有A4.10机会。独立性使用该原预约，不按后来的
披露重写历史；不能补造机会或借源Validation的PASS代替本次封存指标。

指标由A4.5原生输出按asset:N转换，与本次policy.sealed_metric_requirements裁决。
原始行数、最少完整标签数、缺失比例、来源/PIT/AS_KNOWN_THEN和完成时间有效期
共同限制证据状态；缺失或过期为INCONCLUSIVE，不填0、不改原生值、不授资格。
失败/取消保留真实执行状态和INCOMPLETE；尚未授能力便取消时机会可以为空，
已有机会永不退款。正式报告保留原任务、源验证、校准、机会和原生报告引用，
只属EVALUATOR_ONLY，不进入研究者普通读接口。

评估和全部指标在同一事务发表，文件/事务失败保留原消息重试；精确重放不再读取
或发表对象。封存评估不修改原实验结论、Alpha版本或active指针，不再拟合校准。
ACK须等待该Run的精确SEALED评估和发布标记；发表不代表独立Reviewer或资格完成。

### A4.12 有预算的人工封存评估准入

`POST /alpha-versions/{id}/evaluations`与`alpha evaluate <id>`接受schema_version、
cycle_id、policy_id、input_set_id、runtime_id、expected_runtime_revision和limits。
它是Operator命令；CLI须有精确Alpha目标和原请求的一次授权，Mission不得调用。
使用同项目RUNNING Cycle的预算及冻结政策/Sealed InputSet/Runtime，不创建无预算
后台工作；另一政策应进入显式的新Cycle，而不是改写旧政策或原模型。

目标为原EXPECTED_RETURN版本或真实附加校准的SCORE版本。服务沿原正式Validation
找到已付费的原编译、Wasm、预测参数及实际训练元数据；调用者不能提供模型、校准、
镜像、原生路径或研究可见时间。研究可见截止取原训练/Discovery登记质量的实际
available-through上界，不用宽松目录decision_cutoff或当前墙钟代替。
Sealed只读取登记元数据作准入检查，原始市场行仍须等A4.10首次能力预约。

limits.experiments必须为0；原编译确已计入的试验不重复收费，但新的Run仍预约
CPU/墙钟/内存/输出并受Cycle累计预算约束。原生当前能力、冻结镜像、全部输入
及精确政策关联在创建前核对，参数发表、Run/PGMQ、NativeTask和Sealed关联同事务
提交；未知提交重放原命令，不另建任务。拒绝或发表失败不留下半状态。
普通通用ALPHA_EVALUATE仍需试验收费，standalone管理入口不扩展成免费研究接口。
此人工操作只请求评估，不授予资格、Reviewer身份或交付；自动Reviewer使用同样
的模型/数据准备规则与原Cycle预算，不能复用人工授权冒充Operator。
两条可信入口共用同一个事务内准备器：原模型/参数/数据、当前政策及能力检查、
参数发表和Run/PGMQ/Sealed关联不复制。人工入口独立核验授权并保存原命令回执；
准备器本身不是HTTP/MCP权限入口，外层事务提交前仍须重验各自的调用资格。

### A4.13 独立审阅后的可信封存续接

独立Reviewer的全部目标回答已封口后，可信Worker按原排名为PASS目标逐个准入
Sealed任务；不为REJECT/INCONCLUSIVE、失败或取消的会话补做任务。每次消费最多
准入一个目标，复用原PGMQ消息和共享准备器，不增加模型轮次或Operator授权。
`mission_sealed_evaluations`将原审阅reservation与一个原Sealed Run不可变关联，
目标、源Validation、Cycle/政策必须与原选择相同。关联、参数、Run和队列同事务。
Reviewer成功终态及ACK须等待这些关联齐全；实际科学完成/发表仍由科学Worker负责。
重复消费不重复准入或重开Thread，原编译试验不重复计数、所有资源仍占Cycle预算。
准备使用原Reviewer冻结限额及剩余墙钟，当前Runtime能力在事务外刷新；取消/到期
不开始新阶段，项目暂停保留消息。预算不足如实结束Cycle为BUDGET_EXHAUSTED，
过期源证据或需修订输入进入WAITING_INPUT；不假造Sealed评估、退还机会或授予资格。

### A4.14 原独立封存评估的资格裁决

可信Sealed ACK事务在正式评估已封口后，为原独立Reviewer PASS关联的精确目标
裁决资格；不提供Agent/Operator手填PASS或发证接口。必须是本次原Attempt机会、
SUCCEEDED/VALID/PASS的原SEALED评估，且原Discovery、Validation和Sealed数据均为
REAL、VERIFIED、AS_KNOWN_THEN。即使评估政策允许fixture，也不能给fixture发证。
原版本/校准/模型/镜像/源Validation和本次政策关联继续沿用共享检查；暂停/退役的
Alpha不能因新结果被恢复。输入来源和原许可在既有锁下重验有效期及撤销。

资格引用本次Sealed评估及精确Alpha版本/政策；期限不超过本次评估、源Validation
或任何所用原许可的有效期。数据库当前时间形成granted_at，重放不刷新期限、复制
新资格或重新授予已撤销的原资格。缺审阅、科学否定、fixture、过期或失效许可只保留
原评估而不授资格，不把已完成科学任务重试成PASS。原科学ACK、资格和生命周期
更新同事务，失败保留原消息。只在活动版本仍是该目标时将RESEARCH标为QUALIFIED，
不切换活动指针；后续使用仍须检查该精确资格的新鲜度、撤销和交付用途许可。

## A5. Mandate、Candidate、目标与 Release

Operator及精确项目RESEARCH_READ的CLI可分页读取
GET /api/v2/alpha-versions/{id}/qualifications，CLI为alpha qualifications。
QualificationView仅返回原授予/政策/评估引用、授予与到期时间、checked_at，以及
最早撤销的引用、effective_at、reason_code和证据评估引用；不读取Sealed报告或指标。
grant_window_open只表示checked_at在[granted_at,valid_until)且最早撤销尚未生效。
未来撤销也返回，历史过期/撤销记录不隐藏。这个字段不验证当前政策、Alpha生命周期、
REAL/PIT或许可证，不是组合准入/Release资格；页面必须保留这一区别和观察时间。
页面翻页使用原资格ID，不因刷新、活动版本变化或复制版本而混入其他版本的资格。

MandateCreateV1包含schema_version、project_id、runtime_id、expected_runtime_revision
和完整content。创建在原Operator命令事务内锁Project分配版本，重验精确Runtime
最新有效探测与PORTFOLIO_BUILD能力、原模型版本和执行假设镜像，保存原完整响应。
同键只重放原版本；更换配置必须新版本，不更新既有Mandate。读取只供Operator或
精确项目RESEARCH_READ的CLI，不新增Mission权限。创建不授予Alpha资格或Release。
风险厌恶系数risk_aversion作为正Decimal冻结在optimizer.parameters中，单次
AllocationInputV1不再另带该字段，实际求解直接使用原优化器参数。

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
这里的独立组合评估必须为evaluation_kind=PORTFOLIO；候选发布后的FORWARD/HOLD
评估只能用于其原保持研究，不因结果PASS而替代Release的PORTFOLIO评估。

历史目标序列存 Arrow/Parquet，不每 bar 建业务对象。不可行 cash/targets 均 null；LAST_TARGET 是假设，真实权重输入来自下游签发 snapshot，QZ 不建真实账户账本。`sum(asset_weights)+cash_weight=1` 在 mandate tolerance 内，现金字段/保留代码明确；gross/net、组、成本、参与率原生计算，领域层独立合同/容差验证。

原生滚动研究同时输出`qz.portfolio_history/1` TARGETS，媒体类型
`application/vnd.apache.arrow.file`。采用Apache Arrow IPC File，一个RecordBatch，
按原帧、原资产顺序逐行保存：cutoff_ns/asof_ns/valid_until_ns为非空
timestamp(ns,UTC)，instrument_id/currency/solver_status为非空UTF-8，
weight/cash_weight为nullable decimal128(38,18)，单位fraction。现金使用独立列，
在同帧各资产行保持相同，不伪造现金Instrument或经f64转换。失败帧仍逐资产保留
身份/时间/原求解状态，两个权重列均null；不输出未执行后续帧。
schema元数据固定name=qz.portfolio_history、version=1、semantics=SIMULATED_TARGETS、
weight_unit=fraction。帧1..256、每帧资产1..256，单文件最多65536行；受原任务总
输出字节限额约束。发布与采纳用同一Arrow合同核对schema/元数据、全部列值、
顺序与行数，且必须与原请求及qz.portfolio_study逐项一致。三个原生输出（完整
源质量、研究报告、历史目标）同属原manifest；文件本身不授予PASS或Release。

离线组合研究从冻结原模型和目录逐个cutoff重新生成预测、共同收益窗口及原生求解
输入，不要求先有历史Candidate，不伪造LAST_TARGET或下游快照。模拟仅初始化一次
原生现金账户；每次调仓从该账户读取当时equity/net_position及共同已到达价格，
将权益作为本步资本、净敞口权重及剩余现金假设交给同一优化器。实际求解与原生
减仓/增仓成交仍在同一个引擎内执行，不能重启账户、拼独立收益或用上次目标冒充
漂移后的当前权重。原始资本仍来自Mandate，过程观察只是模拟报告，不是真实账户。
模型研究可用截止必须早于首个评估cutoff；每帧只读取截至自身cutoff的目录前缀，
不能把全样本协方差或最后权重回填过去。冻结fuel在各帧均分为硬上限、累计实际
消费；不让每个cutoff重新获得全任务额度。不可行保留原求解诊断、停止后续帧且
不生成组合模拟PASS。完整政策/来源/独立性与Arrow历史、正式发表另行验收。

原生SIMULATE_PORTFOLIO_SEQUENCE复用同一个Nautilus账户，按时序消费2..253个
已冻结Candidate目标文件；每项仅绑定candidate_id、可信candidate_available_ns及
target_artifact_id，并重读原文件核对目标/现金/币种/原有效期。253给既有256项
输入上限保留目录、原费用与任务参数三个位置，不提高原Run限制。每个实际生效点取
原asof与可信可用时间的较晚者，严格递增且不能跨越前一个目标的有效期；不得
回填最后一个目标到历史、跳过中间失败或把费用文件改成临时设置。源目录质量
仍按完整source_selection计算，实际模拟窗口不得超出它或最后目标的有效期。
该入口只是原始序列的独立共享资金执行，不自动发表PORTFOLIO/PASS：正式Store
准入还须冻结政策指定的完整序列、同Mandate/Alpha版本/费用来源及数据用途，
防止事后挑选有利片段；历史目标序列文件、指标、发布与Release资格仍单独验收。

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

调仓计划的MANUAL不带interval/calendar/offset；FIXED_INTERVAL仅带正interval；
离线原生研究的MANUAL在原任务参数中冻结manual_cutoffs_ns（2..256个UTC纳秒）；
首项等于evaluation_start_ns，严格递增且全部位于原评估窗口内。不得排序、去重、
补点或运行后选择有利子集；每项原目标TTL必须覆盖下一截止，末项覆盖评估结束。
FIXED_INTERVAL不接受手工截止覆盖，同样核对目标覆盖与统一帧数/fuel/时间上限。
MANUAL只表达离线研究预先冻结的时点，不授予Agent调仓或下游执行权限。
CALENDAR_SESSION仅带非空calendar_ref与显式offset（可为零或负）。输入最大年龄与
目标TTL均为正秒数，timezone须由原生IANA库识别，不默默替换UTC。日历引用的语法
有效不表示真实日历版本已可用，准入仍须核对冻结原生能力。组合约束的结构检查由
Mandate与实际求解共用；具体资产/组成员和数值可行性仍由冻结输入及原生求解核验。

原生离线CALENDAR_SESSION消费原PARAMETERS会话表，不从周历推断交易所休市。
NativePortfolioStudyRequestV1.calendar绑定artifact_id及原NativeCalendarSessionsV1：
schema_version=1、calendar_ref/calendar_version/timezone、source_reference、
available_at_ns、coverage_start_ns/coverage_end_ns、sessions[{open_ns,close_ns}]。
表的覆盖区间按原收盘时间定义为半开区间，必须包含该范围全部原会话；1..4096项
按时间严格递增、不重叠，不排序、补点或过滤异常。source_reference只作出处记录，
不允许Job访问URL；完整性与许可证由原数据所有者准入负责，结构有效不等于真实来源。
available_at_ns不晚于原evaluation_start_ns，不用后见日历冒充当时已知；不要求
日历早于模型研究结束，二者是独立来源的可用时间。
日历名称及IANA时区必须匹配Mandate，名称/版本还须匹配注册目录的原Universe。
Job重读原文件并逐值核对冻结副本。非日历模式不得附带日历，日历模式不接收手动截止。
每项截止=原close_ns加session_offset_seconds（真实秒，允许负值），仅取评估窗口内
全部截止；覆盖范围先按相反偏移核对，防止边缘漏会话。首项等于评估开始，复用
2..256帧、fuel、原目标TTL及末尾覆盖检查。原表UTC时间表达DST与半日市；此适配
不计算节假日、不自建日历引擎，不授予正式PORTFOLIO/PASS或下游调仓权限。

日历来源沿用Dataset登记，不新增手填日历命令。Runtime原Universe元数据可携带
calendar_sessions:NativeCalendarSessionsV1|null；名称/版本匹配原Universe，UTC覆盖
包含Universe覆盖区间，原可用时间不晚于登记观察时间，结构检查与原生Study共用。
登记在原批次保存qz.calendar_sessions/1 PARAMETERS原文件，access_class=OPERATOR、
created_by=RUNTIME、origin继承原目录，universe_versions.calendar_artifact_id引用它。
缺失保持null，不回填旧Universe或生成默认表。复用Universe时缺失/存在状态及原表
全部字段必须一致；换会话或出处不得沿用相同Universe。批次、命令回执与外键同事务，
失败沿用原发表回滚和未引用对象清理。Universe只读投影显示可选文件ID，不暴露字节。
Runtime的日历Study还必须逐值匹配注册Universe中的原表，不只比较名称/版本。
此登记绑定来源与许可授权，不自行认证交易所日历准确性，不授予REAL/PIT/PASS。

### A5.1 候选子对象唯一性

Operator及精确项目RESEARCH_READ的CLI可读取
GET /api/v2/projects/{id}/portfolio-candidates 的原发布头分页和
GET /api/v2/portfolio-candidates/{id} 的完整不可变头、成员、目标快照。
CLI为portfolio candidate list/show。只返回已封口Candidate及原引用，不读报告字节、
存储位置或Sealed指标，不新增Mission权限。执行状态、solver_status、evidence_status
与原诊断来源分别返回；历史VALID/目标快照不表示当前资格或Release授权。
取消/失败/过期候选仍可查，无目标保持空集合，不补造权重。
React/Ant Design“组合 → 候选快照”使用同一分页和详情接口，项目切换重置游标与
选中项，显示原Decimal字符串和时间，不提供基于历史状态的审批/交付捷径。

每个正式PORTFOLIO_BUILD Run只发布一个不可变Candidate；Worker在原Run锁内读取
原参数、终态回执、Attempt/manifest和原生组合报告，核对完整输出绑定后同事务
发布Candidate及成员/目标。ACK丢失重放原Candidate，不重新运行或追加子项。
原Run虽已终态，但Candidate未完成不可变发布时仍拒绝ACK；对象写入失败回滚
发布事务并保留原队列消息，清理未引用对象后重试，不能提前归档丢失后续处理。
取消/原生失败保留真实Run状态与无目标诊断；求解成功但当前资格、许可或目标期限
失效时保留solver_status，evidence_status标INVALID并去除可交付目标，不能伪称
求解失败。原生成功并不生成Qualification/Release；共享资金验证仍是独立后续任务。

原生SIMULATE_CANDIDATE是该后续链的原目标保持模拟入口，不是Eval/PASS发布接口。
任务冻结candidate_id、candidate_available_ns、dataset_revision_id、target_artifact_id、settings_artifact_id
及NativeSimulationRequestV1。目标使用原qz.portfolio_targets/1完整文档，费用使用
原qz.native_simulation_settings/1，不接受临时手填目标替代原文件。job重读并核对
原Candidate身份、币种、目标/现金、asof/valid_until及完整费用设置；只挂载一个
FORWARD目录、原目标REPORT及费用PARAMETERS，不能借此读取Sealed或其他对象。
candidate_available_ns须由可信准入从原Candidate可用记录取得，不是Operator可选的
回填时间。保持模拟恰含一个原权重/现金点；该点asof和selection.event_start_ns均
取原目标asof与candidate_available_ns的较晚值，结束不晚于原valid_until，不改写
原目标文件、不把最终权重放回可用前。目标从原模拟初始现金资本开始应用，
原Nautilus根据首个因果已知价格调仓；不是恢复真实账户或假造原实际仓位。缺未来
区间、超出原目标时间窗口、缺原文件或改写副本均不授予结果。保持模拟不是策略walk-forward，
不把它冒充完整独立组合评估；正式Store准入、政策/指标/期限与不可变Evaluation
发布另行绑定后才可供Release使用。需要candidate-simulation/2原生镜像能力。

保持模拟的正式准入意图CandidateSimulationRequestV1仅含schema_version、cycle_id、
candidate_id、input_set_id、runtime_id、expected_runtime_revision及有界limits；
不接收目标、费用、可用时间、政策或结果。Operator/精确CLI grant使用独立
PORTFOLIO_SIMULATE操作。Store从同项目已封口成功VALID Candidate重读原目标，
复用LAST_TARGET原文件/行核对；原created_at与asof较晚者是最早模拟起点。
费用及镜像绑定原Build任务与执行假设，Forward窗口只能缩窄到原有效期内且
不能使用尚未可用的数据。运行中Cycle必须使用原Mandate政策；复用原预算、
幂等命令、Run/PGMQ及不可变原生任务绑定，不建立第二队列。
准入返回Run而非Evaluation，不能因保持模拟成功批准Release。正式评估发布仍须
按独立政策、样本数、来源和期限验收；本段合同不表示入口已实现或验收通过。
操作面为POST /api/v2/candidate-simulations与client portfolio simulate，使用同一
严格请求及Idempotency-Key，成功202返回原Run。CLI grant绑定原Candidate及完整
意图；没有模拟权限的身份不能借Build权限调用。请求失败清理仅未引用的新对象。

独立组合指标要求在原EvaluationPolicy的portfolio_metric_requirements冻结，不能
使用metric_requirements或sealed_metric_requirements替代。可空以支持纯Alpha研究；
为空时没有组合PASS条件，不能据此准入正式组合评估或授予Release。非空必须有
1..64项、至少一个required指标，并沿用精确阈值、唯一code/scope、样本数要求及
原方法allowlist校验。历史已冻结政策不回填或复制条件；要改变条件必须新建政策。
保存条件本身不证明原生方法支持、样本足够或评估通过；原生指标适配与发布须另行核验。

正式组合研究的计划在同一EvaluationPolicy以可空portfolio_study_plan冻结：
{schema_version:1,input_set_id:Id,evaluation_start:Time,manual_cutoffs:Time[]|null}。
存在计划时必须同时定义portfolio_metric_requirements。input_set_id必须是同项目
已冻结PORTFOLIO输入，恰含一个原DISCOVERY/VALIDATION数据版本；保存时重验当前许可。
评估结束固定取该原数据版本的event_end，不接受另一个可挑选的结束参数；起点严格
位于原event_start与event_end之间。Time为非负、微秒精度且可表达为原生纳秒。
手动时点如存在须2..256项、首项等于起点、严格递增且均早于原结束；不排序或补点。
计划不持有Mandate副本；正式准入再核对原Mandate调仓模式、TTL、日历、原模型研究
可用截止与整段数据来源。非手动模式不得使用手动时点，手动模式必须有完整原序列。
政策及原InputSet身份与计划不可原地修改，旧/纯Alpha政策保持null，不自动补计划；
没有计划不能启动正式Study或授予PORTFOLIO/PASS，已有FORWARD/HOLD仍只是保持评估。
变更计划必须新政策并走原关联与暴露账本，不能在运行命令中改输入、起点或删选时点。
保存计划仍是研究准备，不启动Run、不读取市场原始字节或证明原生能力与资格。
正式Study命令只接收schema_version、cycle_id、candidate_id、runtime_id、
expected_runtime_revision和limits；沿用PORTFOLIO_SIMULATE授权、预算和队列。
操作入口为POST /api/v2/portfolio-studies与client portfolio study；授权意图
命令为PORTFOLIO_STUDY，映射同一PORTFOLIO_SIMULATE权限，但不接受HOLD意图替代。
候选详情的Study表单固定原Candidate，显式选择本项目运行中Cycle、Runtime和有界
限额；只读展示原Mandate政策计划，不允许编辑窗口或成员。提交前读取Runtime修订；
未知提交结果保留原六字段意图及幂等键，202仅表示Run登记，不能显示为科学通过。
原Candidate只确定Mandate和完整成员，不提供历史持仓；其目标TTL不限制离线研究。
模型研究可用截止包括原Discovery/Validation及用于资格选择的Sealed观测，
取原可用时间的最大值，不用登记墙钟代替历史时间。仅读取Sealed元数据，
不把其数据绑定交给Study。原政策计划、费用、日历和滚动流动性必须重验；
历史单次流动性快照不能替代逐cutoff测量。portfolio_study_tasks保存原Run、
Candidate、政策、数据版本和完整意图；缺少独立PORTFOLIO发表回执时不得ACK。
可信Worker复用原Evaluation事务为Study发表evaluation_kind=PORTFOLIO、mode=STUDY。
原spec/Attempt/manifest、完整质量、逐帧报告和Arrow历史须全部对应；指标只映射
原Study内的实际simulation_request/simulation，不重算收益或借HOLD报告。不可行
保留原诊断且无模拟指标/PASS；取消/失败只按原终态回执发表INCOMPLETE/INCONCLUSIVE。
有效期不晚于原生完成时间加政策TTL、原成员资格及其数据许可/已知撤销时刻，
不取历史窗口结束或当前Candidate目标TTL。发表前后重读原政策/模型/输入/费用/
日历/流动性；到期或失格发表不通过证据，损坏与写入失败保留重试。原发表回执
重放不重读过期来源或刷新期限。读模型元数据或发表不授予审批/Release权限。

组合指标薄适配先核对原模拟请求/结果的账户、时窗与目标绑定，只读取原
Returns统计组（不读canonical的position fallback），scope固定portfolio：
PORTFOLIO_DAILY_RETURN_MEAN对应Average (Return)/nautilus-analysis.ReturnsAverage，
unit=RETURN_PER_DAY，annualization_factor=null；PORTFOLIO_RETURN_VOLATILITY对应
Returns Volatility (252 days)/nautilus-analysis.ReturnsVolatility，
unit=ANNUALIZED_RETURN_STDDEV，annualization_factor=252；PORTFOLIO_SHARPE_RATIO对应
Sharpe Ratio (252 days)/nautilus-analysis.SharpeRatio，unit=RATIO，
annualization_factor=252。三者method_version=0.63.0、frequency=UTC_DAY；
252表示原生每年日数，不是再次乘到原值的系数。波动率/Sharpe由原生按UTC日
复利分箱、样本标准差(ddof=1)计算；Sharpe该路径不扣无风险利率。
适配不重算统计、不补日历空档，observation_count是原日收益条数；period是
原canonical实际模拟起止（纳秒向外取整到微秒），不是声明的更大输入窗口。
原收益不可用时沿用其状态/原因；原统计缺值保留NATIVE_STATISTIC_UNAVAILABLE
及FAILED，不猜测原因；应有统计键缺失则拒绝结果。真实零值保持OK。
所有指标绑定原产物和Evaluation身份，再交既有精确政策比较器；适配本身不创建
Evaluation、不批准Release，保持目标模拟也不冒充策略滚动评估。

Candidate模拟准入在同一原Run/PGMQ事务冻结candidate_simulation_tasks：run_id主键、
candidate_id、policy_id、dataset_revision_id及完整原请求request。政策必须是原
Candidate/Mandate所需政策并与原Cycle一致；数据是原冻结Forward输入的精确成员。
数据库外键与不可变触发器保留这些原身份，不因重试、政策新版本或后续Candidate
替换而改写。原参数发表失败或入队失败不留下该绑定；同意图重放返回原Run。
此关联供终态评估发布/ACK恢复消费，本身不创建评估或授予PASS。

candidate-simulation/2在原参数另存source_selection，来自原登记metadata质量报告
的完整选择，不是缩窄后的保持窗口。原生job复用DATA_VALIDATE同一目录检查路径，
先输出该原窗口的qz.data_quality，再输出实际保持窗口的qz.native_simulation。
两个报告随同一manifest核对schema、原数据版本、完整选择、检查时间与唯一成员。
source_selection覆盖实际选择且bar_types/maximum_rows一致，不能借此读取其他目录；
原登记cutoff不因项目cutoff变晚而扩张。后续缺失比例核对原窗口载入行数与登记行数，
日收益样本数仍只来自实际模拟；质量检查不是收益、PIT认证或策略滚动评估。

可信Worker在原消息ACK前为candidate_simulation_tasks发布不可变FORWARD Evaluation：
subject为原Candidate，原Forward InputSet及原政策不变。FORWARD在此表示该候选
发布后的保持目标研究，不是已执行Release的Forward evidence window，不自动触发
退化、审批或交付。原Run锁、终态回执、Attempt/spec、参数和双报告manifest必须吻合。
指标使用独立portfolio_metric_requirements；null为INCONCLUSIVE，不复制Alpha条件。
实际日收益数检查minimum_observations；原完整登记窗口质量行数与登记行数以
整数/Decimal核对maximum_missing_fraction，超出登记数量INVALID，缺失超限或样本
不足INCOMPLETE。REAL要求原任务来源与登记REAL/VERIFIED/AS_KNOWN_THEN同时成立。
原目标、执行假设、费用、成员资格、许可及期限在报告发表前后重验；原Build、
执行假设和本次Forward输入的全部许可期限在同一最终SQL时点核对。
损坏或发表失败回滚并保留原消息。已不合资格或到期只保留不通过证据，不伪造指标。有效期不晚于
原生完成时间加政策TTL及原目标期限；重放不刷新。取消/失败以原终态发表
INCONCLUSIVE且无指标，不能声称远端已停止。Evaluation、MetricValue、静态原因及
原报告引用在同一事务封口；只有精确关联的已封口Evaluation才允许ACK。

Operator及同项目RESEARCH_READ CLI可分页查询
GET /api/v2/portfolio-candidates/{id}/evaluations；详情与指标复用
GET /api/v2/evaluations/{id}及/{id}/metrics。只读取有原Candidate模拟绑定、
终态回执、Forward冻结输入和qz.candidate_evaluation原生产者报告的已封口评估。
既有Alpha Validation规则不放宽，Sealed及无绑定旧记录仍不在此披露。
CLI为portfolio candidate evaluations，详情/指标沿用evidence show/metrics。
React候选详情展示保持研究评估分页和原指标；缺值保留原因，零值不隐去，方法、
单位、UTC_DAY频率、年化和原有效期分别显示。只有元数据及指标，不下载原报告，
不自动模拟/审批/交付；历史PASS或未过期不等于当前Release授权。

`unique(candidate_alphas.candidate_id,alpha_version_id)`、`unique(candidate_targets.candidate_id,instrument_id)` 是数据库约束，不是普通索引。重复相同请求幂等，冲突409；至少两个不同alpha_id的合格版本才满足多Alpha，不以同Alpha多个版本或重复条目凑数。发布验证每资产唯一权重，再校验sum/gross/net/cash/约束。

### A5.2 原生组合求解的可执行合同

正式PORTFOLIO_BUILD命令的外部意图仅含schema_version、cycle_id、mandate_id、
input_set_id、runtime_id、expected_runtime_revision、current_weights_source、
environment、members[{qualification_id,ensemble_weight}]及有界limits。不得传入
预测值、模型路径、资产当前权重或费用。Store从原Mandate、已认证下游快照、原资格
对应的原生评估任务及执行假设恢复这些内容；相同意图重放原Run，不重新选择版本。
至少两个不同Alpha的当前有效资格，政策与Cycle/原Mandate一致；生命周期、撤销、
时限、原REAL/PIT数据及当前许可须在准入与发布时重验。原任务参数只能由可信Worker
读取，不能借组合任务向Mission暴露Sealed数据。current_weights_source严格为
FORWARD_SNAPSHOT{snapshot_id}或LAST_TARGET{candidate_id}（判别字段kind）。
LAST_TARGET只读取同项目已封口、成功且VALID的原Candidate目标，核对原目标文件与
数据库子项、币种和原时限；保留原决策时点，available为不早于原发布的时点，
不延长期限。派生qz.portfolio_current_weights标明LAST_TARGET及原Candidate，
与新Run参数同事务发布，失败统一清理未引用对象。PAPER降为SYNTHETIC，LIVE也不
升级原SYNTHETIC来源；假设始终不冒充真实账户仓位。FIXTURE/未知来源不能准入。
原Build来源FK与请求不变，发布前重新核对来源。开始Run不是生成Candidate或授予交付资格。
HTTP入口为POST /api/v2/portfolio-builds，原生CLI为client portfolio build；需要
Operator或目标为mandate_id、内容完全相同的PORTFOLIO_BUILD单次grant及幂等键。
返回202和原Run回执，不以内存任务句柄冒充已执行。失败清理沿用Operator事务锁。

保守BAR适配使用原费用文档及上述原Universe、历史流动性来源。
DefaultFillModel非零滑点规划仅覆盖原生支持的线性CurrencyPair/Equity，需要
portfolio-slippage/1。原生L1撮合以概率p在买价加一个tick、卖价减一个tick。
job从本次Forward最后已完成BAR提取价格P、event_ns、available_ns及原Instrument
的tick δ，输出逐资产slippage_references；p=0时列表为空，费用仍为原taker费率f。
p>0时规划费率为f+p·δ/P·(1+f)：参考价P下、舍入前买入手续费与不利价差的
期望比例；f≥0时不低于同参考价的卖出期望比例。只换算上游模型，不抽样或另写
填充模型。系数向上舍入到18位小数，超[0,1]拒绝、不截断。必须0<δ<P；原事件
在选择内、已可用且与预测asof一致；资产顺序、币种及tick与原Forward定义绑定，
发布重读核验。价格来自受信任job读取的原目录，不接收Operator手填系数。
这不是未来成本上界、盘口冲击、逐笔精确费用或DATA_BACKED资格；不含未知深度
效应，不外推历史价到未来。实际成交、费用舍入及共享资金结果仍由同一Nautilus
模拟产生，不从净收益再扣规划系数。DATA_BACKED仍待完整来源适配，不能称Issue62完成。
NativePortfolioBuildRequestV1另冻结完整execution_settings；原transaction_costs_ref
文档以PARAMETERS角色进入job，job重读其原字节并核对完整副本。币种、资本和
逐资产taker费率必须分别匹配Mandate及assets.transaction_cost_rate，缺项或额外
资产拒绝。请求assets仍冻结原taker费率；结果assets仅可按上述参考换算规划费率。
准入与原结果必须声明portfolio-cost-source/1能力；Candidate发布再次读取原文档，
与冻结副本及原保存配置完整相等。这是原费用/模型来源绑定，不是新增滑点算法或DATA_BACKED资格。
费用绑定还必须重验本次Forward目录的原instrument definitions：资产集合、币种及
maker/taker费率均匹配冻结execution_settings。同名资产在Forward中改变费用不能
沿用旧费率求解；Store准入拒绝、发布重新读取原目录核验，job复用模拟的原生市场
与费用校验。不能等到独立模拟时才发现Build使用了另一套费用。

组合求解复用已有 Clarabel 0.11.1，不另写优化算法。原生 job 接受固定资产顺序的预测、同顺序协方差、明确的当前目标/现金、资本与数据支持的费用/流动性，不从两个独立 NAV 的平均值构造组合。资产集合上限256；重复或缺失身份、矩阵尺寸/对称性/正定性问题、非有限数、缺当前权重或费用、无真实来源的流动性均明确失败，不补零。协方差必须来自冻结输入的原生估计，单位为每决策周期收益协方差；年化只在明确参数下用于报告，不隐式乘252。

`PortfolioConstraintsV1` 的现金、全局资产上下界、逐资产覆盖、组上下界、gross/net、turnover及参与率都进入同一个原生问题。turnover明确为本次所有资产的绝对目标变动之和（买卖各计一次，现金是剩余资金，不重复计入交易费用）；当前权重及现金必须来自同一个有来源的快照并在容差内合计1。max_participation按每资产可用成交额除以冻结capital换算，缺流动性不放宽。费用为该资产每单位交易名义金额的明确费率，通过原生目标函数纳入；最终共享资金模拟仍使用同一冻结费用假设，不能二次从模拟净收益扣除。

第一条受支持的求解配置为 CLARABEL/0.11.1 的 MIN_RISK、VARIANCE，包含上述线性约束；MAX_UTILITY将有明确单位的预测及正risk_aversion加入相同QP。VARIANCE 的 max_ex_ante_risk 是每决策周期收益的组合方差上限（不是标准差或年化波动率），必须为正；通过原生 Cholesky 与 Clarabel 二阶锥约束实现 wᵀΣw ≤ 上限。发布时以保存后的 Decimal 权重及同一冻结收益历史重新调用原生协方差与矩阵乘法复核，允许误差仅为上限乘 exposure_tolerance，不是绝对方差容差；非有限、溢出或下溢明确失败。启用方差上限须有 portfolio-variance-bound/1 及 SECOND_ORDER_CONE 镜像能力。逐项扩展必须附原生数值与独立小例验证；这段受支持范围不删除Issue62的完整交付项。

CVAR 同样支持 MIN_RISK/MAX_UTILITY，明确冻结 optimizer.parameters.cvar_confidence
为 (0,1) 内的 Decimal；CVAR 缺值或 VARIANCE 携带非空值均拒绝，不默认95%。
每个原始 return_history 时间列是一项等概率场景，损失为资产权重与该周期收益的
负内积（现金收益为0）；不拟合正态分布、不年化、不通过协方差替代尾部风险。
采用 Rockafellar–Uryasev 场景形式 eta + sum(z)/(N*(1-confidence))，其中
z>=loss-eta 且 z>=0，直接装配已有 Clarabel 原生线性规划；MAX_UTILITY 加入
原预测，费用沿原换手项只计一次。risk_aversion 乘该风险目标。CVAR 的正
max_ex_ante_risk 是同周期损失收益率的预期短缺上限，不是方差；进入同一线性问题。
发布以保存权重重新做原生 ndarray 内积和标准库次序统计，按该经验分布的精确
尾部概率质量复核（含分数个场景和重复损失，不能只平均严格超过VaR的场景）。
置信水平与场景数的乘积/尾部质量先用 Decimal 计算，避免接近1时被浮点舍入抹去。
上限容差同样是上限乘 exposure_tolerance；风险可为负，不能截成0。CVAR不调用
方差估计/Cholesky，其原协方差引用保留为明确的Mandate模型配置但不影响该风险。
需要 portfolio-cvar/1 与 LINEAR_PROGRAM 镜像能力。

RISK_BUDGETING 的 VARIANCE 形式须明确 optimizer.parameters.risk_budgeting：
schema_version=1、正 risky_gross_exposure，以及覆盖原资产集合的 assets，每项为
instrument_id、非负 share 与 sign=LONG|SHORT；share精确合计1，不能按列表位置
猜身份或默认等风险。非风险预算目标必须为空；CVAR沿用同一份显式预算配置。
正share的SHORT不能用于long_only配置；零share资产固定零权重。risky_gross_exposure
明确风险资产总敞口，现金仍为剩余资本，不从真实账户推定。不得用全现金的零风险
冒充预算比例成立。方差风险贡献比例定义为 w_i*(Σw)_i/(wᵀΣw)，须与share一致。

复用Clarabel二次锥风险预算公式。令C=Σ/max(diag(Σ))=LLᵀ，在指定多空方向
最大化共同贡献尺度t，约束||Lᵀw||≤1；正预算资产以二次锥表示
||(2*sqrt(share_i)*t, sign_i*w_i-sign_i*(Cw)_i)||≤sign_i*w_i+sign_i*(Cw)_i。
它等价于w_i*(Cw)_i≥share_i*t²及显式方向；零预算资产固定w_i=0。
贡献求和及总风险上限给出t≤1；正定协方差下原风险预算解达到t=1，各项贡献
不等式均取等，再按显式总敞口规范化。指定方向/尺度的风险预算权重唯一。
统一方差尺度换元不改变规范化权重或相关结构，不是正则化、截断或替换协方差。
发布仍核对原始Σ。不能把敞口/换手/费用等额外项放入第一阶段后宣称仍满足预算。
规范化的
原生权重随后作为等式进入原有Clarabel组合问题，核对全部现金、资产、组、gross/net、
turnover、参与率及风险上限约束并计原费用；冲突返回不可行，不裁剪或换目标。
两阶段均使用冻结solver_tolerance；原生收敛不能替代贡献检查或授权低精度状态。
两次原生求解共享max_iterations和外层资源期限；任何未授权低精度、失败或预算
耗尽都不返回目标。公开iterations合计，残差取两阶段较大值，objective_value为
最终约束/费用阶段原生目标，不冒充风险预算误差。发布以保存后的Decimal权重、
原生协方差/矩阵乘法复核方向、总敞口、非零总风险和每项贡献；贡献误差上限为
总方差乘exposure_tolerance，不能仅信任求解成功。需要portfolio-risk-budget/1
与SECOND_ORDER_CONE镜像能力。

CVAR风险预算使用原置信水平及全部等权损失场景，不以方差近似。原生Clarabel
线性/幂锥最小化 CVaR(-R*w/s)，约束加权几何平均prod((sign_i*w_i)^share_i)≥1，
等价于sum(share_i*log(sign_i*w_i))≥0。依原资产顺序将几何平均拆成三维PowerCone，
每步指数为前缀份额/本次累计份额（Decimal计算后只在原生边界转浮点）。其中s为原收益
绝对值的最大值，仅作统一变量换元；零预算资产固定零，s=0明确失败。
一阶最优条件给出各项w_i*g_i=share_i*CVaR；先规范化显式总敞口，再进入上述
同一固定权重约束阶段，不把费用、收益或敞口附加到风险预算目标。这里要求有限
正总风险；零/负风险或非强制正风险的方向可能使原生问题无界，保留实际失败/无界
且无目标，不裁剪收益、不伪造正风险。这不改变普通CVAR目标允许负风险的合同。

成功CVAR风险预算结果必须携带cvar_risk_budget_witness（schema_version=1及
scenario_weights），顺序严格对应原return_history时间列；直接取第一阶段原生
场景不等式的对偶权重p，不排序、截断或归一化。其他目标/风险及所有失败结果
此字段为空。可信发布器独立核对长度、有限非负、sum(p)=1、
p_j≤1/(N*(1-confidence))；概率和及上界使用exposure_tolerance的相对容差。
再以原始收益计算g=-R*p，核对-wᵀR*p与独立经验CVaR相等、
w_i*g_i=share_i*CVaR（风险值和贡献误差均≤正CVaR*exposure_tolerance）。
这验证原CVaR的合法次梯度，允许并列尾部不同的合法分配，不手选有利场景。
原Decimal权重的方向/总敞口检查和两阶段迭代/低精度策略仍有效。
CVAR第一阶段原生gap容差取min(solver_tolerance, exposure_tolerance²)，因为目标
误差的二阶收敛不能直接保证贡献的一阶精度；原可行性容差及最终贡献检查不变。
该更紧停止上限不是成功保证；原生低精度、无解或发布复核失败仍不得变成成功。
需要portfolio-cvar-risk-budget/1、portfolio-cvar/1、POWER_CONE及LINEAR_PROGRAM
能力，并提供独立解析、并列/分数尾部、对偶篡改和真实受管任务证据。

固定权重预测聚合使用ndarray 0.17.1的原生矩阵乘法：行是已对齐Alpha预测，列是
冻结资产顺序，不拟合权重、不补缺值、不把score当收益。首个固定组合适配接受
2..256行、1..256资产，非负Decimal混合权重精确合计1且至少两项为正；不自动归一化
或以等权替代错误输入。输入/输出非有限值和形状不一致明确失败。该数值入口不证明
两行来自不同Alpha或拥有资格；可信编排仍须检查精确版本、单位/期限/币种/时点、
覆盖率、当前资格和数据用途，再将实际聚合预测传入同一原生优化器。组合权重不是
最终资产权重，也不能平均两个独立回测NAV来代替共享资金模拟。

原生聚合的对齐输入保留每个原Alpha/版本、收益单位、币种、固定bar期限、预测
时点/可用时点、混合权重和同序资产预测。版本不得重复，至少两个不同Alpha具有
正混合权重；不能用同Alpha的多个版本或零权重成员凑数。只能聚合
RETURN_PER_HORIZON，不把原score或缺少共同基准定义的residual收益换标签混入。
预测时点必须相同，可用时点位于预测时点至决策时点之间；决策与预测时差不超过
冻结max_input_age_seconds，边界按原生纳秒比较。首个入口要求全部选定资产都有
有限预测，不能丢列、补零或重排成员来掩盖覆盖缺失。固定duration/variable期限在
对应原生预测与对齐证据完成前明确不可用；这不删除完整合同的后续支持要求。
每个成员还须保留同序完整BarType；复用Nautilus规范解析，核对实际instrument_id、
外部LAST时间bar及各资产相同BarSpecification，避免把分钟bar与小时bar的相同计数
误当共同期限。字符串一致只是数值输入合同，真实目录/预测产物仍由可信编排绑定。

本机allocate使用AllocationInputV1作为数值入口，不授予来源或交付资格。受管
PORTFOLIO_BUILD则接收dataset_revision_id与NativePortfolioBuildRequestV1：冻结
selection、完整mandate、current_weights_artifact_id/current_weights、assets及原members。每个成员保留
alpha_id/version_id、model_artifact_id、可选calibration_artifact_id、target_kind、
ensemble_weight与原NativeForecastParametersV1；不接受调用方填写的预测或收益。
目录只来自原FORWARD挂载，模型/校准只来自显式MODEL产物；原生job按同序资产与
共同完整窗口产生历史收益，分别执行原Wasm并对SCORE应用冻结校准，不重新拟合。
各资产最后预测时点必须相同且新鲜；缺失/预热/时间错位不补值。结果以
qz.native_portfolio/1保存原数值input、allocation及真实consumed_fuel，采纳侧核对
原Mandate、成员、selection、预算和目标合同。Store仍须在准入/发布事务核验原
Alpha资格、政策、REAL/PIT、许可、资金/费用来源；运行成功本身不授予这些权利。
旧受管矩阵/手填预测输入不保留兼容路径，原有单次allocate数值检查仍保留。

受管组合的当前权重必须有独立原产物：NativePortfolioBuildRequestV1使用
current_weights_artifact_id及完整current_weights（PortfolioCurrentWeightsV1），
替代单独current_cash_weight。文档包含schema_version、source、asof_ns、available_ns、
valid_until_ns、base_currency、cash_weight和同序weights[{instrument_id,weight,currency}]。
source为FORWARD_SNAPSHOT{downstream_id,external_message_id}或LAST_TARGET{candidate_id}；
NONE不能执行依赖换手的求解，不补零。快照不包含账号、NAV、持仓数量或凭据。
原生job必须从明确REPORT输入读取该JSON并逐字段匹配冻结副本，再核对币种、
资产顺序/唯一性、时点/新鲜度、到期以及原资产当前权重；cash与资产合计在Mandate
容差内为1。LAST_TARGET保留假设身份，不能冒称下游真实快照。独立输出绑定使用同一
冻结副本。该计算合同不证明发送者或来源真实性；Store正式准入/发布仍须重验
下游身份/原Candidate、不可变产物及有效资格。单次allocate仍为无资格数值入口。

原生输出保留OPTIMAL/ACCEPTABLE_INACCURATE/INFEASIBLE/UNBOUNDED/FAILED，只有策略明确接受的成功状态且全部发布约束在冻结容差内再次通过时，才带targets与cash。无解、数值失败、迭代上限、后验约束不通过时，两者均为空，不生成100%单资产或平滑修正的备用权重。权重只在求解器数值边界转换，公开存储继续使用DecimalValue；转换后的权重必须重新验证总和及全部限额。求解成功本身不是Qualification/Release批准。

AllocationInputV1不再接收手填covariance矩阵，必须提供原covariance_estimator和
PortfolioReturnHistoryV1：schema_version、base_currency、horizon_kind/value、
instrument_ids、bar_types、共同end_ns/available_ns以及资产行优先asset_returns。
资产顺序、bar、币种和期限与forecasts精确一致；窗口结束严格递增且非零，每列
available_ns为全部资产最晚可用时点，不早于窗口结束、不晚于决策时点。所有资产
共享2..100000个完整窗口，总收益值不超过1000000；有限简单收益不得小于-1。
不填补、重排或删列。原生ndarray-stats/0.7.0以ddof=1估计每期限协方差后直接进入
同一Clarabel问题，不年化、不添加jitter；奇异矩阵仍明确拒绝。镜像必须提供
portfolio-models/4，旧矩阵输入不兼容。该数值因果合同不证明来源，可信编排仍须
将收益历史绑定冻结目录、许可与原产物，再构建具备资格和完整评估的Candidate。

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

可信服务产生的原生 Turn 请求与公开总结沿原 Mission 角色保存：研究者为RESEARCH，
独立Reviewer为EVALUATOR_ONLY。角色来自不可变会话，不由请求者指定访问级别；
Reviewer的Turn准入只接受本Run/Attempt的原qz.mission_turn请求，不因此允许读取任意
封存参数或报告。请求重放、发送读取和总结发表共用该角色规则；普通产物读取不扩权。

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

受信任 Mission 驱动观察到仍在运行的 Turn 的原生累计差额达到本轮 token 预约时，
先在当前 Run/Attempt fence 内追加 `mission.token_limit` 事件并提交取消意图，再调用
原生 `turn/interrupt`。事件 v1 仅含 `schema_version`、`reservation_id`、
`observed_tokens`、`reserved_tokens`（后两者为 A0 bigint 字符串）；每个预约只保留
首次达到阈值的观察，不是完整用量回执。该事件与取消状态同事务；未对账期间同 Cycle
不得再预约或首次发送模型请求，既有请求仍可查询、中断、如实结算。最终回执不得低于
已记录的原生用量，迟到观察也不得与已有完整回执矛盾。完整回执最终不超额
时可解除这项待对账门禁，真实超额则继续由账本阻止新支出。不把提醒配置、事后用量通知
或异步中断宣称为逐 token 硬限额：通知前及中断竞态内可能已产生额外消耗。
若先确认原生失败/中断、后收到其部分用量，仍记录达到阈值的事实并关闭新支出；
已确认的原生失败不能改写成中断。原生成功且完整用量可结算时直接如实结算，
不因迟到通知把已成功的 Turn 伪装成取消。

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

下游当前权重登记为POST /api/v2/forward/weights（DownstreamWeightsSubmitV1）。
只接受现有DOWNSTREAM机器身份的FORWARD_SUBMIT范围，并绑定其精确project/downstream；
Operator、CLI、Mission和Automation不能冒充下游。请求只含project_id、environment
（PAPER或LIVE）、external_message_id、asof_ns、available_ns、valid_until_ns、base_currency、
cash_weight和weights；source中的downstream_id由认证身份生成。环境须被启用的
DownstreamIntegration允许。时点不得在数据库当前时间之后，期限须尚未结束；
币种/资产唯一性/有限精确十进制及完整现金加权重合计由领域检查，组合消费另用Mandate容差。
external_message_id在精确project/downstream/environment内标识不可变原消息；相同内容
重放原回执，不同内容409。回执与REPORT（qz.portfolio_current_weights/1）及
forward_weight_snapshots来源关系同事务提交；报告source固定FORWARD_SNAPSHOT，
不含凭据、账号、NAV或持仓数量。PAPER产物标为SYNTHETIC；LIVE为认证下游原始报告，
不等于QZ独立核验或研究资格。跨环境不得替换、重标签或更新旧行，失败回滚并回收未发布对象。
该入口不授予审批、交付或下游执行权限；后续组合准入还须绑定环境、Mandate与当前有效资格。

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

handoff_transfers [immutable, native transition evidence]
  handoff_id: Id UNIQUE FK handoff_offers
  downstream_id: Id FK downstream_integrations
  external_claim_id: text
  claimed_at: Time
  provenance: RECORDED_TRANSITION|LEGACY_CLAIMED_STATE
  FK (handoff_id,downstream_id,external_claim_id,claimed_at)
    -> handoff_offers(id,downstream_id,external_claim_id,claimed_at)

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

人工拒绝HTTP为POST /api/v2/releases/{id}/rejections，ReleaseRejectV1含
schema_version=1、downstream_id、environment=PAPER|LIVE、expected_latest_decision_id
（首次null）、reason_code（1..120字符）及reason（1..2000字符）。重新考虑为
POST /api/v2/release-decisions/{id}/reopen，ReleaseReopenV1含schema_version=1、
expected_latest_decision_id（必填且等于路径）、同样的reason_code/reason。
分别绑定精确Release的RELEASE_REJECT或精确Decision的RELEASE_REOPEN近期人工授权。
两者在原Candidate锁内比较该candidate/downstream/environment的最新Decision，
冲突409；成功201只追加历史，不改旧Release、Approval、Handoff或执行事实。
失效/归档项目及停用下游仍允许人工记录拒绝或重新考虑，后者不授予新准入资格。
GET /api/v2/releases/{id}/decisions分页返回该Candidate跨Release的全部原决定，
Operator及精确项目RESEARCH_READ的CLI可读。client release reject/reconsider/decisions
复用这些端点；未知提交结果保留原请求与幂等键，不能改键强行越过最新决定。

审批证据InputSet由服务端在同一事务按原Release评估的报告/方法产物去重冻结为PORTFOLIO，只存原引用，不复制或披露私有字节。客户端不能指定/替换evidence_set_id；普通研究输入与执行侧的EVALUATOR_ONLY限制不放宽，回滚不留下半成品证据集。

人工审批 POST `/api/v2/releases/{id}/approvals` 使用 ReleaseApproveV1：schema_version=1、downstream_id、environment=PAPER|LIVE、expected_downstream_revision、expected_latest_decision_id（首次null，重新考虑后精确引用最新REOPEN）、valid_until。精确RELEASE_APPROVE近期人工grant绑定路径Release和完整请求。先在原Project/Candidate锁内重验原REAL Package与当前全部Release来源，再核对冻结证据、无活动REJECT、原决定CAS以及当前下游配置/真实新鲜探测、版本/环境/市场合同。有效期不得超过原Release或当前来源许可/资格/证据期限；本次探测60秒期限不延长，后续Offer/Claim须再取新鲜探测。

审批不可变地保存 downstream_revision、decision_ordinal（尚无决定为0）和 readiness_observation_id。后续Offer/Claim必须要求当前配置revision与审批绑定一致、当前决定ordinal与审批绑定一致且非REJECT，同时重查撤销/期限/全部来源和当前新鲜readiness；新探测本身不改审批，原探测ID仅作审批审计。REOPEN递增决定序号，因而旧审批不能复活，须重新审批。历史审批缺少这组三字段时保留原行且不补造，只能读历史，不能供新交付。GET `/api/v2/approvals/{id}`返回原审批元数据（Operator或精确项目RESEARCH_READ CLI），不是有效性或交付授权证明。client release approve 与 approval show 使用同一合同。

审批的 `evidence_set_id` 必须属于 Release 的精确 Candidate 所在项目，且已冻结，
用途只能为 `PORTFOLIO|FORWARD`，其中必须包含 Release 所引用评估的报告及方法版本
产物（同一报告可只登记一次）。因此同项目但无关的证据集合、Discovery 输入、草稿或
他项目证据都不能成为交付授权。Paper/Live、自动政策及资格/新鲜度门禁继续独立重查；
仅满足关系约束不是授予审批的权限。新数据库约束不篡改过去已冻结的错误审批。

人工 Offer POST `/api/v2/handoffs` 使用 HandoffOfferV1：schema_version=1、release_id、approval_id、supersedes_handoff_id（该项目/mandate/下游/环境首次null，否则精确引用最新Offer）、expires_at。HandoffOffer人工grant绑定approval_id与完整请求。服务端从原审批取下游/环境，不接受覆盖；在项目/Candidate/下游/审批锁内重验原审批绑定、原Package全部当前来源、证据、撤销和新鲜readiness，expires_at不超过任何来源与审批期限。当前人工入口只消费OPERATOR审批；FROZEN_POLICY由后续政策入口按完整政策合同实现，不以人工入口绕过。

每个原Release/下游/环境只产生一个Offer，即使换幂等键或审批UUID也不能再次发送该版本；同Candidate已领取的目标不能换Release再领。不同Candidate的再平衡必须引用同项目/mandate/下游/环境最新Offer作为supersession。前版若仍OFFERED，同事务转REVOKED；已领取版本只保留显式后继关联，不改执行事实。delivery_sequence由原下游锁内递增，客户端不能提供。新表约束遇到历史重复应使迁移失败并保留原数据，不清理或改写历史。Offer的supersedes_handoff_id属于不可变绑定。

GET `/api/v2/handoffs/{id}`仅返回原绑定及当前状态；Operator/精确项目RESEARCH_READ CLI可读；下游只可使用DOWNSTREAM_CLAIM或DOWNSTREAM_ACK读取自身且属于其项目的Offer。`client handoff offer/show`复用同一合同。创建Offer不调用下游网络、不授予Agent权限、不代表CLAIMED；领取/撤销/失效竞争按以下原生状态机继续实现。

Claim POST `/api/v2/handoffs/{id}/claim` 使用 HandoffClaimV1：schema_version=1、external_claim_id（1..200 UTF-8字节，非空且无首尾空白/控制字符）、package_schema_version。Idempotency-Key必须等于external_claim_id，原下游ID加该编号构成原生幂等范围；编号不能转用于其他Offer或不同请求。仅精确项目/下游且具有DOWNSTREAM_CLAIM的原生机器凭据可领取，Operator/CLI/Mission不能代领。首次领取在原项目/Candidate/下游/审批/Offer锁内重查Offer状态、原审批全部绑定、撤销、原REAL Package/许可/资格/期限及新鲜readiness，按数据库实际时间采纳。返回HandoffClaimViewV1（原领取元数据与原TargetPackageV1），不开放任意Artifact或执行控制权限。

每个下游external_claim_id只能对应一个原生转移；相同编号/内容的已完成请求返回原回执，不因后来审批/配置/拒绝/TTL变化再次转移。当前机器身份仍须有效且属于原项目/下游；换编号重领同Offer冲突，换Offer复用编号冲突。历史已领取而缺少本入口原回执不能补造新领取。可信Worker每次原生轮询按数据库时间、行锁与SKIP LOCKED至多处理128个OFFERED且expires_at已到的Offer，仅转EXPIRED；领取在行锁后独立检查时间，不依赖清理任务及时运行。原子回滚必须包含状态、转移和回执。

ACK POST `/api/v2/handoffs/{id}/ack` 使用 HandoffAckV1：schema_version=1、external_ack_id（同Claim的1..200 UTF-8字节规则，等于Idempotency-Key）、external_claim_id（已领取时精确原编号，未领取拒绝时null）、outcome=ACKNOWLEDGED|REJECTED、reason_code/reason（同人工决定长度限制）。仅原项目/下游的DOWNSTREAM_ACK机器身份可提交。CLAIMED可转ACKNOWLEDGED或REJECTED，并保留原转移；OFFERED仅在尚未到期时允许REJECTED且不能带领取编号。ACK是事实回执，不重查已转移对象的当前资格/readiness/TTL，不因审批后来撤销丢弃合法迟到回执；当前机器身份仍须有效。每个Offer只采纳一次终态ACK；同外部编号/内容重放原回执，换内容、目标或编号不能重写终态。只记录数据库实际ack时间，不接受客户端回填时间或真实订单字段。client handoff ack使用该合同。

审批撤销 POST `/api/v2/approvals/{id}/revoke` 使用 ApprovalRevokeV1：schema_version=1、expected_latest_revocation_id（首次null，否则原审批最新id）、effective_at（null立即；显式时间须不早于本次数据库时间）、reason_code/reason。APPROVAL_REVOKE人工grant绑定原审批与完整请求。在原项目/Candidate/审批锁内CAS追加不可变撤销，不能删除旧撤销或把生效日推后恢复权限；所有消费者取最早生效日。立即生效时只将该审批仍OFFERED的记录转REVOKED；已领取/ACK/拒绝历史均保留。允许撤销归档或失效项目的旧审批。GET `/api/v2/approvals/{id}/revocations`按id倒序分页读取原记录，权限同审批历史；client approval revoke/revocations复用合同。可信Worker按原行锁批量处理已生效撤销或到期的未领取Offer；Claim与撤销在同一原项目/审批锁序下竞争，不能两者都得到新交付效果。

### A7.2 领取历史与 Forward 报告来源（增量迁移 017）

`handoff_transfers` 复用 A0 的 id/created_at；一条 Handoff 至多一次转移。
新的 OFFERED→CLAIMED 在已取得父行锁、校验数据库实际时钟后，由原生 AFTER
触发器同事务写入 RECORDED_TRANSITION，精确绑定领取元组。OFFERED→REJECTED
不得凭空附带领取字段。合法领取后再拒绝必须保留已有转移事实及已接受反馈，不能回写历史。
新建转移记录禁止使用 LEGACY_CLAIMED_STATE；该来源仅为升级时明确标记的历史回填。

迁移先锁 Handoff/Forward，再审计原行。仅当前 CLAIMED/ACKNOWLEDGED 且领取元组
完整的旧行可生成 LEGACY_CLAIMED_STATE；当前 REJECTED 但带 claimed_at 的旧行不能
独立证明曾领取，必须升级失败并保留原行，待原生下游证据的显式人工核对，不得猜测补造。
旧 Forward 的不可变 created_at 与 received_at 均不得早于所属领取时间，且必须具有
精确 Handoff/Downstream 转移记录。不能因升级时已领取而把领取前写入的反馈洗成有效。

新反馈在父行共享锁内仍只接受 CLAIMED/ACKNOWLEDGED，必须存在精确转移记录；
拒绝、撤销与插入按同一父锁串行化。`report_artifact_id` 必须属于 Release/Candidate
同项目，kind=REPORT、media_type=application/json、schema_name=qz.forward_report、
schema_version=1、origin=REAL、access_class=EVALUATOR_ONLY、byte_count>0。
报告内容及其签发者/版本仍须由实际接入服务验证；关系元数据不能代替真实报告字节或
受保护产品验收。禁止原地改写错误历史、把 FIXTURE 重标 REAL 或向研究/PWA 泄露原始报告。
所有既有迁移保持原字节；不兼容历史令整个升级批次回滚。

## A8. 集成、身份与幂等

### A8.0 原生 Codex 连接与会话适配

Codex固定复用官方0.144.4原生App Server。协议以该版本实际二进制 `app-server generate-json-schema --experimental` 的产物为准；不从新版网页猜测旧版字段，也不把原生stdio握手、model/list或无账号thread/start当真实推理验收。QZ仅编写有界stdio关联、原生结果的非秘密投影、现有Turn账本与领域绑定，不嵌入或重写Codex工具循环、OAuth刷新和canonical聊天存储。

每个连接由可信启动方持有原生子进程、stdin/stdout和单个串行RPC锁；每帧最多2MiB，单次RPC有独立时限，连接只保留至多128条非秘密通知投影。EOF、半帧、超限、错误关联ID或超时都返回结果未知且废弃该连接，不能在相同调用里自动重发写RPC。持久化的Thread/Turn/Run身份与发送意图继续用于恢复。连接关闭只说明本机传输终止，不证明远端科学任务停止或ModelTurn尚未消费；现有Turn的确认和用量账本不得清零。

原生Thread在首个用户Turn进入持久存储前不可resume；仅thread/start成功不代表会话已具备重启恢复能力。启动后、首Turn发送前的进程丢失必须保留已记录的原生身份和未发送事实，不通过创建新Thread或注入history/path伪造恢复。正向恢复验收须让真正的官方App Server完成受控本地Responses Turn，再终止并重启进程，按原ThreadID恢复并验证后续请求携带前次公开上下文；不读取或改写原生历史文件。账户真实推理另在已审查Head的受保护环境验收。

初始化明确关闭原始事件和不需要的reasoning通知；协议包络只按方法名读取允许的状态、身份、原生token计数、登录完成布尔值。忽略字段由原生Serde跳过；不反序列化、不存储、不展示隐藏推理、任意native错误文本、账号token或会话原始items。`thread/turns/list`恢复使用 `itemsView=notLoaded`；会话投影仅含ThreadID、TurnID、状态与可观察配置，不复制turn items。服务器请求不属于本适配器允许的工具或交互时返回JSON-RPC方法不支持错误，不自动授予文件、命令、网络或登录权限。

SYSTEM连接不发送model_provider、base_url、apikey覆盖，由官方Codex读取部署选定的原生HOME/CODEX_HOME及其订阅/配置；“使用默认模型设置”时同时省略model、reasoning effort和service tier，保留但不执行此前保存值。CUSTOM_PROVIDER只通过原生model_providers定义和专用env_key解析当前Vault引用，固定responses线缆；不得在argv、数据库回执或日志中放密钥，不得退回SYSTEM认证。模型和推理强度只接受完整、对应profile revision的原生分页目录；effort-only从原生实际模型观察校验，不能用isDefault猜实际模型。显式模型禁止provider fallback，native报告重路由不能仍标记原选择成功。

远程网页的ChatGPT绑定优先采用原生 `account/login/start {type:chatgptDeviceCode}`：只展示原生loginId、verificationUrl和一次性userCode，完成/取消/注销均复用对应native方法；不接收内部chatgptAuthTokens注入，不自行轮询OAuth端点或刷新token。账号读取仅投影需认证/已配置、认证类型与原生计划类型，不读取或返回auth.json、email、access_token、refresh_token。设置/绑定属于Operator，研究Mission无此权限。模型用量须来自原生Thread累计计数的明确Turn区间或原生Turn回执，不能把工具循环中最后一次请求的last误当整个Turn，未知用量必须保留待对账。


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

控制面 wire/事务细化：机器令牌仅通过单个 Authorization Bearer 头传输，固定 `qz2.<UUIDv7 public_token_id>.<43字符原生随机capability>`；拒绝 query/body 令牌、多个头、Cookie+Bearer 混合、错误Bearer回退Cookie。只有首次签发返回完整token，公开CredentialView不含verifier_ref；重试返回原credential metadata且token=null/replayed=true。随机数/Argon2id/SecretVault复用既有组件，单个请求的密码学验证不代替领域事务的期满/撤销/epoch/归属检查。普通机器事务按 project→Mission run→当前 Attempt→principal→credential 锁顺序复核，写命令使用 principal FOR UPDATE 串行化同一主体。credential_epoch只能保持/增加；enabled变化必须严格增加，重启/重新启用不能复活旧证。

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
  issuer_attempt_id: Id? FK run_attempts  # Mission-only immutable issuance binding
  issuer_owner_epoch: bigint? >= 1  # Mission-only native Attempt owner at issuance
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

Mission 凭据的 `issuer_attempt_id` 与 `issuer_owner_epoch` 必须由受信任发行路径在 project→run→Attempt→principal 的原生行锁下从当前有效租约读取并永久绑定。任何客户端自报的旧 epoch、跨 Run Attempt、已过期租约或非 Mission 的 Attempt/owner 绑定均拒绝。每次机器身份、普通读取和写命令同时复核当前 Attempt、owner_epoch 和数据库实际时钟下的 lease_expires_at；同一 Attempt 的接管只增加 owner_epoch，也必须令旧凭据失效。正常续租保持 epoch 不会使当前凭据失效。023 增量迁移只增加可空发行字段并替换原发行守卫，所有旧凭据的原字段保留；历史缺少 owner 绑定的 Mission 仅作审计，必须重新发行，不能猜测回填为现在的 owner。非 Mission 凭据不因该字段为空而失效。原生 PostgreSQL 锁规则依据 https://www.postgresql.org/docs/18/explicit-locking.html；迁移、接管、到期、续租与锁等待后的时限均用真实数据库验证。

### A8.2 原生凭据引用与集成配置的正式管理入口

`POST /api/v2/settings/credentials` 只创建 RUNTIME、DOWNSTREAM、CUSTOM_PROVIDER 或 TLS_CA 用途的原生 SecretVault 对象。请求的非秘密 intent 为 schema_version/purpose/label；value 为只写、有大小限制的内容，无 Debug/日志/回执。近期 Operator 浏览器或绑定完整 intent 的一次性 CLI grant 才能执行，其他机器身份拒绝。服务器在原有 command transaction 内分配 UUID，由现有 AEAD 实现将该 UUID/purpose 绑定加密并 create_new 发布；不建立另一个密钥库/刷新器/哈希身份。相同键先核对非秘密 intent，再由可信原生解密比较原值，完全相同才返回原对象引用；不同内容409。原文件发布而数据库结果不明时保留对象，不清理可能已引用的秘密。凭据注册返回 id/purpose/label/created_at，不返回原值；配置读取只显示 configured 状态。原生密钥对象属于外部存储引用，不冒充一个可经公开 Artifact API 下载的产物。

Runtime 与 Downstream 配置使用明确的 `/api/v2/integrations/runtimes`、`/downstreams` 集合和 `/{id}`，不开放任意表操作。create/update 分别进入同一 OperatorCommand union；更新要求 expected_revision。Runtime 非秘密配置为 name/endpoint/tls_policy/allowed_capabilities/enabled/development_http，protocol_version 固定当前原生合同1；Downstream 为 name/endpoint/accepted_package_versions/environments/enabled/development_http。配置写入必须验证 SecretVault 引用的精确用途；PINNED_CA 必须有有效原生 PEM CA 引用，SYSTEM_CA 不能混带自选 CA。更新不传新的 credential_ref 表示保留当前版本；转 SYSTEM_CA 明确清除 CA 绑定但不删除旧加密对象。仅部署显式 development-http 且 literal loopback 的端点可以使用 HTTP，生产默认 HTTPS；URL 不接受 userinfo/query/fragment。保存配置不发起网络请求，enabled/声明的 capability 不等于 readiness；后续 probe 必须经部署允许列表与原生 TLS/DNS 绑定，按精确配置 revision 采纳真实结果。

公开配置 DTO 不回传 credential_ref/CA 存储位置，只显示 credential_configured/ca_configured 和实际非秘密配置。Operator 可读配置；DOCTOR_READ 的 CLI/AUTOMATION 只读同一无秘密诊断 DTO，不获得管理或原生对象读取能力。写权限仍为近期人类或一次性完整意图 grant。旧不可变会话/Run 保存其原配置版本，配置更新不能改写已派发任务；当前检查/新准入必须重新判断 revision 与能力有效期。URI 语法、字段/类型/未知字段、原生凭据用途、幂等/CAS、撤销/锁等待、真实 HTTP/数据库和原始命令回执均需回归。此管理入口不是 Runtime 网络或生产完整链路已验收的声明。

### A8.3 Codex Profile 管理与原生目录观测

`/settings/codex` 是 Profile 集合（GET/POST），`/settings/codex/{id}` 提供单项 GET/PATCH；保留 `/settings/codex` PATCH 作为明确携带 profile_id 的当前配置命令，不根据“第一行”选择账号。创建以 name、部署已登记的 home_binding、profile_origin、严格 SYSTEM/CUSTOM_PROVIDER connection 和 SavedModelSettingsV1 为意图。更新绑定 profile_id/expected_revision，只改名称、连接和保存的模型设置；home_binding/profile_origin 是不可变身份，切换原生账号目录须登记新 Profile，不原地接管旧 Thread。一个 home_binding 只能被一个 Profile 占用。公开 binding 仅为1–64字节可打印标识符，不是宿主路径；部署启动文件掌握原生 binary、HOME、CODEX_HOME、working_directory 和显式环境变量名。API 不创建、遍历、复制、chown 或删除原生账号目录，不返回真实路径/环境值。

SYSTEM 请求不得带 base_url/credential_ref；CUSTOM_PROVIDER 创建必须给合法 HTTPS base_url 与 CUSTOM_PROVIDER 用途的不可变 SecretVault 引用，更新省略/null 引用仅在已有 CUSTOM_PROVIDER 时保留。系统不复制 auth.json 或在数据库保存 token。保存任意语法有效的模型/effort 不代表其当前可运行，default=true 保留但不执行这些值。模型目录、实际生效的模型/provider/effort/service_tier 与请求保存值分开显示。

`POST /codex/probe` 明确携带 profile_id/expected_revision，近期 Operator 或绑定完整意图的 CLI grant 可调用。准备事务完成后才启动实际 pinned App Server，读取 account/read 与完整 model/list，并通过无推理 ephemeral Thread 观察原生实际默认配置；显式设置再由原生目录校验及 Thread 响应确认。Fast 仅选择目录实际公告的 priority（或该锁定版本仍公告的 fast）service tier；没有公告则拒绝，不用 isDefault 或字符串相似匹配猜测。default=true 不注入 tier。探测不得发 turn/start、执行研究或触发登录，也不是账号真实推理证明。

探测返回后在原命令幂等事务内再次核对 Profile revision、授权和120秒总期限，写入唯一不可变 codex_profile_observations（id/profile_id/profile_revision/observed_at/valid_until/严格非秘密outcome）与完整命令回执；无原生I/O发生在持锁事务内。Available 目录最多4096个唯一ID，各项有一致 revision/fetched_at；有效期最多60秒。Unavailable 保留明确原因，不制造默认目录；重试同键返回原响应，不再探测。`GET /codex/models?profile_id=...`、`GET /codex/account?profile_id=...` 只读取当前revision最近观测，返回 freshness 与原始 observed_at，不隐式刷新、启动模型进程或把旧成功覆盖最新失败。

### A8.4 Codex 设置的浏览器与 CLI 合同

设置页增加 Codex 专用标签，使用正式 Profile/部署绑定/观测接口。创建和更新在确认后才采纳服务端响应；同一失败重试保留完整意图与 Idempotency-Key。编辑器打开时冻结原配置及 expected_revision，后台刷新不得把旧表单偷偷绑定到新 revision。409 保留用户输入并要求重新载入，不能覆盖其他配置。未保存对话框、凭据登记和待确认探测期间，PWA 更新与设置标签切换均不强制卸载表单。

模型与 Slider 的可选推理强度只能来自同 Profile/revision 且未过期、最近读取成功的完整原生目录。model=null 时使用原生已观察的实际模型决定 effort 能力，不用 isDefault 猜测。Slider 的零位置明确表示不覆盖原生设置，其余位置严格对应目录顺序；目录失效后不能选择新覆盖。历史未知模型/effort/Fast 保存值继续显示并可显式清除，以恢复原生默认，不把故障配置锁死；default=true 仍原样保留这些保存值而不执行。切换连接或换凭据后，旧目录不能用于确认新连接。取消保存不撤销已完成的凭据登记，也不改变历史 Thread。

CLI 的 codex list/show/homes/models/account 只读正式 HTTP 非秘密视图，不隐式启动 Codex；create/update/probe 从 stdin 读取共享严格 Rust DTO，绑定显式 UUID、CAS 与单次 Operator grant。probe 的位置参数和正文 profile_id 必须一致。命令不接任意原生 RPC、HOME 路径、token 文件、shell 或认证模式回退。原生账号登录的后续交互独立于配置保存与无推理探测；未完成登录/推理验收不能因目录探测成功而被宣称完成。

### A8.5 原生 Codex 账号操作

`POST /codex/login/start` 与 `POST /codex/logout` 接收 schema_version/profile_id/expected_revision，均需近期 Operator 或完整命令绑定的单次 CLI grant，只允许 SYSTEM Profile。CUSTOM_PROVIDER 继续使用独立上游凭据，不借登录入口切换系统订阅。先在原有 Operator 幂等事务登记账号操作及202接受回执，再由可信进程取得唯一发送许可并调用 pinned Codex 的 account/login/start(chatgptDeviceCode) 或 account/logout。JSON-RPC ID 不是重试保证；同键重放只读原接受回执，不再次启动登录或注销。

`codex_account_operations` 保存本项目人工操作的 id、profile_id/profile_revision、action=LOGIN|LOGOUT、state=REQUESTED|WAITING|CANCEL_REQUESTED|SUCCEEDED|CANCELLED|FAILED|UNKNOWN、created_at/updated_at/revision、deadline_at、dispatch_started_at?、native_login_id?、cancel_requested_at?、cancel_dispatch_started_at?、finished_at?、reason_code?、account_snapshot?。接受引用与期限不可变，唯一活动 Profile 操作约束避免同时改变一个账号目录。原生 OAuth、token、邮箱、device userCode 与 canonical history 不入库。只有实际原生结果可形成成功/取消；UNKNOWN 不表示账号未变化，也不允许自动重发。

原生客户端和设备码仅在现有部署绑定的有界进程所有者中保留。登录窗口最多15分钟，是本项目等待期限，不冒称 OAuth 设备码有效期。Codex 自行等待授权并保存/刷新令牌；QZ仅消费 login/completed 和 account/read 的允许字段。初始/同键 POST 可向经过原命令授权的发起方返回仍在该进程中的设备码。GET `/codex/login/{id}` 与 `/codex/login?profile_id=...` 仅返回非秘密状态，不启动进程、不返回设备码。浏览器断开不取消已接受的操作，进程重启不伪造会话恢复或删除认证文件。

`POST /codex/login/cancel` 接收 schema_version/operation_id/expected_revision，先持久化取消意图，再由原客户端调用 native cancel。原生 canceled 才标记CANCELLED；notFound、超时或进程终止均不能确认取消，保留UNKNOWN。取消与登录成功竞态保留实际已完成结果，不能抹掉新账号。等待截止也须先登记取消意图再尝试原生取消；尚未取得发送许可的取消可记录CONFIRMED_NOT_SENT。旧操作超出数据库等待期限时，新的明确人工账号命令可将旧未发送操作记为等待失败、已发送操作记为UNKNOWN，再创建新意图；这不是对旧请求自动重放。原生绑定锁未释放时不发新RPC。

开始账号变更即使旧账号/模型观测失效；只有账号操作结束后产生的新探测才能重新证明配置可用。活动操作期间不修改Profile或采纳探测；既有Mission预算、Thread和远端Run不清零或取消。默认单API进程持有账号操作，每个已登记CODEX_HOME复用现有原生客户端串行锁，不另建OAuth服务或分布式认证平台。数据库中断、服务退出或所有者丢失只报告无法确认。受保护的真实账号登录/推理验收与本地协议/状态测试分开，未执行时T07不得标记完成。

设置导航在窄屏使用官方Ant Design Select，桌面使用Tabs，二者共享同一四类设置与未保存操作守卫；不保留溢出菜单造成的错误tablist子角色，也不关闭可访问性检查。

## A9. 索引、保留与迁移核对

必要索引：projects(state,updated_at)；research_cycles(project_id,ordinal DESC)；experiments(family_id,ordinal)；runs(project_id,state,queued_at)；run_attempts(run_id,attempt_no)、活动 lease_expires_at partial index；run_events(run_id,seq)；artifacts(producer_run_id)；input_set_items(input_set_id)；evidence_exposures(root_lineage_id,dataset_revision_id)；evaluations(subject_alpha_version_id,concluded_at DESC)、evaluations(subject_candidate_id)；metric_values(evaluation_id,metric_code,scope)；candidate_alphas(candidate_id,alpha_version_id)；candidate_targets(candidate_id,instrument_id)；releases(candidate_id)；handoff_offers(downstream_id,environment,state,delivery_sequence)；forward_messages(handoff_id,stream_id,sequence,message_revision)；wake_events(state,not_before)。所有 owned FK 有适用 `(id,project_id)` 唯一及复合 FK。

被正式评估/审批/交付引用的 artifacts 默认保留，临时日志/未采纳产物先查引用再清理；审计、trial ledger、sealed exposure 不因清空历史删除。市场目录/缓存由原生工具管理。迁移核对逐类旧新 ID/行数、悬空 FK=0、产物可读率、时间/精度、失败/人工决策/legacy revalidation、未继承权限/审批/凭据；旧 PASS 不是新 qualification；旧实现不在当前源树保留。

# 附录 B：完整接口、状态机、故障测试与 CI

所有路径均为目标合同，不声称旧主干已实现。每行都需要 request/response schema、权限、状态转换、正负测试及代码/CI 证据。服务器生成字段不可由客户端赋值。

## B0. 合同源、版本与持久化

`contracts` 为默认 Rust 的 HTTP/MCP 共用 DTO、错误、事件、政策、产物源，生成 OpenAPI/JSON Schema/TypeScript；批准的 Python 适配消费同一合同，不复制平行真相。Codex 协议从 pinned 原生二进制生成，不发明近似 DTO。HTTP `/api/v2`，远端 `/runtime/v1`，产物 `qz.*.v1`；不兼容改主版本，可选字段按明确兼容策略，生成物提交且 CI diff。引用环、candidate cash/current weights、草稿冻结、readiness、sealed 预约和允许清单规则已完整纳入 A0–A8。

HTTP 客户端原生 Ajv standalone 生成直接引用原始 schema：重复纯 `$ref` 必须复用同一个原生函数，不能为每个路由包装新 schema 反复编译；存在 sibling keyword 的 schema 仍按原始路径完整编译，不因去重丢失约束。回归以实际 Rust OpenAPI 的每个 route/status/media 与独立 Ajv 编译结果比较，并断言重复引用函数相同。静态依赖与应用合同通过原生 Rollup 分包，所有必要静态 chunk 继续预缓存；每文件保留 Workbox 2 MiB 上限，不能靠增大限制或忽略缺失资源绕过构建失败。API/SSE 仍为 NetworkOnly，缓存只包含构建产物。

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
| GET /alpha-versions/{id}/calibration | 附加校准元数据及源版本原Validation，不读取模型 | Operator/精确项目CLI；qz alpha calibration |
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

### B2.1 原生 HTTP CLI 与共同错误合同

实际发行入口为同一 `server` 二进制的 `client` 子命令，不另外创建兼容别名目录或数据库CLI。CLI以固定命令映射复用Rust请求与响应DTO；`--origin`只接受显式HTTPS origin，`--credential-file`读取Unix私有文件中的现有qz2机器凭据，`--ca-certificate`可选择原生CA。仅同时明确 `--development-http` 与字面量loopback才允许HTTP；禁止关闭TLS校验、代理、重定向、隐式重试、任意URL、SQL或SecretVault读取。CLI操作数据库与读取生产Provider凭据不在此入口的能力中。

写入从stdin读取最多16MiB严格JSON，未知字段、错误UUID/十进制版本与不匹配父资源绑定拒绝。每次写入要求用户给定 `--idempotency-key`；受保护管理命令另需 `--operator-grant`，其值仅进入既有 `X-Operator-Grant` Header。`operator-grant`命令以完整OperatorGrantRequest、近期TOTP向正式接口申请单次grant，不取得持久Operator权限，不自动续期。结果未知时只允许用户以原命令/key/正文显式重放，CLI不自动换key、重复发送或把失败写为成功。

API与CLI的Problem/FieldError移动到同一 `contracts::http`；API仍只生成既有封闭错误码/安全字段，CLI严格核对HTTP status、application/problem+json、UUID/Revision与同一RustDTO。输入、原生传输错误和不合合同的远端响应只打印封闭本地错误，不回显凭据、stdin、宿主路径或native错误。成功JSON写stdout；已验证Problem写stderr并退出1。产物导出先读取同一ID不可变元数据，随后核对正式content接口的media和精确byte_count，输出原始字节，不把Rust源码错误当成octet-stream。

Run watch复用 `eventsource-stream=0.2.3` 的原生SSE分帧，来源：https://docs.rs/eventsource-stream/0.2.3/eventsource_stream/ 。每次最多3600秒/10000事件/16MiB线缆字节，单事件不超过已有公开合同，原生cursor、run_id、seq和event_type必须一致且单调。兼容未知event_type保留公开envelope及cursor，不猜业务状态；reset-required要求重读。客户端结束、Ctrl-C或断线只输出最后cursor和 `cancellation_requested=false`，不调用取消接口。SSE不是第二套事件数据库或重连调度器。

原生子进程+TCP回归必须覆盖请求/响应DTO、精确header、未知结果显式重放、拒绝重定向/重复JSON/反射凭据、原生SSE及bigint；另以实际Axum+TOTP+PostgreSQL演练CLI获得单次grant、登记Source、回执重放、冲突和权限拒绝。HTTP fixture的成功不代替真实数据库/完整研究闭环，尚未实现的B2命令仍是同一Issue62的后续必交模块，不允许隐藏为已交付。

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

### B3.1 原生 stdio MCP 入口与只读 Mission 边界

`quazonai mcp` 复用官方 `rmcp = 3.2.0` 的 stdio framing、初始化、工具路由、参数校验与取消；不实现另一套 JSON-RPC，不向 Agent 开放 HTTP 代理或数据库连接。入口不加载 DATABASE_URL、STATE_DIR、SecretVault、Operator session 或 Provider 凭据。启动参数由可信 Mission launcher 传入：`api_origin, project_id, cycle_id, run_id, attempt_id, brief_id, development_http`；五个身份必须是已有合同的 UUIDv7，不能来自工具参数。独立范围受限机器能力仅从 `QUAZONAI_MCP_TOKEN` 环境变量读取，拒绝空值/非法原生 token；不提供 token 命令行选项，不将其写入日志、错误、MCP 内容或子进程。启动参数不是授权事实，必须向现有 `/auth/machine` 与精确 Run 接口重新验证。

启动与每次工具调用均验证：机器类型严格为 MISSION，project/run 与启动绑定一致，downstream 为空、凭据未到期；Run 为该项目/周期的 AGENT_RESEARCH，active_attempt_id 与启动 attempt 一致，状态为 DISPATCHING/RUNNING、deadline 未过。必须同时具有 RUN_READ 和 RESEARCH_READ；撤销、认证失效、身份/Attempt变化和不兼容响应拒绝调用。机器/API 事务的原生授权仍是最终事实源，不能因为 MCP 缓存过一次成功就跳过。MCP 同时最多4个调用，单次总时限最多15秒且不晚于启动时读取的 Run/凭据到期时刻；到期时结束 stdio 服务，不因断线取消远端 Run。

origin 只能是无userinfo/query/fragment/额外路径的 HTTPS origin；HTTP 仅在显式 development_http 且 host 为原生 IPv4/IPv6 loopback 时允许，不能用任意主机名作开发豁免。HTTP 客户端禁止重定向、环境代理、Cookie 与自动重试，连接超时3秒、单请求超时10秒；读取过程累计限额1MiB，不依赖 Content-Length。请求地址只能由固定路由和已验证 Id 构造。stdio 输入在交给原生 SDK 前使用 Tokio AsyncRead 的累计8MiB会话配额，避免对端不发送换行时无限缓冲；这是整个连接的字节额度，不是单帧或模型 token 预算，不另造 JSON-RPC parser。stdout 只用于 SDK 协议，结构化日志移到 stderr；默认关闭 SDK/HTTP 正文跟踪，只保留服务自身安全日志。失败返回封闭安全错误码/HTTP状态，不回显上游原始正文、URL、请求头或底层错误文本。

Attempt fencing 不得只在 Artifact 写入或 MCP 配置层实施：统一 `authority::machine` 按 A8.1 的 Project→Run→Attempt→Principal→Credential 原生锁顺序，检查 `runs.active_attempt_id` 与不可变发行绑定 `issuer_attempt_id`、`issuer_owner_epoch`，并在锁等待后以数据库实时钟验证当前租约。所有 MISSION scope 都要求当前 Attempt 与发行 Attempt 非空且相同、当前 owner_epoch 与发行 owner_epoch 相同且租约未到期；同一 Attempt 的接管也令旧进程的读取、自省和写入失效，不能通过普通 HTTP 绕开 MCP。历史缺少任何绑定的签发原样保留审计，但不构成当前授权，不推断回填当前 Attempt/owner；可信任务服务为当前有效租约重新签发。CLI/AUTOMATION/DOWNSTREAM 的非 Mission 语义不改变，公开 DTO 不暴露内部发行绑定或秘密。

首批实际接入的工具是 `research.get_brief{brief_id}` 与 `run.get{run_id}`：请求严格拒绝未知字段，Id 的 JSON Schema 直接复用 `contracts::Id` 的原生 schema。Brief 只能是启动绑定的同项目版本，state=FROZEN 且 frozen_at 存在；不能以 DRAFT 或另一个有效 Brief 代替已冻结任务。Run 只能读取绑定 Mission，返回现有 RunSnapshotV1，不能替客户端猜百分比或任务成功。工具只返回已反序列化的公开 DTO，未知字段/合同版本不兼容明确失败。未完成的 B3 工具不登记为假成功/空实现；本入口不是完整 W2/W3/T01–T42 的验收替代，实验提交、科学任务、证据披露、原生 Codex 闭环及其全部隔离仍必须在同一 PR 完成。

回归必须包括官方 SDK client 的实际 stdio/duplex 初始化与 tools/list/call、未知工具与未知字段、非UUIDv7、错 Mission/项目/周期/Attempt、非冻结 Brief、撤销后下一调用失败、截止时间、并发上限、响应超额、重定向不跟随与无秘密错误。协议/HTTP故障测试可以使用有明确标记的测试服务，但不能称为 PostgreSQL授权或原生 Codex生产闭环证明；真正授权链另外以实际 Axum/原生 PostgreSQL/Mission issuance 测试验证。依赖锁与生成物只能由原生工具产生后检查，禁止手造 registry checksum。

### B3.2 受限研究写工具与工作区文件能力

`artifact.submit` 与 `experiment.propose` 调用现有 HTTP 产物/实验服务，不直接写数据库，不创造另一个上传或幂等机制。工具参数严格拒绝未知字段；`artifact.submit` 接 `schema_version=1, kind=CODE|PARAMETERS|REPORT, workspace_relative_path, idempotency_key`，`experiment.propose` 接 `idempotency_key, proposal:ExperimentProposalV1`。提案的 cycle_id 必须等于启动绑定；服务端仍重新核对当前 Mission/Attempt/租约、项目、Family、产物与预算。创建成功仅代表产物或提案发布，不是科学运行成功、REAL、PASS、Alpha 或 Qualification。

文件根只由可信 launcher 的 `--workspace-root` 绝对路径传入；未配置时文件工具明确返回 MCP_CONFIGURATION_INVALID，不猜当前目录。客户端不能提供根路径、URL、环境变量、身份或token。`MissionFiles` 持有原生目录文件描述符，根打开时 O_DIRECTORY/O_NOFOLLOW；内部复用 rustix 的 `openat`，逐个单路径组件相对于仍持有的父目录描述符，以 O_DIRECTORY/O_NOFOLLOW/O_NONBLOCK/O_CLOEXEC 打开中间目录，最终以 O_NOFOLLOW/O_NONBLOCK/O_CLOEXEC 打开普通文件。此入口以 rustix 的逐组件 `openat` 明确实施拒绝软链，而不依赖高层目录库如何解释 custom flags。必须分别测试根内软链和越界软链；原有回执未证明旧实现的根内软链反例，不能将推测写成已复现失败。拒绝绝对路径、空组件、`.`/`..`、反斜杠、控制字符、任意隐藏组件（含 .git/.env）、超过32层或512字节的相对名，拒绝软链、非单链接文件、FIFO/设备、空文件、非UTF-8或超过2MiB的文件。读取只使用已打开句柄，复核长度和原生修改时间；根目录被替换不转向新根。此处的时间复核只是发现并发修改，不作为不可变业务身份或内容证明；真实发布仍由 ArtifactStore 和 HTTP 事务完成。

每次写调用先重验 authority；文件读取前检查 ARTIFACT_SUBMIT，提案前检查 EXPERIMENT_SUBMIT，文件读取后、网络提交前再检查当前 Mission 与截止。非可取消 blocking I/O 使用独立4槽持有到实际读取结束，不因工具超时释放其文件读取容量。累计响应仍限1MiB，传输禁止重定向/代理/自动重试，POST只构造固定 `/api/v2/artifacts` 或 `/api/v2/experiments`。原生 idempotency_key 形状与普通 HTTP 相同；客户端结果不明时保留原key重放，不能自动换键。仅接受201及严格CommandResult DTO；产物须精确project/run/attempt、AGENT/SYNTHETIC/RESEARCH和对应schema/字节长度，提案须精确cycle/family/原始字段及CODEX作者绑定、初始PENDING且没有科学结果。异常响应不能当成功或直接回显上游正文。

tools/list 中登记的写工具都有真实 HTTP 实现；没有所需机器scope、未配置工作区或当前任务失效时清楚拒绝，而非成功返回空对象。原有读工具不因新增写工具取得Operator能力；approve/publish/handoff.claim/db.query/secret.read/http.fetch_any仍不存在。对应测试分别证明原生SDK协议、真实文件句柄安全和真实Axum/PostgreSQL/加密发证的工具写事务；关系fixture用于构造父Cycle/Run时须明确标识，不冒充T42全新业务入口或原生Codex完整研究闭环。

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

### B4.1 控制面到 Runtime 的固定出站边界

部署为每个 Runtime 显式登记 HTTPS origin 与允许连接的原生 SocketAddr 列表；此列表不是浏览器可修改的配置，也不来自 Agent/JobSpec。适配器复用 reqwest 0.12.23 的 resolve_to_addrs、原生 TLS、redirect::Policy::none 和 retry::never；保留原 Host/SNI，仅连接部署批准的地址，不再做第二次系统 DNS 查询。禁止环境代理、Cookie、自动重试和压缩解码；总请求10秒、连接3秒、响应累计1MiB。拒绝未登记origin、userinfo/query/fragment、额外路径、metadata/link-local/multicast/unspecified地址、IPv4-mapped旁路；显式开发部署只允许literal-loopback HTTP。秘密用敏感Authorization头，仅在端点与TLS检查后交给原生客户端；错误不回显URL、响应正文或原生诊断。明确批准的同机loopback HTTPS同样可用，仍必须通过原Host/SNI与CA验证，不需要把生产服务切换为HTTP开发模式；允许列表之外的loopback与所有metadata目标继续拒绝。此内部适配器不成为Agent任意HTTP工具。

Run admission 的原生 RuntimeSnapshot 同时冻结 ca_certificate_ref 与 development_http，接管使用原快照而非后来修改的配置。历史缺CA快照只用于审计，不得推断CA或降级信任。020迁移保留历史PINNED_CA记录，以NOT VALID检查避免凭空制造CA；所有新写入/变更仍必须满足绑定，历史不完整配置不能准入真实传输。

### B4.2 Runtime 探测发布与场景准入

`POST /api/v2/integrations/runtimes/{id}/probe` 接收 schema_version/expected_revision；命令是近期 Operator 或精确单次 CLI grant 的 RUNTIME_PROBE，目标为 Runtime 而不是客户端指定的观察ID。前置短事务重验权限、原始幂等回执、enabled和revision，产生仅供可信服务持有的探测票据；网络和原生TLS在事务外执行。后置事务再次验证授权/epoch/expiry、精确配置revision和20秒总票据期限，只发布真实原生响应或封闭失败原因，不接受客户端自报capabilities。解码后的键和值也不得包含出站凭据，重定向和原始错误响应不披露。失败同样形成不可变观察，不沿用上一条成功。

观察保存到 runtime_probe_observations，并以同事务绑定真实 ArtifactStore 已发布的 qz.runtime_probe/1 原始文档。该产物是 Operator 范围的运行环境观察，不是研究评估、Alpha资格或目标Package。观察有效期固定为探测开始后60秒，原始回执重放不能延长期限。Runtime 自报 checked_at 不得早于这次探测开始5秒以上。配置表的 last_capability_snapshot_artifact_id 仅是观察指针：刷新它不改变 integration_revision；真正配置变更增加revision并清空指针。旧的不可变观察继续可审计，但不得供新配置准入。只读 readiness 返回 NOT_CHECKED/DISABLED/STALE/UNAVAILABLE/AVAILABLE、准确版本、最近观察和 configured∩observed 的 available_job_kinds；连接可用但交集为空时没有任何任务准入资格。

### B4.3 原生任务线协议、不可变输入复制与恢复边界

本节细化 B4，而不建立第二套领域队列、Agent Harness 或资格判断。JobSpecV1/ResultManifestV1 使用 Rust Serde+utoipa 生成；未知字段拒绝，任务 JSON 和结果 manifest 各最多1MiB。external_job_id 必须精确为小写规范 UUIDv7 run_id + `/` + 无前导零的正 u32 attempt_no；HTTP 客户端通过原生 URL path-segment 编码，将整个 external_job_id 作为一个段，不拼接客户端路径。所有 DB 时间为 UTC 微秒。原生 Serde 逐项检查原始响应的解码键和值，拒绝重复键（包括转义后相同的键）及凭据反射；不得先折叠成 JSON map 后遗漏前一个值。

JobSpecV1 完整字段是 schema_version=1、run_id、attempt_no、首次发送的 owner_epoch、external_job_id、job_kind、固定OCI image_ref、input_set_id、按冻结 ordinal 排序的 inputs、parameters_artifact_id、limits、deadline_at、requested_output_schemas。RuntimeInputV1 仅有 DATASET(revision_id,registered_ref,storage_version,role) 或 ARTIFACT(artifact_id,storage_version,byte_count,role)；registered_ref 必须命中部署目录注册表，不是 URL 或宿主路径。请求不接受 command、environment、任意挂载、workdir、Docker socket、上游 bearer 或资格状态。parameters_artifact_id 是可信 Worker 根据已冻结对象建立的不可变执行配置；可以独立于冻结 InputSet 的研究成员，但必须被精确 Run/Attempt 的发送记录绑定，不能追加/改写已冻结 InputSet。若它也在 inputs 中，其 role 只能是 PARAMETERS。外部接管仍对账原始 JobSpec，不以当前 epoch 或新配置改写旧发送意图。

RuntimeJobLimitsV1 为 cpu:u16[1,1024]、cpu_seconds:正 DbCounter、memory_mib:正 u32、wall_seconds:正 u32、output_bytes:正 DbCounter。cpu_seconds 不超过 cpu*wall_seconds；运行时以原生 CPU 配额及独立墙钟限额限制整个任务，实际越额保留为失败证据，不把观测裁回预约。输出当前实现上限64MiB，单个研究输入对象上限64MiB，总研究对象输入上限256MiB，inputs1..256，输出 schema1..64且(name,version)唯一；部署能力可以更小，不能由模型放大。方法、数据、图像及准入依赖真实 capability，不从类型声明推定可运行。

为不依赖控制面与远端共享宿主目录，增加仅可信服务可用的 `PUT /runtime/v1/objects/{artifact_id}`：Content-Type 固定 application/octet-stream，`X-QZ-Storage-Version` 是1–120个非空白可打印 ASCII 字节的原生不可变版本；二进制体严格有界，原始对象身份/版本/完整字节相同幂等返回，任一冲突409。成功200/201返回 RuntimeObjectReceiptV1(schema_version,artifact_id,storage_version,byte_count)，不是自由 JSON。上传者不能指定宿主路径、对象后端或原生输出 origin；每任务只装入精确 JobSpec 的对象和可信参数，不能因对象已在远端就给另一个任务读权限。输出增加 `GET /runtime/v1/jobs/{external_job_id}/artifacts/{storage_ref}`，storage_ref 为 UUIDv7，必须属于该任务已封口 manifest，不能用 URL/bucket 名/跨任务对象取代。以上不是浏览器/研究 Agent 的任意文件接口；对象复制同样受服务认证、大小、数量、磁盘可用量和任务授权限制。

RuntimeJobStatusV1 包含 schema_version、run_id、attempt_no、external_job_id、state=ACCEPTED|RUNNING|CANCEL_REQUESTED|SUCCEEDED|FAILED|CANCELLED、has_result、submitted_at、started_at?、finished_at?。终态与 finished_at 必须对应；成功必须有已开始时间和真实结果。取消请求 RuntimeCancelV1 包含 schema_version、run_id、attempt_no、owner_epoch。取消要先持久化原生身份的撤销记录，再使已经建立的原生任务停止；未见过的身份也建立永久 tombstone，迟到 POST 不得复活。只有已经停止或明确永久不能再启动才返回 CANCELLED；单独 GET404、HTTP 超时、断线或本进程失去 lease 均不是这种证据。已终态的精确任务重放原终态，完成/取消并发只采纳一个结果。

ResultManifestV1 为 schema_version、run_id、attempt_no、external_job_id、input_set_id、state=SUCCEEDED|FAILED|CANCELLED、真实 engine_versions、started_at?、finished_at、resource_usage、artifacts、error?。resource_usage 的 wall_milliseconds/output_bytes 为 DbCounter，cpu_nanoseconds/peak_memory_bytes 可空；缺精确最终观测保留 null，不补0。output_bytes 指实际发表输出载荷之和，不包括 manifest 本身；控制面采纳时另将 manifest 原生字节计入 Run 总产物额度。每个 RuntimeOutputV1 包含 kind=MODEL|SIGNALS|TARGETS|REPORT|METRICS|DATA_QUALITY、schema(name,version)、storage_ref:Id、storage_version:Revision（当前为1）、byte_count、media_type；不包含用户可自选的 origin、access_class、资格或审批。必须唯一、实际大小合计吻合、种类/媒体类型/请求 schema 匹配。成功需要全部请求 schema 和真实输出，错误/取消不能带可发表的科学输出。

错误是封闭 RuntimeFailureClass/RuntimeFailureCode 及由 code 唯一导出的静态 safe_message，不接受上游 stderr、请求体、堆栈或凭据反射。Worker 除严格反序列化，还要按精确 Run/Attempt、原 input_set、原生镜像/方法版本、当前授权、实际对象字节/schema/来源与资源约束验证后才进入 Store 的 fenced 采纳事务。解析 manifest 不等于校验真实 Arrow/JSON/Wasm 或授予 PASS。成功进程可以产生 INCONCLUSIVE/REJECT 科学结论，资格服务独立决定。

控制面继续使用 PostgreSQL/PGMQ/Store 的发送意图、租约、结算回执和 ACK 顺序。远端持久化只管理原生 job 的身份/撤销/结果，以成熟嵌入数据库事务和原生 OCI container ID 恢复；不增加第二套研究预算、工作流或任务队列。整个原生任务只创建/启动一次，crash-after-submit 先查原身份。合同测试只是边界回归，必须另有真实 OCI、网络/文件/资源隔离、崩溃恢复和完整 Web/CLI 证据后才能勾选 T01/T24/T34/T35/T42。

### B4.4 远端原生执行器与持久身份

`apps/runtime` 使用既有 Rust Axum/Tokio/Serde 合同及 SQLx 0.8.6 的 SQLite 后端，原生 Docker API 复用 Bollard 0.21.1；不建立第二套研究业务数据库、预算或工作流。SQLite WAL + synchronous FULL、短 `BEGIN IMMEDIATE` 事务管理远端身份、不可变有界对象和终态；宿主原生文件锁保证同一状态目录只运行一个执行监督进程。部署根与 Docker Unix socket 由操作者明确配置，不接受网络命令指定路径、环境、命令或挂载。参考原生 API：<https://docs.rs/sqlx/0.8.6/sqlx/struct.Pool.html#method.begin_with>、<https://docs.rs/bollard/latest/bollard/struct.Docker.html>、<https://docs.rs/bollard/latest/bollard/models/struct.HostConfig.html>。版本以 Cargo.lock 和实际原生验证为准，不用浮动 latest 构建。

网关的 `runtime_meta` 只保存服务器分配的 instance_id；`runtime_jobs` 以 canonical external_id 为主键，同时唯一 run_id/attempt_no，保存不可变 spec_json、首次 owner_epoch/submitted_us/deadline_us、一次写入的原生 ContainerCreateBody/容器ID/START intent，以及单调取消owner、取消时间、停止原因、隔离屏障ID和唯一终态/manifest。内部 phase 为 QUEUED/CREATING/CREATED/STARTING/RUNNING/TERMINAL，公开状态仍使用 B4.3；不把内部phase作为研究状态。`input_objects` 保存UUID/原生storage_version/精确BLOB/byte_count，同身份版本与原始字节才重放；`job_outputs` 以 external_id/storage_ref 唯一并原子保存元数据与原始字节，全部输出、manifest和终态同事务发表。64MiB单对象、256MiB研究对象总输入、1MiB请求/manifest及部署磁盘总额度分别检查；满额明确拒绝，不自动删除引用或terminal tombstone。SQLite BLOB事务代替额外的文件与元数据提交间隙，不新增应用内容hash。

网关先打开配置及持久journal，使Docker停机时仍可读取原任务、接收取消tombstone和幂等重放；这不表示执行readiness。实际引擎调用首次通过Bollard原生API版本协商建立客户端，失败不缓存假成功，不改用另一个socket或放宽权限。服务的doctor/capabilities检查必须独立执行，未通过不能接受新任务。调度仅读取最多4096条小型身份/phase/deadline元数据，不把全部大JobSpec装入内存；配置降低并发或pending上限不能隐藏已有任务/取消/到期记录。

每项 JobKind 只映射部署登记的digest固定镜像和固定 `job execute` 入口。可信配置将已有不可变对象及原生目录挂载到固定 `/input`，仅该Job的输出目录可写。原生镜像、协议与方法版本先经Docker inspect核验才公布能力；缺引擎/镜像/实现的JobKind不能虚报AVAILABLE。数值、目录、预测、优化和模拟继续由独立job中的Nautilus/Clarabel/Arrow/Wasmi等真实组件完成，网关不嵌入第二计算引擎、不授予科学PASS。

持久CREATE/START意图先于原生外部调用；不持SQLite事务等待Docker。恢复只认原生container ID和精确实例/Run/Attempt标签，不因启动ACK丢失新建身份，也不重启已退出容器。已持久化START但无法证明请求未发送时，不再次START：先按原身份恢复或安全终结，再由控制面的有界基础设施重试政策裁决。取消先保存tombstone/owner；普通404不能证明晚到CREATE/START不会发生。已存在或可能正在创建的任务，确认停止并移除其旧container ID，再以相同原生名称建立永不启动的屏障容器，才可封口取消；晚到CREATE冲突、晚到START旧ID失败。未知结果保持待对账，不谎报CANCELLED。终态不可更新，进程重启和迟到重复请求不能复活身份。

镜像引用允许登记仓库原生 `name@sha256:<64hex>`，或同一Docker引擎原生 `sha256:<64hex>` image ID；两者均由Docker解析不可变镜像，绝不接受tag/短ID或由应用计算替代身份。后者用于同机已构建镜像与原生CI，无需为本机测试另建registry。Job使用non-root、只读rootfs、无网络、cap-drop、no-new-privileges、固定只读输入、受限临时空间、PIDs/CPU/内存/文件/墙钟/输出额度；仅可信网关持有Docker socket。固定 `job run-bounded` 在实际启动时从只读JobSpec计算剩余绝对deadline和wall_seconds，exec镜像内GNU timeout，再运行同一job execute；复用原生timeout和Docker init/cgroup收束子进程，Gateway退出不撤销已运行任务的墙钟限制。停止容器不会重启，接管不再次发送START。原生资源观测缺失保留null，实际超限保留失败，绝不裁成成功。原生结果回到控制面仍必须做精确关联、实际字节/schema、授权和独立评估；Runtime终态成功不是Qualification。

原生SQLite并发/事务/重开、真实HTTP及真正Docker隔离/取消/重启测试分别验收。开发环境没有Docker权限不允许跳过后宣称成功；在精确Head的独立Docker CI运行相同测试，结果缺失或失败继续阻塞交付。此细化不删除其余W0–W8/T01–T42，Gateway、Worker、真实研究、完整Web/CLI与恢复链均须实际完成。

### B4.5 Runtime 线合同与探测回收补充

针对1649b4d7的原生review，Runtime输出以同一Rust登记表绑定schema名/版本、kind与media_type；返回qz.data_quality却标REPORT不再合法，未知或未接通的输出schema明确拒绝。已支持的输出原生storage_version固定字符串`"1"`，1字节至64MiB的output_bytes边界与原生DbCounter使用相同的生成器形成精确字符串schema，不能仅发布PostgreSQL bigint总范围。范围生成不增加第二个数值解析器，运行校验仍复用原生整数/领域上限。

仓库OCI名称由固定`oci-spec 0.10.0`的distribution::Reference解析，复用官方Docker distribution语法；只允许带完整小写sha256 digest的引用，拒绝URL、相对目录、空组件、标签单独引用与无效仓库名。同机Docker的原生完整sha256 image ID仍按其固定原生形状单独接受；不把它作为分发仓库名解析、不计算新的业务散列。上游API与feature依据：<https://docs.rs/oci-spec/0.10.0/oci_spec/distribution/struct.Reference.html>、<https://docs.rs/crate/oci-spec/latest/features>；仅启用distribution，不重建OCI语法或引入另一个容器执行平台。

探测发布失败或发布后票据过期时，原始事务先结束；可信服务对本次确实分配并尝试发布的精确快照ID，在新的短事务重新获取Operator命令的同一全局authority行锁，然后在主库确认没有app.artifacts引用，才通过原生ArtifactStore删除该未引用对象并同步目录。未知提交若已成功，则原生记录存在，必须保留；无法重获锁或判断时保留而不猜测删除。不开放客户端指定ID/路径的删除接口、不扫描或删除其他业务产物，也不把“回收失败”改写成探测成功。重复同键重放不发布另一个对象；明确失败的回收允许幂等重试。

### B4.6 固定科学子任务与编译隔离

`NativeTaskParametersV1` 以 operation 作封闭判别，参数由冻结Parameters产物读取：COMPILE_MODEL(code_artifact_id)、VALIDATE_DATA(selections[dataset_revision_id,NativeBarSelectionV1])、EVALUATE_ALPHA(dataset_revision_id,model_artifact_id,NativeForecastRequestV1)、BUILD_PORTFOLIO(AllocationInputV1)、SIMULATE_PORTFOLIO(dataset_revision_id,NativeSimulationRequestV1)。不得用任意Shell、环境变量、宿主路径、动态模块或URL扩展operation。编译及目录验证属于DATA_VALIDATE准备任务，其余分别映射ALPHA_EVALUATE/PORTFOLIO_BUILD/PORTFOLIO_SIMULATE。操作、JobKind、完整输入成员及输出schema在准入、材料化和job入口都以同一Rust领域规则核对。

Rust预处理本身可读取文件，因此COMPILE_MODEL必须是无数据目录的独立Run：只挂该Code与该Parameters，禁止任何额外Dataset、Model、其他研究产物或Sealed。固定rustc 1.98.1/wasm32-unknown-unknown和纯predict ABI；编译无Cargo依赖、build.rs、网络或用户可选参数，失败不生成可信MODEL。后续EVALUATE_ALPHA使用已发布的MODEL与精确授权目录，Wasmi无导入的实例拿不到编译器、环境或标签；不能把“编译器和Sealed数据在同容器但没主动读取”当隔离。一次实验的trial预约只在首个阶段计入，编译失败仍保留该trial，后续阶段不得再计同一个trial或绕过累计CPU/输出预算。

固定job入口 `job execute` 默认读取运行时提供的/input/spec.json、/input/objects/UUID和/input/catalogs/datasetUUID，输出仅位于本次/output。与既有forecast/simulate本机入口一致，受信任本机诊断/原生子进程回归可显式给 `--input-root` / `--output-root`；它们不是HTTP/MCP字段，Gateway固定传递默认挂载并禁止JobSpec替换命令或参数。大型输出原始字节以服务器分配UUID写入，逐次写入累计检查output_bytes，最后发表唯一NativeJobOutputIndexV1；索引至多1MiB、64个产物，未封口索引不能作为成功。编译输出qz.wasm_model与qz.model_compilation；目录验证qz.data_quality；预测qz.native_forecast；优化qz.native_allocation；模拟qz.native_simulation，均明确v1。优化INFEASIBLE保留真实诊断与空目标，不变造等权目标。原生预测含标签的报告为受限科学输入，不冒充带AlphaVersion/单位/可见时间语义的qz.alpha_signal.v1；后者必须由独立受信任科学发布服务验证和形成。

### B4.7 原生 Runtime 审查修订：物化配额、终态与认证合同

SQLite 中的输入/结果 BLOB 配额不能漏掉执行目录中的副本。增量迁移增加仅用于原生磁盘占用的 `materialization_reservations(external_id PRIMARY KEY REFERENCES runtime_jobs, byte_count>0, reserved_us)`；它不持有研究预算或工作流。首次任务提交在同一原生 SQLite 写事务内，同时为最终输出BLOB和精确 JobSpec 的全部复制输入、spec、输出上限与1MiB索引预约磁盘额度，再返回ACCEPTED；不足时整个准入回滚，不留下永远无法物化的QUEUED任务。物化、重放及升级恢复复用同一个计量函数和原有预约；不能等返回接受后才首次发现可预计算的配额不足。该额度与已存输入、输出 BLOB 及未完成输出预约共同计入 storage_quota_bytes。只有已确认唯一终态、结果与实际输出已在 SQLite 原子封口，才可回收该任务自己的物化副本；原始输入BLOB、正式输出BLOB、manifest、Run身份、取消屏障和tombstone全部保留。先完成原生文件删除/fsync，再删除预约，失败/提交不明保留占用并在恢复时重试，不能先释放额度后猜测删除。新物化采用确定性的原生run/attempt暂存名，失败或重启不能积累无限随机暂存副本。升级恢复在独占状态目录下识别现有任务的副本与原始spec；未知或不可验证目录保留并明确阻塞自动回收，不把用户文件当垃圾。

取消请求不改写已经发生的原生失败。处理已退出容器时，在删除容器前保存真实退出/OOM/超时原因及原生完成时间；若该失败发生于 cancel_requested_us 之前，则屏障建立后的终态仍为 FAILED，而不是 CANCELLED。故障事实、开始/结束时间与失败原因使用同一原生journal不可变记录恢复，不能因网关在删除容器后崩溃丢失证据。成功/失败/取消仍只发布一个终态；对运行中任务发出取消后的信号退出不能倒推成先前失败。

所有 `/runtime/v1` 操作的原生 OpenAPI 明确声明 `RuntimeBearer` HTTP bearer security scheme、必需的单一认证头和401 RuntimeProblem/application-json响应；不声明匿名或Cookie认证。缺失/重复/混合Cookie凭据均由同一真实中间件拒绝，生成合同与逐路由原生HTTP回归一致。

原生目录请求不仅绑定registered_ref/version/partition，还必须逐项绑定质量报告中已经登记的完整BarType及Instrument集合。VALIDATE_DATA、EVALUATE_ALPHA、SIMULATE_PORTFOLIO都在挂载前校验；请求decision_cutoff_ns不得晚于该登记质量快照的decision_cutoff_ns，事件区间及cutoff可收窄但不可扩张。目录后来出现的其他品种或超出登记可见截止的迟到记录不能因目录路径相同而获得授权。原生查询同时限定事件区间与当时可见截止，先由上游查询引擎过滤，再执行maximum_rows解码上限，不能把区间外较新记录计入请求行数，也不能丢掉区间内迟到但在cutoff前已可得的记录。预测预热行没有成功predict调用，future label和label_available都必须为null，label_reason为INDICATOR_WARMUP；仅成功预测后才形成未来标签，样本末尾真正未完成的标签另用LABEL_NOT_COMPLETE。

原生镜像组装使用已打开文件描述符校验/复制依赖，目标create-exclusive，不以先exists/stat再按路径读取作身份依据；遇同名已存在对象只按已打开描述符逐字节核对。工具链内动态库解析使用该固定工具链的真实库目录，不忽略ldd的not-found；生成准备目录与实际Docker build/run分别记录，准备完成不能冒充OCI运行通过。失败回执保留对应原生ldd输出和命令，不上传整份工具链、镜像根或秘密。

## B5. 事务与状态机

### B5.0 正式研究冻结与周期启动

冻结允许非归档项目，成功时同事务将本 Brief 设为项目当前 Brief，但不自动启用项目。浏览器流程为草稿保存→冻结执行上下文→明确启用项目→明确选择两个角色的配置→启动；不能要求项目先 ACTIVE 才允许冻结，从而与激活的冻结 Brief 前置条件相互阻塞。页面复用项目编辑器，不另建隐式激活命令。

Brief freeze 使用严格 schema_version/expected_revision/execution_context；execution_context 包含 runtime_id/runtime_revision 和同项目的 discovery_input_set_id/validation_input_set_id/sealed_input_set_id。三个输入必须已冻结、角色及 Dataset 集合与 Brief bindings 完全一致；validation 输入必须就是冻结 SelectionRule 的 comparison_input_set_id，三者 decision_cutoff 一致。冻结事务锁定当前 Operator/项目/Brief，复用现有数据授权重检、验证 Family/Policy/Universe/ExecutionAssumptions、预算与 horizon，读取精确版本的尚有效原生 Runtime capability。需要 REAL/PIT 的政策不能冻结未知来源或未核验数据；原生 label interval 不支持时明确拒绝，不靠自报指标补齐。成功将 execution_context 与 Brief 同事务封口，冻结后更新内容或上下文均禁止，只能新版本。

Cycle start 使用 schema_version/brief_id/expected_revision（Project revision），以及必填 researcher_profile/reviewer_profile（各为 profile_id/expected_revision），只接受 ACTIVE Project 与属于它的 FROZEN Brief。启动事务锁定并核对两个 Codex Profile 的精确当前版本、已登记原生绑定和无进行中的账号操作，冻结在不可变启动关联及原始回执中，不选“第一个账号”或静默采用新版本。两角色可以显式选同一个 Profile，但必须使用不同的持久 Thread，Reviewer 不继承 Researcher 的聊天上下文。Profile 选择属于本次 Cycle，不污染可复用 Brief；历史启动记录保留空绑定，不补造账号或启动新 Mission。启动不以60秒探测缓存替代实际 Mission 的原生连接检查；后续发现 Profile 已修改则停止新模型调用，明确要求新 Cycle，不把旧选择指向新配置。

重新检查被冻结输入的当前许可及原生 Runtime 当前 readiness，不把 freeze 当永久许可。首个 Run 为 DATA_VALIDATE：通过真实受限 Runtime 再确认登记数据可执行后，由 Worker 进入 Codex Mission；这一步不是另一条研究路径，也不产生 Alpha/PASS。Cycle、budget snapshot、Run、Event、PGMQ消息、启动关联与原始HTTP回执在一个 SQLx/PGMQ 事务中提交，任何后半步失败均回滚；HTTP202返回准确Cycle/Run身份和两个 Profile 选择，不允许业务手工SQL补父对象。人工开始和自动唤醒统一受每日周期额度约束，自动入口另受政策cooldown/去重，不能通过换UUID重置历史。

Mission 控制会话与科学任务的并发分别有界：每个 Cycle 同时最多一个活动 Mission，科学任务继续受 max_parallel_runs 约束；等待科学结果的 Mission 不占掉唯一科学槽。只由可信服务确定试验计数，非试验准备/组合/模拟任务和 Mission 使用0，不得把 Alpha试验伪装为管理任务。CPU/内存/输出/墙钟和模型token/turn预算仍适用于非试验任务；0仅表示不新增试验，绝非无限资源。历史已记账的Run不原地改写或退还。

### B5.0.1 原生任务定义、Worker 与 DATA_VALIDATE 正式入口

`server worker` 使用现有非owner应用角色、ArtifactStore、SecretVault、显式RuntimeTargets与PGMQ；默认并发2、可配置1–32。每次消息的 claimant 都有唯一 worker_owner_id，不能让同进程的重投消息共享一个尚存租约的主动驱动。Worker与研究job隔离，科学任务只能经固定RuntimeTransport/JobSpec调用原生网关，不在Worker/API里执行科学引擎。AGENT_RESEARCH由原生Codex Mission驱动，不能塞入科学容器伪装完成。

增量原生关联：`run_native_tasks(run_id PK/FK run_admissions, parameters_artifact_id FK artifacts, input_bindings RuntimeInputV1[1..256], image_ref原生不可变镜像, cpu[1..1024], capability_snapshot_artifact_id FK, output_schemas RuntimeArtifactSchemaV1[1..64], origin, access_class RESEARCH|EVALUATOR_ONLY, created_at)`与首次Run/PGMQ同事务创建，禁止后补或改写。`run_native_attempts(attempt_id PK/FK,run_id FK run_native_tasks,spec_json JobSpecV1,created_at)`在当前fence、NOT_SENT下冻结一次；接管保留原始spec、owner_epoch及external_job_id，不因当前owner改变而重建远端身份或发送正文。`run_native_outputs(attempt_id,remote_storage_ref) PK,artifact_id UNIQUE/FK,created_at`记录远端原生对象与Store分配的本地对象关系；三个表均不可变，不新增队列、业务hash或独立研究状态机。

`POST /api/v2/data/validate` 接严格DataValidateRequest(schema_version,project_id,input_set_id,runtime_id,expected_runtime_revision,limits JobLimitsV1)，单次Operator命令目标为已有InputSet，202回执返回唯一排队Run。limits.experiments必须0，其余资源为正且受原生能力约束，最多2个并发无Cycle数据验证。固定任务CPU上限取ceil(cpu_seconds/wall_seconds)，至少1且不超过原生max_cpu；不满足时在入队前拒绝，不能保存一个之后必因JobSpec资源合同失败而无法派发的任务。此管理入口仅允许DISCOVERY/VALIDATION的完整已登记Dataset输入，拒绝artifact-only、SEALED/FORWARD和任意镜像/命令/原始报告/URL。InputSet的项目、授权、原生metadata与Runtime身份必须逐项一致。NativeTaskParametersV1::ValidateData从已发表metadata生成，选择不超过原生bar类型、事件和可见截止范围；InputSet更晚的cutoff不能扩大老snapshot的attested cutoff，使用两者较早者。参数原生发表、任务定义、Run/事件/PGMQ/完整202回执原子提交，复用既有enqueue_standalone_run的事务组合形式，不复制准入/预算规则。

Worker首次发送前核对配置revision及真实新鲜capability；等待过久可在当前Run/Attempt/fence授权下重新探测，不借用Operator权限或要求每分钟人工点击。网络在事务外，发表重用RuntimeProbe的原生共同校验与同一观测表；取消/过期/旧fence拒绝，遇到并发更新的更晚有效观测直接复用，不重复发表文件。已发送未知任务只恢复原身份，不因当前readiness不佳跳过查询/取消。每次上传精确参数/CODE/MODEL原生对象之前和本地读取后重验冻结输入授权，上传回执必须保留原UUID/version/bytes。

现有begin_run_dispatch是唯一发送许可：true才发一次POST；未知ACK、失败响应、重投和接管只查同一identity，不重POST。Accepted只记录ACKNOWLEDGED，不表示科学计算已经RUNNING。Worker每10秒续约60秒，续约future与任务共同拥有，任务退出不能遗留续约循环；失去fence或停止服务后不再发新副作用，已开始的有界I/O/本地发表完成或失败后回收。关闭Worker不等于停止远端job；本机未发送的取消/到期可由数据库证明直接收束，已发送的404不能证明停止。只有网关完整且匹配身份的永久取消tombstone可被识别为ConfirmedAbsent。

采纳完整原始ResultManifestV1、相同任务的原生输出、schema/kind/media/version/bytes和本地producer关系后，当前fence所有者才可发表Store分配的文件批次。原生payload总数/大小受冻结output_bytes；manifest是独立最多1MiB的包络，不错误计作payload。输出、remote/local映射、唯一终态receipt、事件与试验记账使用既有accept_run_terminal的事务组合入口同提交；随后才archive。回执重放必须保持原始manifest字节、当前归属Attempt/owner，旧fence不因任务终态而重新获得发表权。取消胜出时不发表迟到的成功payload，原始manifest可作为失败/取消审计保留。输出缺失/格式错误在已证明远端停止后记录INVALID_INPUT，不制造科学PASS；网络暂不可用继续对账，不把传输错误变成数据无效。

原生计算成功只形成生产者绑定的原始质量或科学产物，仍需独立Evaluation/Exposure/Qualification；FIXTURE/SYNTHETIC/PIT未核验不得升级。数据验证管理任务不增加试验、不读取Sealed，不是逃避研究预算或资格门禁的新入口。文件发表而事务失败只在重新取得原Run锁并确认精确对象无正式引用后回收；未知提交保留，不能扫描删除其他Run/用户数据。

### B5.0.2 Worker 独立审查修订（37e5713e）

共享PGMQ runs队列的原生计算消费者先按不可变run_native_tasks与允许的科学kind筛选可见记录，再调用PGMQ1.10原生conditional read；不得先隐藏AGENT_RESEARCH或未定义任务再拒绝。直接驱动入口同样在任何Attempt/Run变更前检查不可变驱动归属。PGMQ负责visibility/read_ct，QZ只选择本消费者负责的领域身份；不新建队列或重写投递算法。

只有Store在真实数据库时间下提交取消状态、cancellation_requested_at及事件后返回Cancel行动，Worker才发送取消RPC；本机时钟不能另开一条无持久意图的取消路径。已收到精确任务的有效终态status后，结果manifest的Contract/ResponseLimit属于不可变输出无效，按当前fence以INVALID_INPUT收束而不反复重试；身份未确认、认证/网络不可用仍保留对账。没有合法manifest时不制造qz.job_result或科学产物，记录静态NATIVE_MANIFEST_INVALID/ NATIVE_MANIFEST_LIMIT原因。

原始产物不仅验证Serde反序列化，还逐项核对任务参数与JobSpec：DATA_VALIDATE报告必须一一对应全部冻结Dataset与原始selection，行数/资产集/时间/可见性在同一冻结范围；编译报告绑定CODE与实际MODEL的native ref/字节数/ABI；预测的资产、时间、单位、期限和模型参数与请求一致；分配结果复用精确权重/约束规则并绑定资产集合；模拟结果的初始资金/币种/区间与原生权益序列一致。Store在结果发表事务内读取原始不可变PARAMETERS再次执行共同关联校验；错误输出形成失败审计而不是成功Run或无限重试。该结构验证不替代后续独立统计评估、PIT或资格。

### B5.0.3 Cycle 首个原生数据任务

Cycle start 与独立 `data/validate` 复用同一个受信任的 metadata→PARAMETERS→NativeTaskDefinition 适配器；不是在收到202后补填原生任务。Cycle启动事务在验证冻结Brief、当前许可/Runtime和预算后，读取精确Discovery元数据产物，原生发表固定参数，创建初始Run/PGMQ、绑定run_native_tasks并写cycle_startups及完整回执，同事务提交。参数读写失败、没有正式登记metadata、原生能力不支持或后半步故障均不留下Cycle、预留或队列半状态；未引用的本次参数由已有Operator原生对象回收路径核对处理。初始任务是有预算的Cycle准备工作，不能以standalone入口绕过周期配额；同命令重放不再读写原始文件。

混合InputSet可以同时引用Dataset与其他研究产物，但DATA_VALIDATE必须逐项选择其中全部Dataset且每项均已有原生登记与当前授权；已有非Dataset产物只保留为输入集合的上下文引用，不读取或上传，不得用其中的用户PARAMETERS替换可信服务生成的验证参数。没有Dataset的artifact-only输入仍拒绝，存在一个未登记或越权Dataset则整体拒绝，不能通过过滤join静默漏掉该成员。

已有历史Cycle若缺少原生定义，不推断参数、不回填成功；读取仍显示其原始状态。新的正式Cycle必须能被当前native Worker选中并按同一Run/Attempt实际验证其登记数据。测试中的合成metadata明确保持FIXTURE/UNVERIFIED，不能借这段启动链路赋予REAL或Alpha资格。

### B5.0.4 从原生准备结果进入 Mission

`run_missions` 是 Run 的不可变驱动归属与角色关联，不是另一套任务状态机：run_id唯一并引用run_admissions，project_id/cycle_id、role=RESEARCHER|INDEPENDENT_REVIEWER、精确profile_id/profile_revision，同Cycle同角色至多一个；角色配置必须等于cycle_startups冻结选择。Mission沿用Run/Attempt、PGMQ、租约与逐轮账本，不要求科学Runtime声称支持AGENT_RESEARCH。

科学Worker在首次完成和终态重投两个ack入口前统一处理正式Cycle的initial_run_id。只有精确终态SUCCEEDED、当前accepted Attempt及其原生DATA_QUALITY产物才可推进；fixture来源不因此变REAL。准备失败/取消如实结束Cycle。项目暂停时保留待处理消息，不丢失恢复入口。研究者Mission准入、预算、Run/事件/PGMQ与run_missions同事务提交；重复通知只读取已存在关联。配置/输入需人工处理时进入WAITING_INPUT，预算耗尽如实收束，基础设施结果未知仍保留重试消息。创建Mission不代表已有Thread或产生模型结果。

Mission与科学消费者共用PGMQ的条件读取实现，各自只选不可变驱动关联；直接claim也复核归属。Mission首次dispatch核对冻结Profile与当前账号状态，不要求科学Runtime的Agent能力。run_missions同时保存非秘密Profile快照及私有credential_ref，作为原生恢复的原始配置，永不含密钥内容。Profile改变会阻止新Thread/付费turn，但不能改写已收到的原生身份或妨碍账本对账。

Mission的Runtime能力刷新适用于两种原始角色，均重验冻结Runtime版本、启用状态、当前租约和期限；不能因独立Reviewer角色而静默跳过这些检查。这不授予Reviewer科学任务或Sealed原始数据的读取权。

原生thread/start只能在已有Run首次发送许可后调用；返回的thread_id、版本和公开有效设置先写codex_sessions，随后才能预留并发送付费turn。写入只接受当前Attempt/fence和精确run_missions绑定，唯一Run及Profile/Thread约束禁止换会话；重复回执必须逐项相同。发送期间Profile修改或取消不抹掉已观测Thread，保存旧版本回执不授予新turn权。未知start且没有已持久Session时不得盲目重建Thread；尚无Session便不可能合法发送付费turn，但也不能把可能存在的空Thread描述为远端成功/已取消。不复制原生聊天或工作区绝对路径到公开响应；native_history_ref只保存原生Thread引用。

首轮可以在DISPATCHING且Thread已绑定、Attempt为SENT_UNKNOWN/ACKNOWLEDGED时预约；恢复轮也可在RECONCILING预约，但前一轮必须已有真实结算，不能绕过未知发送。不能先假造RUNNING以满足账本前置条件；Run仅在实际模型运行观测后变化。所有新预约/发送仍验证冻结Profile、当前lease、期限与项目/Cycle状态。科学Runtime无需Agent能力，但首次Mission准入/派发仍要求其配置版本与冻结执行上下文一致且enabled，避免将新连接配置冒记为旧版本。

可信Mission服务复用现有machine_principals/credentials、原生随机能力、Argon2验证和SecretVault签发MCP凭据，不新增认证算法。签发只接受精确run_missions与当前Attempt/owner_epoch；当前项目/Cycle、Profile/账号和期限全部重验。每个Mission只有该服务创建的主体，每个Attempt/owner至多一次签发，同一public_token_id/verifier_ref回执可以核对，不能重新显示原明文。接管使用新owner的签发并推进主体epoch，旧token永久失效；禁用主体不得自动启用。研究者仅获RESEARCH_READ/EXPERIMENT_SUBMIT/ARTIFACT_SUBMIT/EVIDENCE_READ/RUN_READ，独立Reviewer不获EXPERIMENT_SUBMIT；没有Operator/Downstream、数据库、取消或任意URL权限。期限不晚于Run deadline，数据库只存原生验证器的私有引用；明文只交给受信任的MCP进程配置，不进模型消息或日志。

Mission bootstrap复用CodexDeployment的原生账户、完整catalog与设置解析；有限的只读ephemeral探测不发模型轮，也不作为Mission身份。实际Mission的持久Thread仍只在Run首次发送许可后创建。受信任启动器用指定私有根下的run_id创建独立空Git工作树，禁用系统/个人Git配置与模板，不克隆QZ源码或认证文件；重启复用同目录，缺失/损坏的Git目录明确失败，不覆盖残留文件。工作区不能包含原生HOME/CODEX_HOME秘密范围。冻结Profile的默认覆盖继续省略；恢复必须重用原Thread且其实际model/provider/effort/service tier与原始回执一致，否则停止，不换身份或暗改默认设置。MCP启动后检查原生工具清单；本步骤只确认连接和Session，不标RUNNING、不发送付费轮，也不代替后续资源约束/逐轮Worker/科学结果回送。

MCP身份校验与实验提案入口使用同一有效Mission状态集合：DISPATCHING/RUNNING/RECONCILING，并继续要求当前Attempt/owner、项目、Cycle、scope与deadline。RECONCILING不表示新轮已运行，但允许当前owner重新连接MCP与续作；不能先假造RUNNING来绕开恢复前置条件，也不能让旧token借新租约继续读写。

### B5.0.5 独立 Reviewer 的有限审阅阶段

Researcher原终态SUCCEEDED且选择COMPLETE、有非空冻结审阅目标时，在其ACK事务内
用Cycle冻结的reviewer_profile创建独立AGENT_RESEARCH Run/PGMQ及run_missions关联。
预算继续累计，不重置研究者token/费用、不重复计试验；取消、失败或不完整选择不启动
付费Reviewer。已有Reviewer时重放原关联；配置需处理或预算不足明确记录Cycle状态。

Reviewer以独立原生Thread按冻结排名逐个审阅目标，不继承研究对话。可信Worker仅把
原CODE、原试验PARAMETERS和明确列出的Validation元数据/指标复制到本Run工作区的
review-<experiment_id>目录，复用不可覆盖的本地ArtifactStore存取，文件名是原UUID。
不复制原生账号、聊天、隐藏推理、Sealed原始行或校准系数。原代码/参数仍是研究数据，
不是新的权限或指令。输入副本不是新证据；审阅请求持久绑定原版本、试验、评估与Turn。
Reviewer不获ARTIFACT_SUBMIT/EXPERIMENT_SUBMIT，不能把审阅材料上传成普通研究产物。

每个目标至多一条有界审阅Turn（命令mission/review/<experiment_id>），使用现有预约、
发送意图、原生恢复、真实用量及公开总结。原生公开总结文本应为schema_version、
alpha_version_id、decision=PASS|REJECT|INCONCLUSIVE、reasons的JSON对象；reason为
1至32条、每条1至1024字节。仅精确目标且原生成功结算的总结可投影为审阅记录。
格式错误或目标不符保留原总结并记录INCONCLUSIVE/INVALID_NATIVE_REVIEW_RESPONSE，
不猜测PASS、不新开修复轮。审阅记录绑定原reservation/summary，不是Operator审批、
Sealed评估或资格。全部目标有审阅记录且原生账本齐全才可成功结束审阅会话；失败轮
须先完成真实用量对账，取消不要求补做未启动目标。后续自动Sealed/资格仍独立裁决。

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

### B5.6 Run 生命周期持久化实现合同

原生 revision trigger 必须允许零个额外 immutable 字段（runtime/downstream 配置）；TG_ARGV 的空值按空数组处理，仍保护 id/created_at 并递增 revision。已有迁移保持不变，修正通过新增迁移发布。

Run 的工作准入由受信任领域服务调用 `Store::enqueue_run`，不是浏览器/Agent 可提交任意
镜像或 shell 的通用执行接口。输入必须是同项目、已冻结的 input set，Cycle 与冻结 Brief
预算逐字段一致；job kind 必须在指定 runtime 的登记能力中。一个不可变 `run_admissions`
保存精确 run/project/cycle、资源预约、runtime revision 与非秘密运行配置快照、PGMQ 初始
消息 ID。command key 在 Cycle 内唯一；同 key 同请求返回原 run，冲突失败。预约、Run、
首个持久事件与原生 pgmq.send 同一事务；CPU 额度按已承诺的上界累计，不因失败/取消退款。
全部 trial（含内部搜索）在准入时计数；终态把预约 trial 转为已用，不删失败历史。

#### Cycle/Experiment 与 Run 的同事务组合入口

`Store::enqueue_run_in_transaction(tx, key, submission)` 接收调用方持有的原生 SQLx
Transaction 所有权，沿用同一套 Run 准入实现；不另开连接、不自行提交、不复制预算或
PGMQ 逻辑。成功返回 `(tx, CommandResult<RunSnapshotV1>)`；新建和幂等重放分支都交回
原事务。返回的 Run snapshot 仍为未提交结果，只有外层 COMMIT 成功后才能响应成功或
允许分派。COMMIT 应答丢失仍为结果未知，不能声称已回滚或盲目重建。

任何领域/SQL 错误不交回事务；丢弃执行 future 同样由 SQLx 原生 Drop 回滚所传事务范围。
正式 Cycle 启动必须传入包含 Cycle 创建的最外层事务，不能先提交 Cycle、再传新事务或
仅包住入队的 savepoint。已有 `enqueue_run` 只是开始最外层事务、调用该入口、提交并返回
的便捷入口。调用方仍先完成精确操作授权，并按 authority→project→cycle→run 锁序组合
Brief/Cycle/Experiment/命令回执；不能在锁后重新取得更早的 authority 锁。

本入口不创建公开通用任务执行接口，不授予 Brief 冻结、原生能力、Sealed、模型发送或
Qualification 权限。正式冻结/启动服务仍须核验当前数据/PIT/方法能力、项目版本、冷却/
每日配额等完整合同；此处的可组合事务能力不能冒充这些服务已经完成。对应真实
PostgreSQL 回归在 `crates/store/tests/atomic_cycle_admission.rs`：独立连接提交前不可见、
显式回滚、同事务/已提交重放不提前提交、队列注入失败、预算/命令拒绝、延迟外键提交
失败及锁等待期间取消。测试代码存在不代表已执行或通过，也不等于 T42 无 SQL 用户链路。

`read_run_messages` 复用 PGMQ 原生 visibility/read count。领取必须重新锁住 project→cycle→
run→attempt；visibility 不授予结果采纳权。活动租约不被别的 Worker 抢占；过期接管保持
同 attempt 和 `<run_id>/<attempt_no>`，只递增 owner epoch，并进入 RECONCILING。
`begin_run_dispatch` 先持久 SENT_UNKNOWN 再允许首次外部 submit；重试返回需 reconcile，
不第二次授予首次发送。runtime 快照供恢复使用，不随 Operator 后续改连接而指向其他服务器。
真实远端生命周期仍须由隔离 runtime 适配器实现，本存储接口本身不是远端隔离或工具闭环证明。

取消请求与结果采纳按同 Run 行串行化。未 dispatch 的排队工作可直接取消；已经领取/发送的
工作先 CANCEL_REQUESTED，必须有受信任适配器返回的终止事实才能落终态。真实 FAILED
不改成 CANCELLED；取消意图先提交时原生成功产物只作诊断。`run_terminal_receipts` 追加
精确 run/attempt、原生终态、manifest（可空）、原因与观测时间及采纳后的 Run 终态。同一
run 最多一个终态回执。完全相同重传可只读确认；变化字段、跨 attempt、旧 owner 的新事实
均失败。SUCCEEDED 必须绑定已由可信适配器验证、同 run/attempt 的登记 REPORT manifest，
不能用别的运行或任意 UUID 冒充；存储层不据此生成 PASS/Qualification。

只有持久终态及回执已经提交，才可 archive PGMQ 消息。消息丢失/ACK 丢失不重复转用额度、
追加终态或发布。新 Run 重试是新的明确准入，不在 UNKNOWN 时创建第二个 attempt。

读取接口 GET `/runs`（project_id、state、cursor、limit）、GET `/runs/{id}`、GET
`/runs/{id}/events` 及 POST `/runs/{id}/cancel` 共享现有浏览器/Bearer 权限。机器必须具有
RUN_READ/RUN_CANCEL 且绑定同项目；Mission 只能读自己 Run，不能取消。取消为计算控制，
不消费审批 grant、触发 Broker 动作或修改下游交易。取消体包含 schema_version、
expected_revision；还需 Idempotency-Key。同一 key 的重传返回当时原响应，不覆盖新状态。

事件序号沿用数据库现有原生触发器，在 Run 锁下追加并更新 cursor。状态事件只包含允许的
state/reason，不含 provider原文、secret、工作区路径、隐藏推理或任意消息。查询事务在
授权后锁 Run 快照并读取 seq>cursor；缺口/超前/不适用 cursor 失败，不以空数组掩盖丢失。
SSE 复用 Axum 原生 Event/Sse 和成熟 Stream adapter，逐批从持久表读取并重新核对撤销/
到期；bounded batch/总连接数避免无限堆积。断连只停止读取，不取消计算，不持有数据库
事务或连接等待客户端。未实现的实时百分比不伪造。没有通知也会轮询持久事件。

首次 dispatch 的 runtime 配置锁等待结束后重新检查 DB lease 与 deadline；不能沿用等待前的有效性。续租只延长不缩短既有有效租约，返回数据库实际提交的新到期时间。

原生失败必须保留冻结的 failure_class 和受控 failure_code（1–64 个大写 ASCII/数字/下划线），不得存原始 stderr；明确不存在的任务不得同时提交 manifest。终态回执的项目/Run/Attempt/状态与 revision/event cursor 必须与同事务锁定的 Run 一致，不能单独插入可供 ACK 的伪回执。

此 Store 增量只接受已登记的 `kind=REPORT`、`media_type=application/json`、`schema_name=qz.job_result`、`schema_version=1` 的 result manifest；生产 run/attempt/project 及字节上限一致。这个名字是业务产物 schema，不是新的目录或内容哈希。实际 JSON 内容、输出引用和科学字段仍须可信 runtime adapter 在登记之前按 ResultManifestV1 校验；单独插入元数据不算完成该验证。

### B5.7 准入授权、无 Cycle 工作与终态保护（2026-09-06 审查修订）

冻结 InputSet 是不可变历史，不是永久读取许可。每次新 `enqueue_run` 和每个 Attempt
唯一首次 `begin_run_dispatch` 事务都复用 A4.3 的成员验证；按 source→runtime（包括执行
runtime）→grant 的稳定顺序锁定，最后读取数据库当前时间与已提交的撤销，再验证启用状态、
用途、source/grant 绑定、partition/PIT/as-of、生效和到期。校验失败时不得增加预算、消息、
Run、发送意图或事件。已经提交的准确幂等回执、SENT_UNKNOWN 对账和真实终态结算不因
事后撤销而删除、重发或退款。锁外预检不能替代这些事务内检查。

受信任 `enqueue_standalone_run` 只接 `project_id/input_set_id/runtime_id/runtime_revision/
kind/limits/max_parallel_runs`。仅 IMPORT、EXPORT、DATA_VALIDATE 可用；limits.experiments
必须 0，cpu_seconds/wall_seconds/memory_mib/output_bytes 正值且满足原生整数边界；部署侧
可信调用者给出 1–65535 的项目管理任务并发上限，不能由 Agent 降格研究任务。EXPORT 可读取
归档项目；IMPORT/DATA_VALIDATE 不在归档项目开始。无 Cycle 任务仍须同项目冻结输入、
当前数据许可和登记的原生 runtime 能力。没有伪造的 Cycle，也不借此绕过研究预算。

`run_admissions.cycle_id` 可空，但 `(run_id,project_id)` 必须精确引用 Run，cycle_id 必须与
Run 完全一致（含 NULL）。有 Cycle 继续 UNIQUE(cycle_id,command_key)；无 Cycle 使用
partial UNIQUE(project_id,command_key) WHERE cycle_id IS NULL。身份、规范请求、资源、
运行配置与初始消息快照仍不可变。锁序 project→可选 cycle→run→attempt；只有有 Cycle 的
终态结转实验预约，无 Cycle 不修改研究账本。上述接口是内部受信任准入，不是公开任意执行入口。

已领取 Attempt 若 lease 与 deadline 均过期且 dispatch_state=NOT_SENT，恢复事务直接把
本地 dispatch_state 设 TERMINAL、Run 设 FAILED/DEADLINE_EXCEEDED，并追加唯一终态回执/
事件及一次性额度结转，不再续租。保留原 owner/epoch/external_job_id；runtime_state 保持
UNKNOWN，回执明确 source=NOT_DISPATCHED，不能捏造远端失败或不存在证明。尚有效的其他
owner 租约不抢占；SENT_UNKNOWN/ACKNOWLEDGED 仍对账，不能据本地超时宣告远端已停。

Run 进入终态后，其所有 Attempt 的新增、修改、删除均被持有父 Run 行锁的原生触发器拒绝，
包括没有 manifest/accepted_at 的失败与取消。正确采纳先修改 Attempt 再同事务终结 Run/
receipt；之后的准确重传只读原始回执。Attempt.error_class/error_code 仅存真实失败元数据，
成功和成功后取消不存 RUNTIME_SUCCEEDED 等 Run 原因；Run.terminal_reason_code 仍保留原因。
旧终态历史不通过迁移原地改写。浏览器取消必须 300 秒内的 TOTP 认证；读取不要求近期认证，
机器取消仍仅允许精确项目的 CLI/AUTOMATION RUN_CANCEL，不赋予 Mission/Downstream 权限。

部署迁移使用脱离请求池的专用连接，在原生迁移 advisory lock 前 SET statement_timeout=0，
仍使用 lock_timeout=5s。全部批次和角色授权同事务；结束后关闭连接，不把该设置归还请求池。
正常请求的 15 秒超时不变。失败证据由实际返回确认，不把正在运行的 CI/review 当成失败。

## B6. SSE 与恢复

生产端使用封闭 RunEventKind 和严格 RunStatePayload；消费端 RunEventV1.event_type 保留
1–120 字节安全 ASCII 名称（首字母 a–z，其余 a–z/0–9/下划线/点），payload 是不超过
65536 UTF-8 字节且 schema_version=1 的公开 JSON 对象。已知 run.created/run.state_changed
仍经严格原生 Serde 校验；未知但兼容的事件保留 envelope 并推进游标，不更改状态投影。
主版本不兼容、非法名称、非对象或超大负载仍失败，不跳过错误行伪造连续性。浏览器和 CLI
复用原生 SSE；未知类型不触发永久 reset-required 循环。

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

正文TargetPackageV1使用package_schema_version="1"、environment_origin=DEMO|REAL，
不接受未知字段。qualification_refs至少2项，其余evaluation_refs/input_revision_refs/
provenance_artifact_refs均非空、去重且保持原顺序；engine_versions与兼容市场能力引用
均非空。targets使用明确target_weight字段，不能混入quantity/订单指令。
constraints_summary保留原PortfolioConstraintsV1，exposure_tolerance保留原Mandate
十进制容差；cost_assumption_ref指向原执行假设。与原PortfolioTargetsV1逐项核对
Candidate、币种、目标顺序/权重和现金；Package起点不早于原目标asof，终点不晚于
原目标valid_until。这个正文合同只检查结构和绑定，不替代当前来源/资格/政策/
独立PORTFOLIO/PASS、Package持久化、Release或审批事务。

人工Release创建意图ReleaseCreateV1仅含schema_version=1、candidate_id、
evaluation_id，以精确Candidate的RELEASE_CREATE授权和幂等键执行。服务端从原
REAL Candidate及目标产物、Mandate、已发表的独立PORTFOLIO/PASS及执行假设组装REAL Package，
不能请求覆盖权重、来源、市场能力或有效期。复用原Candidate数值/来源校验与
当前许可/资格锁；valid_from取服务器当前时间，valid_until不超过原目标、评估、
资格及许可期限。先发布原生不可覆盖文件，再同事务保存PACKAGE、Release及命令
回执；文件发布后重新核对来源/授权和期限。旧成功回执可重放但不延长目标。
此入口不创建Approval/Offer，也不以FORWARD/HOLD或DEMO替代独立PORTFOLIO。
POST /api/v2/releases与client release create消费该意图，201只表示冻结Release。
GET /api/v2/releases/{id}与client release show返回原ReleaseViewV1，不重新判定
资格或延长期限；只供Operator及精确项目RESEARCH_READ的CLI读取，不向Mission
开放。创建失败仅清理确认未被数据库引用的本次新对象；响应丢失保留原键重放。

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

### B9.1 CodeQL 的语言迁移与精确源版本

旧默认配置仍按 Python 扫描已删除 Python 的重写 Head，会因无源码失败；不能通过放回假 Python 文件、跳过 queries 或忽略该失败造绿。采用 GitHub 官方 CodeQL advanced setup，按 Git 对应 commit 的已跟踪文件决定 Rust、JavaScript/TypeScript、Actions、Python 的实际分析范围。PR 的 Head 与仍在使用旧代码的 main/base 分别检出和分析，上传各自精确 ref/sha；base 检查明确标为 base，不能冒充 Head 验证。Rust 用官方支持的 build-mode=none（仍需原生 rustup/cargo，RustAnalyzer可能执行构建脚本），普通PR不注入生产秘密。

切换是一次受控的开发环境操作：先保存默认配置，确认已审阅工作流与提交、GitHub权限及本地完整验证，再解除 default setup 对 advanced upload 的原生互斥并推送同一PR。发布失败且未形成远端提交时恢复旧默认设置；不删除历史分析/告警、不撤销分支保护、不降低查询集。新工作流必须实际运行并上传成功后才算接通，配置写入不代表已扫描。默认主干仍为旧版本期间，PR附带真实base分析保留其Python等语言覆盖；合并后push/schedule继续分析实际main。最终交付仍要求适用检查无失败，不将旧默认配置的失败伪称通过。

依据：GitHub 官方 [Rust 构建选项](https://docs.github.com/en/code-security/reference/code-scanning/codeql/build-options-for-compiled-languages)、[两种设置互斥](https://docs.github.com/en/code-security/reference/code-scanning/troubleshoot-analysis-errors/two-codeql-workflows)、以及固定 CodeQL Action `cdf488f595d80d6e07e03d4674febd5ab45fa938` 的原生 `languages/build-mode` 和 `ref/sha/category` 输入。工作流不复制扫描引擎或生成伪 SARIF。

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

可信服务的`prepare_mission_turn`将公开请求的`qz.mission_turn` PARAMETERS产物、预算预约与PGMQ通知放在同一事务，复用`reserve_turn`的全部准入与幂等逻辑。产物只含QZ公开prompt、Run/Session/Attempt、command_key和turn_kind，不含原生聊天、隐藏推理或凭据；逐字节重放不换请求，文件发布后复核数据库lease/deadline。失败或未知提交保留可能发布的对象，由现有Run锁定的未引用对象清理入口对账，不能删潜在已提交产物。请求字节计入Mission输出限额。恢复只读取精确生产者与固定schema的原请求，checkpoint只投影最新预约、发送意图、原生Turn绑定、终态、用量回执和已结算token累计；没有ACK/用量时维持未知与预算占用，不根据队列、空列表或改后的Profile猜测重发/退款。

可信原生单轮驱动只消费上述已发表请求与唯一发送许可，不拼接聊天历史或接管工具循环。原生累计token减去同一Session已结算累计量后才形成本轮用量；终态与用量分开，丢失用量不补零，丢失发送ACK不按原生Turn列表的位置猜配。原生终态的首次接收时间由数据库记录；重放保留首次时间。冻结Run或未结算Turn截止由数据库先提交取消意图，驱动再请求原生interrupt；interrupt响应、本地退出和关闭连接均不冒充远端已停止。原生0.144.4无可靠费用报价：没有可信计价来源时只支持冻结UNAVAILABLE费用模式的新调用，其他费用模式明确拒绝发送，不拿预约估算额充当实际费用。已发送的旧费用预约仍保留原终态和未知用量，不能为恢复自动改预算。

原生thread/tokenUsage/updated是每条模型响应后的中途累计通知，不是失败/中断Turn的完整用量证明。工具后的下一请求可能断流且无用量，驱动不得把前一响应的数字结算为整轮FAILED/CANCELLED。当前驱动只在原生COMPLETED与同轮用量均已观察到、且费用无需未知报价时结算；失败/中断保留真实终态和全部预约供对账。底层账本仍可接收独立可信的完整失败用量证据，不以这一特定原生适配的限制篡改历史回执或伪造0。

Linux Mission进程资源边界复用systemd原生user scope与util-linux prlimit，不建立另一个容器/进程调度平台。scope以Run命名；旧scope仍活动时同Run不能启动第二份进程组。CPUQuota取冻结cpu_seconds/wall_seconds的保守速率，MemoryMax、MemorySwapMax=0、TasksMax和剩余RuntimeMaxSec作用于整个组；prlimit将原生单文件限制为64MiB并禁用可能含凭据的core dump。原生内部SQLite/WAL/rollout并非QZ研究产物，不能把较小的研究输出预算错误用作它们的文件上限；原生文件达到上限须保持原文件和不可用事实，不能自动删除聊天/数据库来恢复。scope继承可信启动器的白名单环境，而不是service manager全局环境，不用--setenv或参数传递凭据。剩余墙钟来自数据库观测时间，不因重连重置；内核配额以原生周期执行，不声称零误差CPU计时。低于原生1ms/1s配额精度的预算明确拒绝，不能悄悄放宽。原生退出/资源终止仍不构成远端模型停止或零费用证据。冻结研究输出总额度继续在产物发表事务检查，不被原生文件上限替换，单文件上限不冒充工作区总磁盘配额；磁盘准入/恢复仍须独立验收。

连接成功初始化后，以自己的原生子进程PID及内核cgroup成员关系确认所有权，持有原cgroup.kill文件描述符。关闭/异常Drop只向这个原生句柄请求整组终止，正常关闭等待原组消失；不能仅杀App Server主PID留下bwrap/MCP，也不按可复用的scope名称杀后来的进程组。无法确认所有权时不授予清理能力。原生文件描述符及inode只用于OS资源所有权，不是领域身份/资格或应用hash。受低CPU配额约束的Mission RPC最多等待60秒、MCP启动最多45秒，仍受原有110秒bootstrap总界限和Run剩余墙钟约束；普通账户/目录探测保持原20秒RPC限制。不得把等待放大为新任务预算。

原生turn/start返回的是提交入队确认，不能据其InProgress字段假造已开始计算；只有实际TurnStarted事件或原生started_at才能标记RUNNING并发送普通turn/interrupt。取消与完成仍可能竞态：原生-32600拒绝不等于已取消；必须等待精确原Turn的真实终态通知或保留已有持久化通知，缺失则保持未知，不换Thread、不重发或补造用量。

锁定0.144.4已复现：真实工具续轮断流时，`turn/completed`为FAILED，但
`thread/turns/list(itemsView=notLoaded)`重建视图可为COMPLETED。因此列表和start ACK
仅用于恢复精确身份、读取可观察开始状态，不能单独形成终态/成功回执。驱动只采纳
精确Turn的真实`turn/completed`通知或此前已持久化的同一通知；原生失败不会被列表
“成功”覆盖。收到通知即在当前fence下记录，不因后续断连而丢掉已知终态。只有真实
成功终态和完整用量同时已知才自动结算；丢失终态通知且没有持久化记录时保持UNKNOWN
和预约，即使列表显示完成也不猜测退款/成功。-32600中断拒绝后只在有界窗口内等待
该Turn的真实通知，不用列表投影补造终态。

Researcher首轮由可信Worker根据已绑定Session及冻结Brief准备公开请求；已有任何Turn
时保留原预约/请求，不再插入首轮或覆盖人工已授权的接续。请求只列出本Mission的
公开身份、冻结Brief引用与研究边界，不复制原生聊天或从宿主读取任务提示。
首轮token预约使用同Cycle尚未使用/预约的剩余额度；未配置token上限时仅以既有
PostgreSQL bigint可表示余量作账本预约，不把它宣称为业务token限额。完整结算后
剩余额度仍可用于后续业务阶段；未知用量继续占用原预约。有费用上限但没有原生
计费能力时不准备可发送的首轮，不编造估算。余额读取不是准入许可，实际公开产物、
预约及PGMQ仍由现有prepare_mission_turn事务再次重验并原子提交，竞争失败不重发模型。
首轮准备本身不启动模型、不结束Mission，也不代表科学任务或资格已经完成。

常驻Worker仅在显式配置CodexDeployment、内部Mission API origin和私有工作区根时
消费Mission消息；缺配置不领取或隐藏这些消息，科学任务仍可独立消费。科学与Mission
各自最多parallelism个在途驱动，共用原PGMQ和Run租约，不新增队列或Agent工具循环。
Mission领取后全程每10秒续约60秒，失去租约或服务关闭即停止本机驱动并回收原生
子进程；这不是远端取消/预算退款证据。只准备缺失的首轮并驱动既有精确预约，已结算
的轮不再开连接/重发；轮结算本身不ack Mission消息，必须另经完整研究阶段收束。

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

#### 消费端事件对象生成约束（3944785219）

RunEventV1.payload 的原生生成schema明确 type=object，required=[schema_version]，schema_version整数严格为1，允许其他公开扩展属性。运行时仍检查65536字节上限，已知run.created/run.state_changed负载严格按RunStatePayload验证；未知兼容事件不得伪造状态投影。对象schema不能代替已知事件语义、权限或负载大小检查。

Brief 草稿成员替换要求部署迁移仅对 app.brief_data_bindings 追加 DELETE 授权；不得对所有 app 表或任何历史账本授予 DELETE/TRUNCATE/TRIGGER。该单表 DELETE 仍经父 Brief 行锁和 DRAFT 状态触发器；FROZEN 后拒绝全部成员改动。必须使用真实非所有者运行身份执行新增/替换/冻结拒绝回归，不能只用数据库owner证明可运行。
