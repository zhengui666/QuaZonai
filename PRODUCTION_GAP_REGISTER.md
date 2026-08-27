# QuaZonai 闭环与生产可用 Gap 清单

> 基线日期：2026-08-27  
> 核对基线：`main@7b3eb5d70e550bedb951762dbd1a8724cb6494c2`  
> 上位事实源：[`DESIGN.md`](DESIGN.md)，特别是 §49.1 的生产完成与发布准入合同  
> 文档性质：把 `DESIGN.md` 已定义的产品、架构、生产治理、运营、安全与合规要求拆解为可关闭的实现差距和验收证据；不得反向改写产品事实

## 1. 使用规则

本清单**不使用优先级、严重度、阶段、排期或先后等级**。编号只用于追踪，章节只按领域分组。所有条目都从 `DESIGN.md` 的产品/架构要求或 §49.1 的生产完成与发布准入合同展开；任何未关闭项都阻断“项目闭环完成”和“生产级可用”的结论。

本清单可以把上位要求拆成更具体的工程证据，但不能新增领域对象、状态、API、用户旅程或 ownership。发现需要新增产品/架构/生产准入事实时，必须先更新 `DESIGN.md`，再追加或修改本清单。

每个 Gap 只有在同时满足以下条件时才可以勾选：

1. 对应代码、数据库迁移、配置、文档已合入 `main`；
2. 有可重复执行的自动化测试或运行证据，而不是截图、口头说明或 mock-only 证明；
3. GitHub 必需工作流全部通过；
4. 与 `DESIGN.md`、`OPERATIONS.md`、`CLI.md`、`README.md` 和 Agent Skill 一致；
5. 不突破 QuaZonai 的边界：QuaZonai 只负责研究、组合、审批、交付和反馈，不拥有 broker credential、订单、成交、仓位、账户、NAV 或下游运行控制；
6. PR 已在 GitHub 上请求 `@codex review`，且最新审查认为没有问题；
7. 没有依赖人工修改数据库、手工复制内部文件、临时脚本或未记录的运维步骤才能成功。

实现顺序可以按技术依赖安排，但依赖顺序不代表优先级。

## 2. 当前基线事实

以下事实用于说明本清单的来源，不构成新的产品定义：

- `DESIGN.md` 明确写明目标方案已锁定，但当前实现尚未 conforming / release-ready。
- 当前 `main` 分支未启用保护规则，也没有 required status checks。
- 当前基线的 GitHub `CI` 与 `Operator Auth E2E` 成功，但 `Frontend` workflow run `32994213674` 在 `Unit and interaction tests` 失败，后续生产构建和浏览器 E2E 被跳过。
- 当前 Compose 只定义 `postgres`、`migrate`、`api`、`finite-worker`；设计要求的独立 `agent-worker`、`evaluator` 及更完整的 worker ownership 尚未形成。
- `README.md` 声明 Polars、NumPy、Optuna、CVXPY 等研究栈，但 `backend/pyproject.toml` 的 research extra 当前只直接包含 Optuna 与 PyArrow，代码中也没有完整 Canonical Research Engine。
- 创建 Research Program 时当前只排入一个 `ALPHA_DISCOVERY` Mission；Mission runner 启动 Codex、写入 `RESULT.md` 并把 Mission 标为成功，但没有完成 Mission DAG、Mission Tool Server、研究产物入库、Alpha 晋级和后续组合链路。
- 当前 Data Source API 主要登记元数据；尚未形成可执行的数据采集、增量同步、数据质量、point-in-time 验证和 Dataset Revision 生产链路。
- `Search Ledger`、`Evidence Exposure`、`Evaluation Episode`、独立 Sealed Evaluator 等目标能力主要存在于设计文档，未形成完整可运行领域实现。
- 当前 Candidate Package builder 根据已存在 Candidate members 生成固定 passthrough wheel，并把目标权重映射为 raw alpha；它是接口骨架，不是由真实 Feature、Alpha、Calibration、Portfolio Policy 产物冻结而成的生产 Package。
- `CLI.md` 描述的命令面明显大于当前 `backend/src/cli/main.py` 已实现命令面。
- Operator 认证、TOTP、trusted browser、cookie 加密和多项安全加固已经有较完整实现，但这不能替代研究闭环、生产部署、备份恢复、可观测性和发布治理。

## 3. 项目完成的统一门槛

- [ ] **GAP-DONE-01 — 目标与事实完全对齐。** `DESIGN.md` 中所有 V1 当前态要求均有实现和自动化证据；尚未交付的未来能力不得继续以当前能力口吻出现在 README、UI、API 或营销描述中。
- [ ] **GAP-DONE-02 — 真实纵向闭环成立。** 在干净环境中，从 Idea 到 Charter、Mission DAG、真实数据、Alpha、独立评估、Portfolio Candidate、Candidate Package、人工 Paper Approval、下游 claim、完整 Paper feedback、Forward Evidence、Degradation/Wake-up 全链路自动完成。
- [ ] **GAP-DONE-03 — Live 边界成立。** 只有完整且有效的 Paper Forward Evidence 才能创建新的 Live Approval；Paper Approval 永不隐式授权 Live；QuaZonai 不执行交易，也不控制已领取的下游运行时。
- [ ] **GAP-DONE-04 — 生产验证成立。** 安装、升级、迁移、备份、恢复、密钥轮换、故障重启、事件重放、资源耗尽和回滚均有测试或演练证据。
- [ ] **GAP-DONE-05 — 发布门禁成立。** 所有 GitHub 必需检查成功、无未解决 review thread、`@codex review` 最新结论无问题、版本和变更记录完整，才能合并或发布。
- [ ] **GAP-DONE-06 — 无占位实现。** 生产路径中不存在以 mock、seed、固定权重、空实现、`SYSTEM_NOOP`、静态示例或手工注入业务事实代替真实能力的情况。
- [ ] **GAP-DONE-07 — 无竞争事实源。** 设计、实现、测试、运行手册、CLI 和 Skill 对同一能力的状态、名称、边界和用法保持一致。

## 4. 治理、仓库与发布

- [ ] **GAP-GOV-01 — 建立设计符合性矩阵。** 把 `DESIGN.md` 的领域对象、状态机、API、页面、隔离要求和验收测试映射到代码路径、迁移、测试名称和运行证据；矩阵必须随相关 PR 更新。
- [ ] **GAP-GOV-02 — 修复当前主干失败状态。** 查明并修复 Frontend workflow 的单元/交互测试失败，重新运行被跳过的生产构建和浏览器 E2E，禁止把“CI workflow 成功”代替“所有适用 workflow 成功”。
- [ ] **GAP-GOV-03 — 保护主干。** 为 `main` 配置禁止直接推送、禁止 force push、要求 PR、要求适用 CI/Frontend/Operator Auth E2E/安全检查成功、要求 review thread 解决和最新 head 审查；管理员也不应默认绕过。
- [ ] **GAP-GOV-04 — 统一 PR 完成协议。** PR 模板必须记录受影响的 DESIGN 条款、Gap ID、迁移影响、回滚办法、测试证据、运维影响和安全边界；所有新增需求/bugfix 均执行 CI + `@codex review` + 合并流程。
- [ ] **GAP-GOV-05 — 清理临时和遗留文件。** 审核并删除 `.upload-test`、临时 review 文档、过时说明、旧 execution-control 术语、无用途兼容层和不能解释所有权的文件；保留项必须有明确用途和 owner。
- [ ] **GAP-GOV-06 — 建立版本与发布记录。** 使用一致的语义版本、release notes、数据库 schema version、Package contract version、Plugin API version 和兼容矩阵；禁止代码版本长期停留在 `0.1.0` 而对外宣称 production。
- [ ] **GAP-GOV-07 — 建立可追踪关闭机制。** 每个 Gap 关闭必须关联一个或多个 PR、测试和运行证据；发现新 Gap 时追加到本文件，不能只在 issue、聊天或 review comment 中保留。
- [ ] **GAP-GOV-08 — 约束能力声明。** README、首页和文档必须区分“已实现并验证”“实现骨架”“设计目标”；在闭环验收前，不得使用 autonomous production、release-ready 或 live-ready 等无证据表述。

