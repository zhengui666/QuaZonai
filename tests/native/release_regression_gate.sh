#!/usr/bin/env bash
# Exact release regressions in the existing isolated PG/native environment.
set -euo pipefail
: "${DATABASE_URL:?Use only the explicitly injected disposable PostgreSQL instance}"
if [[ ${QZ_TEST_PG_CONTAINER:-} != store-database || $DATABASE_URL != postgres://postgres:*@127.0.0.1:55432/postgres ]]; then
  printf 'Use only the existing isolated store-postgres CI database.\n' >&2
  exit 2
fi
if (($# == 0)); then set -- cargo; fi
cargo=("$@")
evidence=${QZ_TEST_EVIDENCE:?Use the existing isolated job evidence directory}
mkdir -p "$evidence"

run_case() {
  local tag=$1 package=$2 kind=$3 target=$4 features=$5 case=$6
  local -a selection=(--lib) feature_flags=()
  if [[ $kind == test ]]; then selection=(--test "$target"); fi
  if [[ -n $features ]]; then feature_flags=(--features "$features"); fi
  local list_status list_tee_status matched test_status=not_run test_tee_status=not_run executed=0 passed_one=0
  local -a pipeline
  set +e
  "${cargo[@]}" test --locked -p "$package" "${feature_flags[@]}" "${selection[@]}" "$case" -- --exact --list 2>&1 | tee "$evidence/$tag-list.log"
  pipeline=("${PIPESTATUS[@]}")
  set -e
  list_status=${pipeline[0]}
  list_tee_status=${pipeline[1]}
  matched=$(grep -Fxc -- "$case: test" "$evidence/$tag-list.log" || true)
  if [[ $list_status == 0 && $list_tee_status == 0 && $matched == 1 ]]; then
    set +e
    "${cargo[@]}" test --locked -p "$package" "${feature_flags[@]}" "${selection[@]}" "$case" -- --exact --nocapture --test-threads=1 2>&1 | tee "$evidence/$tag-test.log"
    pipeline=("${PIPESTATUS[@]}")
    set -e
    test_status=${pipeline[0]}
    test_tee_status=${pipeline[1]}
    executed=$(grep -Fxc -- 'running 1 test' "$evidence/$tag-test.log" || true)
    if grep -Eq -- '^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured;' "$evidence/$tag-test.log"; then
      passed_one=1
    fi
  fi
  printf 'case=%s\nlist_status=%s\nlist_tee_status=%s\nmatched=%s\ntest_status=%s\ntest_tee_status=%s\nexecuted=%s\npassed_one=%s\n' \
    "$case" "$list_status" "$list_tee_status" "$matched" "$test_status" "$test_tee_status" "$executed" "$passed_one" \
    > "$evidence/$tag-status.txt" || return 1
  [[ $list_status == 0 && $list_tee_status == 0 && $matched == 1 &&
     $test_status == 0 && $test_tee_status == 0 && $executed == 1 && $passed_one == 1 ]]
}

failed=0
run_case mission-optional-wall-cancel store test missions '' optional_wall_credentials_do_not_survive_cancellation || failed=$((failed+1))
run_case mission-optional-wall-revoke store test missions '' optional_wall_credentials_do_not_survive_revocation || failed=$((failed+1))
run_case mission-optional-wall-disable store test missions '' optional_wall_credentials_do_not_survive_disabled_principal || failed=$((failed+1))
run_case issuance-old-owner server test artifacts_http native-codex mission_attempt::attempt_bound_legacy_credential_cannot_borrow_the_current_owner_on_upgrade || failed=$((failed+1))
run_case issuance-old-unbound server test artifacts_http native-codex mission_attempt::legacy_unbound_mission_is_preserved_but_requires_new_issuance_after_upgrade || failed=$((failed+1))
run_case issuance-doctor store test authority_invariants '' doctor_upgrade_requires_explicit_revocation_and_preserves_original_issuance || failed=$((failed+1))
run_case mission-native-stack server test mission_worker native-codex controlled_real_declaration_registers_original_reviewed_qualification || failed=$((failed+1))
run_case service-existing-start server lib '' native-codex codex_native::service::native_tests::native_stop_replace_cancels_an_existing_start_and_leaves_no_populated_group || failed=$((failed+1))
run_case service-queued-start server lib '' native-codex codex_native::service::native_tests::native_stop_replace_cancels_a_deterministically_queued_start || failed=$((failed+1))
run_case service-late-launcher server lib '' native-codex codex_native::service::native_tests::native_persistent_mask_rejects_a_sigstopped_late_launcher || failed=$((failed+1))
run_case rebalance-release-retry store test experiment_compilations '' qualified_portfolio::automatic_rebalance::frozen_policy_rebalance_queues_one_original_bounded_build || failed=$((failed+1))
printf 'cases=11\nfailed=%s\n' "$failed" > "$evidence/release-regression-combined-status.txt"
if ((failed != 0)); then exit 1; fi
