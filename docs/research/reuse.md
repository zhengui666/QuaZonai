# Rust 复用决策与可复核证据

核查日期：2026-09-05。此表是实施选择及证据，不是完成矩阵。完整产品合同仍在 DESIGN；不得把上游 README 的功能列表当成本项目已实现能力。

## 决策规则

某能力有满足目标合同的 Rust 组件就使用 Rust。不得以旧实现方便、本机工具链缺失、一次编译错误或赶工为理由改用 Python。确有能力缺口时，先在本文件提交具名 Rust 候选、目标版本/API、失败复现及其原因，再自主选择范围最小的 Python 上游适配。查不到不等于证明不存在。优先复用不能变成自研数值、认证、消息队列或 Agent 框架。

第一方目录直接使用 job、contracts、domain、server、runtime、store、integrations，不使用 qz- 前缀。删除旧代码/旧专属测试和部署，不把它们复制进 legacy；Git 保存历史。用户数据、数据库、备份和 LICENSE/NOTICE 不在源码清理范围内。

## 已核查并实际运行的 Rust 能力

| 能力 | 选择及来源 | 实际证据 | 边界与风险 |
|---|---|---|---|
| 回测与原生策略生命周期 | [Nautilus 官方 Rust 概念文档](https://nautilustrader.io/docs/latest/concepts/rust/)；[发布族 v2.0.0rc4](https://github.com/nautechsystems/nautilus_trader/releases/tag/v2.0.0rc4)；Rust backtest/model/trading 0.63.0 | [源码中的原生 EMA 示例](https://github.com/nautechsystems/nautilus_trader/blob/v2.0.0rc4/crates/backtest/examples/engine_ema_cross.rs)；[实际运行 33952841460](https://github.com/zhengui666/QuaZonai/actions/runs/33952841460) | 明确启用 examples/test-support，关闭默认及 Python 特性；native Cargo tree 无 PyO3。运行得到745 iterations、12 orders、24 events。只是synthetic兼容性，不是正式目标权重/共享资金/隔离验收。上游2.0发布族为RC，不能声称稳定版。 |
| 二次/锥规划求解器 | [Clarabel.rs](https://github.com/oxfordcontrol/Clarabel.rs)，[官方 Rust 安装/使用](https://clarabel.org/stable/rust/installation_rs/)，0.11.1 | 同一运行实际编译；本地直接 DefaultSolver 的 diag(1,4)、sum=1、long-only 得0.7999999999997491/0.20000000000025078，与手算0.8/0.2在1e-5内；冲突约束返回PrimalInfeasible | 原生solver接管算法。构造业务约束矩阵不是另写求解器；未验证的风险/容量/组约束不得宣称支持。不可行certificate不是可发布权重。 |
| Arrow IPC | [Apache Arrow Rust](https://arrow.apache.org/rust/arrow_ipc/index.html)，arrow-array/schema/ipc 56.2.0 | 实际编译；本地RecordBatch→FileWriter→FileReader逐值/schema/provenance回读 | 不需要PyArrow或自制IPC；正式Alpha时序/nullable/单位合同仍需独立测试。 |
| ISO币种 | [iso_currency Rust API](https://docs.rs/iso_currency/0.7.0/iso_currency/)，0.7.0 | 实际编译；用Currency::from_code验证成员，而不是只验三个大写字母 | 上游是版本化代码表，不是在线ISO服务；不把ZZZ/USDT等形似值当ISO货币。货币用途、是否可计费仍属于产品校验。 |
| 精确标量及生成合同 | uuid、chrono、BigDecimal、Serde、utoipa | 当前Rust基础测试验证UUIDv7、PostgreSQL bigint JSON字符串、NUMERIC(38,18)、必需nullable字段、原生OpenAPI生成 | 不使用f64保存资金/权重，不自写UUID/时间/十进制库。JSON Schema不是数据库权限或业务Gate的替代。 |

Nautilus [发布版 Cargo.toml](https://github.com/nautechsystems/nautilus_trader/blob/v2.0.0rc4/Cargo.toml) 指定 Rust 1.98.0、edition2024、LGPL-3.0-only。因此升级工具链，而不是因为旧1.90不够就保留Python桥接。运行33952789894最初失败是我方anyhow=1.0.99与上游^1.0.104冲突，修正为发布版要求后33952841460成功；该失败不是Rust能力缺口。

Nautilus示例复用保留原版权/LGPL声明；QZ原有AGPL/NOTICE不修改。Cargo.lock/npm lock/OCI digest属于原生供应链完整性，不用于业务ID、审批或证据资格。

## 控制面：复用成熟 Rust 组件，不建立平行框架

| 能力 | 复用对象 | 我方只承担 |
|---|---|---|
| HTTP/异步与HTTP客户端 | [Axum/Tokio](https://github.com/tokio-rs/axum)、[reqwest](https://github.com/seanmonstar/reqwest) | 产品路由、严格DTO、允许地址/权限/错误映射；不重建HTTP/TLS |
| 持久化与投递 | [SQLx](https://github.com/launchbadge/sqlx)、[PostgreSQL](https://www.postgresql.org/docs/current/)、[PGMQ](https://github.com/pgmq/pgmq) | 同事务预算/领域/事件、Attempt租约及唯一结果采纳；PGMQ visibility不是业务authority，不能宣称外部exactly-once |
| 认证 | [totp-rs](https://github.com/constantoine/totp-rs)、[tower-sessions](https://github.com/maxcountryman/tower-sessions)、RustCrypto AEAD | 首次本机bootstrap、TOTP防重放、会话撤销、CSRF、主体scope；不写密码学 |
| MCP | [官方 Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk) | mission-scoped权限及业务桥接，不写MCP协议栈 |
| 容器/隔离 | [Bollard](https://github.com/fussybeaver/bollard)、原生OCI/内核限制 | 固定JobSpec到容器映射/恢复及tombstone，非root/无网络/只读/资源约束；不把Prompt当隔离 |
| Codex | [官方 App Server](https://developers.openai.com/codex/app-server)、[官方Harness架构](https://openai.com/index/unlocking-the-codex-harness/) | stdio客户端与任务/权限映射；模型工具循环、Thread历史、原生登录/刷新都交Codex |
| CLI | [Clap](https://github.com/clap-rs/clap)及生成HTTP合同 | 薄客户端，不复制SQL/审批状态机 |
| Web产品面 | [官方 Ant Design](https://ant.design/)、React/TypeScript、TanStack Query、ECharts | 明确版本/证据/授权后果的业务组合；不造基础组件，不以Rust/WASM包装antd制造额外层 |

这些是选型，不声称上述全部控制面已经实现。Codex官方Rust app-server-client的in-process路径会嵌入完整runtime；其remote路径使用WebSocket。不能为了复用包名违反本项目稳定stdio/独立进程边界。采用官方生成schema及标准JSONL薄适配不等于另造Agent Harness；实际账号、同Thread工具闭环及秘密隔离仍须受保护验收。

## 科学能力继续逐项核查，不能默认Python

| 候选 | 已确认公开API | 尚需证明/决策 |
|---|---|---|
| [rust-portfolio-opt 0.2.0](https://docs.rs/rust-portfolio-opt/latest/rust_portfolio_opt/) | Rust/nalgebra的历史/EMA/CAPM收益，sample/EWMA/Ledoit-Wolf/OAS风险矩阵，优化及Black-Litterman | 核查锁定源码及独立numerical golden。默认252年化必须显式覆盖；不能混同skfolio按输入周期计量的约定。尚未把它接入正式组合。 |
| [Covstream](https://github.com/gratus00/Covstream) | Rust固定维度Welford协方差、非有限输入拒绝、FixedAlpha/ClippedAlpha收缩 | 上游声明Lean实数规格不是Rust f64形式证明。给定alpha的收缩不自动等于已拟合Ledoit-Wolf最优alpha；不能冒充动态维度/缺失数据估计。 |
| [Optuna官方Rustuna](https://github.com/optuna/rustuna) | 官方Rust实现，sampler/storage/core模块，TPE等公开能力 | 当前标experimental，Cargo workspace 0.1.0-dev；需固定commit并测试ask/tell、失败trial和预算。不能由于Python Optuna更熟悉而绕过核查。也不能使用其丢弃历史优化清除QZ试验账本。 |
| [model-selection-rs](https://docs.rs/model-selection-rs/latest/model_selection_rs/index.html)、[solow-cv](https://docs.rs/solow-cv/latest/solow_cv/) | Rust时间感知splitters；公开TimeSeriesSplit接口 | 必须检查具体gap/max_train/test_size语义，验证固定horizon purge。普通KFold不是时间隔离；不能据名字宣称CPCV或变量区间重叠支持。 |
| [Linfa](https://rust-ml.github.io/linfa/)、[Smartcore](https://smartcorelib.org/) | Rust估计器/常规交叉验证 | 特定预测/校准接口需按实际需求测试，不能把通用KFold冒充purged CV。 |
| [skfolio](https://skfolio.org/)、Qlib、Optuna Python | 原方案的科学上游候选 | 不保留“科学计算全部Python”的笼统例外。只有具名Rust缺口及证据提交后才批准最小Python能力；不再用Python调用已可用的Nautilus/Arrow/Clarabel。 |

**当前批准的生产Python例外：无。** 没有能力证明的项目继续实现/验证，不能用空实现、永久关闭核心Feature或把required指标缺失当通过。DSR/PBO仍不默认启用，缺required能力返回INCONCLUSIVE；不能把CPCV当PBO。

## 证据与最终边界

33952841460是单独的复用研究运行；其中临时resolver/vendor结果只是开发输入，不是产品已提交锁的验收。正式源代码更新后必须按实际Head使用已提交Cargo.lock执行fmt check、Clippy、unit/proptest、原生回测/solver/IPC及合同diff；禁止CI格式化/改写源码后宣称原提交通过。

已有原生fixture/纯领域函数不代表新数据库/API/Worker/完整Agent/多Alpha/交付/Ant Design/迁移/恢复完成。最终仍须W0–W8、T01–T42、最新Head全部适用CI、明确无问题Codex review、零未解决线程，才允许合并并复核main。

## PostgreSQL / SQLx 逐轮账本复用（2026-09-05）

采用 Rust SQLx **0.8.6**（原生 PostgreSQL driver、Tokio、migration、test 宏），在独立数据库复用 PostgreSQL **18** 与 PGMQ **1.10.0**。本地实际测试版本为 PostgreSQL18.1；CI 继续固定原有原生 OCI digest 并输出实际版本。没有新增 Python 例外，也没有自建消息队列、迁移运行器或模型工具循环。

上游依据：
- [SQLx0.8.6 test 宏](https://docs.rs/sqlx/0.8.6/sqlx/attr.test.html)：每个测试创建隔离数据库并应用指定迁移；失败保留用于诊断。采用 `#[sqlx::test(migrations="../../migrations")]`，不以 SQLite/in-memory mock 代替 PostgreSQL。
- [SQLx0.8.6 migrate 宏](https://docs.rs/sqlx/0.8.6/sqlx/macro.migrate.html)：复用原生 embedded migration runner；build.rs 监听 migrations，新增迁移也触发重编译。
- [PostgreSQL18 约束](https://www.postgresql.org/docs/18/ddl-constraints.html)：跨表身份用复合外键和唯一约束；CHECK 为 NULL 也可能通过，故 UUID 变体等检查显式要求 TRUE。原生 numeric domain 约束避免 typmod 提前舍入。
- [PostgreSQL18 事务隔离](https://www.postgresql.org/docs/18/transaction-iso.html)：短事务行锁协调预算/身份；外部模型调用不持数据库行锁。发送前持久唯一 intent，未知结果不重新发送、不退还预约。

QZ 独有的部分仅为字段关系、许可/资格引用、不可变发布、同一 Mission/Turn 的预算和阶段规则。队列读写/归档、连接池、事务、迁移、精确数值和测试数据库管理均由成熟组件承接。`Store` 不是另一份 LLM Harness，也不把 PGMQ 的至少一次投递解释为外部模型 exactly-once。

初始 DDL 只实现记录和关系约束，严格 JSON 参数、身份认证、资格授权、Sealed sandbox 与完整模型闭环仍须由相应服务实现并验收；不能把60张表或 fixture 关系的存在作为产品完成证据。