## 5. 产品边界与用户旅程

- [ ] **GAP-PRODUCT-01 — Idea 解析真实可用。** 用结构化解析产出 market scope、Universe、horizon、data domains、explicit exclusions、material/system assumptions；当前基于关键词的简单推断不能作为生产 Charter。
- [ ] **GAP-PRODUCT-02 — 一轮高价值澄清。** 仅在会改变研究边界时生成一次、最多 1–3 个问题；答案必须参与最终 Charter，未完成澄清不能创建正式 Program。
- [ ] **GAP-PRODUCT-03 — Charter 冻结。** 正式开始后 Charter 不原地修改；超出范围必须建立关联 Program，所有依赖对象和 UI 都引用同一不可变 Charter 事实。
- [ ] **GAP-PRODUCT-04 — 重叠 Idea 语义判断。** 不能只按原始文本完全相等检测重复；需要可解释的语义/领域重叠判断、IdeaContribution、Branch/新 Program 建议及 Evidence Exposure 继承。
- [ ] **GAP-PRODUCT-05 — 正常旅程只保留两类人工动作。** 除提出 Idea 和审批系统唯一推荐 Candidate 外，研究推进、重试、数据准备、实验、评估和组合构造不得要求逐步人工操作。
- [ ] **GAP-PRODUCT-06 — Program 管理状态正确。** Pause、Resume、Archive、Restore、Cooling、Blocked、Approval Pending、Waiting for Feedback 的转换、唤醒条件和审计事件必须由明确状态机约束。
- [ ] **GAP-PRODUCT-07 — Readiness 不得虚假。** `SYSTEM_READY`、`RESEARCH_READY`、`PAPER_HANDOFF_READY`、`LIVE_HANDOFF_READY` 必须验证所有真实依赖，而不是仅通过“存在一行 Data Source/Downstream”推导。
- [ ] **GAP-PRODUCT-08 — 用户决策面完整。** Home、Action Center、Research Pulse、Approval Inbox 和 Handoff Feedback 只呈现可行动事实；token、trial 数和文件修改数不得冒充研究进展。
- [ ] **GAP-PRODUCT-09 — 无执行控制泄漏。** Web、CLI、API、Package、插件、文档和示例中不得出现下单、持仓管理、broker 操作、TradingNode 控制、stop runtime 或 claimed handoff revoke。
- [ ] **GAP-PRODUCT-10 — 单用户边界一致。** 不引入半成品 tenant、organization、workspace、RBAC 或多人审批；Operator Authentication 只作为部署访问边界。

## 6. 领域模型、数据库与不可变事实

- [ ] **GAP-DOMAIN-01 — 补齐领域对象。** 实现 DESIGN 列出的 Research、Evidence、Alpha、Portfolio、Approval、Package、Handoff、Feedback、Degradation、Runtime Configuration 和 Plugin 对象；不能仅以开放 JSON 字段替代核心关系。
- [ ] **GAP-DOMAIN-02 — 补齐状态机。** 每个对象只允许设计规定的转换；终态不可恢复，非法转换返回稳定业务错误，数据库约束和服务层同时防守。
- [ ] **GAP-DOMAIN-03 — 补齐不可变版本。** Dataset、Feature、Alpha、Calibration、Qualification、Mandate、Capital Context、Candidate、Approval、Package 和 Contract 的实质变化必须创建新版本，禁止覆盖旧事实。
- [ ] **GAP-DOMAIN-04 — 补齐关系与 lineage。** Program、Branch、Mission、Alpha、Evaluation、Candidate、Approval、Package、Handoff 和 Forward Evidence 的父子关系可查询、可审计、不可因复制或重命名丢失。
- [ ] **GAP-DOMAIN-05 — 一致的并发控制。** 所有公开 mutation 使用事务、row lock、`expected_revision/state/version` 和 Idempotency-Key；唯一约束竞争必须转换为业务冲突，不能泄漏 500。
- [ ] **GAP-DOMAIN-06 — 一致的数据库约束。** UUID、UTC timestamptz、numeric 业务数值、状态 CHECK、唯一约束、外键策略、必要索引、非空和范围约束与设计一致。
- [ ] **GAP-DOMAIN-07 — 事件与状态同事务。** 状态变更和 Domain Event 原子提交；失败不得留下“事件成功但状态失败”或相反情况。
- [ ] **GAP-DOMAIN-08 — Artifact 事实完整。** 大文件存持久卷，数据库保存 owner、kind、路径、媒体类型、长度、版本和生命周期；路径必须限定在配置 root 内。
- [ ] **GAP-DOMAIN-09 — 无应用级 hash Gate。** schema、迁移、Package、Plugin、Approval、Idempotency 和 Workspace 不得新增业务 SHA/checksum/digest/fingerprint 身份；验证使用显式版本、标准 metadata、schema 与运行 fixture。
- [ ] **GAP-DOMAIN-10 — 迁移可演进。** Alembic 迁移必须支持从最近受支持版本升级，具备失败恢复和备份前置检查；禁止只依赖删除数据库重建作为生产迁移方案。
- [ ] **GAP-DOMAIN-11 — 数据保留与清理。** 正式研究事实不可物理删除；临时 worktree、staging、过期 job lease、失败上传和可回收缓存有明确清理策略，不得无界增长。
- [ ] **GAP-DOMAIN-12 — 启动预检完整。** 数据库 schema、目录权限、密钥、运行版本、磁盘空间和关键配置不满足时 fail closed，并返回可操作错误。

## 7. Market Universe、数据源与 Dataset Revision

- [ ] **GAP-DATA-01 — Market Universe 可执行。** 至少实现一个生产 Universe Version，包含 instrument schema、membership、calendar/session、currency、cost/capacity family、risk compatibility 和 downstream compatibility。
- [ ] **GAP-DATA-02 — 真实 Data Connector。** 至少实现一个经批准的 `DATA_CONNECTOR`，可在隔离 runner 中完成认证、全量/增量拉取、分页、重试、限流、断点和错误分类。
- [ ] **GAP-DATA-03 — Data Source 版本化。** Provider、fields、license、allowed usage、revision policy、update cadence、credential requirement 和 release pin 必须形成不可变版本；修改不能静默影响旧 Dataset。
- [ ] **GAP-DATA-04 — 数据凭证边界。** Data credential 加密存储、write-only、可轮换，只进入 connector runner；不得进入 Codex、Mission shell、日志、事件或前端。
- [ ] **GAP-DATA-05 — Dataset Revision 生产链。** Connector 输出必须形成显式 Dataset Revision，记录 source/release/schema/universe、event time、available time、ingested time、质量、许可和 point-in-time 状态。
- [ ] **GAP-DATA-06 — Point-in-time 正确性。** 明确区分事件发生时间、现实可获得时间和系统摄取时间；研究查询严格按当时可获得信息过滤，并有 look-ahead 回归测试。
- [ ] **GAP-DATA-07 — 数据质量 Gate。** 自动检查 schema、类型、排序、重复、缺失、coverage、单位、币种、calendar、survivorship、revision/restatement、corporate action 和异常值；失败归类为 Data Evidence。
- [ ] **GAP-DATA-08 — Canonical 存储。** 使用 Arrow/Parquet 定义稳定 schema、分区、压缩、读取和升级策略；大型数据不写入 JSONB，也不依赖 DataFrame 隐式类型。
- [ ] **GAP-DATA-09 — 数据 lineage 与可重放。** 从原始 source response 到 canonical revision、质量结果、研究输入引用全程可追踪；同一 Revision 可在离线环境重复加载。
- [ ] **GAP-DATA-10 — 数据授权与许可。** 每个生产数据源记录授权主体、允许用途、保存期限、再分发限制和审计依据；无授权数据不能进入 canonical store 或 Candidate Package。
- [ ] **GAP-DATA-11 — 受控数据访问。** Codex 只能通过 Mission Tool Server 获取列表、schema、summary、受限 sample 或实验引用；禁止任意 curl/wget、网页 snippet 或模型记忆成为定量事实。
- [ ] **GAP-DATA-12 — 数据调度与新鲜度。** 实现摄取计划、迟到/断流检测、revision 到达事件、重试和告警；数据未达到有效窗口时 Program 应等待或 BLOCKED，而不是生成假结果。
- [ ] **GAP-DATA-13 — 数据生命周期。** 定义 raw、canonical、discovery、sealed、forward 分区和权限；删除、归档、压缩、恢复过程不破坏已引用 Revision。
- [ ] **GAP-DATA-14 — 数据生产验收。** 在干净 Compose 环境从真实或官方 sandbox provider 获取一段数据，生成 Revision，并通过 schema、point-in-time、重放和研究读取测试。

