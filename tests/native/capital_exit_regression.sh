#!/usr/bin/env bash
# Selected Capital exit transactions and original native/Poller HTTP bridges.
# Reuses the release gate's strict list/run checks and the existing PG18/PGMQ job.
set -euo pipefail
mode=pg
case ${1:-} in
  --native-only) mode=native; shift ;;
  --bridges-only) mode=bridges; shift ;;
esac
if [[ $mode != native ]]; then
  : "${DATABASE_URL:?Use only the explicitly injected disposable PostgreSQL instance}"
  if [[ ${QZ_TEST_PG_CONTAINER:-} != store-database || $DATABASE_URL != postgres://postgres:*@127.0.0.1:55432/postgres ]]; then
    printf 'Use only the existing isolated store-postgres CI database.\n' >&2
    exit 2
  fi
fi
if (($# == 0)); then set -- cargo; fi
cargo=("$@")
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"
evidence=${QZ_TEST_EVIDENCE:?Use the existing isolated job evidence directory}
mkdir -p "$evidence"
evidence=$(cd "$evidence" && pwd)
source "$root/tests/native/exact_regression_case.sh"
failed=0
cases=0
pg_cases=0
pure_cases=0
native_cases=0
bridge_cases=0
phase=select
: > "$evidence/capital-exit-combined-status.txt"
trap 'status=$?; printf "selected_cases=%s\npg_cases=%s\npure_cases=%s\nnative_cases=%s\nbridge_cases=%s\nfailed=%s\nphase=%s\nexit=%s\n" "$cases" "$pg_cases" "$pure_cases" "$native_cases" "$bridge_cases" "$failed" "$phase" "$status" >> "$evidence/capital-exit-combined-status.txt"' EXIT
run_required() {
  local group=$1 tag=$2 status=0
  shift
  run_case "$@" || status=$?
  cases=$((cases+1))
  case "$group" in
    pg) pg_cases=$((pg_cases+1)) ;;
    pure) pure_cases=$((pure_cases+1)) ;;
    native) native_cases=$((native_cases+1)) ;;
    bridge) pg_cases=$((pg_cases+1)); bridge_cases=$((bridge_cases+1)) ;;
  esac
  printf '%s=%s\n' "$tag" "$status" >> "$evidence/capital-exit-combined-status.txt"
  if ((status != 0)); then failed=$((failed+1)); fi
}

# These two original-engine regressions need no PG. The existing routine/full
# Job lib runs still retain the four production Paper tests and all other cases.
if [[ $mode == native ]]; then
  phase=execute-original-paper
  run_required native capital-paper-empty-scope job lib '' native-sandbox-test,native-paper-test polymarket_streaming_paper::capital_exit_tests::production_paper_first_stale_fence_retains_empty_scope_for_silent_deadline
  run_required native capital-paper-real-opener job lib '' native-sandbox-test,native-paper-test polymarket_streaming_paper::capital_exit_tests::production_paper_first_stale_fence_keeps_real_opener_pending_until_quote
  phase=complete
  ((failed == 0))
  exit
fi

