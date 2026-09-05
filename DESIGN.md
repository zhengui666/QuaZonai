# QuaZonai 产品与架构事实源

> 重写基线：2026-09-05，Issue #62。本文是唯一完整的产品与架构事实源。
> **状态：Draft 集成实施中；目前只有部分 W0 原生兼容性代码及其执行证据，不是可交付的新系统。**

Issue #62 正文及规范附录 A、B 是本次交付的完整验收输入；本文将其架构约束纳入仓库。
不得以本文摘要、实现记录、局部 CI 或未实现项列表缩小该 Issue 的范围。发现差异时，
先补齐本文和实现，不得通过降低验收标准解决。`README.md` 是入口，`OPERATIONS.md` 是
运行说明，`CLI.md` 是操作合同，`docs/architecture/issue-62-execution.md` 只记录实际证据，
均不得覆盖本文。旧 #58 设计已原样保存为 `docs/architecture/legacy-issue-58-design.md`，
仅用于迁移对照，不再是新架构的实现计划。

## 1. 当前事实与目标严格分开

| 部分 | 当前事实 | #62 必须交付的目标 |
|---|---|---|
| 既有服务 | 旧 Python 后端和旧前端仍存在、仍是当前运行实现 | 完整 Rust 控制面和官方 Ant Design 前端；验收后整体切换并删除旧入口 |
| 原生科学计算 | `apps/qz-job` 的固定 FIXTURE 探针直接调用 skfolio、CLARABEL、Nautilus、Arrow | 独立隔离的真实研究、评估、组合和共享资金原生模拟 |
| Codex | Rust 测试程序验证原生 App Server 握手、账号读取、完整模型分页和 Thread 配置 | 真实账号、同一原生 Thread 内真实工具/Job 回读、独立 Reviewer、恢复和安全隔离 |
| PGMQ | 测试 SQL 验证原生事务回滚、重复投递、结果与确认回滚 | 与正式领域事实同事务的生产任务投递、租约、恢复、取消和去重 |
| 发布与自动化 | 新 Rust 系统尚无实际交付链 | 多 Alpha → Package → Approval → Paper/Live Handoff → Forward → Wake 全链路 |
| 运维与体验 | 新系统迁移、恢复、截图、移动/PWA、受保护验收尚未完成 | W0–W8、T01–T42 全部完成且证据绑定最新 Head |

本表不是拆分 Issue、另开缩小范围 PR 或把剩余部分移到 Future Work 的授权。
同一 Draft 集成 PR 必须承载完整工作包；W0 成功也不能合并或关闭 #62。

## 2. 产品职责与正常业务链

QuaZonai 是单 Operator、自托管的持续量化研究与目标组合工作台。正常用户负责提出
Idea 和审批交付；配置数据授权、模型账号、运行环境、组合约束及故障处理是低频管理。

```text
Idea → 冻结 ResearchBrief → 有预算的 ResearchCycle
  → 原生 Codex 提议并调用真实研究 Jobs → 独立 Review / 封存评估
  → 至少两个有效 Qualified Alpha → 原生组合优化 → 共享资金 Nautilus 模拟
  → 不可变 Target-only Package → Approval → Paper / Live Handoff
  → 真实 Forward Evidence → Degradation Observation → 受限 Wake / 新 Cycle
```

“没有合格候选”是正常终态，不允许不断重试直到得到赢家。暂停禁止启动新研究周期，
但应保存真实 Wake 事实；确定性再平衡不应偷偷重启 LLM 研究循环。

QZ 不拥有交易执行。不得新增券商/交易所凭证、订单、真实成交、仓位、账户、NAV 或
执行风控账本；不得启动、停止、撤单、平仓或恢复下游交易节点。交付只包含 Alpha
信号、目标权重、约束、版本与证据。Nautilus 回测内的模拟成交属于内部评估证据，
不属于真实执行控制或下游运行管理。

## 3. 目标模块与原生复用边界

