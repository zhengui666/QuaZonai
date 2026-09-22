from pathlib import Path


def replace(path, old, new):
    p = Path(path)
    s = p.read_text()
    assert s.count(old) == 1, (path, old)
    p.write_text(s.replace(old, new))


replace('apps/runtime/tests/native_oci.rs',
    '#[path = "support/oci.rs"]\nmod support;',
    '#[path = "../../job/tests/support/polymarket.rs"]\nmod polymarket;\n#[path = "support/oci.rs"]\nmod support;')
replace('apps/runtime/tests/native_oci.rs',
    '''async fn real_native_rolling_study_uses_original_models_in_one_account() {
    use contracts::{''',
    '''async fn real_native_rolling_study_uses_original_models_in_one_account() {
    native_rolling_study(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_native_polymarket_study_preserves_collateral_and_calendar_year_metrics() {
    native_rolling_study(true).await;
}

async fn native_rolling_study(prediction: bool) {
    use contracts::{''')
replace('apps/runtime/tests/native_oci.rs',
    '''    let (catalog, mut request, wasm) = market::study_liquidity("10000000", "0.4");
    market::calendar_schedule(&mut request);''',
    '''    let (mut catalog, mut request, wasm) = market::study_liquidity("10000000", "0.4");
    if prediction {
        // Only source types change. The actual Runtime, OCI, original Wasm models,
        // rolling liquidity, shared account and output-binding checks remain identical.
        catalog = tempfile::tempdir().unwrap();
        let expiration = 4 * 86_400_000_000_000;
        polymarket::write_catalog(catalog.path(), 2 * 1440 + 20, "0", expiration, true);
        polymarket::settings(&mut request.execution_settings, "0", expiration);
        request.mandate.base_currency = "pUSD".into();
        request.source_selection.bar_types = polymarket::IDS.iter()
            .map(|id| format!("{id}-1-MINUTE-LAST-EXTERNAL")).collect();
        for (asset, fee) in request.assets.iter_mut().zip(&request.execution_settings.fee_rates) {
            asset.instrument_id = fee.instrument_id.clone();
            asset.currency = "pUSD".into();
            asset.transaction_cost_rate = fee.taker.clone();
        }
    }
    market::calendar_schedule(&mut request);''')
replace('apps/runtime/tests/native_oci.rs',
    '''    assert_eq!(manifest.engine_versions["portfolio-history"], "1");
    assert_eq!(manifest.artifacts.len(), 3);''',
    '''    assert_eq!(manifest.engine_versions["portfolio-history"], "1");
    if prediction {
        assert_eq!(manifest.engine_versions["polymarket-research"], "1");
        let capabilities: contracts::runtime::RuntimeCapabilitiesV1 = f
            .json(Method::GET, &["capabilities"], None, &[StatusCode::OK]).await;
        assert!(capabilities.venues.iter().any(|v|
            v.venue == "POLYMARKET" && v.instrument_classes.iter().any(|c| c == "BinaryOption")
                && v.expiry_and_settlement
        ));
    }
    assert_eq!(manifest.artifacts.len(), 3);''')
replace('apps/runtime/tests/native_oci.rs',
    '''    for (metric, native_key) in metrics.iter().zip([
        "Average (Return)",
        "Returns Volatility (252 days)",
        "Sharpe Ratio (252 days)",
    ]) {''',
    '''    let period = if prediction { 365 } else { 252 };
    assert_eq!(metrics[0].annualization_factor, None);
    assert_eq!(metrics[1].annualization_factor, Some(f64::from(period)));
    assert_eq!(metrics[2].annualization_factor, Some(f64::from(period)));
    for (metric, native_key) in metrics.iter().zip([
        "Average (Return)".to_owned(),
        format!("Returns Volatility ({period} days)"),
        format!("Sharpe Ratio ({period} days)"),
    ]) {''')

replace('CLI.md',
    '''原生单币种模拟中 CurrencyPair 仅支持 MARGIN，Equity 支持 CASH/MARGIN；
执行假设和实际运行共用锁定 Nautilus 0.63.0 的该限制，不自动转换旧配置。''',
    '''原生单币种模拟中 CurrencyPair 仅支持 MARGIN，Equity 支持 CASH/MARGIN。
POLYMARKET BinaryOption 使用 CASH、原抵押币和 NAUTILUS_POLYMARKET 费用模型；
需要 polymarket-research/1 镜像、原费用表及覆盖生命周期的来源记录，不自动转换旧配置。
独立数据准备命令 `polymarket-history fetch/import` 的构建、参数、目录与限制见
[Polymarket 历史数据与原生研究](docs/polymarket-history.md)。它不代替 Dataset 登记或研究准入。''')
replace('OPERATIONS.md',
    '''Equity 支持 CASH 或 MARGIN。系统拒绝不支持的账户/资产类组合，不自动切换账户
模型或改写旧执行假设；这只是模拟配置，不涉及真实券商账户。''',
    '''Equity 支持 CASH 或 MARGIN。POLYMARKET BinaryOption 选择 CASH、long-only、
原 USDC／USDC.e／pUSD 和 NAUTILUS_POLYMARKET 费用模型，需要 polymarket-research/1
镜像及原费用／生命周期证据。升级后重新探测 Runtime，并建立新执行假设；不能把
旧探测、旧费用或 USD 配置当作新能力。系统不自动切换账户模型或改写旧执行假设。
这些都是模拟配置，不涉及真实券商账户；[数据准备与使用边界](docs/polymarket-history.md)
说明完整操作顺序。研究使用的抵押币与 Codex 费用预算的 ISO 法币分开。''')