## 8. Canonical Research Engine

- [ ] **GAP-RESEARCH-01 — 研究依赖与代码一致。** 实际引入并锁定设计选定的 PyArrow/Parquet、Polars、NumPy、Optuna、CVXPY 等依赖；README 不得声明未安装或未使用的栈。
- [ ] **GAP-RESEARCH-02 — Feature Pipeline 合同。** 定义输入 schema、窗口、缺失处理、available-at 语义、输出 schema、版本和可重复执行方式；Feature artifact 必须进入 lineage。
- [ ] **GAP-RESEARCH-03 — Alpha Model 合同。** 实现 `RawAlphaFrame`，明确 `RELATIVE_SCORE` 与 `CALIBRATED_RETURN`，包含 instrument、as-of、horizon、validity、uncertainty 和 model version。
- [ ] **GAP-RESEARCH-04 — Calibration 合同。** Relative score 只有经过独立 Calibration 才能成为 expected return；Calibration 版本、训练窗口、稳定性和失败原因可审计。
- [ ] **GAP-RESEARCH-05 — Risk/Cost/Capacity 模型。** 每个 Universe 提供现实、版本化、可验证的风险、交易成本、滑点、流动性和容量假设；不能以零成本默认值晋级。
- [ ] **GAP-RESEARCH-06 — Target-weight evaluator。** 实现确定性向量化 evaluator，从 signal/target weight 到 rebalance、成本、收益、风险和 turnover；不得模拟 broker order lifecycle。
- [ ] **GAP-RESEARCH-07 — 时间序列验证。** 实现训练/验证/测试、walk-forward、purge/embargo、regime 和 out-of-sample 规则，防止随机 K-fold、泄漏或重复使用独立证据。
- [ ] **GAP-RESEARCH-08 — Search Ledger。** 保存所有重要成功、失败、淘汰和重复尝试，包括 feature family、model、calibration、参数区间、split、policy、subset 和 promotion。
- [ ] **GAP-RESEARCH-09 — 多重检验控制。** Promotion 指标必须考虑搜索空间、试验次数、重复暴露和 selection bias；不能只取最佳回测。
- [ ] **GAP-RESEARCH-10 — 可重复性。** 固定随机种子、输入 Revision、代码/artifact version、参数、环境、依赖和运行配置；同一输入重复运行得到相同确定性结果或有解释的容差。
- [ ] **GAP-RESEARCH-11 — 资源控制。** 对实验运行时间、内存、并发、临时磁盘和输出大小设置硬限制；超限是明确失败，不得拖死 API/Worker。
- [ ] **GAP-RESEARCH-12 — 失败分类。** 数据失败、代码失败、统计失败、成本失败、容量失败、校准失败和基础设施失败分别记录，不能都归为 Mission FAILED。
- [ ] **GAP-RESEARCH-13 — Artifact 注册。** Feature、Alpha、Calibration、评估结果和 TargetPortfolioFrame 由系统验证后注册，Codex 写出的任意文件不能直接推进领域状态。
- [ ] **GAP-RESEARCH-14 — 基准与反事实。** 每个研究必须对比可解释 baseline、成本前后、简单策略和必要反事实；无增量价值不得晋级。
- [ ] **GAP-RESEARCH-15 — 性能基线。** 用代表性数据规模建立 CPU、内存、I/O 和耗时基准，确定单机支持范围；性能退化进入 CI 或发布验收。
- [ ] **GAP-RESEARCH-16 — 真实研究验收。** 从一个冻结 Charter 和真实 Dataset Revision 生成可重放的 Feature、Alpha、Calibration、评估和 Search Ledger，而不是只生成文本报告。

## 9. Mission DAG、Codex Harness 与 Agent Runtime

- [ ] **GAP-AGENT-01 — 持久 Mission DAG。** 实现 PLAN_RESEARCH、DATA_REQUIREMENT、DATA_QUALITY、HYPOTHESIS、FEATURE_RESEARCH、ALPHA_DISCOVERY、CALIBRATION、ROBUSTNESS、PROMOTION_REVIEW、PORTFOLIO_ASSEMBLY、DEGRADATION_DIAGNOSIS 等节点及依赖。
- [ ] **GAP-AGENT-02 — Deterministic Orchestrator。** Mission 结束后由领域事件和 policy 决定解锁、replan、promotion、cooling、等待或 blocked；Codex 只能建议图，不能直接写正式状态。
- [ ] **GAP-AGENT-03 — MissionContract。** 每个 Mission 冻结 objective、role profile、inputs、capabilities、outputs、success/failure、evidence scope、disclosure、branch 和 workspace revision。
- [ ] **GAP-AGENT-04 — AgentProfileVersion。** 角色、模型偏好、reasoning effort、developer instructions、tool set、workspace rules、runtime 和 output contract 版本化。
- [ ] **GAP-AGENT-05 — 独立 agent-worker。** 从通用 finite worker 中分离明确的 Agent Worker ownership、健康检查、资源限制和生命周期；Compose 拓扑与设计一致。
- [ ] **GAP-AGENT-06 — App Server 协议固定。** 精确 pin 官方 Codex App Server/SDK，生成并测试 JSON/TypeScript schema；不把 experimental WebSocket、dynamicTools 或 Project API 作为正确性前提。
- [ ] **GAP-AGENT-07 — Thread resume/retry。** 同一 Mission crash 后可恢复 durable Thread；不能恢复时创建新 attempt，保留已完成 idempotent side effects、Search Ledger 和 artifacts。
- [ ] **GAP-AGENT-08 — Mission Tool Server。** 实现 mission-scoped stdio MCP，至少覆盖 dataset、experiment、artifact、alpha、calibration、portfolio、evidence 和 result reporting 工具。
- [ ] **GAP-AGENT-09 — Tool 强制授权。** 每次调用重新校验 Mission state、capability、resource scope、expected revision/state 和 idempotency；Agent 不能调用 Approval、Handoff publish、Secret、Plugin activation 或 Admin mutation。
- [ ] **GAP-AGENT-10 — Workspace Manager。** 按 Program private repo、Research Branch、Mission 独占 worktree 管理 lease、create/remove、accept changes、commit 和 revision；Codex 不执行 branch/merge/rebase/worktree 管理。
- [ ] **GAP-AGENT-11 — OS 级隔离。** 除 developer instruction 外，用 mount、用户权限、容器/进程和 sandbox 确保 Mission 无法读取 Core source、其他 Program、sealed data、Secrets、数据库和 Docker socket。
- [ ] **GAP-AGENT-12 — 网络边界。** 默认网络关闭；真实 command preflight 证明外网和禁止路径不可达，批准数据只能经 MCP 能力获取。
- [ ] **GAP-AGENT-13 — Secret 隔离。** Provider key one-shot broker、环境清理和不落盘规则有端到端测试；Operator、DB、Data 和 Downstream secret 均不能进入 App Server、shell、事件或 RESULT。
- [ ] **GAP-AGENT-14 — Event Projection。** 投影 thread/turn/item、command、file change、MCP call、plan、diff、exit status 和 runtime error；不保存隐藏 chain-of-thought。
- [ ] **GAP-AGENT-15 — 结构化结果接收。** `RESULT.md` 只能作为可读 artifact；正式结果必须通过 schema、artifact validation、Domain Validator 和对应 Gate 后才能生成下游 Mission 或业务对象。
- [ ] **GAP-AGENT-16 — 调度公平与收敛。** 实现并发 slot、fair scheduling、重复拒绝、有限 Mission、Cooling/Wake 和 runaway 防护；不得用无限 Thread 或无限“继续研究”循环。
- [ ] **GAP-AGENT-17 — 真实 Codex E2E。** 使用 pinned App Server 在隔离环境运行一个 Mission，调用受限 MCP、生成验证产物、模拟 crash/resume，并证明 Secret/forbidden path 不可达。

