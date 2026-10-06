//! One-process native science entrypoints. No database or delivery authority.
use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(
    name = "job",
    version,
    about = "Bounded native research computations; no delivery authority"
)]
struct Arguments {
    #[command(subcommand)]
    command: Operation,
}

#[derive(Subcommand)]
enum Operation {
    /// Observe bounded public Polymarket native events; no execution client or orders.
    #[cfg(feature = "native-node-observer")]
    PolymarketDataProbe(job::polymarket_data_probe::Arguments),
    /// Foreground native Sandbox execution with explicitly configured public live data.
    #[cfg(feature = "native-paper")]
    Paper(job::paper_node::Arguments),
    /// Real current public data to native Paper simulation; no financial orders.
    #[cfg(feature = "native-paper")]
    PolymarketPaper(job::polymarket_paper_host::Arguments),
    /// Apply the immutable native wall deadline before executing the fixed job entrypoint.
    RunBounded,
    /// Project official native snapshots into downstream account observation envelopes.
    NativeAccountObservation(job::account_observation_cli::Args),
    /// Execute one typed native operation. Root overrides are trusted local CLI only.
    Execute {
        #[arg(long, default_value = "/input")]
        input_root: PathBuf,
        #[arg(long, default_value = "/output")]
        output_root: PathBuf,
    },
    /// Solve one frozen allocation request read from stdin.
    Allocate,
    /// Predict using one immutable native catalog and a bounded Wasm artifact.
    Forecast {
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        model: PathBuf,
    },
    /// Execute every independent native fold; stdout remains restricted evidence.
    ValidateAlpha {
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        model: PathBuf,
    },
    /// Independently replay a frozen feature-aware target policy in native folds.
    EvaluateExperiment {
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        model: PathBuf,
        #[arg(long, value_parser = parse_id)]
        dataset_revision_id: contracts::Id,
        #[arg(long, value_parser = parse_id)]
        model_artifact_id: contracts::Id,
        #[arg(long, required = true)]
        features: Vec<PathBuf>,
        #[arg(long, required = true, value_parser = parse_id)]
        feature_artifact_id: Vec<contracts::Id>,
    },
    /// Apply a frozen model without fitting; stdout is restricted held-out evidence.
    EvaluateSealedAlpha {
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        model: PathBuf,
        #[arg(long)]
        calibration: Option<PathBuf>,
    },
    /// Replay frozen target weights in one native simulated account.
    Simulate {
        #[arg(long)]
        catalog: PathBuf,
    },
    /// Recompute rolling portfolio targets inside one native simulated account.
    StudyPortfolio {
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        objects: PathBuf,
    },
}

fn parse_id(value: &str) -> std::result::Result<contracts::Id, &'static str> {
    contracts::Id::try_from(value.to_owned()).map_err(|_| "expected a canonical UUIDv7")
}

fn input<T: DeserializeOwned>() -> Result<T> {
    const MAX_INPUT_BYTES: u64 = 8 * 1024 * 1024;
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        anyhow::bail!("native job input limit");
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn output<T: Serialize>(value: &T) -> Result<()> {
    let mut stream = std::io::stdout().lock();
    serde_json::to_writer(&mut stream, value)?;
    writeln!(stream)?;
    stream.flush()?;
    Ok(())
}

fn model_bytes(path: &Path, maximum_bytes: usize) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > maximum_bytes as u64 {
        anyhow::bail!("native model file limit");
    }
    // Only the runtime's registered read-only model mount is passed here. These
    // local CLI arguments are not exposed as arbitrary HTTP/MCP filesystem reads.
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        anyhow::bail!("native model is not a file");
    }
    let mut bytes = Vec::new();
    file.take(maximum_bytes as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum_bytes {
        anyhow::bail!("native model file limit");
    }
    Ok(bytes)
}

