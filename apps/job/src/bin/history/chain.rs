//! Original EVM evidence stays outside scientific jobs and never supplies PIT clocks.
use super::*;
use alloy_core::{
    primitives::{Address, Bytes, B256, U256},
    sol,
    sol_types::SolEvent,
};
use serde_json::Value;

// Protocol ABI facts, not a vendored exchange implementation.
// Polymarket/ctf-exchange ITradingEE and ctf-exchange-v2 ccc05960 ITradingEE.
sol! {
    #[allow(clippy::too_many_arguments)] // Fixed protocol event fields.
    interface V1 {
        event OrderFilled(bytes32 indexed orderHash, address indexed maker, address indexed taker,
            uint256 makerAssetId, uint256 takerAssetId, uint256 makerAmountFilled,
            uint256 takerAmountFilled, uint256 fee);
    }
    #[allow(clippy::too_many_arguments)] // Fixed protocol event fields.
    interface V2 {
        event OrderFilled(bytes32 indexed orderHash, address indexed maker, address indexed taker,
            uint8 side, uint256 tokenId, uint256 makerAmountFilled, uint256 takerAmountFilled,
            uint256 fee, bytes32 builder, bytes32 metadata);
    }
}

#[derive(clap::Args)]
pub struct Arguments {
    /// Original two-endpoint JSON archive emitted by runtimes/data/evm.py.
    #[arg(long)]
    snapshot: PathBuf,
    /// Original native InstrumentAny JSON array, including historical observation times.
    #[arg(long)]
    instruments: PathBuf,
    #[arg(long)]
    start_seconds: u64,
    #[arg(long)]
    end_seconds: u64,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=86400))]
    bar_seconds: Option<u32>,
    #[arg(long)]
    pub output: PathBuf,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema_version: contracts::SchemaV1,
    chain_id: u64,
    retrieved_at: DateTime<Utc>,
    query: Query,
    observations: Vec<Observation>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Query {
    blocks: Vec<u64>,
    addresses: Vec<Address>,
    topic0: B256,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    endpoint: String,
    chain_id: Value,
    blocks: Vec<BlockObservation>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BlockObservation {
    number: u64,
    header: Value,
    logs: Value,
    received_at: DateTime<Utc>,
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Header {
    number: String,
    hash: B256,
    parent_hash: B256,
    timestamp: String,
    receipts_root: B256,
    transactions_root: B256,
    state_root: B256,
    transactions: Vec<B256>,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Log {
    address: Address,
    topics: Vec<B256>,
    data: Bytes,
    block_number: String,
    block_hash: B256,
    transaction_hash: B256,
    transaction_index: String,
    log_index: String,
    removed: bool,
}

fn result<T: serde::de::DeserializeOwned>(response: &Value) -> Result<T> {
    ensure!(
        response["jsonrpc"] == "2.0" && response["id"] == 1 && response.get("error").is_none(),
        "INVALID_RPC_RESPONSE"
    );
    Ok(serde_json::from_value(
        response
            .get("result")
            .context("MISSING_RPC_RESULT")?
            .clone(),
    )?)
}

fn quantity(value: &str) -> Result<u64> {
    let result = u64::from_str_radix(value.strip_prefix("0x").context("RPC_HEX_QUANTITY")?, 16)?;
    ensure!(
        format!("{result:#x}") == value.to_ascii_lowercase(),
        "NONCANONICAL_RPC_QUANTITY"
    );
    Ok(result)
}

fn endpoint_host(endpoint: &str) -> Result<String> {
    ensure!(
        endpoint.len() <= 2000
            && !endpoint
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || "@?#".contains(c)),
        "PUBLIC_RPC_ENDPOINT_REQUIRED"
    );
    let parsed = url::Url::parse(endpoint)?;
    ensure!(
        parsed.scheme() == "https" && parsed.port() != Some(0),
        "PUBLIC_HTTPS_RPC_REQUIRED"
    );
    let host = parsed.host_str().context("PUBLIC_RPC_ENDPOINT_REQUIRED")?;
    // Ports and trailing DNS dots do not make a second independent host.
    Ok(host.trim_end_matches('.').to_ascii_lowercase())
}

fn checked_block(
    block: &BlockObservation,
    query: &Query,
    retrieved_at: DateTime<Utc>,
) -> Result<(Header, Vec<Log>)> {
    let mut header: Header = result(&block.header)?;
    let at = quantity(&header.timestamp)?;
    ensure!(
        quantity(&header.number)? == block.number
            && header.hash != B256::ZERO
            && at <= u64::try_from(block.received_at.timestamp())?
            && block.received_at <= retrieved_at,
        "CHAIN_HEADER_IDENTITY_OR_TIME"
    );
    header.number = format!("{:#x}", block.number);
    header.timestamp = format!("{at:#x}");
    ensure!(
        header.transactions.iter().collect::<BTreeSet<_>>().len() == header.transactions.len(),
        "DUPLICATE_CHAIN_TRANSACTION"
    );
    let mut logs: Vec<Log> = result(&block.logs)?;
    ensure!(logs.len() <= MAX_ROWS, "CHAIN_LOG_LIMIT");
    let mut indices = BTreeSet::new();
    for log in &mut logs {
        let index = quantity(&log.log_index)?;
        let transaction_index = usize::try_from(quantity(&log.transaction_index)?)?;
        ensure!(
            !log.removed
                && log.block_hash == header.hash
                && quantity(&log.block_number)? == block.number
                && header.transactions.get(transaction_index) == Some(&log.transaction_hash)
                && log.transaction_hash != B256::ZERO
                && query.addresses.contains(&log.address)
                && log.topics.first() == Some(&query.topic0)
                && indices.insert(index),
            "CHAIN_LOG_IDENTITY_OR_QUERY"
        );
        log.block_number = format!("{:#x}", block.number);
        log.log_index = format!("{index:#x}");
        log.transaction_index = format!("{transaction_index:#x}");
    }
    logs.sort_by_key(|log| quantity(&log.log_index).expect("validated index"));
    ensure!(
        logs.windows(2).all(
            |w| quantity(&w[0].transaction_index).expect("validated index")
                <= quantity(&w[1].transaction_index).expect("validated index")
        ),
        "CHAIN_TRANSACTION_ORDER"
    );
    Ok((header, logs))
}

fn exchange(address: Address, addresses: &[&str]) -> bool {
    addresses
        .iter()
        .any(|a| address == Address::from_str(a).expect("constant address"))
}

pub(super) struct Fill {
    id: String,
    pub(super) at: u64,
    asset: String,
    maker: Address,
    taker: Address,
    order_hash: B256,
    cash: U256,
    shares: U256,
    fee: U256,
    v2: bool,
    buy: bool,
    builder: B256,
    metadata: B256,
}

fn decode(log: &Log, at: u64) -> Result<Fill> {
    let id = format!(
        "137_{}_{}",
        quantity(&log.block_number)?,
        quantity(&log.log_index)?
    );
    // Strict data length also rejects appended bytes that some ABI decoders ignore.
    ensure!(log.topics.len() == 4, "CHAIN_EVENT_TOPICS");
    if exchange(log.address, &v2::EXCHANGES_V2) {
        ensure!(log.data.len() == 7 * 32, "CHAIN_EVENT_DATA");
        let event =
            V2::OrderFilled::decode_raw_log_validate(log.topics.iter().copied(), &log.data)?;
        ensure!(event.side <= 1, "CHAIN_V2_SIDE");
        let buy = event.side == 0;
        Ok(Fill {
            id,
            at,
            asset: event.tokenId.to_string(),
            maker: event.maker,
            taker: event.taker,
            order_hash: event.orderHash,
            cash: if buy {
                event.makerAmountFilled
            } else {
                event.takerAmountFilled
            },
            shares: if buy {
                event.takerAmountFilled
            } else {
                event.makerAmountFilled
            },
            fee: event.fee,
            v2: true,
            buy,
            builder: event.builder,
            metadata: event.metadata,
        })
    } else {
        ensure!(
            exchange(log.address, &EXCHANGES) && log.data.len() == 5 * 32,
            "CHAIN_V1_EXCHANGE_OR_DATA"
        );
        let event =
            V1::OrderFilled::decode_raw_log_validate(log.topics.iter().copied(), &log.data)?;
        let buy = event.makerAssetId == U256::ZERO;
        ensure!(
            buy != (event.takerAssetId == U256::ZERO),
            "CHAIN_CASH_LEG_REQUIRED"
        );
        Ok(Fill {
            id,
            at,
            asset: if buy {
                event.takerAssetId
            } else {
                event.makerAssetId
            }
            .to_string(),
            maker: event.maker,
            taker: event.taker,
            order_hash: event.orderHash,
            cash: if buy {
                event.makerAmountFilled
            } else {
                event.takerAmountFilled
            },
            shares: if buy {
                event.takerAmountFilled
            } else {
                event.makerAmountFilled
            },
            fee: event.fee,
            v2: false,
            buy,
            builder: B256::ZERO,
            metadata: B256::ZERO,
        })
    }
}

fn units(value: U256) -> Result<Decimal> {
    Decimal::from_str(&value.to_string())?
        .checked_div(Decimal::from(1_000_000))
        .context("CHAIN_AMOUNT_RANGE")
}

impl Fill {
    fn summary(&self) -> bool {
        exchange(
            self.taker,
            if self.v2 {
                &v2::EXCHANGES_V2
            } else {
                &EXCHANGES
            },
        )
    }

    fn trade(&self, instrument: &InstrumentAny, quality: &mut Quality) -> Result<TradeTick> {
        ensure!(
            instrument.quote_currency().code.as_str() == if self.v2 { "pUSD" } else { "USDC.e" },
            "CHAIN_COLLATERAL_MISMATCH"
        );
        normalized_trade(
            instrument,
            &self.id,
            self.at,
            units(self.shares)?,
            units(self.cash)?,
            self.maker == self.taker,
            quality,
        )
    }

    pub(super) fn corroborate(
        &self,
        row: &Row,
        quality: &mut Quality,
    ) -> Result<(Decimal, Decimal)> {
        ensure!(
            self.v2
                && self.id == text(row, "id")?
                && self.asset == text(row, "token_asset_id")?
                && self.at == seconds(row, "timestamp")?
                && self.maker == Address::from_str(text(row, "maker")?)?
                && self.taker == Address::from_str(text(row, "taker")?)?
                && self.order_hash == B256::from_str(text(row, "order_hash")?)?
                && text(row, "maker_direction")? == if self.buy { "BUY" } else { "SELL" }
                && self.builder == B256::from_str(text(row, "builder")?)?
                && self.metadata == B256::from_str(text(row, "metadata")?)?,
            "CHAIN_VENDOR_IDENTITY_MISMATCH"
        );
        for (name, raw) in [
            ("token_amount", self.shares),
            ("usdc_amount", self.cash),
            ("fee_usdc", self.fee),
        ] {
            let Field::Double(value) = field(row, name)? else {
                bail!("V2_NORMALIZED_FLOAT_REQUIRED:{name}");
            };
            let integer = u64::try_from(raw)?;
            ensure!(integer < (1_u64 << 52), "CHAIN_VENDOR_FLOAT_RANGE");
            let normalized = integer as f64 / 1_000_000.0;
            // The observed vendor error is at most one float ULP. This comparison
            // permits that representation error only against a proven ABI integer;
            // amounts always come from the ABI, never from rounding the vendor.
            ensure!(
                value.is_finite()
                    && *value >= 0.0
                    && [
                        normalized.to_bits(),
                        normalized.next_up().to_bits(),
                        normalized.next_down().to_bits()
                    ]
                    .contains(&value.to_bits())
                    && (Decimal::from_str(&value.to_string())? - units(raw)?).abs()
                        < Decimal::new(5, 7),
                "CHAIN_VENDOR_AMOUNT_CONFLICT"
            );
            if name != "fee_usdc" && v2::normalized_amount(row, name).is_err() {
                quality.chain_recovered_amounts += 1;
            }
        }
        quality.chain_corroborated_fills += 1;
        Ok((units(self.shares)?, units(self.cash)?))
    }
}

pub(super) struct Evidence {
    snapshot: Snapshot,
    sha256: String,
    fills: BTreeMap<String, Fill>,
}

impl Evidence {
    pub(super) fn load(path: &Path) -> Result<Self> {
        let snapshot: Snapshot = read_json(path)?;
        ensure!(
            snapshot.chain_id == 137
                && snapshot.retrieved_at <= Utc::now()
                && (2..=4).contains(&snapshot.observations.len())
                && (1..=4096).contains(&snapshot.query.blocks.len())
                && snapshot.query.blocks.windows(2).all(|w| w[0] < w[1]),
            "CHAIN_SNAPSHOT_SCOPE"
        );
        let addresses = &snapshot.query.addresses;
        let version2 = snapshot.query.topic0 == V2::OrderFilled::SIGNATURE_HASH;
        ensure!(
            (version2 || snapshot.query.topic0 == V1::OrderFilled::SIGNATURE_HASH)
                && !addresses.is_empty()
                && addresses.len() <= 2
                && addresses.iter().collect::<BTreeSet<_>>().len() == addresses.len()
                && addresses.iter().all(|a| exchange(
                    *a,
                    if version2 {
                        &v2::EXCHANGES_V2
                    } else {
                        &EXCHANGES
                    }
                )),
            "CHAIN_EXCHANGE_QUERY_REQUIRED"
        );
        let mut hosts = BTreeSet::new();
        let mut canonical = Vec::new();
        for (index, observation) in snapshot.observations.iter().enumerate() {
            ensure!(
                hosts.insert(endpoint_host(&observation.endpoint)?),
                "DISTINCT_RPC_HOSTS_REQUIRED"
            );
            let chain_id: String = result(&observation.chain_id)?;
            ensure!(quantity(&chain_id)? == snapshot.chain_id, "RPC_WRONG_CHAIN");
            ensure!(
                observation.blocks.len() == snapshot.query.blocks.len(),
                "CHAIN_BLOCK_COVERAGE"
            );
            let mut total = 0;
            for (position, block) in observation.blocks.iter().enumerate() {
                ensure!(
                    block.number == snapshot.query.blocks[position],
                    "CHAIN_BLOCK_COVERAGE"
                );
                let checked = checked_block(block, &snapshot.query, snapshot.retrieved_at)?;
                total += checked.1.len();
                ensure!(total <= MAX_ROWS, "CHAIN_LOG_LIMIT");
                if index == 0 {
                    if let Some((previous, _)) = canonical.last() {
                        let previous: &Header = previous;
                        ensure!(
                            quantity(&previous.timestamp)? <= quantity(&checked.0.timestamp)?,
                            "CHAIN_BLOCK_TIME_ORDER"
                        );
                        if quantity(&previous.number)?.checked_add(1) == Some(block.number) {
                            ensure!(
                                previous.hash == checked.0.parent_hash,
                                "CHAIN_PARENT_MISMATCH"
                            );
                        }
                    }
                    canonical.push(checked);
                } else {
                    ensure!(canonical[position] == checked, "RPC_EVIDENCE_DISAGREEMENT");
                }
            }
        }
        let mut fills = BTreeMap::new();
        for (header, logs) in canonical {
            for log in logs {
                let fill = decode(&log, quantity(&header.timestamp)?)?;
                ensure!(
                    fills.insert(fill.id.clone(), fill).is_none(),
                    "DUPLICATE_CHAIN_EVENT"
                );
            }
        }
        Ok(Self {
            snapshot,
            sha256: digest(path)?,
            fills,
        })
    }

    pub(super) fn require_v2_exchanges(&self) -> Result<()> {
        ensure!(
            self.snapshot.query.topic0 == V2::OrderFilled::SIGNATURE_HASH
                && self.snapshot.query.addresses.len() == 2,
            "CHAIN_V2_BOTH_EXCHANGES_REQUIRED"
        );
        Ok(())
    }

    pub(super) fn observed_at(&self) -> DateTime<Utc> {
        self.snapshot.retrieved_at
    }

    pub(super) fn metadata(&self) -> Value {
        serde_json::json!({
            "snapshot": self.snapshot, "sha256": self.sha256,
            "assurance": "Two RPC endpoints agree on the selected logs and block identities; not an independently verified receipt trie or historical availability proof.",
            "rights": "Original public blockchain facts; RPC provider terms apply. No dataset redistribution license or QuaZonai source grant is inferred."
        })
    }

    pub(super) fn matching(&self, id: &str) -> Result<Option<&Fill>> {
        let (block, _) = event_key(id)?;
        if self.snapshot.query.blocks.binary_search(&block).is_ok() {
            Ok(Some(
                self.fills
                    .get(id)
                    .context("VENDOR_FILL_ABSENT_FROM_CHAIN")?,
            ))
        } else {
            Ok(None)
        }
    }

    pub(super) fn check_archive_selection(
        &self,
        instruments: &BTreeMap<String, InstrumentAny>,
        start: u64,
        end: u64,
        identities: &BTreeMap<String, String>,
    ) -> Result<()> {
        for fill in self.fills.values() {
            if fill.at >= start
                && fill.at < end
                && instruments.contains_key(&fill.asset)
                && !fill.summary()
            {
                ensure!(
                    identities.contains_key(&fill.id),
                    "CHAIN_FILL_ABSENT_FROM_ARCHIVE"
                );
            }
        }
        Ok(())
    }
}

pub fn prepare(args: &Arguments) -> Result<NativeArchive> {
    ensure!(
        args.start_seconds < args.end_seconds
            && args.end_seconds <= u64::try_from(Utc::now().timestamp())?,
        "INVALID_HISTORY_WINDOW"
    );
    if let Some(interval) = args.bar_seconds {
        ensure!(
            (1..=86400).contains(&interval)
                && args.start_seconds.is_multiple_of(u64::from(interval))
                && args.end_seconds.is_multiple_of(u64::from(interval)),
            "FULL_BAR_INTERVALS_REQUIRED"
        );
    }
    let evidence = Evidence::load(&args.snapshot)?;
    let (instruments, by_token) = load_instruments(&args.instruments)?;
    let mut archive = NativeArchive {
        schema_version: contracts::SchemaV1,
        source_reference: evidence.snapshot.observations[0].endpoint.clone(),
        source_observed_at: evidence.observed_at(),
        source_metadata: Value::Null,
        instruments,
        trades: Vec::new(),
        quotes: Vec::new(),
        deltas: Vec::new(),
        bars: Vec::new(),
        closes: Vec::new(),
    };
    let mut quality = Quality {
        scanned_rows: evidence.fills.len() as u64,
        ..Quality::default()
    };
    for fill in evidence.fills.values() {
        if fill.at < args.start_seconds || fill.at >= args.end_seconds {
            continue;
        }
        let Some(instrument) = by_token.get(&fill.asset) else {
            continue;
        };
        if fill.summary() {
            quality.exchange_summaries_excluded += 1;
            continue;
        }
        archive.trades.push(fill.trade(instrument, &mut quality)?);
        quality.selected_rows += 1;
    }
    archive.trades.sort_by_key(|t| {
        (
            t.instrument_id,
            t.ts_event,
            event_key(t.trade_id.as_str()).expect("validated ID"),
        )
    });
    if let Some(interval) = args.bar_seconds {
        aggregate(&mut archive, interval)?;
    }
    archive.source_metadata = serde_json::json!({
        "chain_evidence": evidence.metadata(), "quality": quality,
        "selection": {"start_seconds": args.start_seconds, "end_seconds": args.end_seconds, "bar_seconds": args.bar_seconds},
        "instruments_sha256": digest(&args.instruments)?,
        "availability": "UNVERIFIED: block time is an event-time proxy. Present RPC retrieval does not backdate historical receipt or finality.",
        "coverage": "Only explicit queried blocks/contracts/event topic and selected assets/window. Sparse blocks do not imply continuous coverage.",
        "trade_semantics": "Original ABI integers; exchange-counterparty summaries excluded, self trades retained, aggressor unknown and native price rounding counted.",
        "fees_and_settlement": "Original event fee bytes are retained. Historical fee regimes, outcomes and settlement availability are not inferred."
    });
    super::super::validate(&archive)?;
    Ok(archive)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(directory: &Path) -> (Arguments, Value) {
        let event = V2::OrderFilled {
            orderHash: B256::repeat_byte(1),
            maker: Address::repeat_byte(2),
            taker: Address::repeat_byte(3),
            side: 0,
            tokenId: U256::from(123),
            makerAmountFilled: U256::from(23_190_588),
            takerAmountFilled: U256::from(72_470_587),
            fee: U256::ZERO,
            builder: B256::ZERO,
            metadata: B256::ZERO,
        }
        .encode_log_data();
        let header = serde_json::json!({
            "number":"0x64", "hash":B256::repeat_byte(4), "parentHash":B256::repeat_byte(5),
            "timestamp":"0x3c", "receiptsRoot":B256::repeat_byte(6),
            "transactionsRoot":B256::repeat_byte(7), "stateRoot":B256::repeat_byte(8),
            "transactions":[B256::repeat_byte(9)]
        });
        let log = serde_json::json!({
            "address":v2::EXCHANGES_V2[0], "topics":event.topics(), "data":event.data,
            "blockNumber":"0x64", "blockHash":header["hash"],
            "transactionHash":header["transactions"][0], "transactionIndex":"0x0",
            "logIndex":"0x305", "removed":false
        });
        let rpc = |result: Value| serde_json::json!({"jsonrpc":"2.0", "id":1, "result":result});
        let snapshot = serde_json::json!({
            "schema_version":1, "chain_id":137, "retrieved_at":"2026-09-01T00:00:00Z",
            "query":{"blocks":[100], "addresses":v2::EXCHANGES_V2, "topic0":V2::OrderFilled::SIGNATURE_HASH},
            "observations":(["https://a.invalid/", "https://b.invalid/"].map(|endpoint| serde_json::json!({
                "endpoint":endpoint, "chain_id":rpc("0x89".into()),
                "blocks":[{"number":100, "header":rpc(header.clone()), "logs":rpc(serde_json::json!([log])),
                    "received_at":"2026-09-01T00:00:00Z"}]
            })))
        });
        let mut instrument = serde_json::to_value(super::super::tests::instrument()).unwrap();
        instrument["BinaryOption"]["currency"] = "pUSD".into();
        let instruments = directory.join("instruments.json");
        fs::write(&instruments, serde_json::to_vec(&vec![instrument]).unwrap()).unwrap();
        let args = Arguments {
            snapshot: directory.join("chain.json"),
            instruments,
            start_seconds: 0,
            end_seconds: 120,
            bar_seconds: Some(60),
            output: directory.join("native"),
        };
        fs::write(&args.snapshot, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        (args, snapshot)
    }

    fn vendor_row() -> Row {
        let mut fields = [
            ("id", "137_100_773".to_string()),
            ("token_asset_id", "123".into()),
            ("order_hash", B256::repeat_byte(1).to_string()),
            ("maker", Address::repeat_byte(2).to_string()),
            ("taker", Address::repeat_byte(3).to_string()),
            ("maker_direction", "BUY".into()),
            ("builder", B256::ZERO.to_string()),
            ("metadata", B256::ZERO.to_string()),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), Field::Str(v)))
        .collect::<Vec<_>>();
        fields.extend([
            ("timestamp".into(), Field::Long(60)),
            // Exact regression from the actual 2026-08-09 archive.
            ("token_amount".into(), Field::Double(72.47058700000001)),
            ("usdc_amount".into(), Field::Double(23.190588)),
            ("fee_usdc".into(), Field::Double(0.0)),
        ]);
        Row::new(fields)
    }

    #[test]
    fn raw_chain_roundtrip_recovers_source_loss_without_promoting_qualification() {
        assert_eq!(
            V2::OrderFilled::SIGNATURE_HASH.to_string(),
            "0xd543adfd945773f1a62f74f0ee55a5e3b9b1a28262980ba90b1a89f2ea84d8ee"
        );
        let directory = tempfile::tempdir().unwrap();
        let (args, _) = fixture(directory.path());
        let evidence = Evidence::load(&args.snapshot).unwrap();
        let (_, instruments) = load_instruments(&args.instruments).unwrap();
        let mut quality = Quality::default();
        assert!(v2::fill(&vendor_row(), &instruments, &mut quality, None).is_err());
        let tick = v2::fill(&vendor_row(), &instruments, &mut quality, Some(&evidence))
            .unwrap()
            .unwrap();
        assert_eq!(
            tick.size.as_decimal(),
            Decimal::from_str("72.470587").unwrap()
        );
        assert_eq!(quality.chain_recovered_amounts, 1);
        assert_eq!(quality.chain_corroborated_fills, 1);
        assert_eq!(tick.ts_init, tick.ts_event); // proxy remains explicitly UNVERIFIED
        let archive = prepare(&args).unwrap();
        assert_eq!(archive.trades, vec![tick]);
        assert_eq!(archive.bars.len(), 1);
        let report = super::super::super::import(archive, &args.output).unwrap();
        assert_eq!(report.coverage, "UNPROVEN");
        assert_eq!(report.historical_availability, "UNVERIFIED");
        assert!(!report.registered_in_quazonai);
        let mut catalog = nautilus_persistence::backend::catalog::ParquetDataCatalog::from_uri(
            args.output.join("catalog").to_str().unwrap(),
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            catalog
                .query::<TradeTick>(None, None, None, None, None, true)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec![nautilus_model::data::Data::Trade(tick)]
        );
    }

    #[test]
    fn disagreeing_or_inconsistent_rpc_evidence_fails_before_publication() {
        let directory = tempfile::tempdir().unwrap();
        let (args, snapshot) = fixture(directory.path());
        for pointer in [
            "/chain_id",
            "/observations/1/chain_id/result",
            "/observations/1/blocks/0/header/result/hash",
            "/observations/1/blocks/0/header/result/timestamp",
            "/observations/1/blocks/0/logs/result/0/transactionHash",
            "/observations/1/blocks/0/logs/result/0/blockHash",
            "/observations/1/blocks/0/logs/result/0/removed",
            "/observations/1/blocks/0/logs/result/0/data",
        ] {
            let mut bad = snapshot.clone();
            *bad.pointer_mut(pointer).unwrap() = Value::Null;
            fs::write(&args.snapshot, serde_json::to_vec(&bad).unwrap()).unwrap();
            assert!(prepare(&args).is_err(), "{pointer}");
            assert!(!args.output.exists());
        }
        let mut disagree = snapshot.clone();
        disagree["observations"][1]["blocks"][0]["logs"]["result"][0]["logIndex"] = "0x306".into();
        fs::write(&args.snapshot, serde_json::to_vec(&disagree).unwrap()).unwrap();
        assert!(Evidence::load(&args.snapshot)
            .err()
            .unwrap()
            .to_string()
            .contains("RPC_EVIDENCE_DISAGREEMENT"));
        fs::write(&args.snapshot, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        let evidence = Evidence::load(&args.snapshot).unwrap();
        let (_, instruments) = load_instruments(&args.instruments).unwrap();
        assert!(evidence
            .check_archive_selection(&instruments, 0, 120, &BTreeMap::new())
            .is_err());
        assert!(evidence.matching("137_100_774").is_err());
        assert!(evidence.matching("137_101_774").unwrap().is_none());
        for name in [
            "token_asset_id",
            "maker",
            "order_hash",
            "maker_direction",
            "token_amount",
            "timestamp",
        ] {
            let mut row = vendor_row().into_columns();
            let target = &mut row.iter_mut().find(|(key, _)| key == name).unwrap().1;
            *target = match name {
                "token_amount" => Field::Double(72.470586),
                "timestamp" => Field::Long(61),
                _ => Field::Str("wrong".into()),
            };
            let mut quality = Quality::default();
            assert!(
                evidence.fills["137_100_773"]
                    .corroborate(&Row::new(row), &mut quality)
                    .is_err(),
                "{name}"
            );
        }
    }

    #[test]
    fn v1_abi_keeps_cash_token_side_and_excludes_summary_without_using_order_hash_as_identity() {
        let event = V1::OrderFilled {
            orderHash: B256::repeat_byte(1),
            maker: Address::repeat_byte(2),
            taker: Address::from_str(EXCHANGES[0]).unwrap(),
            makerAssetId: U256::from(123),
            takerAssetId: U256::ZERO,
            makerAmountFilled: U256::from(2_000_001),
            takerAmountFilled: U256::from(1_000_000),
            fee: U256::ZERO,
        }
        .encode_log_data();
        let mut log = Log {
            address: Address::from_str(EXCHANGES[0]).unwrap(),
            topics: event.topics().to_vec(),
            data: event.data,
            block_number: "0x64".into(),
            block_hash: B256::repeat_byte(4),
            transaction_hash: B256::repeat_byte(9),
            transaction_index: "0x0".into(),
            log_index: "0x1".into(),
            removed: false,
        };
        let first = decode(&log, 60).unwrap();
        assert_eq!(first.asset, "123");
        assert_eq!(
            units(first.shares).unwrap(),
            Decimal::from_str("2.000001").unwrap()
        );
        assert_eq!(units(first.cash).unwrap(), Decimal::ONE);
        assert!(first.summary());
        log.log_index = "0x2".into();
        let second = decode(&log, 60).unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(first.order_hash, second.order_hash);
        log.data = [log.data.as_ref(), &[0_u8; 32]].concat().into();
        assert!(decode(&log, 60).is_err());
    }

    #[test]
    fn v2_sell_recovers_cash_and_does_not_add_the_matching_summary_to_volume() {
        let directory = tempfile::tempdir().unwrap();
        let (args, mut snapshot) = fixture(directory.path());
        let event = V2::OrderFilled {
            orderHash: B256::repeat_byte(1),
            maker: Address::repeat_byte(2),
            taker: Address::repeat_byte(3),
            side: 1,
            tokenId: U256::from(123),
            makerAmountFilled: U256::from(39_030_100),
            takerAmountFilled: U256::from(38_639_799),
            fee: U256::ZERO,
            builder: B256::ZERO,
            metadata: B256::ZERO,
        }
        .encode_log_data();
        for observation in snapshot["observations"].as_array_mut().unwrap() {
            let logs = observation["blocks"][0]["logs"]["result"]
                .as_array_mut()
                .unwrap();
            logs[0]["data"] = serde_json::to_value(&event.data).unwrap();
            let mut summary = logs[0].clone();
            summary["logIndex"] = "0x306".into();
            summary["topics"][3] =
                format!("0x{}{}", "0".repeat(24), &v2::EXCHANGES_V2[1][2..]).into();
            logs.push(summary);
        }
        fs::write(&args.snapshot, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        let archive = prepare(&args).unwrap();
        assert_eq!(archive.trades.len(), 1);
        assert_eq!(archive.trades[0].price.as_decimal(), Decimal::new(99, 2));
        assert_eq!(archive.trades[0].size.as_decimal(), Decimal::new(390301, 4));
        assert_eq!(
            archive.source_metadata["quality"]["exchange_summaries_excluded"],
            1
        );
        let evidence = Evidence::load(&args.snapshot).unwrap();
        let mut row = vendor_row().into_columns();
        for (key, value) in &mut row {
            match key.as_str() {
                "maker_direction" => *value = Field::Str("SELL".into()),
                "token_amount" => *value = Field::Double(39.0301),
                "usdc_amount" => *value = Field::Double(38.639799000000004),
                _ => {}
            }
        }
        let mut quality = Quality::default();
        let (shares, cash) = evidence.fills["137_100_773"]
            .corroborate(&Row::new(row), &mut quality)
            .unwrap();
        assert_eq!(shares, Decimal::new(390301, 4));
        assert_eq!(cash, Decimal::new(38639799, 6));
        assert_eq!(quality.chain_recovered_amounts, 1);
        assert_ne!(
            endpoint_host("https://[2001:4860:4860::8888]/").unwrap(),
            endpoint_host("https://[2001:4860:4860::8844]/").unwrap()
        );
        assert_eq!(
            endpoint_host("https://example.com.:8443/").unwrap(),
            "example.com"
        );
    }

    #[test]
    fn one_float_ulp_cannot_replace_a_different_base_unit_near_the_integer_limit() {
        let directory = tempfile::tempdir().unwrap();
        let (args, _) = fixture(directory.path());
        let mut evidence = Evidence::load(&args.snapshot).unwrap();
        let raw = (1_u64 << 52) - 1;
        let fill = evidence.fills.get_mut("137_100_773").unwrap();
        fill.shares = U256::from(raw);
        let normalized = raw as f64 / 1_000_000.0;
        for invalid in [normalized.next_up(), normalized.next_down()] {
            let mut row = vendor_row().into_columns();
            row.iter_mut()
                .find(|(key, _)| key == "token_amount")
                .unwrap()
                .1 = Field::Double(invalid);
            let error = fill
                .corroborate(&Row::new(row), &mut Quality::default())
                .unwrap_err();
            assert!(error.to_string().contains("CHAIN_VENDOR_AMOUNT_CONFLICT"));
        }
    }
}