## 10. 独立评估、Search Exposure 与统计隔离

- [ ] **GAP-EVAL-01 — 独立 evaluator 服务。** Compose 中运行独立 evaluator，不能挂载 Codex workspace/CODEX_HOME，不能使用 Agent Tool，只能访问分配的 sealed Revision 和候选 artifact。
- [ ] **GAP-EVAL-02 — 三层证据区。** Discovery、Sealed Promotion、Forward Evidence 在存储、权限、数据引用和 API 上明确分离。
- [ ] **GAP-EVAL-03 — Evaluation Episode 状态机。** 实现 PLANNED、SEALED、ASSIGNED、EVALUATING、EVALUATED、DISCLOSED、CONSUMED、FAILED、INVALIDATED 及合法转换。
- [ ] **GAP-EVAL-04 — Evidence Exposure Graph。** 记录任何被 Codex、人类或后代候选看到的独立评估信息，并沿 Program/Branch/Alpha/Candidate lineage 传播。
- [ ] **GAP-EVAL-05 — Episode 消费规则。** 任何有助于后续研究的信息披露后，该 Episode 对相关 lineage 永久 CONSUMED；复制 Program、换 Thread 或重命名模型不能清零。
- [ ] **GAP-EVAL-06 — Deterministic Disclosure。** Level 1 仅返回设计允许的分类，不泄漏日期、instrument、精确指标、阈值差距或调参方向；映射由代码 policy 完成，不由 LLM 自由总结。
- [ ] **GAP-EVAL-07 — 完整私有结果。** evaluator 向 Core 保存完整私有结果和 policy disclosure；人类 Level 2 报告与 Codex Level 1 输出严格分离。
- [ ] **GAP-EVAL-08 — 评估可重放。** 输入 Revision、candidate artifact、policy version 和 runtime 固定；相同输入结果确定，失败和 invalidation 有审计依据。
- [ ] **GAP-EVAL-09 — 泄漏测试。** 自动测试证明 Codex、Mission filesystem、MCP、日志、错误、前端和 Package 均不能获得 sealed raw data 或被禁止的明细。
- [ ] **GAP-EVAL-10 — 评估失败语义。** Data Quality failure、基础设施失败和候选统计失败分开处理；只有有效独立评估才能用于 Qualification 或 Portfolio Promotion。

## 11. Alpha Library

- [ ] **GAP-ALPHA-01 — 版本对象完整。** FeaturePipelineVersion、AlphaModelVersion、AlphaCalibrationVersion、AlphaQualification 和 Evaluation 引用均为明确关系，而不是仅保存几个可空 UUID/JSON。
- [ ] **GAP-ALPHA-02 — 双通道准入。** 实现 Standalone Quality Gate 和 Portfolio Contribution Gate，产生 PRIMARY_ALPHA、DIVERSIFIER_ALPHA、HEDGE_ALPHA、REGIME_SIGNAL、RISK_MODULATOR。
- [ ] **GAP-ALPHA-03 — Shadow 规则。** SHADOW_ALPHA 可参加受限 contribution research，但不能单独 Handoff、直接进入 Live 或宣称通过 standalone gate。
- [ ] **GAP-ALPHA-04 — Qualification 生命周期。** ACTIVE、WATCH、QUARANTINED、RETIRED、SHADOW 只按设计前进；旧 quarantined 版本不能恢复，必须创建新 Model/Calibration/Qualification。
- [ ] **GAP-ALPHA-05 — 可解释 evidence。** Library 展示 universe、horizon、role、search-adjusted evidence、成本、容量、稳定性、lineage、exposure 和 degradation，不只展示任意 metrics JSON。
- [ ] **GAP-ALPHA-06 — 无人工晋级捷径。** Web、CLI、Agent 和 API 都不能手动 activate/restore Alpha 绕过评估。
- [ ] **GAP-ALPHA-07 — 产物可复现。** 任一 Qualification 可从冻结 Dataset、Feature、Model、Calibration 和 evaluator policy 重建并核对输出。

## 12. Portfolio Construction

- [ ] **GAP-PORT-01 — Mandate Version。** objective、risk preferences、target behavior、concentration、turnover、capacity、roles、policy families、universes、rebalance、downstream compatibility 和 validity 条件不可变版本化。
- [ ] **GAP-PORT-02 — Capital Context。** 实现 ADMIN/Downstream feedback 来源的 CapitalContextVersion、有效期和 base currency；它是研究快照，不是 broker account。
- [ ] **GAP-PORT-03 — 自动创建 Portfolio Program。** Enabled Mandate + Qualified Alpha Pool + 可证明组合机会且无等价活跃 Program 时自动创建；没有 Alpha 时进入 WAITING_FOR_ALPHA。
- [ ] **GAP-PORT-04 — Eligibility 与角色池。** 冻结合格 Alpha snapshot，按角色、Universe、horizon、有效期、容量和 downstream compatibility 筛选。
- [ ] **GAP-PORT-05 — 冗余与共同来源。** 实现相关性、tail dependence、共同 feature/data/source 和 lineage clustering，避免多个表面不同但同源的 Alpha 被当作独立。
- [ ] **GAP-PORT-06 — Policy families。** 实现并验证 Equal Weight、Volatility Scaling、Risk Parity、HRP、Constrained Mean-Variance、Mean-CVaR；各自输入语义和约束明确。
- [ ] **GAP-PORT-07 — Portfolio Search Ledger。** 保存 Alpha subset、role、policy、constraint、rebalance、capital scenario 和所有淘汰结果，用于多重检验与 Material Improvement。
- [ ] **GAP-PORT-08 — Multi-Universe。** 处理 base currency、calendar、horizon normalization、cross-universe covariance/factor、tail dependence、liquidity、cost、capacity、currency 和 regime correlation。
- [ ] **GAP-PORT-09 — Portfolio-level Sealed Evaluation。** 组合候选必须经过独立评估、marginal contribution、权重稳定性、policy sensitivity 和 tail/concentration Gate。
- [ ] **GAP-PORT-10 — Immutable Candidate。** Alpha、policy、weight rule、Mandate、Capital Context、risk/cost/capacity、constraint、rebalance 或 contract 任一改变都创建新 Candidate。
- [ ] **GAP-PORT-11 — TargetPortfolioFrame。** 输出 as-of/effective interval、Universe、instrument、target weight、confidence、portfolio state 和 Candidate ID；禁止 BUY/SELL/order/TIF。
- [ ] **GAP-PORT-12 — Material Improvement。** 对现有推荐或已批准组合计算 search-adjusted edge、marginal contribution、稳定性、tail、成本、容量、解释性和 novelty；不足时不创建 Approval。
- [ ] **GAP-PORT-13 — 真实组合验收。** 从至少两个合格 Alpha 构造、评估并冻结一个可重放 Portfolio Candidate，证明约束、容量和权重一致。

