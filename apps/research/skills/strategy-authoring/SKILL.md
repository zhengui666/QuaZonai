---
name: strategy-authoring
description: 内部 Pi Durable 研究 Agent 编写、构建和运行兼容 Nautilus 的 Rust 策略；规定接口，不限定策略类型。
---

# Rust 策略编写合同

**本合同只明确接口与执行方式，不限定可编写的策略。** 任何兼容当前 Nautilus 原生能力的策略均可实现；策略种类、交易逻辑、参数、标的数量与频率由研究目标决定，不设预注册名称白名单。下文数据、订单与完整代码都只是接口示例，不是唯一模板；可按需组合原生 Strategy、Actor、Controller 及其公开能力，实际可执行范围取决于已配置的数据、适配器与版本。

本参考适用于**已显式配置的可信 owner Rust crate**，不是动态 Python、任意目录编译或改写 QuaZonai/Nautilus。先读 `native_runner_preparation_status` 和 owner 的 Cargo.toml、src/lib.rs、现有 factory。这里的实际 Rust API 对应 Nautilus `634e09456c949f488b2bfcf6b6897b33cbf218d7`；若环境版本不同，先核对该版本签名，不照搬 develop 文档。所有 Nautilus crate 采用同一官方 rev/兼容 features，避免两份不相容类型。不得改上游或另写撮合/指标/收益引擎。

## 与外部操作 Agent 的协作

外部 Agent 通过 QuaZonai CLI 提交目标、续接指令、读取研究对话和结果；本内部 skill 负责把已受理目标落实为策略源码、构建、运行和评价。使用当前研究绑定的 workspace 与原生任务工具，保留构建、执行、报告的原任务 ID；把结论、实际验证范围和下一步写入研究回复，便于外部通过同一研究读取与继续。

内部逻辑评估使用已登记的 implementation source 材料与 `native_assessment_read_source`；数据综合评估使用原任务 evidence manifest 与 `native_assessment_read_exposure` 分页读取。按实际读取范围形成判断，而不是把主研究摘要当独立验证。材料登记与原生结果读取不改变策略种类或交易逻辑。

## 1. owner 与 launcher 各写什么

owner 是 library crate，factory module 名称可配置；例如选 `research` 时，`src/lib.rs` 导出 `pub mod research;`。实际配置的 module 必须提供这两个准确签名：

```rust
pub fn config(catalog: &std::path::Path, name: &str, config: &serde_json::Value)
    -> anyhow::Result<nautilus_backtest::config::BacktestRunConfig>;
pub fn install(engine: &mut nautilus_backtest::engine::BacktestEngine,
    name: &str, config: &serde_json::Value) -> anyhow::Result<()>;
```

`name` 选择 owner 实现，`config` 解析并保留明确参数。config 返回**原生 BacktestRunConfig**，不是 `ConfiguredNativeRun`。后者由 QZ 生成的 launcher 包装，install closure 调用 owner install。不要手写/替换生成的 launcher 或 main，不把当前进程的全局 runner 当已更新。

准确执行顺序：factory config → 原生 BacktestNode 构建 → node.build → 获取对应 BacktestEngine → owner install → node.run → 原生结果。owner 不再创建第二个 node、调用 run 或自行回放循环。install 用 `engine.add_strategy(strategy)?`（不传 Box）或 `engine.add_actor(actor)?` 注册，必要时 `engine.add_instrument(instrument)?` 和 `engine.add_data(data, None, true, true)?`。不要在 config 调用交易或用另一个进程冒充本次执行。

## 2. Strategy 的准确写法

策略结构包含 `core: StrategyCore`；构造 `StrategyCore::new(StrategyConfig { strategy_id: Some(...), order_id_tag: Some(...), ..Default::default() })`。使用 `nautilus_trading::nautilus_strategy!(Type)`，然后 `impl nautilus_common::actor::DataActor for Type` 实现生命周期/数据回调。每个实例使用唯一 strategy ID/order ID tag。

以下仅列常见回调，非可用数据类型清单；Bar、订单簿、定时器等同样可使用该版本原生 API。

