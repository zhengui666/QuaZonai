//! Chain-derived fill observations, not reconstructed public market trades.
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, NaiveDateTime};
use nautilus_core::UnixNanos;
use nautilus_hyperliquid::{
    common::enums::HyperliquidFillDirection, websocket::messages::WsFillData,
};
use nautilus_serialization::{arrow_custom_data, ensure_custom_data_registered};
use serde_json::Value;

/// Original source coordinates. The row number is physical and zero-based.
pub struct SourceLocation<'a> {
    pub source: &'a str,
    pub revision: &'a str,
    pub file: &'a str,
    pub row_zero_based: u64,
}

/// Source-local naive timestamps have no implicit host timezone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NaiveTimePolicy {
    Reject,
    AssumeUtc,
}

#[arrow_custom_data]
#[nautilus_model::custom_data]
pub struct ChainticksFillObservation {
    pub source: String,
    pub source_revision: String,
    pub source_file: String,
    pub source_row_zero_based: u64,
    pub block_number: u64,
    pub coin: Option<String>,
    pub native_tid: Option<u64>,
    pub native_oid: Option<u64>,
    pub raw_price: Option<String>,
    pub raw_size: Option<String>,
    pub raw_side: Option<String>,
    pub raw_direction: Option<String>,
    pub crossed: Option<bool>,
    pub classification: String,
    pub official_parse_error: Option<String>,
    pub raw_block_time: Option<String>,
    pub raw_local_time: String,
    pub local_time_assumed_utc: bool,
    pub archive_recorded_at: Option<String>,
    pub raw_envelope: String,
    /// Original event.time in milliseconds, represented as nanoseconds.
    pub ts_event: UnixNanos,
    /// Original block.local_time, not archive ingestion or local download time.
    pub ts_init: UnixNanos,
}

pub fn register_observation_type() {
    ensure_custom_data_registered::<ChainticksFillObservation>();
}

fn source_time(raw: &str, policy: NaiveTimePolicy) -> Result<(UnixNanos, bool)> {
    let (nanos, assumed) = match DateTime::parse_from_rfc3339(raw) {
        Ok(at) => (at.timestamp_nanos_opt(), false),
        Err(_) => {
            ensure!(
                policy == NaiveTimePolicy::AssumeUtc,
                "NAIVE_SOURCE_TIME_REQUIRES_EXPLICIT_UTC_ASSUMPTION"
            );
            let at = NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f")
                .context("INVALID_SOURCE_LOCAL_TIME")?;
            (at.and_utc().timestamp_nanos_opt(), true)
        }
    };
    Ok((
        u64::try_from(nanos.context("SOURCE_LOCAL_TIME_RANGE")?)?.into(),
        assumed,
    ))
}

fn classification(fill: &WsFillData) -> &'static str {
    match fill.dir {
        HyperliquidFillDirection::OpenLong
        | HyperliquidFillDirection::OpenShort
        | HyperliquidFillDirection::CloseLong
        | HyperliquidFillDirection::CloseShort
        | HyperliquidFillDirection::LongToShort
        | HyperliquidFillDirection::ShortToLong
        | HyperliquidFillDirection::Buy
        | HyperliquidFillDirection::Sell => {
            if fill.crossed {
                "taker_fill_observation"
            } else {
                "maker_fill_observation"
            }
        }
        HyperliquidFillDirection::Unknown => "unknown_fill_observation",
        _ => "special_event_observation",
    }
}

/// Parses one source envelope without deduplicating, synthesizing users, or applying
/// historical instrument precision. Unsupported fill shapes retain their raw data
/// when both actual source clocks are available. Missing clocks are an explicit error.
pub fn parse_observation(
    raw_envelope: &str,
    source: &SourceLocation<'_>,
    archive_recorded_at: Option<&str>,
    naive_time_policy: NaiveTimePolicy,
) -> Result<ChainticksFillObservation> {
    parse_inner(raw_envelope, source, archive_recorded_at, naive_time_policy).with_context(|| {
        format!(
            "CHAINTICKS_SOURCE_ROW source={} revision={} file={} row_zero_based={}",
            source.source, source.revision, source.file, source.row_zero_based
        )
    })
}

