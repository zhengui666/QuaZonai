#!/usr/bin/env bash
# Exact-case checks on the existing disposable PostgreSQL/PGMQ instance.
# Compiled, listed, filtered-out and ignored cases never count as executed passes.
set -euo pipefail
: "${DATABASE_URL:?Use only a disposable PostgreSQL 18 + PGMQ 1.10.0 test instance}"
if [[ ${QZ_TEST_PG_CONTAINER:-} != store-database || $DATABASE_URL != postgres://postgres:*@127.0.0.1:55432/postgres ]]; then
  printf 'Use only the explicitly injected store-postgres CI database.\n' >&2
  exit 2
fi
if (($# == 0)); then set -- cargo; fi
cargo=("$@")
evidence=${QZ_TEST_EVIDENCE:-target/dataset-forward-evidence}
mkdir -p "$evidence"

run_case() {
  local label=$1 case=$2
  shift 2
  local list_status list_tee_status matched test_status=not_run test_tee_status=not_run executed=0 passed_one=0
  local -a pipeline
  set +e
  "${cargo[@]}" test --locked "$@" "$case" -- --exact --list 2>&1 | tee "$evidence/$label-list.log"
  pipeline=("${PIPESTATUS[@]}")
  set -e
  list_status=${pipeline[0]}
  list_tee_status=${pipeline[1]}
  matched=$(grep -Fxc -- "$case: test" "$evidence/$label-list.log" || true)
  if [[ $list_status == 0 && $list_tee_status == 0 && $matched == 1 ]]; then
    set +e
    "${cargo[@]}" test --locked "$@" "$case" -- --exact --nocapture --test-threads=1 2>&1 | tee "$evidence/$label-test.log"
    pipeline=("${PIPESTATUS[@]}")
    set -e
    test_status=${pipeline[0]}
    test_tee_status=${pipeline[1]}
    executed=$(grep -Fxc -- 'running 1 test' "$evidence/$label-test.log" || true)
    if grep -Eq -- '^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured;' "$evidence/$label-test.log"; then
      passed_one=1
    fi
  fi
  printf 'case=%s\nlist_status=%s\nlist_tee_status=%s\nmatched=%s\ntest_status=%s\ntest_tee_status=%s\nexecuted=%s\npassed_one=%s\n' \
    "$case" "$list_status" "$list_tee_status" "$matched" "$test_status" "$test_tee_status" "$executed" "$passed_one" \
    > "$evidence/$label-status.txt" || return 1
  cat "$evidence/$label-status.txt" || return 1
  [[ $list_status == 0 && $list_tee_status == 0 && $matched == 1 &&
     $test_status == 0 && $test_tee_status == 0 && $executed == 1 && $passed_one == 1 ]]
}

# Preserve every outcome even if another case fails. The aggregate fails closed.
failed=0
cases=0
: > "$evidence/dataset-forward-combined-status.txt"
run_required() {
  local label=$1 status=0
  run_case "$@" || status=$?
  cases=$((cases+1))
  printf '%s=%s\n' "$label" "$status" >> "$evidence/dataset-forward-combined-status.txt"
  if ((status != 0)); then failed=$((failed+1)); fi
}
run_required evidence-owner owner_browser_and_device_read_original_bound_summary_without_raw_values -p store --test dataset_evidence
run_required evidence-authority genuine_diagnostic_and_project_machine_credentials_never_read_evidence -p store --test dataset_evidence
run_required evidence-sealed sealed_and_non_dataset_artifact_references_stop_before_native_io -p store --test dataset_evidence
run_required evidence-missing missing_real_metadata_or_quality_objects_never_fall_back_to_registration_labels -p store --test dataset_evidence
run_required evidence-binding foreign_document_bytes_and_quality_schema_mismatch_are_rejected -p store --test dataset_evidence
run_required evidence-license revoked_license_remains_visible_for_audit_without_relabeling_or_new_use -p store --test dataset_evidence
run_required evidence-http dataset_evidence_http_returns_typed_bound_summary_and_keeps_artifact_route_closed -p server --features native-codex --test data_http
run_required forward-derivation database_derivation_matches_rust_for_null_finite_and_short_wall -p store --test forward_execution_limits
run_required forward-absent absent_candidate_caps_reach_original_native_job_and_replay_without_reading -p store --test forward_execution_limits
run_required forward-finite finite_source_cannot_be_bypassed_by_null_old_tuple_or_wrong_runtime -p store --test forward_execution_limits
run_required forward-replay finite_fixed_tuple_and_snapshot_remain_immutable_on_replay -p store --test forward_execution_limits
run_required forward-upgrade migration_103_to_104_preserves_existing_receipt_and_rejects_new_legacy_bypass -p store --test forward_execution_limits
printf 'cases=%s\nfailed=%s\n' "$cases" "$failed" >> "$evidence/dataset-forward-combined-status.txt"
((cases == 12 && failed == 0))