| 数据 | on_start 订阅 | 回调签名（返回 anyhow::Result<()>） |
| --- | --- | --- |
| TradeTick | `self.subscribe_trades(instrument_id, None, None)` | `fn on_trade(&mut self, tick: &TradeTick)` |
| QuoteTick | `self.subscribe_quotes(instrument_id, None, None)` | `fn on_quote(&mut self, tick: &QuoteTick)` |
| CustomData | `self.subscribe_data(data_type.clone(), None, None)` | `fn on_data(&mut self, data: &CustomData)` |

不是 Python 的 `on_trade_tick` / `subscribe_trade_ticks`。指标直接使用该版官方 indicators 和其更新方法，先核签名。以下 market 单仅演示调用签名，不限定订单类型；limit、stop、撤改单等按原生 API 使用。此例下单需 `use nautilus_trading::strategy::Strategy;`：

```rust
let order = self.order().market(
    instrument_id, side, quantity,
    None, // time_in_force
    None, // reduce_only
    None, // quote_quantity
    None, // exec_algorithm_id
    None, // exec_algorithm_params
    None, // tags
    None, // client_order_id
);
self.submit_order(order, None, None, None)?;
```

`quantity` 用实际 instrument 精度与最小量；side/quantity/时机来自用户策略，不从此片段推定。订单回调、撤单、持仓、资金、费用和统计继续复用原生 Strategy/Cache/Portfolio。仅观察信号可以用 `DataActorCore` + `nautilus_actor!`，但 Actor 回调数不代表交易收益，注册 Strategy 而不下单也不产生盈利证据。

## 3. 回测配置与数据

- BacktestRunConfig 至少一个实际配置的 venue，显式 oms/account/book type、starting_balances 和 fee_model。纯观察运行也不能省略 venue/fees。
- `BacktestVenueConfig::builder().fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()))` 接 **FeeModelAny**；不要照抄低层 SimulatedVenueConfig 的 handle 转换。零费仅限明确的无订单接线验收；真实收益研究必须提供有来源的费率并核对实际成交收费，不能默认零成本。
- 开始/结束纳秒、价格、数量和金额保持准确字符串/原生类型。原生输入用 DataConfig/目录查询，按数据实际覆盖指定窗口；查询范围不等于文件里每条记录都符合范围。区分 ts_event 与 ts_init，不按 tid 排序推断真实顺序。
- 普通市场数据由原生 BacktestDataConfig 加载。当前版本不接受 Custom 的此路径，使用公开 install 钩子加载已注册 CustomData，再由同一个 Nautilus engine 回放。此路径是有界内存加载，不宣称流式无限数据。只读批准训练目录/窗口。
- configured 路径当前仅 file catalog，并检查原生 config.data 的真实路径属于绑定 root。owner 自己查询数据也必须使用同一绑定目录与允许范围，不能另读 OOS。
- `dispose_on_completion(false)` 才能保留 QZ 所读的 canonical/equity 结果；true 的不可用结果不是零收益。

### Chainticks 账户 fill 特别约定

本节仅适用于选用此来源的研究，不要求其他策略使用 Chainticks 或只做主动成交分析。

配置的 launcher 必须启用相应 source import 并在 factory 前调用 `job::chainticks_fill::register_observation_type()`。使用已登记 `ChainticksFillObservation`，不要复制一个同名但不同 Rust 类型。动态读取使用 `data.data.to_json()?`，实际字段有 `source_row_zero_based`、`raw_envelope`（event 与 block），不是猜测的 row/index/fill 字段。

账户 fill **不等于完整公共市场逐笔**。普通主动 fill 筛选必须消费既有 `classification == "taker_fill_observation"`，而不能只看 crossed=true；`maker_fill_observation`、`special_event_observation`、`unknown_fill_observation`、`unsupported_fill_observation` 保留各自含义：maker 是被动成交，可用于相应研究；特殊、未知及不支持事件不能混作普通主动成交。tid=0 不是独立删除规则；来源行身份、重放、账户双方记录和交易身份必须区分，保留原行。Spot Dust Conversion 等特殊事件可 crossed=true。账户 fill 不能据此声称盘口/滑点/可执行收益。

