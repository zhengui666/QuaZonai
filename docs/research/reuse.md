# Rust 复用决策与可复核证据

核查日期：2026-09-05。此表是实施选择及证据，不是完成矩阵。完整产品合同仍在 DESIGN；不得把上游 README 的功能列表当成本项目已实现能力。

## 决策规则

某能力有满足目标合同的 Rust 组件就使用 Rust。不得以旧实现方便、本机工具链缺失、一次编译错误或赶工为理由改用 Python。确有能力缺口时，先在本文件提交具名 Rust 候选、目标版本/API、失败复现及其原因，再自主选择范围最小的 Python 上游适配。查不到不等于证明不存在。优先复用不能变成自研数值、认证、消息队列或 Agent 框架。

第一方目录直接使用 job、contracts、domain、server、runtime、store、integrations，不使用 qz- 前缀。删除旧代码/旧专属测试和部署，不把它们复制进 legacy；Git 保存历史。用户数据、数据库、备份和 LICENSE/NOTICE 不在源码清理范围内。

## 已核查并实际运行的 Rust 能力

2026-09-13：非零滑点规划直接映射锁定nautilus-execution0.63.0的
[L1原生单tick规则](https://github.com/nautechsystems/nautilus_trader/blob/a0400251110653b6d8ae6a9b5b89c4543fa85a2d/crates/execution/src/matching_engine/mod.rs)：
DefaultFillModel按原概率决定是否买加tick/卖减tick，原MakerTaker费用依赖成交价。
QZ只将原最后BAR价格和tick换算为DESIGN A5.2定义的舍入前比例期望，保留原参考
并用已有BigDecimal向上舍入到18位；不抽样、不重写撮合或费用算法，不新增依赖。
该规划系数不是实际费用/未来上界/数据支持资格；实际成交和Money舍入仍归Nautilus。

2026-09-13：原生DATA_VALIDATE的最后BAR名义金额复用已锁定nautilus-model 0.63.0
`Instrument::try_calculate_notional_value(quantity, price, Some(false))`。已读取本机
锁定源码`instruments/mod.rs`的原生实现：线性使用quote currency，反向在该参数下
使用base currency，quanto使用settlement currency，并复用原乘数与Money精度及
溢出检查。QZ只保留原返回值、币种和时间，不另写估值公式或增加依赖。受管真实
Parquet CurrencyPair回归核对10m数量、1.02/2.02价格对应USD 10.2m/20.2m；其他
合约类型的上游API存在不代表已完成QZ逐市场组合验收。该观察不是未来盘口或成本资格。

2026-09-13补充：CVaR风险预算复用Clarabel0.11.1 PowerConeT与原生线性规划，
已核查锁定crate examples/rust/example_powcone.rs及Apache-2.0许可。
[风险预算/Expected Shortfall原论文](https://arxiv.org/pdf/2302.01196)提供
对数预算约束与原始场景CVaR问题；[原生幂锥建模](https://docs.mosek.com/modeling-cookbook/powo.html)
给出等价加权几何平均的三维锥拆分。QZ只装配这些原生行，不实现论文的
切平面/随机梯度算法。原指数锥尝试在分数尾部贡献检查失败后删除，不保留回退。
最终幂锥采用更紧gap停止上限，原发布贡献容差和低精度授权不变。成功结果保留
原场景对偶，ndarray核对合法概率、尾部最优值及贡献；不手选并列尾部。
原生解析回归覆盖0.6/0.7/0.8置信水平、三资产幂锥组合、多空、零份额、
对偶篡改、无界/零风险、费用、小收益与不可行上限；受管任务实际读取下跌
合成目录/Wasm并复核原报告。不是REAL/PIT或完整组合资格/交付证据。

2026-09-13补充：方差风险预算复用Clarabel0.11.1的SecondOrderConeT，沿用已核查
原生二次锥API及Apache-2.0许可。
[原生风险预算参考](https://docs.mosek.com/portfolio-cookbook/risk_parity.html)
提供贡献乘积的旋转二次锥形式。QZ采用等价的标准二次锥、固定单位风险尺度并
最大化共同贡献下界（DESIGN给出等价式及求和证明）；不采用不稳定的指数锥路径。
按明确总敞口规范化，再用原Clarabel组合问题验证固定权重的可行性；不另写优化器。
发布复用原ndarray协方差/矩阵乘法核对贡献。实际原生小例验证等风险2/3、1/3，
非等额份额对应1/2、1/2，多空方向、零份额、约束冲突与共享迭代上限。
此为方差数值复用证据，不是REAL/PIT/资格/完整交付；CVaR使用上述独立幂锥适配。

2026-09-13补充：CVaR复用锁定Clarabel0.11.1的DefaultSolver线性规划入口
（Apache-2.0；已核对原crate examples/rust/example_lp.rs），无新求解器依赖。
QZ只将冻结等权损失场景装配为标准Rockafellar–Uryasev的eta/excess线性约束；
参考[Clarabel原生LP能力](https://clarabel.org/)及
[原作者CVaR组合问题](https://uryasev.ams.stonybrook.edu/research/testproblems/financial_engineering/basic-cvar-optimization-problem-beyond-black-litterman/)。
发布复核复用现有ndarray内积与Rust标准库select_nth_unstable_by/total_cmp，
不另写排序、分位数或优化算法；按冻结Decimal尾部质量计算同一经验风险公式。
实际原生回归验证置信水平0.8/0.6分别产生2/3、1/3与1、0的独立解析解，
也验证风险上限、不可行、分数尾部、重复损失、接近1的置信水平与负风险。
这仅为数值复用证据，不是REAL/PIT、资格或完整组合交付验收。

2026-09-13补充：复用已锁定的nautilus-execution 0.63.0（其原Cargo清单许可为
LGPL-3.0-only，不更改LICENSE/NOTICE）。直接核查原crate的src/models/fill.rs、
fee.rs、latency.rs和nautilus-backtest的SimulatedVenueConfig：DefaultFillModel
接收两项概率及显式种子，MakerTakerFeeModel使用原instrument费率，StaticLatencyModel
原生将base加到insert/update/delete。QZ只绑定这三种角色的类名、版本和严格参数。
真实job simulate回归确认滑点改变共享账户结果、固定种子可重复、基础延迟生效及
错误引用拒绝；不写填充、随机数、费用或延迟算法。具体版本绑定证据见execution文档。

2026-09-13补充：固定混合预测直接复用已锁定`ndarray 0.17.1`的`ArrayView1::dot`
（上游`src/linalg/impl_linalg.rs`行向量/矩阵乘法实现），不新增依赖或自建ensemble
引擎。权重由既有BigDecimal精确检查，进入原生数值边界才转换f64。实际两组预测
聚合后进入同一Clarabel求解，与独立手算0.82/0.18一致；这是合成数值验收，不能
代替Alpha身份、资格、覆盖率、单位/期限对齐或共享资金回测证据。

2026-09-13补充：调仓计划时区复用已在Cargo.lock中的`chrono-tz 0.10.4`，
由domain显式依赖，不新增时区解析器。核查本机锁定上游源码的`Tz: FromStr`示例、
Cargo声明的Rust1.65最低版本及MIT/Apache-2.0许可；实际Rust1.98.1下验证
America/New_York、Asia/Shanghai及未知时区拒绝。锁文件仅为domain增加已有组件
依赖边，不升级组件。这是IANA名称检查，不是交易日历版本或DST调度执行验收。

2026-09-14交易会话候选核查：需要版本化的真实会话、特别休市、半日市与DST，
不能把普通工作日或规则计算成功当作完整交易所日历。实际下载并检查
[trading-calendar 0.2.3](https://docs.rs/trading-calendar/0.2.3/trading_calendar/)
（MIT/Apache-2.0）、[nyse-holiday-cal 0.2.5](https://docs.rs/nyse-holiday-cal/0.2.5/nyse_holiday_cal/)
（MIT）、[usec 0.3.6](https://docs.rs/usec/0.3.6/usec/calendar/)
（MIT）的发布源码，随后在仓库外独立Cargo工作区、Rust1.98.1实际编译运行。
三个原生默认日历都把2025-01-09返回为交易日；这与
[NYSE所有权益/期权市场特别休市公告](https://ir.theice.com/press/news-details/2024/The-New-York-Stock-Exchange-Will-Close-Markets-on-January-9-to-Honor-the-Passing-of-Former-President-Jimmy-Carter-on-National-Day-of-Mourning/default.aspx)
矛盾。不能直接选其中任何默认日历作为完整CALENDAR_SESSION事实源。

探针calendar-probe-LPhPxx的最终执行退出0（复现缺陷，不是正确性通过），
四日的is_trading_day/is_busday/is_business_day原返回分别为：

| 日期 | trading-calendar | nyse-holiday-cal | usec |
|---|---|---|---|
| 2025-01-09 | true | true | true |
| 2021-12-24 | true | false | false |
| 2022-01-03 | false | true | true |
| 2021-06-21 | false | true | true |

最小复现为chrono::NaiveDate解析上述日期，分别调用
TradingCalendar::new(Market::NYSE)?.is_trading_day(date)、
HolidayCal::is_busday(&date)与UsExchangeCalendar::with_default_range(true)
.get_cal().is_business_day(date)。直接依赖精确锁定为上述三版本与chrono0.4.45，
usec运行显式移除ADDITIONAL_RULES环境变量，避免环境补丁伪装原生默认行为。
trading-calendar的src/markets/us/holidays.rs还把周六圣诞移到周一；
nyse-holiday-cal仅有节假日/工作日API，没有会话开闭和半日市输出；usec允许
add_holiday_rule，但缺失特别休市不能由QZ暗补后仍冒称完整原默认日历。

这次核查没有新增产品依赖、改写交易所规则或启用Python例外。后续必须绑定可追溯
的完整原日历数据，或验证能覆盖这些缺口的原生组件，再接入会话调度。
核查时的原生研究仍明确拒绝CALENDAR_SESSION；这不是完成该合同的证据。

随后原生会话适配不选择上述有缺口的默认规则，而消费数据所有者提供的完整、
版本化原会话文件。Rust/Serde重读原字节并绑定注册Universe的calendar_ref/version，
沿用chrono-tz核对原IANA时区，使用Rust原生checked_add_signed把显式秒偏移应用
到原UTC收盘时间；不计算节假日、DST规则或半日市，不下载URL、不新增依赖。
这只实现调度消费边界；原会话完整性、许可证及正式Store来源准入仍须独立验收。

| 能力 | 选择及来源 | 实际证据 | 边界与风险 |
|---|---|---|---|
| 回测与原生策略生命周期 | [Nautilus 官方 Rust 概念文档](https://nautilustrader.io/docs/latest/concepts/rust/)；[发布族 v2.0.0rc4](https://github.com/nautechsystems/nautilus_trader/releases/tag/v2.0.0rc4)；Rust backtest/model/trading 0.63.0 | [源码中的原生 EMA 示例](https://github.com/nautechsystems/nautilus_trader/blob/v2.0.0rc4/crates/backtest/examples/engine_ema_cross.rs)；[实际运行 33952841460](https://github.com/zhengui666/QuaZonai/actions/runs/33952841460) | 明确启用 examples/test-support，关闭默认及 Python 特性；native Cargo tree 无 PyO3。运行得到745 iterations、12 orders、24 events。只是synthetic兼容性，不是正式目标权重/共享资金/隔离验收。上游2.0发布族为RC，不能声称稳定版。 |
| 二次/锥规划求解器 | [Clarabel.rs](https://github.com/oxfordcontrol/Clarabel.rs)，[官方 Rust 安装/使用](https://clarabel.org/stable/rust/installation_rs/)，0.11.1 | 同一运行实际编译；本地直接 DefaultSolver 的 diag(1,4)、sum=1、long-only 得0.7999999999997491/0.20000000000025078，与手算0.8/0.2在1e-5内；冲突约束返回PrimalInfeasible | 原生solver接管算法。构造业务约束矩阵不是另写求解器；未验证的风险/容量/组约束不得宣称支持。不可行certificate不是可发布权重。 |
| Arrow IPC | [Apache Arrow Rust](https://arrow.apache.org/rust/arrow_ipc/index.html)，arrow-array/schema/ipc 56.2.0 | 实际编译；本地RecordBatch→FileWriter→FileReader逐值/schema/provenance回读 | 不需要PyArrow或自制IPC；正式Alpha时序/nullable/单位合同仍需独立测试。 |
| ISO币种 | [iso_currency Rust API](https://docs.rs/iso_currency/0.7.0/iso_currency/)，0.7.0 | 实际编译；用Currency::from_code验证成员，而不是只验三个大写字母 | 上游是版本化代码表，不是在线ISO服务；不把ZZZ/USDT等形似值当ISO货币。货币用途、是否可计费仍属于产品校验。 |
| 精确标量及生成合同 | uuid、chrono、BigDecimal、Serde、utoipa | 当前Rust基础测试验证UUIDv7、PostgreSQL bigint JSON字符串、NUMERIC(38,18)、必需nullable字段、原生OpenAPI生成 | 不使用f64保存资金/权重，不自写UUID/时间/十进制库。JSON Schema不是数据库权限或业务Gate的替代。 |

组合历史目标复用同一已锁定Arrow 56.2.0（Apache-2.0），无新上游包或版本升级。
共享contracts适配仅定义QZ列/元数据及IPC读写，供job与采纳边界复用，不载入数值
引擎。核查该版本原生FileWriter/FileReader、TimestampNanosecondArray::with_timezone
和Decimal128Array::with_precision_and_scale；实测38位精确小数、纳秒、零/null及
错误schema/截断/多batch拒绝。不得将独立原生格式测试冒充正式研究资格。

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

## 原生 Runtime 与 OCI 引用复用（2026-09-10）

`apps/runtime` 使用既有SQLx **0.9.0** 的原生SQLite driver/migration、WAL与FULL同步模式，通过短 `BEGIN IMMEDIATE` 事务管理远端任务身份、对象BLOB与唯一终态；PostgreSQL/PGMQ仍是研究预算与业务权威，不复制业务队列。依据为[原生SQLx事务入口](https://docs.rs/sqlx/0.9.0/sqlx/struct.Pool.html#method.begin_with)及[SQLite WAL语义](https://sqlite.org/wal.html)。真实SQLite并发、重开、取消与输出/manifest事务已进入实际测试，不能用这些测试替代PostgreSQL或Docker隔离。

固定 **Bollard0.21.1** 的Unix pipe与生成Docker模型承接create/inspect/start/kill/remove/stats；我方仅保存固定JobSpec→原生容器映射、一次START意图和取消屏障。`job run-bounded`复用镜像内GNU timeout及Docker init/cgroup，而不建立应用级无限后台watchdog。[Bollard官方源码](https://github.com/fussybeaver/bollard)、[GNU timeout](https://www.gnu.org/software/coreutils/manual/html_node/timeout-invocation.html)提供原生接口依据。原生工作区已完成该固定依赖的编译和常规测试；真正UID/网络/文件/cgroup/取消/崩溃验收是精确Head独立OCI CI，未取得实际结果之前不得宣称通过。

OCI分发引用使用 **oci-spec0.10.0** 的 `distribution::Reference`，仅启用distribution feature，复用官方Docker distribution格式解析，不再以自写允许字符列表误把URL或相对路径当镜像。来源：[Reference API](https://docs.rs/oci-spec/0.10.0/oci_spec/distribution/struct.Reference.html)、[原生features](https://docs.rs/crate/oci-spec/0.10.0/features)。同机Docker完整 `sha256:<64hex>` ID属于原生本地镜像身份，独立按固定格式识别，不当分发仓库解析、不增加应用内容散列或资格门禁。路径/URL/空组件/无digest/大写非法repository等负例在真实Rust领域回归中验证。

上述依赖只由原生Cargo在精确手写manifest基础上产生Cargo.lock；不写registry checksum或升级既有锁定版本来掩盖失败。原生镜像装配只包含选定job二进制、明确rustup工具链/目标标准库、必要原生ELF依赖和timeout，保留可取得的发行许可说明；不复制checkout、模型profile、密钥或开发执行器。没有新增生产Python例外。

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

当前采用 Rust SQLx **0.9.0**（原生 PostgreSQL driver、Tokio、migration、test 宏），在独立数据库复用 PostgreSQL **18** 与 PGMQ **1.10.0**。此前本地实际测试版本为 PostgreSQL18.1，该历史结果不证明新 SQLx 版本已通过；CI 继续固定原有原生 OCI digest 并输出实际版本。没有新增 Python 例外，也没有自建消息队列、迁移运行器或模型工具循环。

上游依据：
- [SQLx0.9.0 test 宏](https://docs.rs/sqlx/0.9.0/sqlx/attr.test.html)：每个测试创建隔离数据库并应用指定迁移；失败保留用于诊断。采用 `#[sqlx::test(migrations="../../migrations")]`，不以 SQLite/in-memory mock 代替 PostgreSQL。
- [SQLx0.9.0 migrate 宏](https://docs.rs/sqlx/0.9.0/sqlx/macro.migrate.html)：复用原生 embedded migration runner；build.rs 监听 migrations，新增迁移也触发重编译。
- [PostgreSQL18 约束](https://www.postgresql.org/docs/18/ddl-constraints.html)：跨表身份用复合外键和唯一约束；CHECK 为 NULL 也可能通过，故 UUID 变体等检查显式要求 TRUE。原生 numeric domain 约束避免 typmod 提前舍入。
- [PostgreSQL18 事务隔离](https://www.postgresql.org/docs/18/transaction-iso.html)：短事务行锁协调预算/身份；外部模型调用不持数据库行锁。发送前持久唯一 intent，未知结果不重新发送、不退还预约。

QZ 独有的部分仅为字段关系、许可/资格引用、不可变发布、同一 Mission/Turn 的预算和阶段规则。队列读写/归档、连接池、事务、迁移、精确数值和测试数据库管理均由成熟组件承接。`Store` 不是另一份 LLM Harness，也不把 PGMQ 的至少一次投递解释为外部模型 exactly-once。

初始 DDL 只实现记录和关系约束，严格 JSON 参数、身份认证、资格授权、Sealed sandbox 与完整模型闭环仍须由相应服务实现并验收；不能把60张表或 fixture 关系的存在作为产品完成证据。

## 浏览器认证与机密存储（2026-09-06）

采用 [tower-sessions 0.15.0](https://docs.rs/tower-sessions/0.15.0/tower_sessions/)
和 [官方 SQLx Store 修订 d18c9bf](https://github.com/maxcountryman/tower-sessions-stores/blob/d18c9bf76f1d4fb73130dbe5aa643197f14b5d2d/sqlx-store/src/postgres_store.rs)
的 opaque cookie/session 与 PostgreSQL 持久化。该适配器是固定上游 Git 修订，虽仍标注 0.15.0，不能当作已发布的 SQLx0.9 兼容 crate；Time 固定为0.3.47。上游明确警告并发 session 更新可能
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

Linux Mission资源复用systemd user scope和util-linux prlimit，不增加Rust进程管理依赖。
本机原生systemd261.2的scope已验证接受MemoryMax、MemorySwapMax、CPUQuota、TasksMax、
RuntimeMaxSec并在退出后自动清理；正式App Server与子进程验收另行记录。
scope继承调用者环境/stdio，避免service manager环境混入与凭据写入unit Environment。
参考固定上游文档：https://github.com/systemd/systemd/blob/v261/man/systemd-run.xml 、
https://github.com/systemd/systemd/blob/v261/man/systemd.resource-control.xml 。

The pinned `rust-v0.144.4` implementation formats the initialization response as
`originator/CARGO_PKG_VERSION` followed by platform and terminal details.
`initialize_processor` sets the originator from this probe's fixed clientInfo.name;
the probe clears inherited environment overrides. The adapter compares only this
first product/version token exactly and writes the verified observed version.
It does not implement semver compatibility negotiation or infer a version from a
substring in platform/suffix text. Real native stdio execution is still required.

- https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/login/src/auth/default_client.rs
- https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/app-server/src/request_processors/initialize_processor.rs

The Mission adapter reuses native named permissions, stdio MCP, tool discovery
and tool dispatch. In this release, named permission catalogs need an explicit
`default_permissions` for configuration refresh, not only the typed Thread
permission selector. The Linux sandbox re-execs the pinned Codex executable;
its exact canonical file is read-allowed when installed outside `:minimal`,
without exposing its parent directory or native credential HOME. Model tools
may be deferred behind native `tool_search` according to the advertised model
and provider capabilities; obsolete `features.tool_search` flags are not used
to invent a direct-only tool surface. QZ does not implement a second tool loop.

The pinned host loads CODEX_HOME/AGENTS.override.md or AGENTS.md independently
of project_doc_max_bytes, with no stdio opt-out. Mission preflight rejects their
presence and personal instruction overrides without reading their contents or
changing the profile. Use a dedicated native profile; authentication remains
owned by Codex. This is an explicit native limitation, not a claim that the
thread's project-document setting suppresses global instructions.

- https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/codex-home/src/instructions/mod.rs

Reproducible check: `cargo test --locked -p server --features native-codex --test
mcp_authoring`, using the pinned `CODEX_NATIVE_BIN` and a disposable PG18/PGMQ1.10
database. Model responses are controlled fixtures; native process, discovery,
stdio MCP, HTTP, Store, sandbox and persistent Thread are real. This is not the
protected real-account T07 or complete fresh-user T42 acceptance.

- https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/core/src/config/mod.rs
- https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/core/src/tools/spec_plan.rs
- https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/core/tests/common/responses.rs
- https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/app-server/src/request_processors/turn_processor.rs (queued start ACK versus actual TurnStarted; interrupt completion race)
- https://www.postgresql.org/docs/18/sql-createtrigger.html (native deferred aggregate publication)
- https://www.postgresql.org/docs/18/explicit-locking.html (native row locks and post-wait rechecks)

## Complete deployment transaction and native session DDL

The pinned upstream `tower-sessions-sqlx-store` revision `d18c9bf76f1d4fb73130dbe5aa643197f14b5d2d` exposes only `migrate(&self)`;
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

### Bundled SQLite WAL reliability and SQLx compatibility

Runtime pins `libsqlite3-sys =0.37.0` with SQLx `=0.9.0`; the bundled engine is
SQLite **3.51.3**, which includes the upstream WAL-reset corruption fix.
This is preventive data reliability, not evidence that existing user data is corrupt.
Sources: [SQLite WAL](https://www.sqlite.org/wal.html),
[3.51.3 release](https://www.sqlite.org/releaselog/3_51_3.html), and
[the binding](https://docs.rs/crate/libsqlite3-sys/0.37.0).

The initial Runtime-only change failed native Cargo resolution because old and new
SQLite bindings both declare `links=sqlite3`. Unifying the existing SQLx consumers
and using the exact official session adapter above avoids a second native library,
database framework or first-party session store. The actual Cargo resolver owns
Cargo.lock; no registry checksums or dependency graph are handwritten.

SQLx0.9 requires `SqlSafeStr`. Existing dynamic SQL fragments are audited constants,
closed choices or identifiers already quoted by the native PostgreSQL/projection
path. Use upstream `AssertSqlSafe` at those call sites without changing SQL, value
binds, authority, locks or transactions. It is an assertion, **not a sanitizer**;
no blanket conversion for arbitrary strings is introduced. Owned strings and
borrowed `&str` retain the actual query's lifetime. The migration call uses native
`run_direct(None, connection, false)`: all pending versions execute, not skip.
Sources: [SQLx0.9](https://github.com/transact-rs/sqlx/discussions/4271),
[SqlSafeStr](https://docs.rs/sqlx/0.9.0/sqlx/trait.SqlSafeStr.html), and
[upstream migration execution](https://github.com/transact-rs/sqlx/blob/v0.9.0/sqlx-core/src/migrate/migrator.rs).

The original session DDL attribution above remains historical. The new adapter's
schema, bound CRUD and MessagePack record format were compared with the original
source; that inspection does not replace real migration/session/TOTP/recovery
tests. The existing Runtime journal test reads `sqlite_version()`, WAL mode,
instance identity and integrity through the **linked SQLx connection**, not a host
CLI. Existing PostgreSQL, browser, OCI and cold-restore checks remain required.
[PR #87](https://github.com/zhengui666/QuaZonai/pull/87) owns exact-Head results and
failed iterations. Compilation or this dependency fix is not complete T40/T42
acceptance or an owner-host deployment.

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
| 事务与数据库锁 | SQLx 0.9.0 `Transaction`；PostgreSQL18行锁/FK/trigger | 冻结预算、精确 Run/Attempt 关联、唯一回执与终态 |
| 至少一次任务投递 | PGMQ1.10.0 `send`/`read`/`archive` | 先持久化发送意图、同 Attempt 接管、旧 epoch 拒绝；不声称 exactly-once 外部执行 |
| 事件流 | Axum0.8.9 `Sse`/`Event`/`KeepAlive`；futures-util0.3.34 `unfold` | 从现有 run_events 按原生序列分页，每批核验角色与作用域 |
| 连接与时限 | Tokio1.53.1 Semaphore/timeout | 每进程32条流、每批16条、60秒重连，不另造消息总线/后台广播任务 |

依据：
- https://docs.rs/sqlx/0.9.0/sqlx/struct.Transaction.html
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


## 交付身份、领取时间与原生仓位证明（2026-09-06）

继续复用 PostgreSQL18 原生行锁、不可变外键记录、CHECK/触发器和递归 CYCLE：
https://www.postgresql.org/docs/18/queries-with.html#QUERIES-WITH-CYCLE
https://www.postgresql.org/docs/18/explicit-locking.html
https://www.postgresql.org/docs/18/functions-datetime.html
`CURRENT_TIMESTAMP` 固定在事务开始，不能用于锁等待后判断领取是否过期；正式 Claim
用 `clock_timestamp()`，状态及期限判断发生在持有行锁之后。跨进程没有自研内存锁或
去重 hash。仅检查 Package 类型/归属/来源的元数据不是完整科学证据验证。

Nautilus Rust 0.63.0 的 `BacktestResult.total_positions` 是原生运行结果字段：
https://docs.rs/nautilus-backtest/0.63.0/nautilus_backtest/result/struct.BacktestResult.html
锁定源码 `engine.rs::get_result` 与 `result.rs` 已核对；探针直接记录并要求原生非零
仓位数，而非根据订单条数推算成交。验收用 EmaCross 仍为不可交付 FIXTURE，不是
多 Alpha 或共享资金生产策略。这些能力已有 Rust 实现，不需要 Python 例外。

## 固定期限验证、原生估计与有界研究代码（2026-09-09）

- 固定 `solow-cv=0.7.3`，只复用 `TimeSeriesSplit` 和 `CombinatorialPurgedKFold`。已读取下载的精确源码：前者提供 test_size/gap/max_train_size，后者枚举全部 C(group_count,test_group_count) 测试块组合并按块边界剔除 purge/embargo。入口先限制观测、组数、折数和全部索引，避免上游组合数/乘法无界；独立小例逐个验证实际索引，不把 crate 名称当正确性证明。上游2026-09-05的新发行并无长期生产保证，启用范围只依据固定源码审查和本仓库原生回归。来源：https://docs.rs/crate/solow-cv/0.7.3 。
- 不采用 `torsh-series=0.2.0` 的同名 CombinatorialPurgedCV：原生源码是若干 offset 的向前窗口，不是所有测试块组合；其 PurgedTimeSeriesCV 首段还可能选择测试后的训练数据，不符合本项目 walk-forward 合同。`sklears-model-selection=0.2.0` 的 PurgedGroupTimeSeriesSplit 也不是本合同的 CPCV。不是语言生态结论，而是这两个具体 API 的不匹配。
- 样本协方差复用 `ndarray-stats=0.7.0` 的 CorrelationExt::cov(ddof=1)，固定 `ndarray=0.17.1`。每行是一个资产、每列是同一观测时刻，先检查非空、至少两期、等长、有限及资产顺序；不补零、默默丢列或隐式年化。来源：https://docs.rs/ndarray-stats/latest/ndarray_stats/trait.CorrelationExt.html 。
- SCORE 的仿射校准复用 `linregress=0.5.4` 的原生 FormulaRegressionBuilder/RegressionModel，只向 fit 提供该折被允许的训练标签，保存原生系数与精确训练输入/期限。固定公式 return ~ score，不接受用户公式/任意列。预测只应用冻结模型，不再次估计、不拿验证或 sealed 标签拟合。常数、缺值、非有限和样本不足明确拒绝；没有无条件“score即收益”转换。来源：https://docs.rs/linregress/0.5.4/linregress/ 。
- `wasmi=2.0.0` 无WASI/任何宿主导入，显式stable+portable-dispatch，原生fuel、内存、表和栈限制。实际未优化无限循环回归曾暴露未选择portable dispatch的宿主栈溢出；启用上游portable loop后，11项真正Wasm执行/拒绝测试全部通过。此处不是Wasmi2.0已被审计或整个宿主编译隔离已验收的声明。来源：https://docs.rs/wasmi/2.0.0/wasmi/#crate-features 。

没有新增Python例外，没有自写CV、协方差估计器、回归拟合器、解释器或优化算法。每个结果仍需同Run/Attempt、输入、政策、权限、独立评估与生产门禁验证。数值功能存在不能代替完整Issue62交付。

最终校准继续复用同一原生拟合结果，不另跑估计。已核对锁定linregress0.5.4的
lib.rs：RegressionModel只有Debug/Clone，无serde/from-parameters构造；predict使用
原始斜率矩阵乘法再加截距。持久模型保存原系数，用已锁定ndarray0.17.1的原生
数组乘加应用相同单变量仿射函数，并以真实RegressionModel.predict对照；不为
序列化重新拟合伪样本、不新增依赖。最后折按原生顺序固定选择，不按指标挑选。
来源：https://docs.rs/linregress/0.5.4/linregress/struct.RegressionModel.html 。
本次网页工具不能读取该页面，API依据实际已下载的精确版本源码核查，运行证据另记。

## 独立Alpha分折指标复用（2026-09-12）

继续使用已锁定且已安装的ndarray-stats0.7.0（MIT OR Apache-2.0），不引入新依赖：
核对精确源码correlation.rs/deviation.rs后复用pearson_correlation与root_mean_sq_err。
相关矩阵每行变量、每列观测；常数/不足样本预检，原生非有限结果保留FAILED/null。
RMSE只比较相同horizon的收益，不能把未校准SCORE代入。接口与边界来源：
https://docs.rs/ndarray-stats/0.7.0/ndarray_stats/trait.CorrelationExt.html
https://docs.rs/ndarray-stats/0.7.0/ndarray_stats/trait.DeviationExt.html
实际执行/参考值的验证结果另记execution文档；复用登记本身不是数值验收。

受管分折结果采纳也复用锁定solow-cv0.7.3（BSD-3-Clause、Rust1.80、默认无feature）
的同一个有界薄适配`domain::execution::validation`。Job原生执行直接复用该模块，
不另写切分器、组合枚举或校准拟合；Domain只以原请求/源行数核对逐折原始索引及
结果合同。新增Cargo边只指向已锁定anyhow/solow-cv，没有新增包或Python例外。

## 原生目录能力的回退路径（2026-09-08）

上游 GHSA-hp8f-xmx4-4qrg 指出：含尾斜杠的多层软链可突破旧版手工路径解析；Linux openat2 不可用/被阻止时也会触发该后端。官方3.x修复版本为3.4.6，4.x为4.0.3。本工作区检查时 cap-std facade=3.4.5，但已锁定的 cap-primitives=3.4.6，不能据 facade 名字宣称正在运行的解析器仍有漏洞。本次将 facade 同步固定3.4.6，令最小依赖要求亦覆盖补丁，不转向4.x或重写路径解析。

- 官方通告：https://github.com/bytecodealliance/cap-std/security/advisories/GHSA-hp8f-xmx4-4qrg
- 固定3.x发行：https://docs.rs/crate/cap-std/3.4.6
- 测试复用：https://docs.rs/seccompiler/0.5.0/seccompiler/ （Rust VMM原生seccomp构建/加载）

`directory_confinement.rs` 在全新测试子进程用成熟 seccompiler 令 openat2 分别返回 ENOSYS/EPERM，先通过 rustix 原生 openat2 确认注入生效，再执行完全相同的原生目录打开/文件读取/文件创建拒绝和合法根内软链对照。仅使用测试私有临时目录的兄弟哨兵；没有宿主真实文件、生产密钥或数据库操作。真实 ArtifactStore/SecretVault 原生字节发布/读取亦在每种模式内测试；父进程不安装过滤器，不用mock成功回执。seccompiler和libc直接依赖仅属于Linux dev-dependencies，不把测试注入带入运行服务。测试是否通过必须以实际命令和最新Head CI为证，不能把依赖声明或内核探测当完整T34/T35通过。


## 原生 Parquet 页脚边界（2026-09-16）

锁定的 Nautilus persistence 0.63.0 经 DataFusion 使用 Parquet 59.3.0。
该版 [push decoder](https://github.com/apache/arrow-rs/blob/59.3.0/parquet/src/file/metadata/push_decoder.rs#L389)
在读取页脚声明的元数据长度时直接做无符号减法；真实原生目录测试中，将该长度改为
`u32::MAX` 会触发下溢 panic。共享 `job::catalog::load_catalog` 先复用原生目录列表，
仅用标准库读取每个已挂载 Parquet 文件的固定八字节页脚，并核对声明长度不越过文件。
元数据、压缩、类型和时间过滤仍完全交给原生组件，没有新增解析器或依赖。
此预检会对已限定、不可变目录中的每个文件多读一次页脚；升级到经本回归验证会拒绝
越界长度的原生读取器后删除预检，不把它扩展成格式或数值引擎。

`apps/job/tests/catalog.rs` 以原生写入的目录验证截断页脚、超长元数据及损坏压缩页均返回
错误且保留输入字节，恢复原文件后仍能读取原行情。压缩膨胀行数上限另有真实 SNAPPY
回归。这些检查不替代 T35 的容器内压缩炸弹资源限制及安全错误回传验收。