```text
apps/qz              Rust API、Worker、CLI 及 MCP 入口（目标，未实现）
apps/qz-runtime      受信任远端 Gateway 与一次性 Job 生命周期（目标，未实现）
apps/qz-job          一次进程的原生计算；当前只实现 W0 探针
crates/qz-domain     无 HTTP/SQLx 的领域决策与状态转移（目标，未实现）
crates/qz-contracts  Serde / OpenAPI / JSON Schema / Arrow 合同源（目标，未实现）
crates/qz-store      SQLx、PostgreSQL、PGMQ 与事务（目标，未实现）
crates/qz-integrations  Codex / OCI / 原生存储和下游适配（目标，未实现）
frontend            React + TypeScript + 官方 Ant Design（目标，未迁移）
migrations          新数据库的显式迁移（目标，未实现）
runtimes            锁定的上游运行依赖；不是自建 Python Web 后端
contracts/generated 从正式合同和锁定原生工具生成的版本化文件（目标）
tests               单元、合同、真实原生、数据库竞争、安全、故障和 E2E
```

控制面、业务编排、持久化与外部 API 必须使用 Rust。Python 只能作为成熟原生科学库的
运行依赖、上游必要扩展点、隔离的研究输入或测试/构建工具；不能把新 Python 服务
包装在 Rust 外壳里。自建 queue、回测撮合、数值优化器、Agent Harness、OAuth 刷新、
重复前端基础组件均不属于允许范围。

优先复用 PostgreSQL/PGMQ、SQLx、成熟 Rust HTTP/CLI/MCP SDK、官方 Codex App Server、
Bollard/OCI、NautilusTrader、skfolio/CVXPY/CLARABEL、Optuna，以及确有需要且通过验证
的上游数据/研究组件。没有执行验证的候选依赖不能宣称生产可用；确需适配上游缺口，
必须记录版本、接口缺口、最小适配范围和替代方案，不能扩展为自建内核。

## 4. 当前 W0 实现的准确合同

### 4.1 原生计算探针

`qz-job verify-native --output NEW_DIRECTORY` 只接受不存在的输出目录，创建私有目录，
完成后退出。固定输入和所有输出均为 `origin=FIXTURE`、`deliverable=false`，不能进入
Qualification、Release、Paper/Live 或真实数据能力判断。

Rust/PyO3 直接构造并调用上游对象，不嵌入 Python 业务源代码：

- `optimization.rs` 调用 skfolio `MeanRisk` 和 CLARABEL。两资产样本协方差比例为 1:4，
  long-only、预算 1 的独立手工参考为 0.8/0.2，绝对容差 1e-5。参考仅用于测试，不得
  作为生产优化 fallback。通过上游 `scale_objective` 与原生求解容差保证精度，要求
  原生 `problem_.status == optimal`，拒绝非有限、维度错误或错误权重。
- `backtest.rs` 调用 Nautilus `BacktestEngine`、上游测试 Equity 和上游
  `EMACrossLongOnly`。要求真实处理 64 根固定 Bar 并产生原生成交报告；引擎成功或失败
  后均 dispose。它不是目标组合适配器，更不证明多 Alpha 共享资金验收完成。
- `arrow.rs` 调用原生 PyArrow IPC 写入、读取并比较 Table。没有成功报告的 Arrow
  文件不是成功结果，也不证明正式 Artifact 已被持久接受。
- `report.rs` 先写 `.pending` 文件、完成 JSON/换行/文件同步，再用同文件系统原子
  create-if-absent 发布正式报告，绝不覆盖现有报告。写入或同步失败时不能出现正式
  成功报告名；临时文件永不作为成功证据。该测试报告发布不是生产 Artifact Store，
  不宣称目录级崩溃一致性或 Job 完成事实。发布后的 stdout 关闭不能改写已提交结果。

当前验证组合为 Rust 1.90.0、PyO3 0.25.1、CPython 3.12.12、Nautilus 1.231.0、
skfolio 1.0.3、CVXPY-base 1.7.2、CLARABEL 0.11.1、PyArrow 25.0.0；平台为 Linux
x86_64，CI 验证 Ubuntu 24.04。`Cargo.lock`、科学库平台 hash lock 和 Codex npm lock
固定实际依赖；其他平台在独立验证前不属于已支持矩阵。

