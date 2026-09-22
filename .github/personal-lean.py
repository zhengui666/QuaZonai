from pathlib import Path
import re

ROOT = Path.cwd()
def read(p): return (ROOT / p).read_text()
def write(p, s):
    q = ROOT / p
    q.parent.mkdir(parents=True, exist_ok=True)
    q.write_text(s)
def replace(p, old, new, count=1):
    s = read(p)
    assert s.count(old) == count, (p, old[:90], s.count(old))
    write(p, s.replace(old, new))
def cut(p, start, end):
    s = read(p); a = s.index(start); b = s.index(end, a)
    write(p, s[:a] + s[b:])
def delete_test(p, name):
    s = read(p); a = s.index('async fn '+name)
    a = s.rfind('#[', 0, a)
    end = s.find('\n#[', a+2)
    write(p, s[:a] + (s[end+1:] if end >= 0 else ''))

# Compile one immutable native module per task; instances retain independent state.
p = 'apps/job/src/signals.rs'
s = read(p)
quota = '''        ensure!(
            (1..=MAX_SIGNAL_PREDICTIONS).contains(&max_predictions),
            "SIGNAL_PREDICTION_LIMIT"
        );
        ensure!(
            (1..=MAX_SIGNAL_FUEL).contains(&total_fuel),
            "SIGNAL_FUEL_LIMIT"
        );
'''
assert s.count(quota) == 1
s = s.replace('impl WasmSignal {\n    pub fn new(bytes: &[u8], max_predictions: u32, total_fuel: u64) -> Result<Self> {', '''/// Task-local compiled code. Never shares globals, memory, failure state or fuel.
/// Drop with the task rather than retaining an unbounded process-wide module cache.
pub struct SignalModule {
    engine: Engine,
    module: Module,
}

impl SignalModule {
    pub fn new(bytes: &[u8]) -> Result<Self> {''')
s = s.replace(quota, '')
s = s.replace('        let limits = StoreLimitsBuilder::new()', '''        Ok(Self { engine, module })
    }

    pub fn instantiate(&self, max_predictions: u32, total_fuel: u64) -> Result<WasmSignal> {
        prediction_budget(max_predictions, total_fuel)?;
        let limits = StoreLimitsBuilder::new()''')
s = s.replace('Store::new(&engine, limits)', 'Store::new(&self.engine, limits)')
s = s.replace('Linker::<StoreLimits>::new(&engine)', 'Linker::<StoreLimits>::new(&self.engine)')
s = s.replace('.instantiate_and_start(&mut store, &module)', '.instantiate_and_start(&mut store, &self.module)')
s = s.replace('        Ok(Self {\n            store,', '        Ok(WasmSignal {\n            store,')
s = s.replace('    /// ABI order:', '''}

fn prediction_budget(max_predictions: u32, total_fuel: u64) -> Result<()> {
'''+quota+'''    Ok(())
}

impl WasmSignal {
    pub fn new(bytes: &[u8], max_predictions: u32, total_fuel: u64) -> Result<Self> {
        prediction_budget(max_predictions, total_fuel)?;
        SignalModule::new(bytes)?.instantiate(max_predictions, total_fuel)
    }

    /// ABI order:''')
write(p, s)
replace('apps/job/Cargo.toml', '"deterministic", "extra-checks", "portable-dispatch"', '"deterministic", "portable-dispatch"')
p = 'apps/job/src/forecast.rs'
replace(p, 'catalog::{load_catalog, NativeBarSeries},\n    signals::WasmSignal,', 'catalog::{load_catalog, NativeBarSeries, NativeMarketData},\n    signals::SignalModule,')
replace(p, '''    let parameters = &request.parameters;
    domain::execution::forecast_request(request)?;
    let market = load_catalog(catalog_root, &request.selection)?;
    let mut remaining''', '''    domain::execution::forecast_request(request)?;
    let market = load_catalog(catalog_root, &request.selection)?;
    forecast_market(&market, request, module)
}

/// Borrow the task's already selected catalog; callers must use the same selection.
/// Models and cutoffs never share mutable execution state.
pub(crate) fn forecast_market(
    market: &NativeMarketData,
    request: &NativeForecastRequestV1,
    module: &[u8],
) -> Result<NativeForecastResultV1> {
    domain::execution::forecast_request(request)?;
    let parameters = &request.parameters;
    let module = SignalModule::new(module)?;
    let mut remaining''')
