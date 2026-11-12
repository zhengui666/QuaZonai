//! One-process native science entrypoints. No database or delivery authority.
use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::{de::DeserializeOwned, Serialize};
use std::{io::Write, path::PathBuf};

mod native_stdout;

/// Keep upstream native console messages out of the typed stdout transport.
fn native_output<T: Serialize>(run: impl FnOnce() -> Result<T>) -> Result<()> {
    output(&native_stdout::with_stderr_logs(run)?)
}

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
    /// Exact upstream source and precision of this binary.
    BuildIdentity,
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
    /// Discover stored native instruments and market-data file intervals without modifying them.
    CatalogInspect {
        #[arg(long)]
        catalog: PathBuf,
    },
    /// Read native catalog file coverage; inclusive ts_init bounds, not event completeness.
    CatalogCoverage {
        #[arg(long)]
        catalog: PathBuf,
    },
    /// Validate original source-owner metadata; no inferred permission or PIT status.
    ReviewSource,
    /// Plan capital points; the durable owner runs each as an independent native task.
    CapitalCurve {
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long, value_parser = parse_id)]
        dataset_revision_id: Option<contracts::Id>,
    },
    /// Blend explicit aligned native target rows using exact decimal member weights.
    BlendTargets,
    /// Conditional historical capacity from original causal BAR liquidity and targets.
    AssessCapacity {
        #[arg(long)]
        catalog: PathBuf,
    },
    /// Exact, reserve-aware sizing inside an approved pool; target-only, no orders.
    SizeAccount,
    /// Validate an original capital-exit V1 document; does not control an owner.
    ValidateCapitalExit,
    /// Exact native BAR identities for private exposure tracking; no observations.
    CatalogFactorScope {
        #[arg(long)]
        catalog: PathBuf,
    },
    /// Run a compiled upstream strategy on original native market events.
    NativeStrategyBacktest {
        #[arg(long)]
        catalog: PathBuf,
    },
    /// Replay explicit historical walk-forward windows with upstream strategies.
    NativeWalkForward {
        #[arg(long)]
        catalog: PathBuf,
    },
    /// Replay frozen target weights in one native simulated account.
    Simulate {
        #[arg(long)]
        catalog: PathBuf,
        /// Existing DatasetRevision ID; required only by the explicit spot CASH mode.
        #[arg(long, value_parser = parse_id)]
        dataset_revision_id: Option<contracts::Id>,
    },
    /// Matched no-trade HOLD and target replay from original observed spot balances.
    CompareObservedSpot {
        #[arg(long)]
        catalog: PathBuf,
    },
    /// Value original observed spot balances using only original already available marks.
    ValueObservedSpot {
        #[arg(long)]
        catalog: PathBuf,
    },
    /// Digest actual original native instruments, history and BARs at a frozen cutoff.
    CatalogPrefixDigest {
        #[arg(long)]
        catalog: PathBuf,
    },
}

fn parse_id(value: &str) -> std::result::Result<contracts::Id, &'static str> {
    contracts::Id::try_from(value.to_owned()).map_err(|_| "expected a canonical UUIDv7")
}

fn input<T: DeserializeOwned>() -> Result<T> {
    Ok(serde_json::from_reader(std::io::stdin().lock())?)
}

fn output<T: Serialize>(value: &T) -> Result<()> {
    let mut stream = std::io::stdout().lock();
    serde_json::to_writer(&mut stream, value)?;
    writeln!(stream)?;
    stream.flush()?;
    Ok(())
}

fn run(operation: Operation) -> Result<()> {
    match operation {
        Operation::BuildIdentity => {
            serde_json::to_writer(std::io::stdout(), &job::build_identity())?;
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
        Operation::ReviewSource => output(&job::catalog::validate_source(&input()?)?),
        Operation::CapitalCurve {
            catalog,
            dataset_revision_id,
        } => {
            let request = input()?;
            output(&job::capital_curve::plan_only(
                &catalog,
                &request,
                dataset_revision_id,
            )?)
        }
        Operation::BlendTargets => output(&domain::execution::strategy::blend_targets(&input()?)?),
        Operation::AssessCapacity { catalog } => {
            output(&job::capacity::assess(&catalog, &input()?)?)
        }
        Operation::SizeAccount => output(&job::capacity::size_account(&input()?)?),
        Operation::ValidateCapitalExit => output(&input::<
            contracts::capital_exit::NativeCapitalExitDocumentV1,
        >()?),
        Operation::CatalogInspect { catalog } => {
            output(&job::catalog::inspect::query(&catalog, input()?)?)
        }
        Operation::CatalogCoverage { catalog } => {
            output(&job::catalog::coverage::query(&catalog, input()?)?)
        }
        Operation::CatalogFactorScope { catalog } => {
            output(&job::catalog::catalog_factor_scope(&catalog)?)
        }
        Operation::NativeStrategyBacktest { catalog } => {
            native_output(|| job::native_strategy::run(&catalog, input()?))
        }
        Operation::NativeWalkForward { catalog } => {
            native_output(|| job::native_walk_forward::run(&catalog, input()?))
        }
        Operation::Simulate {
            catalog,
            dataset_revision_id,
        } => output(&job::simulation::simulate_explicit(
            &catalog,
            &input()?,
            dataset_revision_id,
        )?),
        Operation::CompareObservedSpot { catalog } => {
            output(&job::observed_spot::compare(&catalog, &input()?)?)
        }
        Operation::ValueObservedSpot { catalog } => {
            output(&job::observed_spot::value(&catalog, &input()?)?)
        }
        Operation::CatalogPrefixDigest { catalog } => {
            output(&job::catalog_prefix::digest(&catalog, &input()?)?)
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
    let mut detail = format!("{error:#}");
    // Fields deliberately has a generic Display for other transports. Preserve
    // its actual field/code diagnostics only at this local native CLI boundary.
    for cause in error.chain() {
        if let Some(domain::DomainError::Fields(issues)) = cause.downcast_ref() {
            for issue in issues {
                detail.push_str(&format!("; {} [{}]", issue.field, issue.code));
            }
        }
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
    use super::{public_error_code, public_failure, Arguments};
    use clap::Parser;

    #[test]
    fn hyper_network_collection_is_not_a_cli_operation() {
        for command in ["capture-spot-candles", "hyperliquid-public"] {
            let error = Arguments::try_parse_from(["job", command])
                .err()
                .expect("Hyper network collection must not be exposed");
            assert_eq!(error.kind(), clap::error::ErrorKind::InvalidSubcommand);
        }
    }

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
        assert_eq!(detail, "界".repeat(2000));
    }

    #[test]
    fn executable_commands_exclude_the_removed_compatibility_probe() {
        for command in ["run-bounded", "execute"] {
            assert!(Arguments::try_parse_from(["job", command]).is_ok());
        }
        assert!(Arguments::try_parse_from(["job", "allocate"]).is_err());
        assert!(Arguments::try_parse_from([
            "job",
            "verify-native",
            "--output",
            "unused-output-directory",
        ])
        .is_err());
    }

    #[test]
    fn default_scientific_job_does_not_expose_paper_commands() {
        assert!(
            Arguments::try_parse_from(["job", "paper", "serve", "--config", "paper.json"]).is_err()
        );
    }

    #[test]
    fn retired_wasm_evaluation_is_not_a_cli_operation() {
        assert!(Arguments::try_parse_from(["job", "evaluate-experiment"]).is_err());
    }
}