### 4.2 原生 Codex 合同探针

`apps/qz-job/examples/codex_contract.rs` 是测试夹具，不是生产客户端或 Agent Loop。
它启动锁定的原生 Codex 0.144.4，以新的 HOME/CODEX_HOME 和清空后白名单环境执行
initialize → initialized → account/read → 完整分页 model/list → thread/start。
默认 Thread 不传模型、推理强度、URL、provider 或 API key；另一 Thread 仅配置原生
模型列表声明支持的 effort，模型仍省略，用来检验两项控制独立。新 HOME 中不得出现
继承的账号或 auth.json。原生 schema 由同版本二进制生成，不以最新网站协议代替。

协议夹具有 frame、队列、总截止时间、分页和重复 cursor/ID 限制；拒绝所有意外服务端
工具/审批请求，不保存原始通知、账户身份、配置转储或隐藏推理。报告明确标记真实
账号验收和同 Thread 推理工具闭环尚未执行。它们不能由这个无账号探针替代。

### 4.3 原生 PGMQ 合同探针

`tests/native/pgmq_contract.sql` 仅在可丢弃的 PostgreSQL/PGMQ 1.10.0 中执行；无端口
公开、无生产账号。测试业务写入和 send 同事务 rollback、可重复投递且 read count
增加、结果与 archive 一起 rollback，以及成功 archive 后不再投递。它明确证明的是
至少一次投递，不是“全局恰好执行一次”。测试临时表不是生产 Run/Attempt 持久化。

## 5. 正式字段与跨接口基本规则（目标）

正式合同使用 `schema_version=1`，拒绝未知字段。ID 使用 UUIDv7；版本/事件序号/计数
使用 BIGINT 范围并以十进制字符串传入 JSON；revision 从 1 开始，更新显式携带
expected_revision。金额和精确权重采用 NUMERIC(38,18)/十进制字符串，不使用 JS
浮点承载。UTC 时间使用 RFC3339，纳秒使用十进制字符串；拒绝无时区时间。

统计值只能是有限浮点或 null。null 必须携带 `status`、`reason`、`method`、`unit`、
样本数和有效区间，不能把错误、不支持、无样本或 NaN 填成 0。

关系必须有真实外键；跨项目资源使用 `(project_id, resource_id)` 等复合约束，不允许
仅凭任意 UUID 关联。正式版本不可变；修订创建新版本及显式关系，撤销是追加事实。
禁止 CASCADE 删除证据历史。互相引用的版本先使用合法 nullable 草稿或 deferrable FK，
不能插入虚构占位 ID。

业务身份、幂等、审批、发布、Package 和数据有效性不引入 SHA/hash/checksum/digest/
fingerprint gate；它们依赖 UUID、关系和原生版本。依赖供应链与原生存储自身完整性
校验不是业务内容寻址，不得反过来成为自制业务可信度证明。

### 5.1 领域事实目录

下列字段组定义目标模型职责；其必填性、精确枚举、约束及接口映射必须逐项落实
Issue #62 规范附录 A、B，不能用一个无约束 JSON 列代替正式实体。