## 13. Approval Snapshot 与 Candidate Package

- [ ] **GAP-APPROVAL-01 — Approval 自动产生。** 只有 Promotion Gate、唯一推荐、Material Improvement、Evidence maturity、无重复 pending、downstream preflight 全部通过才创建。
- [ ] **GAP-APPROVAL-02 — Paper/Live 分离。** 类型、证据、目标 downstream、有效期和批准记录独立；Paper 通过不改变 Live 状态。
- [ ] **GAP-APPROVAL-03 — Snapshot 冻结完整。** Candidate Package 必须先完成构建与 Reference Runtime 验证，随后与 Candidate、Evidence、Alpha/Calibration、Mandate、Capital Context、Policy、Risk/Cost/Capacity/Constraint、Downstream Connection 和 Contracts 一并冻结到 PENDING Snapshot；approve mutation 只能决定并发布该既有 Package，不得在审批时首次构建、替换或重绑定 Package。
- [ ] **GAP-APPROVAL-04 — Stale/Expired 自动判定。** 依赖变化、Qualification 隔离、Forward Evidence 推翻、Capital 超容量、连接/contract/preflight 变化或超时后不可批准，必须重新评估生成新 Snapshot。
- [ ] **GAP-APPROVAL-05 — 单一推荐和节流。** 同 Program 同时最多一个可行动 Approval；拒绝不递补第二名，没有新证据或实质改善不得重复打扰。
- [ ] **GAP-APPROVAL-06 — Reject contract。** 服务端校验固定 reason code 和 note；决定为终态并进入审计，Agent 只能按 disclosure policy 看到允许内容。
- [ ] **GAP-APPROVAL-07 — Human-only 强制。** Agent/MCP/Skill 自动流程不能执行 approve/reject；CLI 命令必须明确要求人类调用并重新读取 freshness。
- [ ] **GAP-PACKAGE-01 — Package 来自真实产物。** 删除“目标权重即 raw alpha”的占位生成逻辑；Feature、Alpha、Calibration、Portfolio Policy wheel 必须由已评估的冻结 artifact 构建。
- [ ] **GAP-PACKAGE-02 — 完整 manifest/schema。** manifest、canonical input、raw/calibrated alpha、TargetPortfolioFrame、evidence、lineage、runtime 和 contract versions 严格验证。
- [ ] **GAP-PACKAGE-03 — Reference Runtime conformance。** 在新的隔离环境安装冻结 wheel，运行 input Arrow fixture，并与 expected alpha/portfolio 输出逐字段验证。
- [ ] **GAP-PACKAGE-04 — 构建原子性。** staging、验证、归档、DB 记录和 AVAILABLE publication 失败时可清理/恢复；不产生半成品或数据库指向不存在文件。
- [ ] **GAP-PACKAGE-05 — 安全边界。** Package 不含 provider/data/downstream/broker secret、URL、account、order type、TIF、order ID、heartbeat、recovery 或 retry 指令。
- [ ] **GAP-PACKAGE-06 — 兼容与支持矩阵。** 固定 Python/runtime、wheel metadata、Arrow schema、Package contract、Downstream compatibility 和大小限制；不兼容时在 Approval 前阻断。
- [ ] **GAP-PACKAGE-07 — 完整性不依赖自定义 hash。** 使用显式 ID/version、长度、标准 wheel metadata/RECORD、ZIP 结构、schema 和 fixture execution 发现损坏。
- [ ] **GAP-PACKAGE-08 — Package 生命周期。** 失败、可用、领取、归档、保留和清理状态清楚；已批准/领取 Package 保持不可变且可审计。

## 14. Handoff、Feedback、Forward Evidence 与 Degradation

- [ ] **GAP-HANDOFF-01 — Downstream Connection Version。** 逻辑 Downstream System、版本化连接、Plugin Release、public config、credential set、Package/Feedback compatibility 和 preflight 分离。
- [ ] **GAP-HANDOFF-02 — Service token 生命周期。** 注册时只返回一次、加密保存、可轮换、可撤销、作用域仅限对应系统；Operator credential 不能代替。
- [ ] **GAP-HANDOFF-03 — 完整 Handoff 状态机。** APPROVED/PUBLISHING/AVAILABLE/CLAIMED/DOWNSTREAM_ACCEPTED/DOWNSTREAM_REJECTED/FEEDBACK_* 及异常状态、deadline 和事件全部实现。
- [ ] **GAP-HANDOFF-04 — Claim 与 revoke 原子竞争。** PostgreSQL row lock 测试证明 AVAILABLE 只可能被一次 claim 或 revoke；CLAIMED 后 QuaZonai 永远不能 revoke/stop/undeploy。
- [ ] **GAP-HANDOFF-05 — Downstream Package 消费。** 提供 Reference Consumer/fake downstream，使用 service token claim、下载、验证、accept/reject，并输出 contract-valid feedback。
- [ ] **GAP-HANDOFF-06 — Feedback Contract 完整。** observation duration、sample size、required fields、deadlines、grace、accepted contracts、disclosure 和 partial/stale/invalid 规则冻结。
- [ ] **GAP-HANDOFF-07 — 幂等与乱序处理。** 重复 claim/accept/feedback、网络重试、迟到消息和状态乱序不能重复生成 Forward Evidence 或破坏终态。
- [ ] **GAP-HANDOFF-08 — Forward Evidence。** 只有完整、有效、contract-compatible 的反馈形成 Episode；部分、迟到、缺失和 invalid 是证据质量问题，不自动判定 Candidate 失败。
- [ ] **GAP-HANDOFF-09 — Live Promotion Gate。** Live Approval 必须引用仍有效的完整 Paper Forward Evidence、当前依赖和独立 re-evaluation。
- [ ] **GAP-HANDOFF-10 — Degradation Policy。** 实现 Alpha/Portfolio health、持续时间、严重性、统计置信、跨 Episode 一致性和 Mandate 影响；状态按 HEALTHY/WATCH/DEGRADING/INVALIDATED 前进。
- [ ] **GAP-HANDOFF-11 — Research Wake-up。** 达到 policy 门槛时创建 Diagnostic Mission 或唤醒 Program，并保留原因、证据和 lineage；不得自动换仓或停止下游。
- [ ] **GAP-HANDOFF-12 — 真实闭环反馈验收。** Paper downstream 完成观测后返回真实格式反馈，系统生成 Forward Evidence、更新 health，并在构造的退化场景下触发 Research Wake-up。

## 15. Runtime Plugins