fn parse_inner(
    raw_envelope: &str,
    source: &SourceLocation<'_>,
    archive_recorded_at: Option<&str>,
    naive_time_policy: NaiveTimePolicy,
) -> Result<ChainticksFillObservation> {
    let envelope: Value =
        serde_json::from_str(raw_envelope).context("INVALID_SOURCE_ENVELOPE_JSON")?;
    let event = envelope
        .get("event")
        .filter(|v| v.is_object())
        .context("SOURCE_EVENT_OBJECT_MISSING")?;
    let block = envelope
        .get("block")
        .filter(|v| v.is_object())
        .context("SOURCE_BLOCK_OBJECT_MISSING")?;
    let block_number = block
        .get("block_number")
        .and_then(Value::as_u64)
        .context("SOURCE_BLOCK_NUMBER_MISSING_OR_INVALID")?;
    let event_ms = event
        .get("time")
        .and_then(Value::as_u64)
        .context("SOURCE_EVENT_TIME_MISSING_OR_INVALID")?;
    let ts_event = event_ms
        .checked_mul(1_000_000)
        .context("SOURCE_EVENT_TIME_RANGE")?
        .into();
    let raw_local_time = block
        .get("local_time")
        .and_then(Value::as_str)
        .context("SOURCE_LOCAL_TIME_MISSING_OR_INVALID")?;
    let (ts_init, local_time_assumed_utc) = source_time(raw_local_time, naive_time_policy)?;
    let (kind, official_parse_error) = match serde_json::from_value::<WsFillData>(event.clone()) {
        Ok(fill) => (classification(&fill), None),
        Err(error) => ("unsupported_fill_observation", Some(error.to_string())),
    };
    let string = |key| event.get(key).and_then(Value::as_str).map(str::to_owned);
    Ok(ChainticksFillObservation {
        source: source.source.to_owned(),
        source_revision: source.revision.to_owned(),
        source_file: source.file.to_owned(),
        source_row_zero_based: source.row_zero_based,
        block_number,
        coin: string("coin"),
        native_tid: event.get("tid").and_then(Value::as_u64),
        native_oid: event.get("oid").and_then(Value::as_u64),
        raw_price: string("px"),
        raw_size: string("sz"),
        raw_side: string("side"),
        raw_direction: string("dir"),
        crossed: event.get("crossed").and_then(Value::as_bool),
        classification: kind.to_owned(),
        official_parse_error,
        raw_block_time: block
            .get("block_time")
            .and_then(Value::as_str)
            .map(str::to_owned),
        raw_local_time: raw_local_time.to_owned(),
        local_time_assumed_utc,
        archive_recorded_at: archive_recorded_at.map(str::to_owned),
        raw_envelope: raw_envelope.to_owned(),
        ts_event,
        ts_init,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location() -> SourceLocation<'static> {
        SourceLocation {
            source: "Chainticks/perp-data",
            revision: "selected-source-revision",
            file: "training.parquet",
            row_zero_based: 7,
        }
    }

    fn envelope() -> Value {
        serde_json::json!({"block":{"block_number":9,"block_time":"2026-05-09T00:00:00.000009942","local_time":"2026-05-09T00:00:00.607467014"},"event":{"coin":"BTC","px":"80153.0","sz":"0.00100","side":"B","time":1778284800000_u64,"startPosition":"0","dir":"Open Long","closedPnl":"0","hash":"0x0","oid":11,"crossed":true,"fee":"0.01","tid":12,"feeToken":"USDC","cloid":null,"twapId":null}})
    }

    #[test]
    fn preserves_original_identity_numbers_and_both_clocks() {
        let raw = envelope().to_string();
        let parsed = parse_observation(
            &raw,
            &location(),
            Some("2026-05-10T00:00:00Z"),
            NaiveTimePolicy::AssumeUtc,
        )
        .unwrap();
        assert_eq!(parsed.raw_envelope, raw);
        assert_eq!(parsed.raw_size.as_deref(), Some("0.00100"));
        assert_eq!(parsed.native_tid, Some(12));
        assert_eq!(parsed.source_row_zero_based, 7);
        assert_eq!(parsed.ts_event.as_u64(), 1778284800000000000);
        assert_eq!(parsed.ts_init.as_u64(), 1778284800607467014);
        assert!(parsed.local_time_assumed_utc);
        assert_eq!(parsed.classification, "taker_fill_observation");
    }

    #[test]
    fn naive_time_requires_opt_in_and_missing_time_names_source_row() {
        let raw = envelope().to_string();
        assert!(
            format!(
                "{:#}",
                parse_observation(&raw, &location(), None, NaiveTimePolicy::Reject).unwrap_err()
            )
            .contains("EXPLICIT_UTC")
        );
        let mut value = envelope();
        value["event"].as_object_mut().unwrap().remove("time");
        let error = format!(
            "{:#}",
            parse_observation(
                &value.to_string(),
                &location(),
                None,
                NaiveTimePolicy::AssumeUtc
            )
            .unwrap_err()
        );
        assert!(error.contains("file=training.parquet row_zero_based=7"));
        assert!(error.contains("SOURCE_EVENT_TIME"));
    }

    #[test]
    fn maker_special_unknown_and_invalid_fills_are_not_discarded() {
        for (direction, crossed, expected) in [
            ("Open Long", false, "maker_fill_observation"),
            ("Spot Dust Conversion", true, "special_event_observation"),
            ("New Future Event", true, "unknown_fill_observation"),
        ] {
            let mut value = envelope();
            value["event"]["dir"] = direction.into();
            value["event"]["crossed"] = crossed.into();
            value["block"]["local_time"] = "2026-05-09T00:00:00.607467014Z".into();
            let parsed = parse_observation(
                &value.to_string(),
                &location(),
                None,
                NaiveTimePolicy::Reject,
            )
            .unwrap();
            assert_eq!(parsed.classification, expected);
            assert!(!parsed.local_time_assumed_utc);
            assert_eq!(
                parsed.official_parse_error.is_some(),
                expected == "unsupported_fill_observation"
            );
        }
        let mut value = envelope();
        value["event"]["px"] = "not-a-number".into();
        let parsed = parse_observation(
            &value.to_string(),
            &location(),
            None,
            NaiveTimePolicy::AssumeUtc,
        )
        .unwrap();
        assert_eq!(parsed.classification, "unsupported_fill_observation");
        assert_eq!(parsed.raw_price.as_deref(), Some("not-a-number"));
    }

    #[test]
    fn official_arrow_round_trip_preserves_source_observation() {
        use nautilus_model::data::Data;
        use nautilus_serialization::arrow::{DecodeDataFromRecordBatch, EncodeToRecordBatch};
        register_observation_type();
        let row = parse_observation(
            &envelope().to_string(),
            &location(),
            None,
            NaiveTimePolicy::AssumeUtc,
        )
        .unwrap();
        let metadata = row.metadata();
        let batch =
            ChainticksFillObservation::encode_batch(&metadata, std::slice::from_ref(&row)).unwrap();
        let decoded = ChainticksFillObservation::decode_data_batch(&metadata, batch).unwrap();
        assert_eq!(decoded.len(), 1);
        let Data::Custom(custom) = &decoded[0] else {
            panic!("expected custom source observation")
        };
        assert_eq!(
            custom
                .data
                .as_any()
                .downcast_ref::<ChainticksFillObservation>(),
            Some(&row)
        );
    }

    #[test]
    fn configured_factory_replays_observations_via_official_actor_without_orders() {
        use crate::native_configured::{ConfiguredNativeRun, run};
        use contracts::{SchemaV1, native_strategy::NativeConfiguredBacktestRequestV1};
        use nautilus_backtest::config::{BacktestRunConfig, BacktestVenueConfig};
        use nautilus_common::{
            actor::{DataActor, DataActorConfig, DataActorCore},
            nautilus_actor,
        };
        use nautilus_execution::models::fee::{FeeModelAny, MakerTakerFeeModel};
        use nautilus_model::enums::{AccountType, BookType, OmsType};
        use nautilus_model::{data::CustomData, identifiers::ActorId};
        use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
        use std::{
            path::Path,
            sync::{Arc, Mutex},
        };

        static RECEIVED: Mutex<Vec<ChainticksFillObservation>> = Mutex::new(Vec::new());
        #[derive(Debug)]
        struct Observer {
            core: DataActorCore,
            data_type: nautilus_model::data::DataType,
        }
        nautilus_actor!(Observer);
        impl DataActor for Observer {
            fn on_start(&mut self) -> Result<()> {
                self.subscribe_data(self.data_type.clone(), None, None);
                Ok(())
            }
            fn on_data(&mut self, value: &CustomData) -> Result<()> {
                let row = value
                    .data
                    .as_any()
                    .downcast_ref::<ChainticksFillObservation>()
                    .context("EXPECTED_SOURCE_OBSERVATION")?;
                assert_eq!(self.clock().timestamp_ns(), row.ts_init);
                RECEIVED.lock().unwrap().push(row.clone());
                Ok(())
            }
        }
        fn factory(root: &Path, name: &str, _: &Value) -> Result<ConfiguredNativeRun> {
            ensure!(name == "source-observer", "UNKNOWN_OBSERVER");
            register_observation_type();
            let mut catalog = ParquetDataCatalog::new(root, None, None, None, None);
            let data = catalog.query_custom_data_dynamic(
                "ChainticksFillObservation",
                None,
                None,
                None,
                None,
                None,
                true,
            )?;
            let nautilus_model::data::Data::Custom(first) = &data[0] else {
                anyhow::bail!("EXPECTED_CUSTOM")
            };
            let data_type = first.data_type.clone();
            // The pinned upstream BacktestDataConfig rejects Custom. Its public install
            // hook and oneshot engine accept preloaded CustomData; no replacement replay.
            let venue = BacktestVenueConfig::builder()
                .name("SIM")
                .oms_type(OmsType::Netting)
                .account_type(AccountType::Margin)
                .book_type(BookType::L1_MBP)
                .starting_balances(vec!["1000 USD".into()])
                .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()))
                .build()?;
            let config = BacktestRunConfig::builder()
                .venues(vec![venue])
                .data(Vec::new())
                .raise_exception(true)
                .dispose_on_completion(false)
                .build()?;
            Ok(ConfiguredNativeRun {
                config,
                install: Box::new(move |engine| {
                    engine.add_data(data, None, true, true)?;
                    engine.add_actor(Observer {
                        core: DataActorCore::new(DataActorConfig {
                            actor_id: Some(ActorId::from("SOURCE-OBSERVER")),
                            ..Default::default()
                        }),
                        data_type,
                    })?;
                    Ok(())
                }),
            })
        }
        let root = tempfile::tempdir().unwrap();
        register_observation_type();
        let first = parse_observation(
            &envelope().to_string(),
            &location(),
            None,
            NaiveTimePolicy::AssumeUtc,
        )
        .unwrap();
        let mut second = first.clone();
        second.source_row_zero_based += 1;
        second.ts_init = (first.ts_init.as_u64() + 1_000_000_000).into();
        // Keep the original event timestamp unchanged: availability, not event order,
        // determines replay. This unit test makes no source completeness claim.
        let mut catalog = ParquetDataCatalog::new(root.path(), None, None, None, None);
        catalog
            .write_custom_data_batch(
                &[
                    CustomData::from_arc(Arc::new(first.clone())),
                    CustomData::from_arc(Arc::new(second.clone())),
                ],
                None,
                None,
                Some(false),
            )
            .unwrap();
        RECEIVED.lock().unwrap().clear();
        let result = run(
            root.path(),
            NativeConfiguredBacktestRequestV1 {
                schema_version: SchemaV1,
                name: "source-observer".into(),
                config: Value::Null,
            },
            Some(factory),
        )
        .unwrap();
        assert_eq!(*RECEIVED.lock().unwrap(), vec![first, second]);
        assert_eq!(result.native_results.as_array().unwrap().len(), 1);
        assert_eq!(result.native_results[0]["total_orders"], 0);
    }
}