| 事实 | 必须保留的身份、字段与约束 |
|---|---|
| research_lineages / projects | parent/root lineage、origin、项目状态 DRAFT/ACTIVE/PAUSED/ARCHIVED、当前 Brief/Policy 引用 |
| research_briefs / brief_data_bindings | 冻结版本、假设/经济含义、Universe、周期/目标单位、预算、评估/执行约束；数据 discovery/validation/sealed/forward 角色与访问域 |
| research_cycles | 有界 turn/experiment/CPU/wall/memory/token/软费用/修复/并行预算、reserved/used、截止时间、正常无候选终态 |
| data_sources / universe_versions / benchmark_versions | 上游来源、许可用途、能力快照、历史成员/退市/日历及版本，不把当前成分股倒灌到历史 |
| dataset_revisions | event_time/available_at、schema/datatype、PIT/质量/修订策略、原生版本、REAL/SYNTHETIC/FIXTURE/LEGACY_UNKNOWN |
| execution_assumptions | 原生市场/引擎、费用/撮合/滑点/延迟、初始资金/币种、日历/结算、参与率与可支持的市场范围 |
| experiment_families / experiments | 根谱系、父提议、代码/参数、Optuna study/trial、输入、Run、结果与全部尝试历史 |
| artifacts / input_sets / input_set_items | 原生对象及版本、media/schema、producer attempt、访问域、provenance、retention；typed artifact XOR dataset 引用 |
| alphas / alpha_versions / calibrations | Alpha 状态、版本/代码/模型、输出单位/预测区间、训练截止、原生校准方法/有效性及真实评估引用 |
| evaluation_policies / evaluations | 冻结切分/封存/最小样本/必需指标；Alpha XOR Candidate；执行状态与 VALID/INVALID/INCOMPLETE/UNSUPPORTED、PASS/REJECT/INCONCLUSIVE 分开 |
| metric_values | value/null、status、reason、unit、频率/年化、n_obs、样本区间、原生方法与 source |
| qualifications / qualification_revocations | 不可变资格、评估/政策/版本/有效期，追加撤销；被撤销资格不能继续授权新交付 |
| evidence_exposures | 根谱系、分区、actor/目的、首次实际读取前插入；新 UUID、分支或崩溃重试不能清零泄漏计数 |
| portfolio_mandates | 冻结目标/风险度量、币种/资金、Universe、原生估计/优化/ensemble、约束、再平衡、容差与执行假设 |
| portfolio_candidates / candidate_alphas / candidate_targets | 原生 solver status、预测/协方差/诊断/目标 artifacts、至少两个有效 Alpha/Calibration/Qualification、当前权重来源、现金与 instrument target weights |
| releases | 不可变 package 原生版本、Mandate/Candidate/Evaluation/市场能力版本、as_of/valid_from/valid_until、DEMO/REAL |
| runs / run_attempts / run_events | 输入与 Cycle、状态、attempt_no/owner_epoch、DB lease/deadline、外部稳定 Job ID、派发/运行状态、manifest 接收、终态与单调事件序号 |
| codex_sessions | RESEARCHER/INDEPENDENT_REVIEWER、原生 thread/turn、profile、请求配置及观察到的 actual 配置、native history 引用和允许公开的结构化摘要 |
| automation_policies / policy_revocations | MANUAL/AUTO_PAPER/AUTO_HANDOFF、真实 Operator 授权、scope/有效期、绑定 Mandate/下游、Paper 窗口、反馈新鲜度与再平衡限制 |
| approvals / approval_revocations | 冻结 Release、PAPER/LIVE、下游、OPERATOR/FROZEN_POLICY、授权证据及有效期、追加撤销 |
| handoff_offers | Release/Approval/下游/环境/序号绑定，OFFERED/CLAIMED/ACKNOWLEDGED/REJECTED/REVOKED/EXPIRED，外部 claim 与 revision |
| forward_messages / forward_evidence_windows | 下游 external message ID、流序号/revision/supersedes、覆盖区间/样本/报告、连续性/完整性/新鲜度与正式 Evaluation |
| degradation_observations / wake_events | 真实评估与 HEALTHY/WATCH/DEGRADED/INSUFFICIENT_DATA；原因、not_before、Cycle、抑制/消耗状态与受限新研究 |
| runtime_integrations / downstream_integrations | TLS endpoint、受限 credentials reference、能力/协议/接受版本/环境及快照 revision |
| codex_profiles | SYSTEM/CUSTOM_PROVIDER 与 MANAGED_VOLUME/OPERATOR_MOUNT 两条独立轴；home/key reference、保留的 model/effort/fast/default 设置及 revision |
| operator_auth_state / trusted_devices | 初始化状态、原生 TOTP secret reference、last accepted step、auth epoch；设备随机令牌 verifier、撤销与 epoch |
| command_receipts | scope + operation + idempotency key、规范化 typed 非秘密请求、资源 ID/status/有效期；同 key 不同请求返回 409 |

