//! Exercise the actual native binary, protocol input and safe error channel.
#[path = "support/market.rs"]
mod market;
use contracts::science::{NativeForecastResultV1, NativeSimulationResultV1};
use market::{forecast_request, market, module};
#[path = "support/command.rs"]
mod native;
use native::command;

#[test]
fn real_native_cli_forecasts_and_replays_a_single_shared_account() {
    let (directory, request) = market("0", 20);
    let model = directory.path().join("model.wasm");
    std::fs::write(&model, module("local.get 2 local.get 3 f64.sub")).unwrap();
    let result = command(
        &[
            "forecast".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
            "--model".as_ref(),
            model.as_os_str(),
        ],
        &forecast_request(&request),
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let forecast: NativeForecastResultV1 = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(forecast.points.len(), 40);
    let result = command(
        &[
            "simulate".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
        ],
        &request,
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let simulation: NativeSimulationResultV1 = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(simulation.consumed_target_points.get(), 2);
    assert!(simulation.orders.get() >= 4);
}

#[test]
fn native_cli_does_not_echo_invalid_model_contents_or_host_paths() {
    let (directory, request) = market("0", 20);
    let model = directory.path().join("private-sentinel.wasm");
    std::fs::write(&model, b"SENTINEL_NOT_FOR_LOGS").unwrap();
    let result = command(
        &[
            "forecast".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
            "--model".as_ref(),
            model.as_os_str(),
        ],
        &forecast_request(&request),
    );
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert_eq!(result.stderr, b"QZ_NATIVE_JOB_FAILED\n");
}

#[cfg(unix)]
#[test]
fn native_cli_rejects_model_symlinks_and_fifos_without_opening_them() {
    let (directory, request) = market("0", 20);
    let model = directory.path().join("original.wasm");
    std::fs::write(&model, module("f64.const 1")).unwrap();
    let link = directory.path().join("link.wasm");
    std::os::unix::fs::symlink(&model, &link).unwrap();
    let result = command(
        &[
            "forecast".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
            "--model".as_ref(),
            link.as_os_str(),
        ],
        &forecast_request(&request),
    );
    assert!(!result.status.success());
    assert_eq!(result.stderr, b"QZ_NATIVE_JOB_FAILED\n");
    let fifo = directory.path().join("fifo.wasm");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .unwrap();
    let result = command(
        &[
            "forecast".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
            "--model".as_ref(),
            fifo.as_os_str(),
        ],
        &forecast_request(&request),
    );
    assert!(!result.status.success());
    assert_eq!(result.stderr, b"QZ_NATIVE_JOB_FAILED\n");
}