- [ ] **GAP-PLUGIN-01 — Capability contract。** 只允许 DATA_CONNECTOR、DATA_TRANSFORM_ADAPTER、RESEARCH_ADAPTER、HANDOFF_CONNECTOR；descriptor、entry point、API version、配置和 secret schema 可验证。
- [ ] **GAP-PLUGIN-02 — 真实调用链。** Plugin 不只完成 wheel 安装/激活，还必须由 Data/Handoff runner 通过固定 release 和 runtime bundle 执行真实 connector 操作。
- [ ] **GAP-PLUGIN-03 — 版本并存与资源 pin。** 新 release side-by-side，既有 Data Source/Connection 固定具体 release；升级需要显式新版本和 preflight，不能静默漂移。
- [ ] **GAP-PLUGIN-04 — 进程隔离。** 第三方 plugin 只在 validator/connector child import；API、worker、agent-worker、evaluator 长进程不得 import，卸载依赖进程退出而非 reload。
- [ ] **GAP-PLUGIN-05 — Wheel 供应链限制。** 只接收 wheel，拒绝 sdist/editable/Git URL/远程动态下载；检查大小、文件名、metadata、entry point、依赖冲突、路径和解压炸弹。
- [ ] **GAP-PLUGIN-06 — 激活/排空/回滚。** RECEIVED 到 REMOVED 的状态、DRAINING、bundle stale、失败恢复、force 语义和 in-use 检查有并发测试。
- [ ] **GAP-PLUGIN-07 — Plugin secret。** 配置与 secret 分离，write-only 加密，按 connector runner 最小注入；不进入 descriptor snapshot、事件或 Agent。
- [ ] **GAP-PLUGIN-08 — 官方样例与合约测试。** 提供至少一个 Data Connector 和一个 Handoff Connector reference plugin，证明 SDK/contract 可被第三方重复实现。

## 16. API、SSE、CLI 与 Agent Skill

- [ ] **GAP-API-01 — API 与 DESIGN 对齐。** 补齐所有已承诺资源、过滤、详情和 mutation；删除未设计的旁路接口，OpenAPI contract test 覆盖稳定 error envelope。
- [ ] **GAP-API-02 — 请求语义一致。** 所有 mutation 支持 Idempotency-Key、expected state/revision、request ID、统一 4xx/409/422/5xx 分类和安全错误详情。
- [ ] **GAP-API-03 — 列表可生产使用。** Program、Mission、Alpha、Candidate、Approval、Handoff、Dataset、Event 等支持分页、稳定排序和必要过滤，避免无界返回。
- [ ] **GAP-API-04 — SSE 可恢复。** Domain Event 的持久 ID、Last-Event-ID、断线重连、认证变化、logout、重启和慢消费者行为有集成测试。
- [ ] **GAP-CLI-01 — CLI 实现与文档一致。** 实现或从文档删除 events watch、codex status/preflight、mandate show/enable/disable、capital context、data-source test、downstream create/preflight、plugin 生命周期等命令。
- [ ] **GAP-CLI-02 — 稳定机器输出。** CLI 使用统一 `{ok,data/error,request_id}` envelope、稳定退出码、`--json`、安全 secret 输入和不打印 credential；当前直接 dump API payload 的行为需统一。
- [ ] **GAP-CLI-03 — 认证边界。** CLI 只使用 machine API token，不读取 browser cookie/password/TOTP；downstream-owned API 只接受对应 service token。
- [ ] **GAP-SKILL-01 — Skill 与真实 CLI 一致。** Skill 的命令、认证、工作流、错误恢复和 human-only Approval 规则必须通过自动 contract test，不能引用未实现命令。
- [ ] **GAP-SKILL-02 — 外部 Agent 安全。** 外部 Skill 仅操作本地 API，不成为 built-in Mission runtime，不直接访问 DB/文件或执行审批；checkout 有效时服从 AGENTS/DESIGN。
- [ ] **GAP-API-05 — 兼容策略。** API/CLI/Package/Plugin contract 的破坏性变化有版本策略、升级说明和兼容测试。

## 17. Frontend 与用户可用性

- [ ] **GAP-FRONTEND-01 — 当前测试恢复全绿。** 修复基线 Frontend unit/interaction failure，确保 lint、typecheck、unit、build、Chromium E2E 全部执行且成功。
- [ ] **GAP-FRONTEND-02 — 页面连接真实领域。** Home、Idea、Research、Alpha、Portfolio、Approval、Handoff、Administration 不得依赖 production seed/mock；每个数据显示来源和刷新状态。
- [ ] **GAP-FRONTEND-03 — 状态完整。** 所有页面有 loading、empty、error、retry、stale、expired、permission/session loss 和 partial data 处理，不因单个请求失败白屏。
- [ ] **GAP-FRONTEND-04 — 领域约束在 UI 中成立。** 用户不能改 Candidate 权重、手工选第二名、修改冻结 Charter/Mandate、恢复旧 Qualification 或控制 claimed downstream。
- [ ] **GAP-FRONTEND-05 — 实时事件。** SSE 更新 Program/Mission/Approval/Handoff/Readiness，断线后从 last event 重放；重复事件不产生重复 UI 状态。
- [ ] **GAP-FRONTEND-06 — 国际化闭环。** English、简体、繁体、日语、韩语、西班牙语、阿拉伯语的 catalog、fallback、plural、日期/数字、`lang/dir`、RTL 和持久化测试全部通过。
- [ ] **GAP-FRONTEND-07 — 可访问性。** 键盘、焦点、ARIA、颜色对比、图表替代文本、表格、dialog、登录和审批流程达到可验证标准。
- [ ] **GAP-FRONTEND-08 — 性能与包体。** 路由懒加载、缓存、长列表虚拟化、图表清理、bundle budget 和真实数据规模下交互响应有基线测试。
- [ ] **GAP-FRONTEND-09 — 安全前端。** 不在 local/session storage 保存 bearer secret；敏感响应 no-store；下载、错误、自由文本和富文本防 XSS，生产 source map/调试信息策略明确。
- [ ] **GAP-FRONTEND-10 — 管理面完整。** Readiness、Codex provider、Worker limits、Data Source、Universe、Mandate、Capital Context、Downstream、Plugin、Storage 和 health 均可查看并遵循权限/secret write-only 规则。
- [ ] **GAP-FRONTEND-11 — 浏览器生产 E2E。** 从登录/直连模式开始，完成 Idea 到 Approval/Handoff/Feedback 的关键页面路径，并覆盖 session expiry、trusted browser、logout 和 stale decision。

## 18. 安全与 Secret