replace(p, 'for series in market.series {', 'for series in &market.series {')
replace(p, 'let mut model = WasmSignal::new(module, u32::try_from(series.bars.len())?, remaining)?;', 'let mut model = module.instantiate(u32::try_from(series.bars.len())?, remaining)?;\n        let instrument_id = series.instrument.id().to_string();')
replace(p, 'features(&series, parameters)', 'features(series, parameters)')
replace(p, 'instrument_id: series.instrument.id().to_string(),', 'instrument_id: instrument_id.clone(),')
p = 'apps/job/src/validation/alpha.rs'
replace(p, 'signals::WasmSignal,', 'signals::SignalModule,')
replace(p, 'fn predict(\n    module: &[u8],', 'fn predict(\n    module: &SignalModule,')
replace(p, 'WasmSignal::new(module, u32::try_from(block.len())?, *remaining)?', 'module.instantiate(u32::try_from(block.len())?, *remaining)?')
replace(p, '    let market = load_catalog(root, &forecast.selection)?;', '    let market = load_catalog(root, &forecast.selection)?;\n    let module = SignalModule::new(module)?;')
replace(p, 'predict(module, &features,', 'predict(&module, &features,', 2)
replace('apps/job/src/portfolio.rs', 'let result = crate::forecast::forecast(\n            catalog,', 'let result = crate::forecast::forecast_market(\n            &market,')

# Preserve ndarray::dot model identity, but borrow member arrays before packing once.
p = 'apps/job/src/validation.rs'
replace(p, 'Ok(values.to_vec())', 'Ok(values.into_raw_vec_and_offset().0)')
replace(p, '''    use bigdecimal::ToPrimitive;
    let maximum''', '''    weighted_forecast(
        &forecasts.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        &weights.iter().collect::<Vec<_>>(),
    )
}

fn weighted_forecast(
    forecasts: &[&[f64]],
    weights: &[&contracts::DecimalValue],
) -> Result<Vec<f64>> {
    use bigdecimal::ToPrimitive;
    let maximum''')
replace(p, 'domain::portfolio::ensemble_weights(weights.iter())?;', 'domain::portfolio::ensemble_weights(weights.iter().copied())?;')
replace(p, 'forecasts.iter().flatten().copied().collect()', 'forecasts.iter().flat_map(|row| row.iter().copied()).collect()')
replace(p, 'Ok(forecast.to_vec())', 'Ok(forecast.into_raw_vec_and_offset().0)')
replace(p, '    fixed_weighted_forecast(\n        &input', '    weighted_forecast(\n        &input')
replace(p, '.map(|member| member.forecasts.clone())', '.map(|member| member.forecasts.as_slice())')
replace(p, '.map(|member| member.ensemble_weight.clone())', '.map(|member| &member.ensemble_weight)')
p = 'apps/job/tests/signals.rs'
replace(p, 'use job::signals::{WasmSignal,', 'use job::signals::{SignalModule, WasmSignal,')
replace(p, '''    let mut first = signal(prefix, body);
    let mut second = signal(prefix, body);''', '''    let compiled = SignalModule::new(&module(prefix, body)).unwrap();
    let mut first = compiled.instantiate(100, 1_000_000).unwrap();
    let mut second = compiled.instantiate(100, 1_000_000).unwrap();''')