fn run(operation: Operation) -> Result<()> {
    match operation {
        #[cfg(feature = "native-node-observer")]
        Operation::PolymarketDataProbe(args) => job::polymarket_data_probe::run(args),
        #[cfg(feature = "native-paper")]
        Operation::PolymarketPaper(args) => job::polymarket_paper_host::run(args),
        #[cfg(feature = "native-paper")]
        Operation::Paper(args) => {
            job::paper_node::run(args)?;
            Ok(())
        }
        Operation::NativeAccountObservation(args) => {
            job::account_observation_cli::run(args)?;
            Ok(())
        }
        Operation::RunBounded => {
            job::bounded::run()?;
            Ok(())
        }
        Operation::Execute {
            input_root,
            output_root,
        } => {
            job::managed::execute(&input_root, &output_root)?;
            Ok(())
        }
        Operation::Allocate => output(&job::allocate(&input()?)?),
        Operation::Forecast { catalog, model } => output(&job::forecast::forecast(
            &catalog,
            &input()?,
            &model_bytes(&model, job::signals::MAX_SIGNAL_MODULE_BYTES)?,
        )?),
        Operation::ValidateAlpha { catalog, model } => output(&job::validation::validate_alpha(
            &catalog,
            &input()?,
            &model_bytes(&model, job::signals::MAX_SIGNAL_MODULE_BYTES)?,
        )?),
        Operation::EvaluateExperiment {
            catalog,
            model,
            dataset_revision_id,
            model_artifact_id,
            features,
            feature_artifact_id,
        } => {
            let request = input()?;
            let parts = features
                .into_iter()
                .map(|path| -> Result<_> {
                    Ok(serde_json::from_slice(&model_bytes(
                        &path,
                        2 * 1024 * 1024,
                    )?)?)
                })
                .collect::<Result<Vec<_>>>()?;
            output(&job::experiment::evaluate(
                &catalog,
                &request,
                dataset_revision_id,
                model_artifact_id,
                &feature_artifact_id,
                &parts,
                &model_bytes(&model, job::signals::MAX_SIGNAL_MODULE_BYTES)?,
            )?)
        }
        Operation::EvaluateSealedAlpha {
            catalog,
            model,
            calibration,
        } => {
            let calibration = calibration
                .map(
                    |p| -> Result<contracts::science::NativeFrozenCalibrationV1> {
                        Ok(serde_json::from_slice(&model_bytes(&p, 8 * 1024 * 1024)?)?)
                    },
                )
                .transpose()?;
            output(&job::validation::evaluate_sealed_alpha(
                &catalog,
                &input()?,
                &model_bytes(&model, job::signals::MAX_SIGNAL_MODULE_BYTES)?,
                calibration.as_ref(),
            )?)
        }
        Operation::Simulate { catalog } => output(&job::simulation::simulate(&catalog, &input()?)?),
        Operation::StudyPortfolio { catalog, objects } => {
            output(&job::study::evaluate(&catalog, &input()?, |id| {
                model_bytes(&objects.join(id.to_string()), 8 * 1024 * 1024)
            })?)
        }
    }
}

fn public_error_code(error: &anyhow::Error) -> &'static str {
    if error.is::<job::managed::CompilerMemoryLimit>() {
        return "QZ_NATIVE_JOB_MEMORY_LIMIT";
    }
    match error.to_string().as_str() {
        "SIMULATION_INSTRUMENT_UPDATES_UNSUPPORTED" => {
            "QZ_SIMULATION_INSTRUMENT_UPDATES_UNSUPPORTED"
        }
        "SIMULATION_MARKET_UNSUPPORTED" => "QZ_SIMULATION_MARKET_UNSUPPORTED",
        _ => "QZ_NATIVE_JOB_FAILED",
    }
}

fn public_failure(error: &anyhow::Error) -> String {
    const MAX_DETAIL_BYTES: usize = 4096;
    let mut detail = format!("{error:#}");
    // Fields deliberately has a generic Display for other transports. Preserve
    // its actual field/code diagnostics only at this local native CLI boundary.
    for cause in error.chain() {
        if let Some(domain::DomainError::Fields(issues)) = cause.downcast_ref() {
            for issue in issues {
                if detail.len() > MAX_DETAIL_BYTES {
                    break;
                }
                detail.push_str(&format!("; {} [{}]", issue.field, issue.code));
            }
        }
        if detail.len() > MAX_DETAIL_BYTES {
            break;
        }
    }
    if detail.len() > MAX_DETAIL_BYTES {
        detail.truncate(detail.floor_char_boundary(MAX_DETAIL_BYTES));
        detail.push_str(" [truncated]");
    }
    format!("{}\n{detail}", public_error_code(error))
}

fn failure_exit_code(error: &anyhow::Error) -> i32 {
    if error.is::<job::managed::CompilerMemoryLimit>() {
        domain::runtime_jobs::NATIVE_MEMORY_LIMIT_EXIT_CODE
    } else {
        1
    }
}

fn main() {
    let args = Arguments::parse();
    if let Err(error) = run(args.command) {
        // Keep stdout reserved for typed evidence. Local stderr retains bounded
        // causes without reading or dumping model, catalog or request contents.
        eprintln!("{}", public_failure(&error));
        std::process::exit(failure_exit_code(&error));
    }
}

#[cfg(test)]
mod tests {
    use super::{public_error_code, public_failure, Arguments, Operation};
    use clap::Parser;

