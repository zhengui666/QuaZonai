"""Apply the exact authored native research integration edits, then remove this file."""
from pathlib import Path

def replace(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if old not in text:
        raise RuntimeError(f"Missing reviewed context: {path}: {old!r}")
    file.write_text(text.replace(old, new))

replace("crates/domain/src/lib.rs", "pub mod portfolio;", "pub mod portfolio;\npub mod prediction;")
replace("apps/job/src/lib.rs", "pub mod portfolio;", "pub mod portfolio;\nmod prediction;")
replace("apps/job/Cargo.toml", 'polymarket-history = ["dep:nautilus-polymarket", "dep:tokio"]', 'polymarket-history = ["dep:tokio"]')
replace("apps/job/Cargo.toml", 'nautilus-polymarket = { version = "=0.63.0", default-features = false, optional = true }', 'nautilus-polymarket = { version = "=0.63.0", default-features = false }')

p = "crates/contracts/src/portfolio.rs"
replace(p, 'pub const NAUTILUS_FEE_CLASS:', 'pub const NAUTILUS_POLYMARKET_FEE_CLASS: &str = "nautilus_polymarket::models::PolymarketFeeModel";\npub const NAUTILUS_FEE_CLASS:')
replace(p, '    NautilusStaticLatency {\n', '''    NautilusPolymarket {
        schema_version: SchemaV1,
        upstream_class: String,
        upstream_version: String,
        parameters: NautilusFeeParametersV1,
    },
    NautilusStaticLatency {
''')
replace(p, '''            (
                "NAUTILUS_STATIC_LATENCY",''', '''            (
                "NAUTILUS_POLYMARKET",
                NAUTILUS_POLYMARKET_FEE_CLASS,
                NAUTILUS_EXECUTION_VERSION,
                NautilusFeeParametersV1::schema(),
            ),
            (
                "NAUTILUS_STATIC_LATENCY",''')

p = "crates/domain/src/portfolio.rs"
replace(p, '''            && upstream_version == NAUTILUS_EXECUTION_VERSION => {}
        _ => return Err(unavailable()),
    }
    let latency''', '''            && upstream_version == NAUTILUS_EXECUTION_VERSION => {}
        NativeModelRefV1::NautilusPolymarket {
            upstream_class, upstream_version, ..
        } if upstream_class == NAUTILUS_POLYMARKET_FEE_CLASS
            && upstream_version == NAUTILUS_EXECUTION_VERSION => {}
        _ => return Err(unavailable()),
    }
    let latency''')

p = "crates/domain/src/catalogs.rs"
replace(p, '("CurrencyPair", NativeAccountKind::Margin) | ("Equity", _) => Ok(()),', '("CurrencyPair", NativeAccountKind::Margin) | ("Equity", _)\n        | ("BinaryOption", NativeAccountKind::Cash) => Ok(()),')
replace(p, '            "Equity" => &value["currency"],', '            "Equity" | "BinaryOption" => &value["currency"],')
old = '''        let maker: contracts::DecimalValue = serde_json::from_value(value["maker_fee"].clone())
            .map_err(|_| bad("execution_fees.maker"))?;
        let taker: contracts::DecimalValue = serde_json::from_value(value["taker_fee"].clone())
            .map_err(|_| bad("execution_fees.taker"))?;'''
new = '''        let (maker, taker): (contracts::DecimalValue, contracts::DecimalValue) = if *class == "BinaryOption" {
            if !crate::prediction::uses_native_fee(&settings.fee_model) {
                return Err(DomainError::CapabilityUnavailable("polymarket_native_fee_model"));
            }
            ("0".parse().map_err(|_| bad("execution_fees.maker"))?, crate::prediction::planning_fee(value)?)
        } else {
            if crate::prediction::uses_native_fee(&settings.fee_model) {
                return Err(DomainError::CapabilityUnavailable("polymarket_binary_instrument"));
            }
            (
                serde_json::from_value(value["maker_fee"].clone()).map_err(|_| bad("execution_fees.maker"))?,
                serde_json::from_value(value["taker_fee"].clone()).map_err(|_| bad("execution_fees.taker"))?,
            )
        };'''
replace(p, old, new)

p = "apps/job/src/simulation.rs"
replace(p, '    fee::{FeeModelHandle, MakerTakerFeeModel},\n', '')
replace(p, '    data::{Bar, BarType, Data},', '    data::{Bar, BarType, Data, InstrumentClose},')
replace(p, '    active_expiry_ns: u64,\n', '    active_expiry_ns: u64,\n    settlement_events: BTreeMap<InstrumentId, InstrumentClose>,\n')
replace(p, '''        let now = event.ts_event.as_u64();
        if now <= self.status.borrow().submitted_after_ns''', '''        let now = event.ts_event.as_u64();
        if crate::prediction::is_native_settlement_order(event.client_order_id.as_str()) {
            let matches = self.settlement_events.get(&event.instrument_id)
                .is_some_and(|close| now >= close.ts_init.as_u64() && event.last_px == close.close_price);
            if !matches {
                self.status.borrow_mut().failure = Some("NATIVE_SETTLEMENT_SOURCE_MISMATCH");
            }
            return;
        }
        if now <= self.status.borrow().submitted_after_ns''')
replace(p, '''                let quantity = instrument.try_make_qty_from_decimal(amount, Some(true))?;''', '''                crate::prediction::order_notional(instrument, now, checked(amount.checked_mul(unit_value))?)?;
                if let InstrumentAny::BinaryOption(binary) = instrument {
                    ensure!(now.checked_add(self.latency_ns).is_some_and(|at| at < binary.expiration_ns.as_u64()),
                        "POLYMARKET_TARGET_EXPIRES_BEFORE_INSERT");
                }
                let quantity = instrument.try_make_qty_from_decimal(amount, Some(true))?;''')
replace(p, '''    domain::portfolio::simulation_settings(settings)?;
    let currency''', '''    domain::portfolio::simulation_settings(settings)?;
    crate::prediction::validate_market(data, settings)?;
    let currency''')
replace(p, '''                InstrumentAny::CurrencyPair(_) | InstrumentAny::Equity(_)
''', '''                InstrumentAny::CurrencyPair(_) | InstrumentAny::Equity(_) | InstrumentAny::BinaryOption(_)
''')
replace(p, '''                && !instrument.has_expiration(),''', '''                && (!instrument.has_expiration() || matches!(instrument, InstrumentAny::BinaryOption(_))),''')
replace(p, '''                "CurrencyPair"
            } else {
                "Equity"''', '''                "CurrencyPair"
            } else if matches!(instrument, InstrumentAny::BinaryOption(_)) {
                "BinaryOption"
            } else {
                "Equity"''')
replace(p, '''        ensure!(
            native_decimal(&rate.maker)? == instrument.maker_fee()
                && native_decimal(&rate.taker)? == instrument.taker_fee(),
            "SIMULATION_FEE_SOURCE_MISMATCH"
        );''', '''        if !matches!(instrument, InstrumentAny::BinaryOption(_)) {
            ensure!(native_decimal(&rate.maker)? == instrument.maker_fee()
                && native_decimal(&rate.taker)? == instrument.taker_fee(),
                "SIMULATION_FEE_SOURCE_MISMATCH");
        }''')
replace(p, '''    let currency = validate_settings(&market, request)?;
    let venue''', '''    let currency = validate_settings(&market, request)?;
    let closes = crate::prediction::close_events(root, &market, &request.selection)?;
    let settlement_events = closes.iter().map(|c| (c.instrument_id, *c)).collect::<BTreeMap<_, _>>();
    let settled_ids = settlement_events.keys().copied().collect::<Vec<_>>();
    let venue''')
replace(p, '        active_expiry_ns: 0,\n', '        active_expiry_ns: 0,\n        settlement_events,\n')
replace(p, '.fee_model(FeeModelHandle::new(MakerTakerFeeModel))', '.fee_model(crate::prediction::fee_model(settings))')
replace(p, '''        engine.add_strategy(strategy)?;
        engine.add_data(events, None, true, true)?;''', '''        events.extend(closes.into_iter().map(Data::InstrumentClose));
        engine.add_strategy(strategy)?;
        engine.add_data(events, None, true, true)?;''')
replace(p, '''        let observed = status.borrow();
        ensure!(observed.failure.is_none(),''', '''        for instrument_id in &settled_ids {
            ensure!(engine.kernel().cache.borrow().positions_open(None, Some(instrument_id), None, None, None).is_empty(),
                "POLYMARKET_NATIVE_SETTLEMENT_INCOMPLETE");
        }
        let observed = status.borrow();
        ensure!(observed.failure.is_none(),''')

p = "apps/job/src/prediction.rs"
replace(p, '''        PolymarketFeeModel.get_commission(order, quantity, price, instrument)''', '''        ensure!(quantity.as_decimal().checked_mul(price.as_decimal())
            .is_some_and(|notional| notional >= Decimal::ONE), "POLYMARKET_RESEARCH_MINIMUM_FILL_NOTIONAL");
        PolymarketFeeModel.get_commission(order, quantity, price, instrument)''')
replace(p, '''                ensure!(series.bars.iter().all(|b| b.ts_event.as_u64() >= activation''', '''                ensure!(series.bars.iter().all(|b| [b.open, b.high, b.low, b.close].iter()
                    .all(|p| p.as_decimal() > Decimal::ZERO && p.as_decimal() < Decimal::ONE)),
                    "POLYMARKET_TRADING_BAR_PRICE_RANGE");
                ensure!(series.bars.iter().all(|b| b.ts_event.as_u64() >= activation''')

# Declare the new actual runtime contract, keeping invalid legacy BinaryOption metadata unadvertised.
for p in ["apps/runtime/src/engine.rs", "runtimes/native/native-job.Dockerfile"]:
    replace(p, ';portfolio-history/1"', ';portfolio-history/1;polymarket-research/1"')
p = "apps/runtime/src/engine.rs"
replace(p, '("portfolio-history".into(), "1".into()),', '("portfolio-history".into(), "1".into()),\n                ("polymarket-research".into(), "1".into()),')
replace(p, '''                    if !matches!(class, "CurrencyPair" | "Equity") {
                        continue;
                    }''', '''                    if !matches!(class, "CurrencyPair" | "Equity")
                        && !(class == "BinaryOption" && domain::prediction::instrument(definition).is_ok()) {
                        continue;
                    }''')
replace(p, '''                        venue,
                        instrument_classes: classes.into_iter().collect(),
                        data_kinds: vec![RuntimeDataKind::Bar],
                        expiry_and_settlement: false,''', '''                        venue,
                        expiry_and_settlement: classes.contains("BinaryOption"),
                        instrument_classes: classes.into_iter().collect(),
                        data_kinds: vec![RuntimeDataKind::Bar],''')
p = "crates/store/src/execution_assumptions.rs"
replace(p, '''        let image = cap
            .image_refs''', '''        if domain::prediction::uses_native_fee(&request.settings.fee_model)
            && cap.engine_versions.get("polymarket-research").map(String::as_str) != Some("1") {
            return Err(domain::DomainError::CapabilityUnavailable("polymarket_research").into());
        }
        let image = cap
            .image_refs''')

# The form still sends a typed frozen model reference, not arbitrary upstream code.
p = "apps/web/src/execution-assumptions.tsx"
replace(p, '  use_bar_liquidity?: boolean;', "  fee_kind?: 'NAUTILUS_MAKER_TAKER' | 'NAUTILUS_POLYMARKET';\n  use_bar_liquidity?: boolean;")
replace(p, 'const { fill, latency, use_bar_liquidity,', 'const { fee_kind, fill, latency, use_bar_liquidity,')
replace(p, "fee_model: { schema_version: 1, adapter_kind: 'NAUTILUS_MAKER_TAKER', upstream_class: 'nautilus_execution::models::fee::MakerTakerFeeModel', upstream_version: '0.63.0', parameters: {} },", "fee_model: fee_kind === 'NAUTILUS_POLYMARKET'\n        ? { schema_version: 1, adapter_kind: 'NAUTILUS_POLYMARKET', upstream_class: 'nautilus_polymarket::models::PolymarketFeeModel', upstream_version: '0.63.0', parameters: {} }\n        : { schema_version: 1, adapter_kind: 'NAUTILUS_MAKER_TAKER', upstream_class: 'nautilus_execution::models::fee::MakerTakerFeeModel', upstream_version: '0.63.0', parameters: {} },")
replace(p, 'initialValues={{ settings: { fee_rates: [{}] } }}', "initialValues={{ fee_kind: 'NAUTILUS_MAKER_TAKER', settings: { fee_rates: [{}] } }}")
replace(p, '      <Typography.Title level={2}>逐资产原生费用</Typography.Title>', '''      <Form.Item name="fee_kind" label="费用模型" rules={[required]}><Select options={[
        { value: 'NAUTILUS_MAKER_TAKER', label: 'Nautilus Maker / Taker' },
        { value: 'NAUTILUS_POLYMARKET', label: 'Nautilus Polymarket' },
      ]} /></Form.Item>
      <Typography.Title level={2}>逐资产规划费率</Typography.Title>''')

replace("DESIGN.md", "## Polymarket 研究抵押币\n", '''## Polymarket 原生研究与组合

原生 BinaryOption 研究使用 POLYMARKET、原 condition/token 身份、原抵押币和
当时可见的资产定义。现有有界 BAR Alpha／组合流程复用这些资产；不得将
第三方价格点伪装为成交 BAR，或把当前 Gamma 快照回填为历史资产定义。
原生执行选择 CASH、long-only 和统一资金账户；不把 NO 买入当成裸卖空 YES。
组合、独立研究和目标证据继续使用现有冻结输入与 Clarabel，不新增回测内核。

NAUTILUS_POLYMARKET 引用锁定 PolymarketFeeModel；费用 schedule 必须存在，
且符合原生 exponent=1/takerOnly 合同。缺失不等于免费；真实零费表可以为零。
模拟普通订单仍由原生模型计算佣金。规划字段 maker=0、taker=rate+0.000005
（rate=0 时为0）为保守费率上界，不是固定实际收费；仅在每次买卖实际成交
名义金额至少为1原抵押币时有效。下单与成交均校验此研究下限，不冒充交易所
最低金额规则。策略当前只发市价单，不计算未观察的 maker 奖励。

到期／结算使用目录中的原生 InstrumentClose(ContractExpired)，独立于 BAR。
预测器不查询结算标签。事件在实际 ts_init 到达原生引擎；到期后没有来源结算
事件的跨到期研究返回 POLYMARKET_PENDING_RESOLUTION，不能按最后价格补0/1。
0、1及50/50兑付由原生引擎处理；其生成的 EXPIRATION 原生平仓只作兑付，
不额外收取交易佣金。既有投资决策订单的延迟和有效期检查仍独立有效。
保留来源中的实际可用时间；该适配不自动代办链上赎回或估算未观察gas。
未知赎回可用性不可据此被标成已验证真实资金占用。

Runtime 镜像声明 polymarket-research/1，仅有效原生资产才广告市场；旧镜像
不具备新能力。登记执行假设要求对应版本；现有许可、PIT、Sealed和目标交付
边界不因该能力放开。新适配须以实际原生结算、共享资金和多Alpha测试验收。

## Polymarket 研究抵押币
''')
