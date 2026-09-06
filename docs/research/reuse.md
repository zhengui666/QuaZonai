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

## 浏览器认证与机密存储（2026-09-06）

采用 [tower-sessions 0.14.0](https://docs.rs/tower-sessions/0.14.0/tower_sessions/)
和 [官方 SQLx Store 0.15.0](https://docs.rs/tower-sessions-sqlx-store/0.15.0/)
的 opaque cookie/session 与 PostgreSQL 持久化。上游明确警告并发 session 更新可能
丢失；因此 QZ 的注销/设备撤销/epoch 存在独立数据库授权记录，任何 middleware
并发回写都不能恢复权限，不自建另一套 session 算法。

TOTP 使用 [totp-rs 5.7.0](https://docs.rs/totp-rs/5.7.0/)，读取锁定源代码的
`TOTP::check` 确认其 constant-time comparison；QZ 只实现数据库 step 防重放和
初始化 CAS。bootstrap verifier 使用 Argon2id，不自己实现 KDF；Secret 使用
[RustCrypto XChaCha20-Poly1305 0.10.1](https://docs.rs/chacha20poly1305/0.10.1/)
与随机 nonce/UUID-purpose AAD、cap-std 3.4.5 受限文件访问。数据库备份不包含
主密钥。这些密码学原生完整性不是研究资格或业务内容 hash。

[PostgreSQL18角色属性](https://www.postgresql.org/docs/18/role-attributes.html)
明确 superuser 绕过权限：运行服务必须使用非owner/non-superuser角色；migration
在独立本机运维命令中执行，不能每次服务器启动自动以管理员建表。


## Native authentication integration: versioned upstream boundary

This adapter uses Rust implementations; no Python exception is requested. It is not a browser authentication service or evidence of complete T36 acceptance.

- totp-rs 5.7.0: native SHA-1, six digits, 30-second TOTP and otpauth URI; https://docs.rs/totp-rs/5.7.0/totp_rs/struct.TOTP.html . Tests use RFC 6238 Appendix B reference outputs; https://www.rfc-editor.org/rfc/rfc6238#appendix-B . Database time, monotonic accepted step, rate limits and operator enrollment remain Store responsibilities.
- Argon2 0.5.3: native salted PHC verifier for random 256-bit bootstrap capabilities; https://docs.rs/argon2/0.5.3/argon2/ . This is the native cryptography exception, not a QZ business hash gate.
- chacha20poly1305 0.10.1: native XChaCha20-Poly1305 authenticated encryption; https://docs.rs/chacha20poly1305/0.10.1/chacha20poly1305/ . The UUID reference and purpose are authenticated additional data. Secret bytes never belong in domain receipts or public API results.
- cap-std 3.4.5 and rustix 1.1.4: bounded directory-relative access and no-follow native file opens; https://docs.rs/cap-std/3.4.5/cap_std/fs/struct.Dir.html and https://docs.rs/rustix/1.1.4/rustix/fs/struct.OFlags.html . Only trusted processes receive the private directory; this does not prove Agent/container isolation.

The local adapter publishes a UUID reference only after file and directory synchronization. Failed partial writes are unreferenced encrypted objects, never successful authority. A damaged or missing master key fails startup; existing keys are not automatically replaced. Actual formatting, compilation and test outcomes are recorded by the development workflow and subsequent read-only CI, not inferred from this research entry.


## Exact native Codex version evidence (0.144.4)

The pinned `rust-v0.144.4` implementation formats the initialization response as
`originator/CARGO_PKG_VERSION` followed by platform and terminal details.
`initialize_processor` sets the originator from this probe's fixed clientInfo.name;
the probe clears inherited environment overrides. The adapter compares only this
first product/version token exactly and writes the verified observed version.
It does not implement semver compatibility negotiation or infer a version from a
substring in platform/suffix text. Real native stdio execution is still required.

- https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/login/src/auth/default_client.rs
- https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/app-server/src/request_processors/initialize_processor.rs
- https://www.postgresql.org/docs/18/sql-createtrigger.html (native deferred aggregate publication)
- https://www.postgresql.org/docs/18/explicit-locking.html (native row locks and post-wait rechecks)

## Complete deployment transaction and native session DDL

The pinned `tower-sessions-sqlx-store 0.15.0` exposes only `migrate(&self)`;
it acquires and commits a new transaction from its private PgPool. It cannot
participate in the caller's migration transaction. Reusing this method after
a committed domain migration is not atomic. Pool injection, a fork, a new
SessionStore, or pretending an advisory lock makes separate commits atomic
would add risk without providing the required behavior.

The minimal adapter reuses its two default-schema DDL statements in an additive
SQLx migration, preserving MIT attribution. Native SQLx still owns versioning,
checksums, transaction/savepoint execution, and migration locks. Native
PostgresStore still owns all session serialization and CRUD. Native catalog
comparison against a table produced by the actual upstream migrator, plus real
upstream CRUD using the application role, guards against drift. Existing schema
mismatches fail the deployment without deleting or rewriting user sessions.

Source: https://github.com/maxcountryman/tower-sessions-stores/blob/b34a2f363217c0c557ee332c8847f4e2d1b5e6b4/sqlx-store/src/postgres_store.rs
License: https://github.com/maxcountryman/tower-sessions-stores/blob/b34a2f363217c0c557ee332c8847f4e2d1b5e6b4/LICENSE

## PostgreSQL ADMIN OPTION 与证据来源边界（2026-09-06）

依据 [PostgreSQL18 GRANT](https://www.postgresql.org/docs/18/sql-grant.html)、
[角色成员关系](https://www.postgresql.org/docs/18/role-membership.html) 和
[`pg_auth_members`](https://www.postgresql.org/docs/18/catalog-pg-auth-members.html)：
ADMIN OPTION 的持有者可以重新授予 SET/INHERIT，即使当前两个选项均为 false。
因此不能仅以 `pg_has_role(..., 'USAGE'|'SET')` 排除间接所有者权限。运行角色检测
从 `current_user` 及 `session_user` 沿原生 INHERIT/SET/ADMIN 成员边进行闭包查询，
再使用原生 catalog/ACL 判断数据库、服务 schema 和对象的危险权限；没有自行维护
角色目录或密码学。无可用选项的纯成员边不视为权限，管理无危险权限的角色仍可使用。

`runtime_role.rs` 新回归在真实 PostgreSQL 上先确认 ADMIN-only 的 USAGE/SET 均为
false，再实际由该低权限登录重新授予自己 SET 并验证原生 TRUNCATE 权限；另覆盖多跳、
SET ROLE 隐藏 session_user 和良性对照。测试中的对象/账号都是隔离、可丢弃的 fixture。

新证据绑定复用 PostgreSQL 事务、触发器和原生 FK：评估报告、方法版本及指标产物必须
来自精确项目/Run；审批必须使用已冻结且包含精确评估报告的本项目证据集合。新增迁移
不改旧 checksum，不重标错误历史，也不据关系完整就判定科学有效或授予交付权限。

[PostgreSQL18预定义角色](https://www.postgresql.org/docs/18/predefined-roles.html)
明确 `pg_read_server_files`、`pg_write_server_files`、`pg_execute_server_program`
可绕过数据库级检查并取得相当于超级用户的权限。因此同一运行角色检查也拒绝直接或
可管理成员链上的这些原生角色；数据导入使用受限客户端协议而非授予服务器文件权限。

## Run 生命周期与持久 SSE（2026-09-06）

本路径全部使用 Rust 与 PostgreSQL 原生能力，没有 Python 例外。

| 所需能力 | 已锁定的复用对象 | QZ 保留的最小职责 |
|---|---|---|
| 事务与数据库锁 | SQLx 0.8.6 `Transaction`；PostgreSQL18行锁/FK/trigger | 冻结预算、精确 Run/Attempt 关联、唯一回执与终态 |
| 至少一次任务投递 | PGMQ1.10.0 `send`/`read`/`archive` | 先持久化发送意图、同 Attempt 接管、旧 epoch 拒绝；不声称 exactly-once 外部执行 |
| 事件流 | Axum0.8.9 `Sse`/`Event`/`KeepAlive`；futures-util0.3.34 `unfold` | 从现有 run_events 按原生序列分页，每批核验角色与作用域 |
| 连接与时限 | Tokio1.53.1 Semaphore/timeout | 每进程32条流、每批16条、60秒重连，不另造消息总线/后台广播任务 |

依据：
- https://docs.rs/sqlx/0.8.6/sqlx/struct.Transaction.html
- https://www.postgresql.org/docs/18/explicit-locking.html
- https://pgmq.github.io/pgmq/api/sql/functions/
- 锁定 Axum 源码 `src/response/sse.rs`：原生 Event 编码/JSON/KeepAlive。
- https://docs.rs/futures-util/0.3.34/futures_util/stream/fn.unfold.html
- https://www.postgresql.org/docs/18/plpgsql-trigger.html

PGMQ visibility 不是业务租约；QZ 只消费其原生表和函数，不增加另一种队列。
Run/预算/事件/PGMQ 入队同事务，正式终态回执与预算转消耗同事务，最后才能 archive。
SSE 不依赖易丢的内存通知；断线只丢弃流及许可，不发送取消业务命令。

首次运行新端到端 Store 用例暴露了既有零参数 `guard_revision()` 的真实 PostgreSQL
错误：`FOREACH expression must not be null`。修复对 TG_ARGV 作空数组归一化，保留
id/created_at/具名外键保护及原生 revision 增量，并通过新增迁移发布，不修改已应用
迁移。它允许正常 runtime 配置变更，但已发送任务仍绑定原 endpoint/credential_ref，
不能因新配置被重定向。runtime 行锁等待后的 lease/deadline 复核使用 DB 当前时间。

原生数据库/真实 loopback HTTP 测试不替代远端隔离、真实数据、原生 Codex 工具循环
或科学资格验收。终态接口只对受信任内部适配器开放，manifest 元数据检查不等于实际
文件内容/模型/科学结论的验证；不存在接收任意 URL/命令/终态的公开接口。