## 6. 研究、封存与数值诚实

所有特征/标签的可见性按 `available_at <= decision_at` 验证，包含源数据修订、时区、
日历、退市及历史 Universe。数据质量或许可不足要显式拒绝，不允许把生成数据标成
REAL。Fixture/Demo 可以用于演示和测试，永远不能获得正式 Qualification/Release。

封存访问计数沿根谱系持久化，必须在真正取数之前提交；崩溃、新分支、重复调用不能
擦除暴露。研究者不能读取 evaluator-only 对象。Reviewer 使用独立上下文及职责，
不能自评、自批或把隐藏 CoT 当成评审事实。

切分、purging/embargo、校准、优化和指标复用经过数值验证的上游。固定 horizon 的
原生支持不等于变长标签区间已支持。CPCV 不等于 PBO；未验证或上游不支持的 DSR/PBO
必须是 UNSUPPORTED/null，政策要求它们时结果为 INCONCLUSIVE，不能手写近似冒充。
必须保留失败/无效/负收益/无候选实验及选择次数，不能只展示赢家。

正式 Alpha Arrow 合同包含 instrument_id、as_of/available_at/horizon_end UTC ns、
nullable finite score/expected_return/uncertainty、coverage、alpha_version 及 dataset/
unit/currency 元信息。Score 不是收益；必须通过仅使用训练期证据的原生校准形成有
明确单位和周期的 expected return，才可参与要求该单位的组合。

## 7. 多 Alpha、组合与原生共享资金模拟

正式 Candidate 至少引用两个仍有效、单位/预测周期兼容的 Alpha Qualification；不得
用两个拷贝 ID 或单 Alpha 100% 权重绕过。冻结 Mandate 包含原生预测/ensemble/协方差/
风险估计及优化配置，预算、现金、资产边界、gross/net、turnover、行业/组别、交易
费用、流动性/参与率和执行假设。当前权重必须来自明确 Forward Snapshot 或 Last Target，
没有时记录 NONE，不捏造实盘仓位。

所有优化由 skfolio/CVXPY/原生 solver 承担。Infeasible、失败、维度/单位错误、非有限
结果产生显式诊断，不产生权重，不使用等权、单资产全仓、静默放宽约束等 fallback。
共享资金组合必须真正进入同一个 Nautilus 引擎；不能把独立回测曲线加权求和当成共享
资金模拟。撮合、费用、成交、保证金/结算等由声明支持的上游能力决定。

市场能力必须有版本、到期日、合约/结算/日历覆盖和对应原生测试。未验证市场不得
领取正式交付。固定 AAPL 示例回测不构成美国股票、期货或预测市场的生产能力认证。

## 8. 可靠执行与恢复

正式 Run 状态为 QUEUED、DISPATCHING、RUNNING、RECONCILING、CANCEL_REQUESTED、
SUCCEEDED、FAILED、CANCELLED。Attempt 有独立编号、owner_epoch、DB lease、deadline、
派发 NOT_SENT/SENT_UNKNOWN/ACKNOWLEDGED/TERMINAL、原生运行状态及 manifest 接收事实。
结果接受和终态转移必须校验当前 owner/epoch/attempt，旧 worker 不能覆盖新事实。

同一 PostgreSQL 事务写业务事实、事件及 PGMQ send。至少一次投递是常态；外部 stable
Job ID 与 durable tombstone 用于重连、lost-submit-ack 和重复投递对账，不靠内存字典。
结果持久化并成功 CAS 终态之后才能确认队列消息；发出取消不等于已经取消，必须等待
真实停止确认。租约过期不直接推导任务失败或重新执行，应先 reconcile。

事件序号由同一 Run 行锁/CAS 分配并持久化。SSE `run_id:seq` 是唯一重放顺序，支持
Last-Event-ID、重复去重与过期游标明确 410；NOTIFY 只是唤醒提示，不是事实源。
不得持有数据库事务/行锁等待 HTTP。Readiness 先获取，再在事务内验证快照 revision、
TTL 与所绑定的政策/资格/数据/运行能力，阻止陈旧检查授权新动作。