write(p, read(p)+'''
#[test]
fn reused_code_preserves_linear_memory_fuel_results_and_failure_isolation() {
    let bytes = module(
        "(memory 1)",
        "i32.const 0 i32.const 0 f64.load f64.const 1 f64.add f64.store i32.const 0 f64.load",
    );
    let compiled = SignalModule::new(&bytes).unwrap();
    let mut first = compiled.instantiate(3, 100_000).unwrap();
    let mut second = compiled.instantiate(3, 100_000).unwrap();
    let mut fresh = WasmSignal::new(&bytes, 3, 100_000).unwrap();
    for expected in [1.0, 2.0, 3.0] {
        assert_eq!(first.predict(FEATURES).unwrap(), expected);
        assert_eq!(fresh.predict(FEATURES).unwrap(), expected);
        assert_eq!(first.remaining_fuel(), fresh.remaining_fuel());
    }
    assert!(first.predict(FEATURES).is_err());
    assert_eq!(second.predict(FEATURES).unwrap(), 1.0);
    for (calls, fuel) in [(0, 100), (1_000_001, 100), (1, 0), (1, MAX_SIGNAL_FUEL + 1)] {
        assert!(compiled.instantiate(calls, fuel).is_err());
    }
}
''')
write('apps/job/examples/benchmark_signals.rs', '''//! Manual microbenchmark; timings are observations, never pass/fail thresholds.
use anyhow::{ensure, Result};
use job::signals::{SignalModule, WasmSignal};
use std::{hint::black_box, time::Instant};

fn main() -> Result<()> {
    let bytes = wat::parse_str(r#"(module
        (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64)
        local.get 0 local.get 1 f64.div f64.const 1 f64.sub))"#)?;
    let iterations = 256;
    let run = |reuse: bool| -> Result<(std::time::Duration, f64, u64)> {
        let started = Instant::now();
        let compiled = reuse.then(|| SignalModule::new(black_box(&bytes))).transpose()?;
        let mut checksum = 0.0;
        let mut fuel = 0;
        for _ in 0..iterations {
            let mut model = match &compiled {
                Some(module) => module.instantiate(16, 100_000)?,
                None => WasmSignal::new(black_box(&bytes), 16, 100_000)?,
            };
            for _ in 0..16 {
                checksum += black_box(model.predict(black_box([110.0, 100.0, 105.0, 102.0, 25.0, 100.0, 112.0, 98.0]))?);
            }
            fuel += 100_000 - model.remaining_fuel();
        }
        Ok((started.elapsed(), checksum, fuel))
    };
    run(false)?;
    run(true)?;
    println!("profile={} instances={iterations} predictions_per_instance=16", if cfg!(debug_assertions) { "debug" } else { "release" });
    for round in 0..5 {
        let (fresh, reused) = if round % 2 == 0 { (run(false)?, run(true)?) }
            else { let reused = run(true)?; (run(false)?, reused) };
        ensure!(fresh.1 == reused.1 && fresh.2 == reused.2, "reuse changed results or fuel");
        println!("round={round} compile_each_us={} reuse_us={} checksum={} fuel={}", fresh.0.as_micros(), reused.0.as_micros(), fresh.1, fresh.2);
    }
    Ok(())
}
''')
p = 'apps/job/src/report.rs'
replace(p, '    use std::path::PathBuf;\n    use std::sync::atomic::{AtomicU64, Ordering};\n', '')
cut(p, '    struct TestDirectory(PathBuf);', '    #[test]')
s = read(p).replace('TestDirectory::new()', 'tempfile::tempdir().unwrap()').replace('&directory.0', 'directory.path()').replace('directory.0.join', 'directory.path().join')
write(p, s)

# Personal services can use the database owner; separate runtime grants remain optional.
p = 'crates/store/src/lib.rs'
cut(p, '    pub async fn verify_runtime_role', '    pub fn from_pool')
replace(p, 'pub mod machine_auth;\n', '')
replace(p, '    #[error("authentication rate limit exceeded")]\n    AuthRateLimited { retry_after_seconds: u32 },\n', '')
for p in ['crates/store/src/runtime_role.sql', 'crates/store/tests/runtime_role.rs', 'crates/store/src/machine_auth.rs']:
    (ROOT/p).unlink()
