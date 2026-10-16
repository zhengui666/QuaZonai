//! TimeSeventeen/Polymarket-v2 OrderFilled is normalized, not original integer logs.
//! Its six-decimal amounts are accepted only with a unique, exact float round trip.
//! Optional original RPC evidence can corroborate rows; historical reception is never inferred.
use super::*;

// Polymarket/ctf-exchange-v2 ccc0596074f4dfd62c944fbca4de252893b82b4b:
// README deployments; Trading.sol emits taker summaries with taker = address(this).
pub(super) const EXCHANGES_V2: [&str; 2] = [
    "0xe111180000d2663c0091e4f400237545b87b996b",
    "0xe2222d279d744050d28e00520010520000310f59",
];

pub(super) fn normalized_amount(row: &Row, name: &str) -> Result<Decimal> {
    let Field::Double(value) = field(row, name)? else {
        bail!("V2_NORMALIZED_FLOAT_REQUIRED:{name}");
    };
    ensure!(value.is_finite() && *value > 0.0, "V2_INVALID_AMOUNT");
    // The shortest decimal must itself be on the six-decimal grid. Do not round
    // arbitrary floats to that grid, even if they are close to a plausible amount.
    let amount = Decimal::from_str(&value.to_string())?;
    let units = amount
        .checked_mul(Decimal::from(1_000_000))
        .context("V2_AMOUNT_RANGE")?;
    ensure!(units.fract().is_zero(), "V2_AMOUNT_NOT_SIX_DECIMALS");
    let candidate = units.normalize().to_string().parse::<u64>()?;
    // Keep candidates and their neighbours exactly representable as integers.
    // Then require one and only one candidate to produce the stored float.
    ensure!(
        candidate > 0 && candidate < (1_u64 << 52),
        "V2_AMOUNT_RANGE"
    );
    let normalized = |n: u64| (n as f64 / 1_000_000.0).to_bits();
    ensure!(
        normalized(candidate) == value.to_bits()
            && normalized(candidate - 1) != value.to_bits()
            && normalized(candidate + 1) != value.to_bits(),
        "V2_AMOUNT_NOT_UNIQUELY_RECOVERABLE"
    );
    // This is a validated normalized amount, not a claim to possess raw ABI bytes.
    Ok(amount)
}

pub(super) fn fill(
    row: &Row,
    instruments: &BTreeMap<String, InstrumentAny>,
    quality: &mut Quality,
    chain: Option<&chain::Evidence>,
) -> Result<Option<TradeTick>> {
    let asset = text(row, "token_asset_id")?;
    let Some(instrument) = instruments.get(asset) else {
        return Ok(None);
    };
    let id = text(row, "id")?;
    event_key(id)?; // Exactly one canonical chain/block/log index; never order_hash.
    let taker = text(row, "taker")?;
    if EXCHANGES_V2
        .iter()
        .any(|address| taker.eq_ignore_ascii_case(address))
    {
        quality.exchange_summaries_excluded += 1;
        return Ok(None);
    }
    // The source retains the legacy column name usdc_amount for V2 collateral.
    ensure!(
        instrument.quote_currency().code.as_str() == "pUSD",
        "V2_PUSD_INSTRUMENT_REQUIRED"
    );
    let (shares, cash) = match chain.map(|e| e.matching(id)).transpose()?.flatten() {
        Some(fill) => fill.corroborate(row, quality)?,
        None => (
            normalized_amount(row, "token_amount")?,
            normalized_amount(row, "usdc_amount")?,
        ),
    };
    // Ignore the provider's derived price/direction columns. A mint or merge
    // fill does not identify a simple buyer/seller aggressor for this outcome.
    // An event-time proxy only; the archive keeps historical availability UNVERIFIED.
    Ok(Some(normalized_trade(
        instrument,
        id,
        seconds(row, "timestamp")?,
        shares,
        cash,
        text(row, "maker")?.eq_ignore_ascii_case(taker),
        quality,
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::{
        enums::AssetClass,
        identifiers::{InstrumentId, Symbol},
        instruments::BinaryOption,
        types::Currency,
    };

    fn instruments(currency: &str) -> BTreeMap<String, InstrumentAny> {
        BTreeMap::from([(
            "123".into(),
            InstrumentAny::BinaryOption(
                BinaryOption::builder()
                    .instrument_id(InstrumentId::from_str("condition-123.POLYMARKET").unwrap())
                    .raw_symbol(Symbol::new("123"))
                    .asset_class(AssetClass::Alternative)
                    .currency(Currency::from_str(currency).unwrap())
                    .activation_ns(0_u64.into())
                    .expiration_ns(u64::MAX.into())
                    .price_precision(4)
                    .size_precision(6)
                    .price_increment(Price::from("0.0001"))
                    .size_increment(Quantity::from("0.000001"))
                    .ts_event(0_u64.into())
                    .ts_init(0_u64.into())
                    .build()
                    .unwrap(),
            ),
        )])
    }

    fn row(id: &str, taker: &str, amount: f64) -> Row {
        Row::new(vec![
            ("id".into(), Field::Str(id.into())),
            ("token_asset_id".into(), Field::Str("123".into())),
            ("maker".into(), Field::Str("alice".into())),
            ("taker".into(), Field::Str(taker.into())),
            ("timestamp".into(), Field::Long(1_785_357_600)),
            ("token_amount".into(), Field::Double(amount)),
            ("usdc_amount".into(), Field::Double(1.0)),
        ])
    }

    #[test]
    fn six_decimal_normalization_is_exact_and_never_blindly_rounded() {
        for (number, expected) in [(0.1, "0.1"), (0.000001, "0.000001"), (2.564101, "2.564101")] {
            let value = row("137_1_1", "bob", number);
            assert_eq!(
                normalized_amount(&value, "token_amount").unwrap(),
                Decimal::from_str(expected).unwrap()
            );
        }
        for number in [
            0.0,
            -1.0,
            f64::NAN,
            f64::INFINITY,
            0.0000001,
            1.0000000000000002,
            // Observed in the pinned 2026-08-09 production partition.
            72.47058700000001,
            38.639799000000004,
            9_007_199_254.740992,
        ] {
            assert!(normalized_amount(&row("137_1_1", "bob", number), "token_amount").is_err());
        }
    }

    #[test]
    fn v2_fills_keep_canonical_identity_and_precision_exclude_both_summaries() {
        let mut quality = Quality::default();
        let mappings = instruments("pUSD");
        let tick = fill(
            &row("137_91101943_1088", "alice", 2.000001),
            &mappings,
            &mut quality,
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(tick.trade_id.as_str(), "137_91101943_1088");
        assert_eq!(
            tick.size.as_decimal(),
            Decimal::from_str("2.000001").unwrap()
        );
        assert_eq!(tick.aggressor_side, AggressorSide::NoAggressor);
        assert_eq!(quality.rounded_trade_prices, 1);
        assert_eq!(quality.self_trades, 1);
        for address in EXCHANGES_V2 {
            assert!(fill(
                &row("137_1_1", &address.to_uppercase(), 2.0),
                &mappings,
                &mut quality,
                None
            )
            .unwrap()
            .is_none());
        }
        assert_eq!(quality.exchange_summaries_excluded, 2);
        for id in ["137_01_1", "[137_1_1,137_1_2]", "0xorderhash"] {
            assert!(fill(&row(id, "bob", 2.0), &mappings, &mut quality, None).is_err());
        }
        for currency in ["USDC.e", "USDC"] {
            assert!(fill(
                &row("137_1_1", "bob", 2.0),
                &instruments(currency),
                &mut quality,
                None
            )
            .is_err());
        }
    }
}