- [ ] **GAP-SEC-01 — 部署访问边界强制。** 生产暴露时必须选择并验证 QuaZonai Operator Authentication 或另一个明确受信的 TLS 访问边界；匿名 direct access 只能 loopback。
- [ ] **GAP-SEC-02 — TLS 与 proxy 配置。** 提供经过测试的 reverse-proxy/tunnel 配置，精确 trusted proxy CIDR、X-Forwarded-For 处理、HTTPS origin、Secure cookie、HSTS 和证书更新。
- [ ] **GAP-SEC-03 — 完整安全头。** 除 frame-ancestors/X-Frame-Options 外，评估并配置适用 CSP、X-Content-Type-Options、Referrer-Policy、Permissions-Policy 和 cache policy。
- [ ] **GAP-SEC-04 — Secret inventory 与轮换。** Master key、cookie key、TOTP、machine token、Codex provider、Data、Downstream、PostgreSQL credential 均有 owner、生成、存储、轮换、撤销和恢复手册。
- [ ] **GAP-SEC-05 — Master key 生命周期。** 明确备份和轮换方式，轮换时可重新加密数据库 secret；丢失时有明确不可恢复影响，不得把 key 写入 DB、镜像或日志。
- [ ] **GAP-SEC-06 — Secret 泄漏测试。** 日志、事件、API、SSE、error、process env、命令行、worktree、Package、Plugin descriptor 和 crash dump 自动扫描禁止 secret。
- [ ] **GAP-SEC-07 — 输入与上传防护。** 限制请求体、文件数、wheel/package 大小、压缩比、路径、文件名、Unicode、JSON 深度和解析时间；异常输入不耗尽资源。
- [ ] **GAP-SEC-08 — SSRF/endpoint 防护。** Codex custom Base URL、Data source 和 Handoff endpoint 的协议、credential、redirect、DNS/IP 范围和超时策略明确，禁止访问本机 metadata/内部敏感服务。
- [ ] **GAP-SEC-09 — 容器最小权限。** 非 root、read-only、cap drop、no-new-privileges、最小挂载、独立服务账户、网络分区和目录权限有运行验证。
- [ ] **GAP-SEC-10 — 数据库安全。** 生产禁用示例密码，连接加密/网络范围、最小权限、备份账号、迁移账号和应用账号分离策略明确。
- [ ] **GAP-SEC-11 — 依赖供应链。** 锁文件、依赖更新、漏洞扫描、CodeQL、secret scanning、SBOM、容器基础镜像 pin 和许可证扫描进入发布门禁。
- [ ] **GAP-SEC-12 — Threat model 与事件响应。** 覆盖 Operator、trusted browser、Codex、Plugin、sealed data、Package、Downstream、数据许可和单机失陷；定义检测、隔离、轮换、恢复和披露流程。
- [ ] **GAP-SEC-13 — 认证压力与时钟。** 登录退避、TOTP clock skew、重复 cookie、session/trusted credential、logout race、restart invalidation 和 token rotation 有负载/回归测试。
- [ ] **GAP-SEC-14 — 审计最小泄漏。** 审计记录 actor、action、resource、result、request ID 和时间，但不保存 password、TOTP setup secret、token、provider key 或隐藏推理。
- [ ] **GAP-SEC-15 — 安全发布复核。** 生产发布前完成独立安全 review，所有确认问题修复或在产品边界内形成明确、可接受、记录化的限制。

## 19. 部署、运行、可靠性与可观测性

- [ ] **GAP-OPS-01 — 生产拓扑符合设计。** Compose 至少形成 postgres、migrate、api、worker、agent-worker、evaluator 的清晰 ownership；服务挂载和网络只授予所需资源。
- [ ] **GAP-OPS-02 — 镜像可重复。** Python/Node/Codex/PostgreSQL/基础镜像和依赖精确固定，构建在干净 runner 可重复；发布镜像与测试镜像一致。
- [ ] **GAP-OPS-03 — Health 与 readiness。** 每个服务区分 liveness/readiness，验证数据库、迁移、目录、Worker lease、Codex、evaluator、data、downstream 和 storage；不能固定返回 SYSTEM_READY。
- [ ] **GAP-OPS-04 — Graceful shutdown。** API/SSE、Worker、Agent child、evaluator、plugin runner 在 SIGTERM 下停止领取新任务、完成/中断当前事务、释放 lease/worktree 和退出。
- [ ] **GAP-OPS-05 — Job 可靠性。** lease 续期、过期回收、attempt、重试分类、dead-letter/人工处置、幂等 side effect 和 worker crash 恢复有测试。
- [ ] **GAP-OPS-06 — 备份与恢复。** PostgreSQL、Dataset、Artifact、Package、Plugin runtime、Program repo 和必要 Codex state 的一致性备份方案、加密、保留和恢复演练完成。
- [ ] **GAP-OPS-07 — 升级与回滚。** 发布前备份、Alembic 升级、应用滚动/停机顺序、版本兼容、失败回滚和旧版本读取新数据的限制写入 Runbook 并演练。
- [ ] **GAP-OPS-08 — 存储容量管理。** 监控数据库、Dataset、Mission worktree、Codex data、Plugin、Package 和日志使用量；定义阈值、告警、GC、归档和只读保护。
- [ ] **GAP-OPS-09 — 资源限制。** 为 API、Worker、Agent child、evaluator、plugin runner 和 Postgres 配置 CPU、内存、PID、文件描述符、临时盘和 timeout；OOM/超时可恢复。
- [ ] **GAP-OPS-10 — 结构化日志。** 日志含 timestamp、level、service、request/job/mission/resource ID 和 error code，支持关联查询与 secret redaction。
- [ ] **GAP-OPS-11 — 指标与仪表盘。** 至少覆盖请求错误/延迟、DB pool、job queue/lease、Mission duration/failure、evaluator、data freshness、Approval/Handoff、SSE、存储和资源。
- [ ] **GAP-OPS-12 — 告警。** 对服务不可用、迁移失败、队列积压、数据断流、sealed evaluator failure、磁盘、备份、认证异常和下游 deadline 配置可操作告警。
- [ ] **GAP-OPS-13 — SLO 与支持范围。** 定义单用户单机支持的请求、Mission 并发、数据规模、恢复目标、备份目标和允许停机；指标可验证而非口号。
- [ ] **GAP-OPS-14 — Runbook。** 安装、启动、停止、登录、Codex auth、数据源、下游、Plugin、备份、恢复、升级、密钥轮换、磁盘满、任务卡住、评估失败和事故处理完整。
- [ ] **GAP-OPS-15 — 时间与时区。** 主机、容器和数据库使用可靠 UTC/时钟同步；TOTP、market calendar、available-at、deadline 和 event ordering 不受本地时区错误影响。
- [ ] **GAP-OPS-16 — 事件重放。** API/Worker 重启后由 PostgreSQL 事实重建 UI/调度状态，LISTEN/NOTIFY 丢失不影响正确性，SSE 从持久 event ID 恢复。
- [ ] **GAP-OPS-17 — 故障注入。** 演练 Postgres 短暂不可用、Worker crash、Codex crash、evaluator crash、connector timeout、磁盘接近满、网络中断和进程重启。
- [ ] **GAP-OPS-18 — 发布后验证。** 每次发布执行 migration preflight、health、核心 API、浏览器 smoke、Worker claim、数据读取、Mission sandbox、evaluator 和 Package/Downstream contract smoke。

## 20. 测试、质量与 CI

- [ ] **GAP-TEST-01 — 测试分层完整。** Unit、PostgreSQL integration、process isolation、Codex protocol、MCP authorization、sealed non-leakage、browser、fake downstream 和 production Compose 各自有真实边界测试。
- [ ] **GAP-TEST-02 — 研究正确性测试。** Point-in-time、survivorship、corporate action、cost、capacity、calibration、walk-forward、多重检验、determinism 和 baseline comparison 有固定 fixture。
- [ ] **GAP-TEST-03 — 并发测试。** Idempotency、stale revision、Program/Mission scheduling、Approval 决策、Handoff claim/revoke、feedback duplicate、runtime config 创建和 plugin lifecycle 使用真实 PostgreSQL 并发。
- [ ] **GAP-TEST-04 — 真实 App Server 测试。** 不能只 mock Codex lifecycle、thread resume、worktree、MCP hard deny 和 secret isolation；CI 或受控验收环境运行 pinned runtime。
- [ ] **GAP-TEST-05 — Package/Consumer 测试。** 构建真实 Candidate Package，在全新环境执行 Reference Runtime，再由 fake downstream claim/download/validate/feedback。
- [ ] **GAP-TEST-06 — 恢复测试。** 数据库迁移、备份恢复、job lease、Mission crash/resume、event replay、SSE reconnect、staging cleanup 和发布回滚自动或定期演练。
- [ ] **GAP-TEST-07 — 安全测试。** Auth、CSRF/origin、cookie races、proxy spoof、path traversal、malicious wheel/zip、SSRF、upload limits、secret leakage 和权限边界覆盖。
- [ ] **GAP-TEST-08 — 前端质量。** 七语言、RTL、accessibility、loading/error/empty、large list、chart lifecycle、session expiry、stale Approval 和真实 E2E 全部进入 workflow。
- [ ] **GAP-TEST-09 — 性能与稳定性。** API/DB/Arrow/Polars/evaluator/SSE/Worker 在支持规模下有 benchmark、长时间运行和资源泄漏测试。
- [ ] **GAP-TEST-10 — 迁移兼容。** 每个 Alembic 版本从受支持旧 schema 升级，应用在升级后读写核心对象，失败时有恢复路径。
- [ ] **GAP-TEST-11 — CI 必须检查。** PR 和 main 的适用 workflow 不允许被 path filter 意外绕过关键跨层验证；跳过的 job 必须符合明确规则。
- [ ] **GAP-TEST-12 — Flaky 管理。** 禁止简单重跑掩盖不稳定；记录、隔离原因并修复，主干持续保持全绿。
- [ ] **GAP-TEST-13 — 静态质量。** Ruff、mypy、TypeScript、ESLint、CodeQL、dependency/secret/license scan 和 schema contract 失败均阻断合并。
- [ ] **GAP-TEST-14 — 独立复核。** 每个跨边界功能有非作者 review；所有 PR 依项目规则请求 `@codex review` 并处理其所有有效问题。
- [ ] **GAP-TEST-15 — Release candidate 验收。** 从空卷安装、导入最小配置、跑完整纵向闭环、备份恢复、升级回滚和安全 smoke，产出不可篡改的运行记录。
- [ ] **GAP-TEST-16 — 证据可定位。** 测试名、日志、artifact 和 PR 能从 Gap ID 反查，避免“某个大测试通过”但无法证明具体条款。