## 4. 可编译、无订单的完整 owner 例子

同发行包的 [assets/research.rs](assets/research.rs) 是完整 module，不是伪代码；内部研究 Agent 同时收到原文。它用原生 Strategy 消费已登记 CustomData，并验证 loaded/received 数量，不下单、不生成假收益。只用于验证生命周期和正确接线，不规定真实策略必须使用 CustomData、此数据源、无订单逻辑或示例名称/参数。可以完全替换示例实现，编写符合研究目标的任何原生兼容策略。

owner Cargo 需要 anyhow、serde_json，及同一固定 rev 的 nautilus-backtest（streaming）、nautilus-common、nautilus-core、nautilus-execution、nautilus-model、nautilus-persistence、nautilus-trading。沿用当前环境 features/锁定依赖，不抄另一版锁文件。示例配置为：

```json
{"schema_version":1,"name":"custom-callback-check","config":{"catalog":"实际已注册训练目录","start_ns":"实际训练起点纳秒","end_ns":"实际训练终点纳秒"}}
```

这些是需替换的资源值，不是可提交的真实数据；具体请求字段还要与本版本 schema 核对。该例不负责注册数据集或创建目录，不可拿 OOS 做接线测试。

## 5. 编辑 → 构建 → 绑定 → 验证

内部研究 Agent 用官方 read/write/edit 编辑显式 workspace，不用 shell。每次调用准确工具：

1. `native_runner_preparation_status {}`，读取真实 owner/factory、revision 和可用环境。
2. 修改 owner 源码。改 Cargo.toml 依赖后调用 `native_runner_build {requestId, expectedRevision, prepareOwnerLock:true}` 让官方 Cargo 离线准备锁；未改依赖通常省略该标志。绝不手改 Cargo.lock，也不与另一研究同时改/编译同一 owner。
3. `native_runner_build_status {requestId}` 读原构建状态和 preflight/build 诊断，直到真实成功。running/checkpoint unknown 不代表终态；终态 UNKNOWN 不改 ID 重发。修正编译错误是新构建，保留旧失败。
4. 用完成 build 的 requestId、当前配置 revision、实际 dataset/model 调 `native_research_start_bound`；它创建**显式绑定新二进制的新研究**，不更新旧研究 executor，也不需要 native_runner_select 全局切换。
5. 由该研究读取 brief/实际 request，先以 `native_strategy_logic_review` 记录全部计划运行的完整请求、窗口和假设，再以 `native_configured_configure {request: 完整原生请求}` 设置 factory/name/config，再通过 `native_configured_backtest` 执行准确配置。先声明窗口/参数/假设，不把 build 成功当回放成功。检查原任务状态、stderr、回调、原生结果、订单/成交及费用证据；无订单例子的成功仅是接线成功。

这些是内部 Agent tools，不是同名 CLI 子命令。外部调用仍用 research start/resume 交给研究 Agent；终止的 submission 才正常 resume。可用 tools 和绑定 capabilities 为准，不编造缺失能力或自动重发原 UNKNOWN。

## 6. 回测不是账户策略

此 factory 只建 BacktestRunConfig；paper/live 需要独立已编译 account registration、适配器、账户配置和显式启动，不能将历史 factory 自动用于 live，也不从回测成功推导实盘许可。不得在此接线例子里加钱包、账户连接或实盘订单。配置保存、构建与 capabilities 读取均不表示策略已启动。

已有执行后的源码/逻辑复核，使用同一 `native_strategy_logic_review`，显式传 `reviewTiming:"POST_EXECUTION"` 和已授权的 `sourceRequestId`。这是新的独立审查 Task，不重发回测、不替换旧事前逻辑评级或报告，也不能补成事前验证协议。子 Agent 仍只读取指定源码材料，不自动获取历史回测或父对话；结合原结果评价时分别读取原审查及执行证据。省略 reviewTiming 仍是事前审查，已有执行时会拒绝。

