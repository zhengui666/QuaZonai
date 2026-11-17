# Rust 策略编写合同

本参考适用于**已显式配置的可信 owner Rust crate**，不是动态 Python、任意目录编译或改写 QuaZonai/Nautilus。先读 `native_runner_preparation_status` 和 owner 的 Cargo.toml、src/lib.rs、现有 factory。这里的实际 Rust API 对应 Nautilus `634e09456c949f488b2bfcf6b6897b33cbf218d7`；若环境版本不同，先核对该版本签名，不照搬 develop 文档。所有 Nautilus crate 采用同一官方 rev/兼容 features，避免两份不相容类型。不得改上游或另写撮合/指标/收益引擎。

## 1. owner 与 launcher 各写什么

owner 是 library crate，`src/lib.rs` 导出 `pub mod research;`。配置的 factory module（此例 `research`）必须提供这两个准确签名：

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

| 数据 | on_start 订阅 | 回调签名（返回 anyhow::Result<()>） |
| --- | --- | --- |
| TradeTick | `self.subscribe_trades(instrument_id, None, None)` | `fn on_trade(&mut self, tick: &TradeTick)` |
| QuoteTick | `self.subscribe_quotes(instrument_id, None, None)` | `fn on_quote(&mut self, tick: &QuoteTick)` |
| CustomData | `self.subscribe_data(data_type.clone(), None, None)` | `fn on_data(&mut self, data: &CustomData)` |

不是 Python 的 `on_trade_tick` / `subscribe_trade_ticks`。指标直接使用该版官方 indicators 和其更新方法，先核签名。普通策略下单需 `use nautilus_trading::strategy::Strategy;`：

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

配置的 launcher 必须启用相应 source import 并在 factory 前调用 `job::chainticks_fill::register_observation_type()`。使用已登记 `ChainticksFillObservation`，不要复制一个同名但不同 Rust 类型。动态读取使用 `data.data.to_json()?`，实际字段有 `source_row_zero_based`、`raw_envelope`（event 与 block），不是猜测的 row/index/fill 字段。

账户 fill **不等于完整公共市场逐笔**。普通主动 fill 筛选必须消费既有 `classification == "taker_fill_observation"`，而不能只看 crossed=true；`maker_fill_observation`、`special_event_observation`、`unknown_fill_observation`、`unsupported_fill_observation` 保留各自含义，不能当普通成交。tid=0 不是独立删除规则；来源行身份、重放、账户双方记录和交易身份必须区分，保留原行。Spot Dust Conversion 等特殊事件可 crossed=true。账户 fill 不能据此声称盘口/滑点/可执行收益。

## 4. 可编译、无订单的完整 owner 例子

同发行包的 [assets/research.rs](../assets/research.rs) 是完整 module，不是伪代码；内部研究 Agent 同时收到原文。它用原生 Strategy 消费已登记 CustomData，并验证 loaded/received 数量，不下单、不生成假收益。只用于验证生命周期和正确接线，之后替换为有明确假设的真实策略。

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
