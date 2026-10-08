#!/usr/bin/env bash
# Routine Rust rule and local component tests, without PostgreSQL or native services.
# Some components use disposable files; --lib/--bins alone is not a unit-test boundary.
set -euo pipefail
if (($# == 0)); then set -- cargo; fi
cargo=("$@")
features=job/catalog-prepare,job/polymarket-history,job/native-paper-test,job/native-sandbox-test

# Keep Server rule tests, but do not enable native-codex's systemd/cgroup probes.
# CLI code is shared by Server and the portable CLI. Preserve its rule/file cases;
# skip only the verified socket/HTTP/FIFO/process cases, not either entire module.
"${cargo[@]}" test --locked --workspace --exclude store --lib --bins --features "$features" -- \
  --test-threads=1 \
  --skip client::origin_tests::scoped_machine_http_is_rejected_before_a_network_connection \
  --skip client::origin_tests::explicit_http_sends_once_and_never_follows_a_redirect \
  --skip client::account_transport::tests::all_other_write_routes_still_require_the_same_key_in_preview_and_sender \
  --skip client::account_transport::tests::fifo_path_replacement_and_open_race_fail_within_a_bounded_subprocess \
  --skip client::account_transport::tests::unknown_delivery_retries_the_same_authenticated_request_then_replays_on_restart \
  --skip client::account_transport::tests::mismatched_receipt_stops_and_permanent_rejection_is_not_retried \
  --skip client::account_transport::tests::retryable_problem_exhaustion_is_bounded_and_retains_original_input \
  --skip managed::tests::compiler_ \
  --skip operator::catalog_prepare::classic_zip::tests

# Store embeds real-PG cases in its lib target; retain its local rule tests.
"${cargo[@]}" test --locked -p store --lib -- \
  --test-threads=1 \
  --skip execution_assumptions::liquidity::tests \
  --skip lifecycle::portfolio::tests \
  --skip lifecycle::portfolio::publication::tests::postgres_checks_the_final_window_without_inventing_qualification \
  --skip lifecycle::portfolio::weights::tests \
  --skip lifecycle::sealed::qualification::tests

# Cargo integration targets also contain pure rule tests. Keep all Contracts/Domain
# rule targets; the architecture case invokes cargo metadata, rustc and git.
"${cargo[@]}" test --locked -p contracts -p domain --test '*' -- \
  --test-threads=1 --skip workspace_dependencies_follow_design_boundaries
"${cargo[@]}" test --locked -p job --test allocation --test signals --test feature_model --test validation -- \
  --test-threads=1 --skip native_cli_consumes_exact_json_without_issuing_delivery_authority
"${cargo[@]}" test --locked -p job --test account_observer -- --test-threads=1
"${cargo[@]}" test --locked -p job --test polymarket_execution_contract -- --test-threads=1
"${cargo[@]}" test --locked -p job --test polymarket_execution_preflight -- --test-threads=1
"${cargo[@]}" test --locked -p job --test polymarket_forward --features "$features" -- --test-threads=1
# Existing native replay with synthetic local Parquet, no network/OCI/account.
"${cargo[@]}" test --locked -p job --test polymarket_target_policy -- --test-threads=1
"${cargo[@]}" test --locked -p runtime --test contracts --test output_size_migration -- --test-threads=1

# Lightweight authentication/identity/publication components use temporary files.
# Full-filesystem/userns/seccomp fault acceptance stays in rust-regression.yml.
"${cargo[@]}" test --locked -p integrations \
  --test authentication --test secret_identity --test secret_publication --test artifact_publication -- \
  --test-threads=1 --skip native_full_filesystem_preserves_objects_and_recovers_without_partial_publication