# Full regression already runs every non-ignored Store/Server case and Job test.
# Its bridge-only call adds only the explicitly ignored entrypoints it cannot run.
if [[ $mode == pg ]]; then
  run_required pg capital-store-01 store test capital_exit '' capital_exit_missing_owner_is_blocked_and_preview_cannot_reserve
  run_required pg capital-store-02 store test capital_exit '' capital_exit_start_replay_preserves_one_account_reserve_and_physical_alias_is_rejected
  run_required pg capital-store-03 store test capital_exit '' capital_exit_old_epoch_cannot_report_ready_and_pause_keeps_reserve
  run_required pg capital-store-04 store test capital_exit '' capital_exit_user_report_and_balance_drop_do_not_reconcile_without_cash_movement
  run_required pg capital-store-05 store test capital_exit '' capital_exit_assessment_is_read_only_and_unregistered_session_cannot_borrow_binding
  run_required pg capital-store-06 store test capital_exit '' capital_exit_concurrent_start_claims_one_reservation_and_evidence_stays_on_claim_credential
  run_required pg capital-store-07 store test capital_exit '' capital_exit_additive_migration_preserves_preexisting_original_receipt
  run_required pg capital-store-08 store test capital_exit '' capital_exit_versioned_documents_and_owner_command_reconcile_are_discoverable
  run_required pg capital-store-09 store test capital_exit '' capital_exit_live_environment_revocation_invalidates_readiness_before_expiry
  run_required pg capital-store-10 store test capital_exit '' capital_exit_fresh_fence_response_cannot_republish_old_snapshot_availability
  run_required pg capital-store-11 store test capital_exit '' capital_exit_availability_requires_the_trusted_venue_binding
  run_required pg capital-store-12 store test capital_exit '' capital_exit_stale_withdrawal_report_recovers_on_original_command_and_epoch
  run_required pg capital-store-13 store test capital_exit '' capital_exit_stale_cancel_accepts_fresh_terminal_evidence_on_original_command
  run_required pg capital-start-expiry-cursor store test capital_exit_start_expiry '' capital_exit_start_preview_expiry_during_cursor_lock_rolls_back
  run_required pg capital-start-expiry-reservation store test capital_exit_start_expiry '' capital_exit_start_assessment_expiry_during_reservation_lock_rolls_back
  run_required pg capital-start-expiry-queue store test capital_exit_start_expiry '' capital_exit_start_expiry_during_queue_lock_rolls_back
  run_required pg capital-start-expiry-replay store test capital_exit_start_expiry '' capital_exit_start_fresh_lock_wait_and_expired_replay_keep_original_receipt
  run_required pg capital-http-01 server test capital_exit_http 'native-codex' capital_exit_preview_http_is_read_only_strict_idempotent_and_owner_only
  run_required pg capital-http-02 server test capital_exit_http 'native-codex' capital_exit_http_start_pause_and_route_body_mismatch_preserve_original_intent
  run_required pg capital-binding-01 store test paper_capital_exit_binding '' paper_capital_exit_concurrent_cross_project_alias_cannot_duplicate_engine_budget
  run_required pg capital-binding-02 store test paper_capital_exit_binding '' paper_capital_exit_independent_sessions_share_labels_without_sharing_capital
  run_required pg capital-binding-03 store test paper_capital_exit_binding '' paper_capital_exit_wrong_source_or_principal_cannot_reuse_an_owner_binding
  run_required pg capital-binding-04 store test paper_capital_exit_binding '' paper_capital_exit_006_preserves_legacy_receipt_and_blocks_proven_engine_alias
  run_required pg capital-registration-01 server test account_observations_http 'native-codex' paper_capital_exit_registration::paper_capital_exit_disabled_intake_preserves_original_receipt_without_registration_header
  run_required pg capital-registration-02 server test account_observations_http 'native-codex' paper_capital_exit_registration::paper_capital_exit_enabled_receipt_ack_requires_successful_exact_source_registration
  run_required pure capital-http-schema server test capital_exit_http native-codex capital_exit_http_contracts_have_exact_routes_closed_types_and_auth_boundaries
  # Payload size must not bypass authentication, exact replay or full JSON validation.
  run_required pg capital-http-evidence-full server test account_observations_http native-codex capital_exit_body_limit::capital_exit_native_evidence_http_preserves_history_without_body_cap
  run_required pg capital-http-assessment-full server test account_observations_http native-codex capital_exit_body_limit::capital_exit_native_assessment_http_keeps_history_intact_without_body_cap
  run_required pg capital-http-command-full server test account_observations_http native-codex capital_exit_body_limit::ordinary_http_commands_accept_complete_large_json_and_keep_auth_and_idempotency
  run_required native runtime-http-object-full runtime test runtime_http '' native_object_upload_accepts_binary_above_former_cap_and_checks_exact_versioned_replays
  run_required native runtime-http-json-full runtime test runtime_http '' native_json_intake_has_no_implicit_body_cap_and_keeps_auth_and_complete_validation
