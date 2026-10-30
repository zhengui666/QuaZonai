//! Temporary example installed into an isolated worktree by prepare.py.
//! Calls existing public business APIs; never installed as a production command.
#[path = "../tests/support/market.rs"]
mod market;

use anyhow::{ensure, Result};
use contracts::{portfolio::RebalanceKind, science::*, Id, SchemaV1};
use nautilus_model::types::Price;
use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
use serde::Serialize;
use std::{fs, io::Write, path::Path};

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    fs::write(path, serde_json::to_vec(value)?)?;
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let dest = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &dest)?;
        } else {
            ensure!(
                entry.file_type()?.is_file(),
                "fixture must contain ordinary files"
            );
            fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

fn benchmark_market(rows: u32) -> Result<(tempfile::TempDir, NativeSimulationRequestV1)> {
    let (original, simulation) = market::micro_lot_market("0", rows);
    let data = job::catalog::load_catalog(original.path(), &simulation.selection)?;
    let directory = tempfile::tempdir()?;
    let catalog = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(16),
        None,
        None,
    )?;
    let price =
        |ticks: u64| Price::from(format!("{}.{:05}", ticks / 100_000, ticks % 100_000).as_str());
    for mut series in data.series {
        ensure!(
            series.instrument_updates.is_empty(),
            "static fixture instruments"
        );
        let (base, step, period) = match series.bar_type.instrument_id().to_string().as_str() {
            "EUR/USD.SIM" => (100_000, 7, 17),
            "GBP/USD.SIM" => (200_000, 11, 19),
            _ => anyhow::bail!("unexpected fixture instrument"),
        };
        catalog.write_instruments(vec![series.instrument])?;
        for bar in &mut series.bars {
            let n = bar.ts_event.as_u64() / market::INTERVAL_NS;
            // Declared synthetic integer-grid cycles, with independent periods.
            // The legacy linear path has nearly zero return covariance and can
            // fail the unchanged strict solver tolerance before replay ends.
            let close = base + 7 * n + 150 * ((n * step) % period);
            bar.open = price(close);
            bar.high = price(close + 20);
            bar.low = price(close - 20);
            bar.close = price(close);
        }
        catalog.write_to_parquet(&series.bars, None, None, None)?;
    }
    let fees = contracts::catalogs::MAKER_TAKER_FEES_FILE;
    fs::copy(original.path().join(fees), directory.path().join(fees))?;
    Ok((directory, simulation))
}

fn fixture(root: &Path, name: &str, rows: u32, start_bar: u64, step_seconds: u32) -> Result<()> {
    let destination = root.join(name);
    fs::create_dir(&destination)?;
    let (catalog, mut build, _) = market::portfolio_from_market(benchmark_market(rows)?);
    build.mandate.capital_assumption = "1000000".parse().unwrap();
    build.execution_settings.starting_capital = build.mandate.capital_assumption.clone();
    build.execution_settings.snapshot_interval_ms = 60_000;
    build.mandate.rebalance_schedule.kind = RebalanceKind::FixedInterval;
    build.mandate.rebalance_schedule.interval_seconds = Some(step_seconds);
    build.mandate.rebalance_schedule.target_ttl_seconds = step_seconds;
    build.mandate.rebalance_schedule.max_input_age_seconds = 120;
    build.mandate.constraints.min_cash_weight = "0.01".parse().unwrap();
    build.mandate.constraints.max_cash_weight = "0.01".parse().unwrap();
    build.mandate.constraints.min_net_exposure = "0.99".parse().unwrap();
    build.mandate.constraints.max_net_exposure = "0.99".parse().unwrap();
    for asset in &mut build.assets {
        asset.current_weight = "0".parse().unwrap();
    }
    build.current_weights.cash_weight = "1".parse().unwrap();
    for weight in &mut build.current_weights.weights {
        weight.weight = "0".parse().unwrap();
    }
    let prototype = build.members[0].clone();
    build.members = (0..16)
        .map(|_| {
            let mut member = prototype.clone();
            member.alpha_id = Id::new();
            member.alpha_version_id = Id::new();
            member.model_artifact_id = Id::new();
            member.ensemble_weight = "0.0625".parse().unwrap();
            member.parameters.total_fuel = Some(market::count(100_000_000));
            member
        })
        .collect();
    let objects = destination.join("objects");
    fs::create_dir(&objects)?;
    for (index, member) in build.members.iter().enumerate() {
        // Sixteen distinct legal Wasm byte strings; exact source hashes are
        // recorded after export. These are synthetic expected-return models.
        let model = market::module(&format!("f64.const 0.{:05}", 1000 + index));
        fs::write(objects.join(member.model_artifact_id.to_string()), model)?;
    }
    write_json(
        &objects.join(build.mandate.constraints.transaction_costs_ref.to_string()),
        &build.execution_settings,
    )?;
    write_json(
        &objects.join(build.current_weights_artifact_id.to_string()),
        &build.current_weights,
    )?;
    let study = NativePortfolioStudyRequestV1 {
        settlements: Vec::new(),
        schema_version: SchemaV1,
        source_selection: build.selection.clone(),
        evaluation_start_ns: market::instant(start_bar),
        manual_cutoffs_ns: None,
        calendar: None,
        research_available_through_ns: market::instant(9),
        mandate: build.mandate.clone(),
        execution_settings: build.execution_settings.clone(),
        rolling_liquidity: None,
        assets: build.assets.clone(),
        members: build.members.clone(),
    };
    domain::execution::portfolio_build_request(&build)?;
    let cutoffs = domain::execution::portfolio_study_cutoffs(&study)?;
    copy_tree(catalog.path(), &destination.join("catalog"))?;
    write_json(&destination.join("build-request.json"), &build)?;
    write_json(&destination.join("study-request.json"), &study)?;
    write_json(
        &destination.join("case.json"),
        &serde_json::json!({
            "origin": "FIXTURE",
            "price_ticks": "base + 7*n + 150*((n*step)%period); EUR=(100000,7,17), GBP=(200000,11,19); 100000 ticks/unit",
            "rows_per_asset": rows,
            "assets": 2,
            "models": build.members.len(),
            "cutoffs_ns": cutoffs,
            "first_evaluation_bar": start_bar,
            "step_seconds": step_seconds,
            "explicit_fuel_per_model": "100000000",
            "build_cache_reuse_expected": 0,
            "study_compilations_baseline_expected": cutoffs.len() * build.members.len(),
            "study_compilations_cached_expected": build.members.len(),
            "count_kind": "structural expectations; no production compilation counter added"
        }),
    )?;
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.first().and_then(|value| value.to_str()) {
        Some("fixtures") => {
            ensure!(args.len() == 2, "fixtures DESTINATION");
            let root = Path::new(&args[1]);
            fs::create_dir(root)?;
            fixture(root, "short", 64, 17, 180)?;
            fixture(root, "long", 4096, 1000, 60_000)?;
        }
        Some("build") => {
            ensure!(args.len() == 3, "build CATALOG OBJECTS < request.json");
            let request: NativePortfolioBuildRequestV1 =
                serde_json::from_reader(std::io::stdin().lock())?;
            let result = job::portfolio::build(Path::new(&args[1]), &request, |id| {
                Ok(fs::read(Path::new(&args[2]).join(id.to_string()))?)
            })?;
            let mut out = std::io::stdout().lock();
            serde_json::to_writer(&mut out, &result)?;
            writeln!(out)?;
            out.flush()?;
        }
        _ => anyhow::bail!("expected fixtures or build"),
    }
    Ok(())
}