## 21. 文档、合规与商业使用边界

- [ ] **GAP-COMPLIANCE-01 — 数据许可台账。** 对生产数据的采集、存储、研究、派生结果和向下游交付权限有书面依据和可审计记录。
- [ ] **GAP-COMPLIANCE-02 — 第三方许可证。** Python/Node/Codex/数据库/Plugin/镜像及打包 wheel 的许可证和 notice 完整，AGPL 分发/网络使用义务得到确认。
- [ ] **GAP-COMPLIANCE-03 — 投资风险表述。** 产品文档明确研究结果不保证收益、历史/回测局限、Paper 与 Live 分离及最终交易责任属于下游和操作者。
- [ ] **GAP-COMPLIANCE-04 — 数据与日志隐私。** 定义保存内容、保留期、备份、删除边界、自由文本和 operator-identifying logs；不保存隐藏推理或不必要 secret。
- [ ] **GAP-COMPLIANCE-05 — 模型/provider 条款。** Codex 和自定义 provider 的数据使用、保留、区域、密钥、模型版本和允许用途符合部署者选择；敏感 sealed data 不发送给模型。
- [ ] **GAP-COMPLIANCE-06 — 生产支持声明。** 明确支持的 OS/CPU/Python/PostgreSQL/浏览器、单机边界、外部 TLS 责任、数据源与下游兼容范围。
- [ ] **GAP-COMPLIANCE-07 — 变更审计。** 影响研究结论、数据、模型、Policy、Approval、Package、Downstream 或安全边界的变更可追溯到 actor、PR、版本和证据。
- [ ] **GAP-COMPLIANCE-08 — 无未经证明的生产声明。** 闭环验收和发布门槛全部满足前，项目状态保持 Engineering Prototype / Pre-Alpha 或其他事实一致的描述。

## 22. 必须通过的纵向闭环验收脚本

以下脚本是一个整体，不代表优先级。所有步骤必须在全新环境中连续完成，并保留 API、Domain Event、DB、Artifact、日志和测试证据。

- [ ] 使用固定 release artifact 和空白持久卷启动生产 Compose。
- [ ] 执行数据库 preflight/migration，所有服务达到各自真实 readiness。
- [ ] 启用 Operator Authentication，使用 password + TOTP 登录；验证 trusted browser、logout 和 machine CLI token。
- [ ] 注册一个真实或官方 sandbox Data Connector、一个 Market Universe、彼此独立且兼容的 Paper Downstream 与 Live Downstream，以及各自的连接版本和 Feedback Contract；两类 downstream preflight 均成功。
- [ ] Connector 拉取数据并生成通过 point-in-time/质量检查的 Dataset Revision。
- [ ] 用户提交一个边界明确的 Research Idea；系统在需要时只进行一轮澄清。
- [ ] 冻结 Research Charter，创建 Program、Branch、Mission DAG 和 durable jobs。
- [ ] Agent Worker 启动 pinned Codex App Server；Mission 仅可访问 worktree 和被授权 MCP。
- [ ] Codex 通过 MCP 使用批准数据/实验能力，产出结构化 artifact；Secret、sealed data 和禁止路径不可达。
- [ ] Orchestrator 连续推进 Data Quality、Feature、Alpha、Calibration、Robustness 和 Promotion Review Mission。
- [ ] Search Ledger 保存所有重要尝试，Evidence Exposure 记录披露。
- [ ] 独立 evaluator 在 sealed zone 执行 Episode，向 Codex 只披露允许的 Level 1 分类。
- [ ] 通过 Gate 的 Alpha 形成不可变 Qualification 并进入 Alpha Library。
- [ ] Enabled Mandate 与 Capital Context 自动触发 Portfolio Program。
- [ ] 系统构造多个候选、保存 Portfolio Search Ledger，并完成 portfolio-level sealed evaluation。
- [ ] 唯一推荐 Candidate 通过 Material Improvement 后，系统先从真实冻结 artifacts 构建不可变 Candidate Package，并在隔离环境通过 Reference Runtime fixture。
- [ ] 系统把该既有、已验证 Package 与完整依赖冻结到 Paper Approval Snapshot；Snapshot 进入 PENDING 前 Package 已可供人类审查。
- [ ] 人类在 Web 审查 Candidate、证据和所绑定 Package 后批准；Agent 无法执行同一动作，approve mutation 不重建或替换 Package。
- [ ] Paper Downstream 使用其 service token 原子 claim、下载、验证并 accept。
- [ ] QuaZonai 在 claim 后不提供 stop/revoke runtime；只能显示状态和 advisory。
- [ ] Downstream 返回 partial feedback，系统记录但不判定 Candidate 失败。
- [ ] Downstream 返回 complete contract-valid feedback，系统生成 Forward Evidence Episode。
- [ ] 在有效 Paper evidence 与已完成 preflight 的独立 Live Downstream 同时存在时，系统重新评估并可生成绑定该 Live 目标的独立 Live Approval；任一条件缺失时进入相应 configuration-required/blocked 状态，不创建不可行动 Approval。
- [ ] 注入性能退化 feedback，Degradation Policy 达到条件后唤醒 Program并创建 Diagnostic Mission。
- [ ] 重启 API/Worker/Agent/evaluator，系统从 PostgreSQL、事件和 artifacts 恢复，不重复副作用。
- [ ] 执行备份、清空环境、恢复，核对 Program、Evidence、Candidate、Approval、Package、Handoff 和 Feedback。
- [ ] 执行受支持版本升级和回滚演练。
- [ ] 所有适用 GitHub workflow、静态检查、安全检查、E2E 和 `@codex review` 通过。
- [ ] README、OPERATIONS、CLI、Skill 和 UI 能准确描述并操作刚刚验证的能力。

## 23. 关闭项目 Gap 的最终声明条件

只有当本文件所有 checkbox 均已勾选，并且每一项都能定位到合入 `main` 的实现、测试和运行证据时，才可以声明：

> QuaZonai 已完成 V1 自治量化研究与组合构造闭环，并达到其单用户、自托管产品边界内的生产级可用状态。

任何新发现的问题、失败场景、设计遗漏或运行限制，必须先作为新的 Gap 写入本文件并完成闭环，不得通过降低验收口径、修改措辞或跳过检查来宣布完成。
