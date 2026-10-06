//! SII-WANGZJ/Polymarket_data raw orderfilled.parquet, not its derived float tables.
//! The published schema stores raw uint256 amounts as 32-byte little-endian values.
//! Original transaction/log identities are retained outside the native 36-byte TradeId.
use super::*;
use alloy_core::primitives::U256;

#[derive(Clone, Debug, Serialize)]
pub(super) struct EventIdentity {
    pub native_trade_id: String,
    pub transaction_hash: String,
    pub block_number: u64,
    pub log_index: u32,
    pub contract: String,
}

fn hex_text<'a>(row: &'a Row, name: &str, bytes: usize) -> Result<&'a str> {
    let value = text(row, name)?;
    ensure!(
        value.len() == 2 + bytes * 2
            && value.starts_with("0x")
            && value[2..]
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "SII_CANONICAL_HEX_REQUIRED:{name}"
    );
    Ok(value)
}

fn token<'a>(row: &'a Row, name: &str) -> Result<&'a str> {
    let value = text(row, name)?;
    ensure!(
        !value.is_empty()
            && value.bytes().all(|c| c.is_ascii_digit())
            && U256::from_str(value).is_ok_and(|n| n.to_string() == value),
        "SII_UINT256_TOKEN_REQUIRED:{name}"
    );
    Ok(value)
}

fn uint256(row: &Row, name: &str) -> Result<U256> {
    let Field::Bytes(value) = field(row, name)? else {
        bail!("SII_RAW_UINT256_BYTES_REQUIRED:{name}");
    };
    ensure!(value.len() == 32, "SII_RAW_UINT256_WIDTH:{name}");
    Ok(U256::from_le_slice(value.data()))
}

fn quantity(row: &Row, name: &str) -> Result<Decimal> {
    let units = uint256(row, name)?;
    ensure!(units > U256::ZERO, "SII_RAW_AMOUNT_NONPOSITIVE:{name}");
    // Fail explicitly outside the pinned native Decimal/Quantity range, never
    // truncate a uint256 or recover exact units from a derived binary float.
    Decimal::from_str(&units.to_string())
        .context("SII_NATIVE_AMOUNT_RANGE")?
        .checked_div(Decimal::from(1_000_000))
        .context("SII_NATIVE_AMOUNT_RANGE")
}

pub(super) fn fill(
    row: &Row,
    instruments: &BTreeMap<String, InstrumentAny>,
    quality: &mut Quality,
) -> Result<Option<(TradeTick, EventIdentity)>> {
    let maker_asset = token(row, "maker_asset_id")?;
    let taker_asset = token(row, "taker_asset_id")?;
    if !instruments.contains_key(maker_asset) && !instruments.contains_key(taker_asset) {
        return Ok(None);
    }
    let contract = text(row, "contract")?;
    let currency = match contract {
        "CTF_EXCHANGE" | "NEGRISK_CTF_EXCHANGE" => "USDC.e",
        "CTF_EXCHANGE_V2" | "NEGRISK_CTF_EXCHANGE_V2" => "pUSD",
        _ => bail!("SII_UNSUPPORTED_EXCHANGE_CONTRACT"),
    };
    let maker = hex_text(row, "maker", 20)?;
    let taker = hex_text(row, "taker", 20)?;
    if EXCHANGES
        .iter()
        .chain(v2::EXCHANGES_V2.iter())
        .any(|address| taker == *address)
    {
        quality.exchange_summaries_excluded += 1;
        return Ok(None);
    }
    ensure!(
        (maker_asset == "0") != (taker_asset == "0"),
        "SOURCE_CASH_LEG_REQUIRED"
    );
    let (asset, cash_field, token_field) = if maker_asset == "0" {
        (taker_asset, "maker_amount_filled", "taker_amount_filled")
    } else {
        (maker_asset, "taker_amount_filled", "maker_amount_filled")
    };
    let instrument = &instruments[asset];
    ensure!(
        instrument.quote_currency().code.as_str() == currency,
        "SII_EXCHANGE_COLLATERAL_MISMATCH"
    );
    let transaction_hash = hex_text(row, "transaction_hash", 32)?;
    hex_text(row, "order_hash", 32)?; // Order hash is not a unique fill identity.
    for name in ["maker_fee", "taker_fee", "protocol_fee"] {
        uint256(row, name)?; // Preserve raw fees in the source; do not invent a fee schedule.
    }
    let block_number = seconds(row, "block_number")?;
    ensure!(block_number > 0, "SII_BLOCK_NUMBER_REQUIRED");
    let log_index = match field(row, "log_index")? {
        Field::UInt(value) => *value,
        Field::Int(value) => u32::try_from(*value).context("SII_LOG_INDEX_RANGE")?,
        _ => bail!("SII_UINT32_LOG_INDEX_REQUIRED"),
    };
    // The collector preserves eth_getLogs.logIndex, which is block-global.
    // Keep the existing canonical chain/block/log identifier and ordering, while
    // checking the independent original transaction/log key in the caller.
    let id = format!("137_{block_number}_{log_index}");
    let trade = normalized_trade(
        instrument,
        &id,
        seconds(row, "timestamp")?,
        quantity(row, token_field)?,
        quantity(row, cash_field)?,
        maker == taker,
        quality,
    )?;
    Ok(Some((
        trade,
        EventIdentity {
            native_trade_id: id,
            transaction_hash: transaction_hash.into(),
            block_number,
            log_index,
            contract: contract.into(),
        },
    )))
}

#[cfg(test)]
#[path = "sii_tests.rs"]
mod tests;