## 9. 原生 Codex 与秘密隔离

SYSTEM 使用 Worker 明确指定的 CODEX_HOME，保留原生 auth.json、config 和订阅登录，
不注入 QZ provider/key/base_url；CUSTOM_PROVIDER 只使用显式授权的对应配置，不得在
失败时静默回退到另一账户。认证选择与 Home 的 MANAGED_VOLUME/OPERATOR_MOUNT 来源
互不混用。不得实现自己的 OAuth token refresh 或要求用户导出浏览器秘密。

保留 Operator 保存的 model、effort、fast_mode；use_default_model_settings=true 时
只屏蔽覆盖参数，不删除保留值，恢复自定义时仍可恢复。模型省略但 effort 非空是合法
独立配置。模型和支持的 effort 来自全部分页后的真实原生列表；actual model/effort
只在原生明确确认时记录，否则为 unknown，不能把 requested 值冒充 actual。

生产协议来源是锁定二进制生成的 schema，使用稳定 stdio initialize/initialized、
Thread/Turn、模型/账号与配置 API；不依赖实验 WebSocket/dynamicTools 绕过权限。
真实原生 Thread 必须发起真实工具调用、提交隔离 Job、读取真实结果并继续同 Thread；
Python/HTTP echo、拼接日志、模拟 reasoning 或假响应均不等于该闭环。

原生账号 account/login/start、device 流程、取消和 logout 由 Codex 执行；QZ 只呈现
受控状态与用户操作，不保存多余令牌。原生隐藏推理、auth 内容、请求 header 与任意
traceback 不能进入公共事件、SSE、模型报告或 CI artifact。

Trusted Codex 进程可访问其授权账号卷，不代表其不受信任 shell/研究代码也能访问。
实际文件系统测试必须证明 untrusted 工作区/子进程无法读取 auth、数据库、sealed
数据、其他工作区、宿主仓库或 Docker socket。仅删环境变量、正则过滤或文档承诺
不能作为隔离通过；当前无账号协议探针没有完成该生产安全验收。

## 10. 远端 Runtime 与 Artifact 边界

远端 Rust Gateway 使用 TLS、明确 allowlist、请求大小限制、能力快照和协议版本；
拒绝 SSRF、DNS rebinding、意外 redirect、任意 URL/宿主路径/环境变量或任意命令。
Gateway 是唯一有权访问本地 Docker/OCI socket 的组件。每个 Job 是独立容器/进程，
非 root、只读根文件系统、无默认网络、capabilities drop，并强制 CPU/内存/PID/时限/
输出字节限制；不允许共享 Python 解释器进程池承载不受信任研究。

JobSpec 绑定 run_id、attempt、owner_epoch、stable external ID、原生镜像/协议版本、
明确输入原生对象版本、资源限制、deadline 和允许的输出 schema。Gateway 的接受、
状态、取消、结果 manifest 与 tombstone 均须跨进程重启保留。

结果先由受控接收者校验 schema/provenance/native version/size，再进入持久化 Artifact
与 Run CAS；路径名、远端 success 字符串、进程 exit=0 或文件存在都不是完整验收。
Artifact 使用原生对象版本和权限域，研究者不能猜 UUID 读取 sealed/Operator/Delivery
域。错误有稳定公开 code，详细敏感诊断只进入受控审计，不把异常文本拼入公开响应。

## 11. Package、Approval 与 Handoff 的事务边界

不可变 Target-only Package 必须在审批之前形成，绑定 Candidate、Mandate、Evaluation、
原生 Artifact 版本、市场能力、环境、as_of 与有效期。新 Package 必须产生新 Release，
不能原地修改已批准内容。Package 只含目标权重/信号/约束及证据引用，不含订单量、
账户状态、执行命令或秘密。

审批验证最新 Qualification/revocation、冻结 Policy、REAL provenance、数据和评估
新鲜度、市场/下游 readiness 与到期时间。领取时在事务内再次验证，claim 与 revoke/
expire 竞争必须有唯一结果。下游只能领取绑定自己的 offer，ack 使用外部稳定身份和
幂等键。已经 claim 的外部交付不能通过 QZ 伪造撤单/停止；撤销只能阻止未来领取并
提供明确 advisory 或新 Release。

