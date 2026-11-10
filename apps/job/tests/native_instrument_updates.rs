//! Native definition-transition regressions preserve original catalog history.
#[path = "support/market.rs"]
mod market;
#[path = "support/command.rs"]
mod native;

use nautilus_model::{
    instruments::{Instrument, InstrumentAny},
    types::Price,
};

#[test]
fn quazonai_rejects_definition_transitions_but_preserves_catalog_history() {
    use contracts::catalogs::{FrozenMakerTakerFeesV1, MAKER_TAKER_FEES_FILE};
    use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;

    // Include a transition at selection-start and one after event-end: late
    // market data and pending orders still depend on the receive-time cutoff.
    for (minute, start) in [(1, 1), (5, 0), (22, 0)] {
        let received = minute * market::INTERVAL_NS;
        let (root, mut request) = market::market("0", 20);
        // Declare this synthetic source's complete receive-time window before
        // replay. Instrument definition changes do not supply or change fees.
        let fee_path = root.path().join(MAKER_TAKER_FEES_FILE);
        let mut fee_source: FrozenMakerTakerFeesV1 =
            serde_json::from_slice(&std::fs::read(&fee_path).unwrap()).unwrap();
        assert_eq!(fee_source.origin, contracts::research::DataOrigin::Fixture);
        fee_source.source_version = "definition-transition-window-v1".into();
        fee_source.valid_until_ns = market::count(24 * market::INTERVAL_NS);
        let frozen_fee_bytes = serde_json::to_vec(&fee_source).unwrap();
        std::fs::write(&fee_path, &frozen_fee_bytes).unwrap();
        request.selection.event_start_ns = market::count(start * market::INTERVAL_NS);
        request.selection.decision_cutoff_ns = market::instant(23);
        domain::catalogs::maker_taker_fee_selection(&fee_source, &request.selection).unwrap();
        let catalog =
            ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, Some(16), None, None)
                .unwrap();
        let original = catalog.instruments(None, None, None).unwrap().remove(0);
        let mut definition = serde_json::to_value(&original).unwrap();
        definition["CurrencyPair"]["price_increment"] = "0.00010".into();
        definition["CurrencyPair"]["ts_event"] = (received - 1).into();
        definition["CurrencyPair"]["ts_init"] = received.into();
        let update: InstrumentAny = serde_json::from_value(definition).unwrap();
        catalog.write_instruments(vec![update.clone()]).unwrap();
        let data = job::catalog::load_catalog(root.path(), &request.selection).unwrap();
        assert_eq!(data.execution_fees.as_ref(), Some(&fee_source));
        let series = data
            .series
            .iter()
            .find(|series| series.instrument.id() == original.id())
            .unwrap();
        assert_eq!(series.instrument_updates.len(), 1);
        assert_eq!(
            serde_json::to_value(&series.instrument_updates[0]).unwrap(),
            serde_json::to_value(&update).unwrap()
        );
        assert_eq!(
            series
                .instrument_at(received - 1)
                .unwrap()
                .price_increment(),
            original.price_increment()
        );
        assert_eq!(
            series.instrument_at(received).unwrap().price_increment(),
            update.price_increment()
        );
        assert_eq!(
            job::simulation::simulate(root.path(), &request)
                .unwrap_err()
                .to_string(),
            "SIMULATION_INSTRUMENT_UPDATES_UNSUPPORTED"
        );
        let output = native::command(
            &[
                "simulate".as_ref(),
                "--catalog".as_ref(),
                root.path().as_os_str(),
            ],
            &request,
        );
        assert!(!output.status.success());
        assert!(
            output.stdout.is_empty(),
            "unsupported replay must publish no result"
        );
        assert_eq!(
            String::from_utf8(output.stderr).unwrap().trim(),
            "QZ_SIMULATION_INSTRUMENT_UPDATES_UNSUPPORTED\nSIMULATION_INSTRUMENT_UPDATES_UNSUPPORTED"
        );
        assert_eq!(std::fs::read(&fee_path).unwrap(), frozen_fee_bytes);
    }
}

#[test]
fn stable_window_after_original_transition_uses_the_original_new_definition() {
    use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;

    let (root, mut request) = market::market("0", 20);
    let catalog =
        ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, Some(16), None, None)
            .unwrap();
    let mut versions = Vec::new();
    for original in catalog.instruments(None, None, None).unwrap() {
        let mut value = serde_json::to_value(original).unwrap();
        value["CurrencyPair"]["price_increment"] = "0.00010".into();
        value["CurrencyPair"]["ts_event"] = (2 * market::INTERVAL_NS - 1).into();
        value["CurrencyPair"]["ts_init"] = (2 * market::INTERVAL_NS).into();
        versions.push(serde_json::from_value::<InstrumentAny>(value).unwrap());
    }
    catalog.write_instruments(versions).unwrap();
    request.selection.event_start_ns = market::count(3 * market::INTERVAL_NS);
    request.target_points[0].asof_ns = market::instant(4);
    let data = job::catalog::load_catalog(root.path(), &request.selection).unwrap();
    assert!(data
        .series
        .iter()
        .all(|series| series.instrument_updates.is_empty()
            && series.instrument.ts_init().as_u64() == 2 * market::INTERVAL_NS
            && series.instrument.price_increment() == Price::from("0.00010")));
    let output = native::command(
        &[
            "simulate".as_ref(),
            "--catalog".as_ref(),
            root.path().as_os_str(),
        ],
        &request,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: contracts::science::NativeSimulationResultV1 =
        serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result.consumed_target_points.get(),
        request.target_points.len() as u64
    );
    assert_eq!(result.native_version, "0.68.0");
}