    #[test]
    fn only_typed_compiler_memory_evidence_selects_the_resource_exit() {
        for error in [
            anyhow::Error::new(job::managed::CompilerMemoryLimit),
            anyhow::Error::new(job::managed::CompilerMemoryLimit).context("native compiler"),
        ] {
            assert_eq!(public_error_code(&error), "QZ_NATIVE_JOB_MEMORY_LIMIT");
            assert_eq!(
                super::failure_exit_code(&error),
                domain::runtime_jobs::NATIVE_MEMORY_LIMIT_EXIT_CODE
            );
        }
        for message in ["NATIVE_COMPILER_MEMORY_LIMIT", "NATIVE_COMPILATION_FAILED"] {
            let error = anyhow::anyhow!(message);
            assert_eq!(public_error_code(&error), "QZ_NATIVE_JOB_FAILED");
            assert_eq!(super::failure_exit_code(&error), 1);
        }
    }

    #[test]
    fn market_capability_error_has_a_stable_public_code() {
        let unsupported = anyhow::anyhow!("SIMULATION_MARKET_UNSUPPORTED");
        assert_eq!(
            public_error_code(&unsupported),
            "QZ_SIMULATION_MARKET_UNSUPPORTED"
        );
        let arbitrary = anyhow::anyhow!("private path /input/model.wasm");
        assert_eq!(public_error_code(&arbitrary), "QZ_NATIVE_JOB_FAILED");
        assert_eq!(
            public_failure(&unsupported),
            "QZ_SIMULATION_MARKET_UNSUPPORTED\nSIMULATION_MARKET_UNSUPPORTED"
        );
    }