AUTO_PAPER/AUTO_HANDOFF 必须有真实 Operator 冻结授权，限制有效期、Mandate、下游、
环境、Paper 样本/连续时长、反馈新鲜度及最大日内再平衡。配置一个布尔开关不等于
授权，不得用演示数据或过期政策自动 Live。

## 12. Forward、纠正与真实 Wake

Forward 使用下游 external message ID 去重，保留 stream sequence/revision/supersedes
和真实覆盖区间；纠正不得再累计成第二份独立样本，重叠窗口不得双计。形成连续、
完整、足量、新鲜的 Evidence Window 后才允许评估或 Paper→Live 判定。

Degradation Observation 必须来自实际完成的有效评估；无数据不是退化，状态明确为
INSUFFICIENT_DATA。Wake 由真实观察或明确授权事件产生，持久化 cause、cooldown、
not_before、budget、消耗与抑制状态。定时扫描不能凭空制造退化，暂停也不能丢失事实。
确定性再平衡、继续监视和重新研究是不同动作，必须有不同授权和预算。

## 13. API、CLI、MCP 与前端合同（目标）

新业务 API 使用 `/api/v2`：auth/bootstrap、projects/briefs/cycles、experiments、data、
alphas/evaluations、portfolios/candidates、releases/approvals/handoffs、forward、policies、
runs/events/artifacts、settings/codex/models/login、readiness/integrations/migration。
所有写接口带适用的幂等键和 expected_revision；同键不同 typed 请求返回 409；长任务
返回 202 与真实 run_id。错误使用统一 Problem code、field_errors、current_revision、
retryable 与 request_id，不能返回隐藏推理或任意异常文本。

CLI 与 Web 共享同一领域应用层和权限规则，不允许 CLI 直接插表或提供管理员旁路来
完成 E2E。MCP 复用官方 rmcp，仅提供 Cycle-scoped 的 research.get_brief、data.describe、
research.search_history、experiment.propose/validate/run、artifact.submit、run.get、
evidence.read、portfolio.propose、research.conclude 等允许工具；没有审批、凭证读取、
任意 HTTP、任意命令或任意路径工具。工具参数、返回值及 Artifact schema 来自同一
Rust 合同源；代码生成产物在 CI 验证没有漂移。

前端整体采用官方 Ant Design、官方 icons 与单一图表方案，不保留 Radix/自建基础
组件与重复图表库作为新系统永久兼容层。工作台覆盖 Idea/研究活动、Alpha 证据、
组合候选与审批、Handoff/Forward、运行与设置；每个不可操作状态显示后端返回的原因。
390/768/1440 宽度均可完成 PC 全部动作，不靠隐藏按钮假装响应式。图表支持证据追溯、
失败/空态、时间单位与方法说明，禁止只展示漂亮收益曲线。

PWA 只缓存允许的静态资源，敏感 API、SSE、认证和写操作不离线缓存；更新提示由真实
Service Worker 生命周期驱动，由用户确认升级，保护未提交表单并避免循环刷新。验证
键盘操作、屏幕阅读器、焦点、色彩、Reduced Motion、移动安全区域及 a11y。

## 14. Operator 与机器身份

浏览器正常登录只输入 Google Authenticator-compatible 6 位 TOTP 动态码，不展示、缓存或提交 username/password。

首次初始化必须使用本地受控 bootstrap，不能让公网第一个访问者抢占系统。二维码/
setup secret 只在受控初始流程展示，确认一次有效 TOTP 后绑定；last accepted step 的
CAS 防止同一时间步重放，统一错误及限速防止探测。信任浏览器是显式可撤销的独立
随机凭证，绑定 auth epoch；重置/恢复必须使旧设备与会话失效。