p = Path('docs/polymarket-history.md')
s = p.read_text().replace('# Polymarket 历史数据准备', '# Polymarket 历史数据与原生研究', 1)
s += '''
## 原生研究与组合

数据准备和研究是两个入口。已经完成来源登记、许可、时点与冻结输入校验的
原生 LAST/EXTERNAL BAR，可用于现有 Alpha 和多 Alpha 组合流程；资产保持
POLYMARKET BinaryOption。TradeTick、QuoteTick 和 L2 能保存进原生目录，但本项目
当前科学输入仍为 BAR。只有成交的公开抓取输出不会自动变成 BAR，也不会自动
获取研究资格；供应商档案转换必须保留原始单位、覆盖和来源事实。

操作顺序：准备原生目录和 Runtime 元数据 → 登记 DataSource、用途授权与数据版本
→ 冻结研究输入 → 探测含 polymarket-research/1 的新 Runtime 镜像 → 在现有执行假设
页面选择原抵押币、CASH 和 NAUTILUS_POLYMARKET → 沿用 Alpha 与组合研究入口。
CLI 复用 `client portfolio assumptions create/list/show` 和现有研究命令，字段以
[CLI](../CLI.md)和 Rust 生成合同为准。旧 Runtime 的历史探测不能替代新镜像的实际探测。

费用引用固定为 `nautilus_polymarket::models::PolymarketFeeModel`、0.63.0。
原 BinaryOption.info 必须有匹配的 condition_id、token_id、fee_schedule 和有效时间；
当前来源不支持的费用参数会明确拒绝，而不是默认为免费。逐资产规划费率使用
maker=0、taker=rate+0.000005（原 rate=0 时保持0），仅作保守优化输入。
每笔买卖至少1单位原抵押币的研究下限保证这一上界；实际佣金仍由 Nautilus 根据
成交价格和原费用模型计算。未观察的返佣、Gas、点差或容量不填0冒充数据支持。

跨到期回放必须提供来源可验证的 InstrumentClose/ContractExpired；其 ts_init
决定事件何时进入回放。正常0/1及50/50兑付均由原生引擎完成，缺失或尚未可用的
结算不会按最后价格补齐。不能把计划到期日当作真实可赎回时点；没有资金可用性
证据的样本仍保持该限制。工具和研究都不发送真实订单或赎回交易。

组合使用至少两个不同 Alpha 的原模型和同一原生现金账户，不平均各自独立的
净值曲线。正式 Polymarket 日收益波动率、Sharpe 和 Sortino 使用上游365日配置；
原股票／外汇路径维持252日。Canonical 原始报告仍原样保留，正式发布的组合指标
来自原生日度资金快照，不采用上游缺少快照时的逐持仓收益兜底。

## 跨组件验证范围

`cargo test --locked -p job --test polymarket` 验证原生费用、结算、共享现金、
多 Alpha 和指标绑定；Runtime 的 native_oci 测试另经真实 OCI、数据挂载、任务
输入、结果产物及发布校验重放组合。Web 币种合同测试证明研究抵押币不会进入
模型账单预算。测试素材明确是合成边界样本，不是已验证的市场 Alpha。

完整公开档案的覆盖、供应商字段转换、历史时点证据、真实数据研究与下游交付
仍需单独验收；导入／注册／运行成功不等于这些验收均通过。
'''
p.write_text(s)

p = Path('.opensdlc/tasks/polymarket-native-research/task.md')
s = p.read_text()
s = s.replace('NOT_RUN. No production data, credentials, deployment, scientific run or complete research workflow has been executed by this task yet.', '''Preparation run 35671282868 successfully compiled the native source, ran the contracts/domain library checks, native-history round-trip tests and Polymarket simulation/study tests, and generated API/web contracts. Native source head: 5f01c7a696c7af87ce37ae6b44980fd818297232; generated-contract head: e76a29be34a420d9da0e096accb909c03ff04d7b. This is scoped intermediate evidence, not final-head CI or production acceptance.

The source fixes mixed native Parquet identities, accepts original research collateral through result binding, and selects native 365-day portfolio statistics for Polymarket while retaining the existing 252-day path. Additional Runtime/OCI and web currency regression checks must pass on the final head.

No real historical archive coverage, trading account, wallet, deployment or real-market Alpha/portfolio acceptance is claimed. Synthetic native computation is executed; real data and end-to-end product qualification are separate unexecuted facts.''')
p.write_text(s)