    #[test]
    fn native_failure_preserves_source_fields_and_bounded_utf8_details() {
        #[derive(Debug)]
        struct Wrapped(domain::DomainError);
        impl std::fmt::Display for Wrapped {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("native request")
            }
        }
        impl std::error::Error for Wrapped {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }
        let fields = domain::DomainError::Fields(vec![contracts::research::FieldIssue {
            field: "split_policy.train_size".into(),
            code: "OUT_OF_RANGE".into(),
            message: "request contents are not echoed".into(),
        }]);
        assert_eq!(
            public_failure(&anyhow::Error::new(fields.clone())),
            "QZ_NATIVE_JOB_FAILED\nresearch contract fields are invalid; split_policy.train_size [OUT_OF_RANGE]"
        );
        for error in [
            anyhow::Error::new(fields.clone()).context("evaluate experiment"),
            anyhow::Error::new(Wrapped(fields)).context("evaluate experiment"),
        ] {
            let diagnostic = public_failure(&error);
            assert!(diagnostic.starts_with("QZ_NATIVE_JOB_FAILED\nevaluate experiment: "));
            assert!(diagnostic.contains("research contract fields are invalid"));
            assert!(diagnostic.contains("split_policy.train_size [OUT_OF_RANGE]"));
            assert!(!diagnostic.contains("request contents are not echoed"));
        }
        let diagnostic = public_failure(&anyhow::anyhow!("界".repeat(2000)));
        let (code, detail) = diagnostic.split_once('\n').unwrap();
        assert_eq!(code, "QZ_NATIVE_JOB_FAILED");
        assert_eq!(detail, format!("{} [truncated]", "界".repeat(1365)));
        assert!(detail.len() <= 4096 + " [truncated]".len());
    }

    #[test]
    fn executable_commands_exclude_the_removed_compatibility_probe() {
        for command in ["allocate", "run-bounded", "execute"] {
            assert!(Arguments::try_parse_from(["job", command]).is_ok());
        }
        assert!(Arguments::try_parse_from([
            "job",
            "verify-native",
            "--output",
            "unused-output-directory",
        ])
        .is_err());
    }

    #[cfg(feature = "native-paper")]
    #[test]
    fn paper_commands_dispatch_through_the_existing_job_executable() {
        for arguments in [
            vec![
                "job",
                "paper",
                "preflight",
                "--config",
                "paper.json",
                "--claim",
                "claim.json",
            ],
            vec!["job", "paper", "serve", "--config", "paper.json"],
            vec![
                "job",
                "paper",
                "apply",
                "--origin",
                "http://127.0.0.1:8080",
                "--credential-file",
                "token",
                "--claim",
                "claim.json",
            ],
            vec![
                "job",
                "paper",
                "status",
                "--origin",
                "http://127.0.0.1:8080",
                "--credential-file",
                "token",
            ],
            vec![
                "job",
                "paper",
                "stop",
                "--origin",
                "http://127.0.0.1:8080",
                "--credential-file",
                "token",
            ],
        ] {
            assert!(matches!(
                Arguments::try_parse_from(arguments).unwrap().command,
                Operation::Paper(_)
            ));
        }
    }

    #[cfg(not(feature = "native-paper"))]
    #[test]
    fn default_scientific_job_does_not_expose_paper_commands() {
        assert!(
            Arguments::try_parse_from(["job", "paper", "serve", "--config", "paper.json"]).is_err()
        );
    }

    #[test]
    fn feature_evaluation_cli_preserves_each_typed_artifact_identity() {
        let dataset = contracts::Id::new().to_string();
        let model = contracts::Id::new().to_string();
        let first = contracts::Id::new().to_string();
        let second = contracts::Id::new().to_string();
        let arguments = [
            "job",
            "evaluate-experiment",
            "--catalog",
            "catalog",
            "--model",
            "model.wasm",
            "--dataset-revision-id",
            &dataset,
            "--model-artifact-id",
            &model,
            "--features",
            "first.json",
            "--feature-artifact-id",
            &first,
            "--features",
            "second.json",
            "--feature-artifact-id",
            &second,
        ];
        let Operation::EvaluateExperiment {
            dataset_revision_id,
            model_artifact_id,
            feature_artifact_id,
            features,
            ..
        } = Arguments::try_parse_from(arguments).unwrap().command
        else {
            panic!("expected feature evaluation operation");
        };
        assert_eq!(dataset_revision_id.to_string(), dataset);
        assert_eq!(model_artifact_id.to_string(), model);
        assert_eq!(
            feature_artifact_id
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            [first, second]
        );
        assert_eq!(
            features,
            [
                std::path::PathBuf::from("first.json"),
                std::path::PathBuf::from("second.json")
            ]
        );
    }

    #[test]
    fn feature_evaluation_cli_rejects_invalid_ids_before_reading_any_inputs() {
        let valid = contracts::Id::new().to_string();
        let arguments = vec![
            "job",
            "evaluate-experiment",
            "--catalog",
            "missing-catalog",
            "--model",
            "missing-model.wasm",
            "--dataset-revision-id",
            &valid,
            "--model-artifact-id",
            &valid,
            "--features",
            "missing-features.json",
            "--feature-artifact-id",
            &valid,
        ];
        for position in [7, 9, 13] {
            for invalid in ["not-a-uuid", "550e8400-e29b-41d4-a716-446655440000"] {
                let mut candidate = arguments.clone();
                candidate[position] = invalid;
                let error = Arguments::try_parse_from(candidate)
                    .err()
                    .expect("invalid ID");
                assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
                assert!(error.to_string().contains("canonical UUIDv7"));
            }
        }
    }
    #[cfg(feature = "native-node-observer")]
    #[test]
    fn public_polymarket_probe_has_only_bounded_data_arguments() {
        assert!(matches!(
            Arguments::try_parse_from([
                "job",
                "polymarket-data-probe",
                "--instrument-id",
                "condition-1.POLYMARKET",
                "--output",
                "probe.json",
                "--max-seconds",
                "30",
                "--max-events",
                "4096",
                "--proxy-env",
                "QZ_EXPLICIT_PUBLIC_PROXY",
            ])
            .unwrap()
            .command,
            Operation::PolymarketDataProbe(_)
        ));
        assert!(Arguments::try_parse_from([
            "job",
            "polymarket-data-probe",
            "--instrument-id",
            "condition-1.POLYMARKET",
            "--output",
            "probe.json",
            "--credential-file",
            "never-read",
        ])
        .is_err());
    }

    #[cfg(not(feature = "native-node-observer"))]
    #[test]
    fn default_scientific_job_has_no_public_polymarket_probe() {
        assert!(Arguments::try_parse_from([
            "job",
            "polymarket-data-probe",
            "--instrument-id",
            "condition-1.POLYMARKET",
            "--output",
            "probe.json",
        ])
        .is_err());
    }

    #[cfg(feature = "native-paper")]
    #[test]
    fn polymarket_paper_run_is_reachable_without_venue_credentials() {
        assert!(matches!(
            Arguments::try_parse_from([
                "job",
                "polymarket-paper",
                "run",
                "--config",
                "original-config.json",
                "--claim",
                "original-claim.json",
                "--frozen-metadata",
                "original-metadata.json",
                "--dataset-revision",
                "original-dataset.json",
                "--source-output",
                "new-source.ndjson",
                "--report-output",
                "new-report.json",
                "--snapshots-output",
                "new-snapshots.ndjson",
                "--binding-output",
                "new-binding.json",
                "--max-seconds",
                "30",
                "--proxy-env",
                "QZ_EXPLICIT_PUBLIC_PROXY",
            ])
            .unwrap()
            .command,
            Operation::PolymarketPaper(_)
        ));
        assert!(Arguments::try_parse_from([
            "job",
            "polymarket-paper",
            "source",
            "--instrument-id",
            "condition-1.POLYMARKET",
            "--output",
            "source.ndjson",
            "--private-key",
            "not-supported",
        ])
        .is_err());
    }

    #[cfg(not(feature = "native-paper"))]
    #[test]
    fn default_scientific_job_has_no_polymarket_paper_host() {
        assert!(Arguments::try_parse_from(["job", "polymarket-paper", "run"]).is_err());
    }
}