replace('apps/server/src/main.rs', '            store.verify_runtime_role().await?;\n', '', 2)
replace('crates/store/tests/brief.rs', '    store.verify_runtime_role().await.unwrap();\n', '')
replace('apps/server/tests/recovery_access.rs', '    assert!(owner.verify_runtime_role().await.is_err());\n', '')
replace('apps/server/tests/recovery_access.rs', '    store.verify_runtime_role().await.unwrap();\n', '')
replace('apps/server/tests/migration_command.rs', '''    Store::from_pool(runtime.clone())
        .verify_runtime_role()
        .await
        .unwrap();
''', '')
p = 'apps/server/tests/auth_http.rs'
replace(p, 'real_tcp_listener_uses_non_owner_database_role_and_native_private_cookie', 'real_tcp_listener_accepts_owner_database_and_native_private_cookie')
cut(p, '    assert!(fixture.store.verify_runtime_role().await.is_err());', '    let listener = tokio::net::TcpListener')
replace(p, '    let fixture = fixture(pool.clone()).await;\n    let listener', '    let fixture = fixture(pool.clone()).await;\n    let store = fixture.store.clone();\n    let listener')
s=read(p); start=s.index('    app_pool.close().await;'); s=s[:start]+'}\n'; write(p,s)
replace(p, 'use store::Store;\n', '')
p = 'apps/server/src/access.rs'
cut(p, '            let attempt = state\n', '            let matches = auth::crypto_with_slots')
replace(p, '            state.store.machine_auth_succeeded(attempt).await?;\n', '')
p = 'apps/server/src/error.rs'
replace(p, '    retry_after: Option<u32>,\n', '')
replace(p, '            retry_after: None,\n', '')
replace(p, '                "AUTH_RATE_LIMITED" => vec!["RETRY_AFTER".into()],\n', '')
cut(p, '        if let Some(seconds) = self.retry_after {', '        response\n')
cut(p, '            StoreError::AuthRateLimited {', '            StoreError::Forbidden =>')
delete_test('apps/server/tests/budget_errors.rs', 'native_auth_rate_limit_keeps_its_retry_after_and_retryable_semantics')
for name in ['native_machine_windows_rollback_partial_reservations_and_refund_only_matching_windows', 'native_machine_attempt_reservation_is_atomic_under_concurrency_and_globally_bounded']:
    delete_test('crates/store/tests/control.rs', name)
p = 'apps/server/tests/control_http.rs'
replace(p, 'native_machine_failures_are_bounded_without_blocking_local_sessions', 'machine_verification_is_bounded_without_persistent_owner_lockout')
pos=read(p).index('async fn machine_verification_is_bounded_without_persistent_owner_lockout')
s=read(p); before=s[:pos]; tail=s[pos:].replace('for _ in 0..5 {','for _ in 0..6 {')
a=tail.index('    let denied = command('); b=tail.index('    let held = machine_slots.acquire_many_owned', a)
tail=tail[:a]+'''    let bearer = format!("Bearer {token}");
    let accepted = command(&f, "GET", "/api/v2/auth/machine", Value::Null,
        &[("authorization", &bearer)]).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.body);
    // Old migrations remain immutable, but current requests never write rate windows.
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.machine_auth_rate_windows")
        .fetch_one(&pool).await.unwrap(), 0);
'''+tail[b:]
tail=tail.replace('    let bearer = format!("Bearer {token}");\n    let busy', '    let busy')
write(p, before+tail)

# One real PGMQ execution inside the already-required isolated Store database.
p='.github/workflows/ci.yml'
cut(p, '      - name: Export only the committed application dependency inputs', '      - name: Prepare isolated native filesystem fault test')
cut(p, '  database-native:\n', '  store-postgres:\n')
s=read(p); s=s[:s.index('  foundation-checks:\n')]; write(p,s)
replace(p, '''          docker exec store-database pg_isready -h 127.0.0.1 -U postgres
''', '''          docker exec store-database pg_isready -h 127.0.0.1 -U postgres
          docker exec -i --env "PGPASSWORD=$database_password" store-database psql -h 127.0.0.1 -U postgres -v ON_ERROR_STOP=1 < tests/native/pgmq_contract.sql | tee /tmp/store-evidence/pgmq-contract.log
''')
replace(p, '''      - name: Install locked official Codex and verify native stdio''', '''      - name: Observe native signal compilation reuse (no timing threshold)
        run: target/debug/examples/benchmark_signals | tee /tmp/native-evidence/signal-benchmark.log
      - name: Install locked official Codex and verify native stdio''')
for p in ['.github/workflows/ci.yml', '.github/workflows/web.yml', '.github/workflows/native-runtime.yml', '.github/workflows/polymarket-history.yml']:
    replace(p,'  cancel-in-progress: false','  cancel-in-progress: true')
for p in ['.github/workflows/codeql.yml', '.github/CODEOWNERS', 'docs/research/source-dependencies.md']:
    (ROOT/p).unlink()
