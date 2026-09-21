# Polymarket 历史数据准备

本工具复用锁定的 Nautilus Rust Polymarket 客户端和原生 ParquetDataCatalog。
它提供操作员数据准备，不发送订单、不读取钱包、不启动研究、不登记 Dataset，
也不将当前元数据或部分历史自动标成 PIT 合格。完整合同见 [DESIGN](../DESIGN.md#polymarket-原生历史数据准备)。

## 构建

```sh
cargo build --locked -p job --features polymarket-history --bin polymarket-history
./target/debug/polymarket-history --help
```

`polymarket-history` 是显式构建的独立工具。默认 `job` 二进制和科学镜像不启用
该特性，现有 `cargo run -p job -- ...` 仍选择 job。原生 SDK 负责网络和格式，
不需要额外 Python 服务、插件平台或交易 API Key。

## 获取公开历史

先选择一个真实的市场 slug 与 UTC 秒级时间区间，再运行：

```sh
./target/debug/polymarket-history fetch \
  --market-slug "$MARKET_SLUG" \
  --start-seconds "$START_SECONDS" \
  --end-seconds "$END_SECONDS" \
  --max-trades 5000 \
  --output /path/to/new-import
```

环境变量由操作员填写；不是预置的真实验收市场。本工具的区间为 `[start,end)`，
end 必须是已经发生的时间。两个 outcome 分别受 `--max-trades` 限制，最大每侧
10,000 条。它不是全量历史下载器；锁定的上游使用 offset 分页，可能在上限返回
部分历史。查询成功、行数小于上限、首末时间看似符合，都不证明中间无缺口。
原生同秒排序包含合成纳秒值，不能据此研究实际毫秒级延迟。

当前 Gamma 元数据按照实际观察时间保存，不回填到交易发生时间。它可能含有
历史结算信息，因此原始输入写在 source-evidence.json；目录资产中剔除已知的
终局/当前状态字段。其余当今元数据也未证明是历史版本，不能自动用作过去的特征。

## 导入现成原生记录

```sh
./target/debug/polymarket-history import \
  --input /path/to/native-archive.json \
  --output /path/to/new-import
```

输入是一个 UTF-8 JSON 对象，不能直接传任意供应商 CSV/Parquet。字段如下：

| 字段 | 来源／含义 |
| --- | --- |
| schema_version | 固定 1 |
| source_reference | 非空来源说明，最长 2000 字节；标注档案版本和选定区间 |
| source_observed_at | UTC 时间，来源实际观测时间，不自动等于历史可见时间 |
| source_metadata | 需要保留的原始来源信息；只保存在目录外 |
| instruments | Nautilus Rust Serde 的 InstrumentAny 列表，限定 POLYMARKET BinaryOption |
| trades | Nautilus TradeTick 列表，可省略 |
| quotes | Nautilus QuoteTick 列表，可省略 |
| deltas | Nautilus OrderBookDelta 列表，可省略 |
| bars | 原生成交 LAST/EXTERNAL Bar 列表，可省略；不是价格观察的占位 OHLCV |

输入必须使用**同一锁定版本**的原生序列化对象。Third-party 档案应先在明确的
来源映射中转换为这些原生对象；该命令不会猜 token、字段单位或行情类型。
允许同时保留不同原生记录类型，不把报价或成交压成另一个虚构事实。
最多 256 个资产、100 万条记录、128 MiB 原始 JSON。超过范围应按已有资产/时间
分片；不通过提高限制来伪装完成全量数据验收。

## 输出与失败

```text
new-import/
  catalog/                Nautilus 原生目录
  source-evidence.json    原始导入输入，含来源与可能的终局元数据
  import-report.json      最后发布的写入报告
```

输出目录必须不存在，不能指向已有目录或已登记的数据版本。失败后可能保留本次
创建的部分目录；没有 import-report.json 的目录未完成发布，不能当作成功结果。
检查原因后选择新的输出目录重试，不覆盖现有用户数据。原始证据不放进科学目录挂载。

报告中的 coverage=UNPROVEN、historical_availability=UNVERIFIED 和
registered_in_quazonai=false 是保留的能力边界，不是错误地填入“已通过”。
原生目录可由兼容的 Nautilus 消费；QuaZonai 当前正式数据注册仍要求自己的
Runtime 元数据、BAR 合同、来源许可与时点验证。此命令不绕过这些步骤。

盘口能写入 Parquet 不等于序列连续、存在可恢复快照或具备真实排队位置。
当前读取到的 fee_schedule 也不是历史费表。本工具不推断结算、赎回、资金占用，
不能用它的完成报告宣称 Polymarket Alpha 或组合模拟已经接通。

## 验证

```sh
cargo test --locked -p job --features polymarket-history --bin polymarket-history
cargo clippy --locked -p job --features polymarket-history --bin polymarket-history -- -D warnings
```

测试用原生 BinaryOption/TradeTick 和 Parquet 往返验证格式，不冒充真实市场数据。
网络历史覆盖、完整费用与生命周期，以及 Alpha/组合研究另须实际证据。
所有执行结论以对应 PR 最新 Head 的 CI 和日志为准。