fi

# Build BOTH test executables in one invocation from this exact checkout. A fresh
# compiler-artifact, including Cargo's verified cached artifact, is authoritative;
# ambient environment paths and shared target globs are never used as evidence.
phase=build-native-bridges
set +e
"${cargo[@]}" test --locked --manifest-path "$root/Cargo.toml" -p job \
  --features native-sandbox-test,native-paper-test --lib --test native_capital_exit \
  --no-run --message-format=json \
  2> "$evidence/capital-exit-build.stderr" \
  | tee "$evidence/capital-exit-build.jsonl"
build_pipeline=("${PIPESTATUS[@]}")
set -e
printf 'cargo_status=%s\nstdout_tee_status=%s\nexecution=not_run_by_build\n' "${build_pipeline[0]}" "${build_pipeline[1]}" > "$evidence/capital-exit-build-status.txt"
cat "$evidence/capital-exit-build.stderr" >&2
[[ ${build_pipeline[0]} == 0 && ${build_pipeline[1]} == 0 ]]
phase=resolve-native-artifacts
python3 "$root/tests/native/capital_exit_artifacts.py" "$evidence/capital-exit-build.jsonl" \
  > "$evidence/capital-exit-artifacts.txt" 2> "$evidence/capital-exit-artifacts.stderr"
mapfile -t artifacts < "$evidence/capital-exit-artifacts.txt"
test "${#artifacts[@]}" -eq 2
export QZ_NATIVE_CAPITAL_EXIT_TEST_BINARY="${artifacts[0]}"
export QZ_PAPER_SERVICE_TEST_BINARY="${artifacts[1]}"

# Listing a child is a prerequisite only. Server supplies its disposable original
# input and executes it; no child list/build is counted as a passing business test.
list_child() {
  local tag=$1 executable=$2 case=$3
  phase="$tag-list"
  "$executable" "$case" --exact --ignored --list 2>&1 | tee "$evidence/$tag-list.log"
  test "$(grep -Fxc -- "$case: test" "$evidence/$tag-list.log" || true)" -eq 1
  printf 'case=%s\nlisted=1\nexecution=not_run_by_list\n' "$case" > "$evidence/$tag-status.txt"
}
list_child capital-native-child "$QZ_NATIVE_CAPITAL_EXIT_TEST_BINARY" \
  capital_exit_http_pipeline::real_http_capital_exit_uses_original_sandbox_events
list_child capital-poller-child "$QZ_PAPER_SERVICE_TEST_BINARY" \
  polymarket_streaming_paper::capital_exit_tests::paper_service_acceptance::production_poller_uses_registered_original_paper_and_real_receipts
phase=execute-native-bridges
run_required bridge capital-native-http server test account_observations_http native-codex capital_exit_native_pipeline::capital_exit_http_releases_partial_cash_with_original_native_events ignored
run_required bridge capital-poller-release server test account_observations_http native-codex paper_service_acceptance::production_paper_poller_pg_releases_with_original_receipts_after_lost_replies ignored
run_required bridge capital-poller-pause-resume server test account_observations_http native-codex paper_service_acceptance::production_paper_poller_pg_silent_pause_then_active_resume_keeps_original_clock ignored
run_required bridge capital-poller-cancelled-resume server test account_observations_http native-codex paper_service_acceptance::production_paper_poller_pg_late_cancelled_reserved_resume_uses_original_intent ignored
run_required bridge capital-poller-waiting-resume server test account_observations_http native-codex paper_service_acceptance::production_paper_poller_pg_late_waiting_evidence_resume_preserves_released_cash ignored
phase=complete
((failed == 0))