会话 cookie 使用 Secure/HttpOnly/SameSite，所有写操作验证同源及 CSRF；不得把 API
暴露等同于匿名可写。机器/CLI 使用范围受限的独立 token；既有 CLI 的
`QUAZONAI_API_TOKEN` 不得由浏览器 cookie、TOTP setup secret 或一次性动态码替代。
Agent/Skill 永不读取、推断、复制或打印浏览器秘密。密钥加密/随机 token verifier
复用成熟密码学组件，不自制认证算法；密码学身份保护不是业务内容 hash gate。

## 15. 迁移、运维与文档

新数据库/API v2/volume 明确隔离。迁移先备份，提供 dry-run、映射清单、外键与计数
核对；旧事实标记 legacy_unknown，旧审批只读，禁止重放为 Live。不能覆盖旧库后才
发现无恢复路径，也不能把双后端长期兼容当成完成。切换后删除旧服务入口、重复
依赖/文档和失效 CI，而不是在新架构尚未可用时先删旧测试制造绿色。

提供实际健康/就绪检查、超时、队列积压、租约和受控日志；备份必须执行真实恢复演练。
验证断网、重启、低磁盘、失联 Gateway、迟到结果、重复消息及身份撤销；没有实际
readiness 的组件返回明确未就绪，而非健康假阳性。

README 对标优质开源项目，清楚说明用途/不做什么、真实可运行 Quickstart、依赖矩阵、
架构、数据/模型/凭证配置、许可、贡献和故障排查。截图由真实运行页面产生，命令在
干净环境执行；没有执行的流程不得写成“一键可用”。LICENSE/NOTICE 保持项目 AGPL
要求，同时遵守各上游组件许可证。

## 16. 完成边界与证据判定

W0 原生复用/协议/隔离/锁定；W1 模型迁移；W2 Rust API/Worker/CLI/MCP；W3 原生 Agent
与恢复/隔离；W4 PIT/研究/独立评估；W5 多 Alpha/组合/交付/Forward/Wake；W6 Ant Design
完整 UI/移动/PWA；W7 运维/文档/清理；W8 T01–T42 最终验收，全部属于同一个 PR。

T01–T42 包括冷启动、无账号演示、SYSTEM/独立 effort/CUSTOM/分页、受保护真实账号、
同 Thread 工具/Job、独立 Reviewer、预算、PIT/根谱系暴露、原生切分/不支持指标、
数值参考、多 Alpha/不可行/成本/共享资金/市场能力、事务/租约/提交恢复/结果确认/
取消/SSE、不可变 Package/领取撤销竞争、Forward 纠正、Auto Live、Wake/再平衡、
实际秘密隔离/恶意研究代码、TOTP、三个视口/a11y/PWA、迁移/恢复、真实文档截图和
只经 Web/CLI 的干净实例全链路。每项需要真实自动化或受保护执行证据，不能只写文件
名、assert true、mock 上游计算或手工 SQL seed 后宣布完成。

普通 CI 不含生产密钥。真实 Codex 账号/真实许可数据/真实远端验收只允许经过审查的
源代码在受保护环境执行；缺配置、未授权或未运行是 BLOCKED，不是 skipped pass。
所需检查族为 rust、db-domain、contracts、frontend、e2e、native-runtime、codex-contract、
security-isolation、recovery、docs-smoke、supply-chain、protected-acceptance 和完整的
rewrite-complete。缺失、失败、取消或本应运行却跳过的检查不能汇总成成功。

交付顺序不可降低：同一 PR 完成全部范围 → 最新 Head 全部适用 CI 通过且相关 PR 的
Codex review 明确无问题、所有 review threads 已解决 → 才允许合并 → 验证 main 检查并
回填迁移/全链路/隔离/恢复/截图证据 → 关闭 #62。更新 Head 后，旧 Head 的绿色和审查
不自动成为新 Head 的通过证据。仅 emoji、无回复或额度不足不是明确审查通过。

**GitHub 上 Codex 只参与 review；禁止要求其实现、修复、提交、自动改代码或代替执行者
完成本 Issue。任何 W0 探针/旧 CI/文档声明都不能把当前 Draft 变成已完成重写。**