综合评估子 Agent 在配置了研究工作区时，可用 `native_assessment_read_report {taskId, offset, limit}` 分页读取该评估清单内原生 Task 的 `request.config.report`。这复用官方 Pi read 的行分页，不运行回测；报告是原 Task 声明路径上的当前文件，不自动等同不可变原生结果或生产者证明。逐页读所需范围、保留截断提示并披露未读内容；不将父 Agent 摘要冒充独立读取。

### 已有行情的正式导入能力

原生 owner 环境可显式设置 `marketImport:true`，下一次 `native_runner_build` 才会编译已有 `market-history` 导入入口；保存配置不编译，也不改变旧研究/旧 Build。该入口使用 high-precision：owner 的官方依赖图必须已显式选择相同精度，否则构建报 `rust_market_import_requires_high_precision_owner`，不静默改变策略精度。通过 owner 自有 Cargo feature 选择官方 high-precision、官方 Cargo 准备锁，再新建 Build 并读取真实 capabilities；不要改旧 capability 回执或切全局 runner来替代绑定。

已有登记 catalog 时，新研究绑定后可用 `native_market_import`。尚无 catalog 时不要冒用其他 dataset 或创建占位研究，按以下正式准备顺序：
1. 在当前普通研究中调用 `native_runner_market_import {runnerBuildRequestId: "成功 Build 的原 requestId", expectedRunnerRevision: 当前实例配置 revision, request: 完整 NativeMarketImportRequestV1}`。request 至少为 `{schema_version:1, manifest_path:"明确授权的manifest绝对路径", instruments_path:"原生定义绝对路径", output_path:"新的输出目录"}`；可选 `start_seconds`/`end_seconds` 为字符串，`path_prefixes` 为相对目录前缀字符串数组，每项必须以 `/` 结尾（如 `trades/2026-04-14/`），不能传单个文件名。只需manifest全部已授权文件时省略或传空数组，不猜前缀；选单文件应在manifest中明确files，而非用文件名冒充目录前缀。若 `received_time:true`，必须显式传字符串 `maximum_receive_delay_ns`。原生合同不支持 bars/resume。此工具复用同一原生 import Task，仅用成功 Build 解析 target，不修改当前研究的旧执行绑定。
2. 读取原 import 结果的 `catalog_path`、`report_path` 和完整所需报告。结果大时按返回提示用 `native_assessment_read_exposure` 分页，不以工具受理当导入成功。
3. 用官方 write 在配置的研究源码工作区写 dataset descriptor JSON，顶层是一个对象或对象数组。现有合同如下，所有值须替换成实际证据，不照抄占位符：
```json
{"id":"新的数据集标识","name":"真实描述","assetClass":"CRYPTO","currency":"实际计价币种","source":"真实来源","start":null,"end":null,"ready":true,"nativeModality":"TRADE_TICK","data":{"catalog":"原import返回的catalog_path"},"policy":{},"preparation":{"archiveLoaded":true,"nativePrepared":true,"scientificQualification":"NOT_ASSESSED"}}
```
`id/name/assetClass/currency/source` 必须是非空字符串；`start/end` 为明确的日期时间字符串或 null，不猜测覆盖；`ready` 为布尔值，只有实际可用时才为 true；`data` 是对象，catalog相对路径按descriptor文件目录解析，优先用实际绝对路径。准备状态不是研究合格，报告中的冲突/缺失、晚到定义等限制写入研究说明，不能伪造收益或准入。不要在descriptor加入凭据或未授权数据。
4. 调 `native_dataset_registration_status {}` 取得**数据源** revision（不同于实例配置 revision），再调 `native_dataset_register {requestId:"本次登记稳定标识", expectedRevision:数据源revision, path:"工作区内descriptor绝对路径"}`。复用既有append-only registry，校验真实工作区路径（含符号链接）；原request重放返回原回执，不覆盖旧dataset。
5. 使用登记返回的真实 datasetId、成功 Build 原requestId、当前实例配置revision和真实model调用 `native_research_start_bound`。导入、登记、绑定是不同事实；不改全局runner、不重绑旧研究。
原请求/输出与失败记录保留；UNKNOWN不重发。晚到定义只能保留其真实 ts_init 并披露静态情景，不能回填历史定义。导入成功不等于数据准入、OOS有效或策略回测完成。
