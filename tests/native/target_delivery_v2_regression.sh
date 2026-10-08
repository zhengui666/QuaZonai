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
evidence=${QZ_TEST_EVIDENCE:-target/target-delivery-v2-evidence}
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
: > "$evidence/target-delivery-v2-combined-status.txt"
run_required() {
  local label=$1 status=0
  run_case "$@" || status=$?
  cases=$((cases+1))
  printf '%s=%s\n' "$label" "$status" >> "$evidence/target-delivery-v2-combined-status.txt"
  if ((status != 0)); then failed=$((failed+1)); fi
}
run_required target-v2-upgrade real_upgrade_preserves_v1_history_and_retires_active_receipt_replays -p store --test target_delivery_v2_upgrade
run_required target-v2-probe historical_probe_versions_stay_readable_but_only_v2_is_currently_deliverable -p store --test target_delivery_v2_upgrade
printf 'cases=%s\nfailed=%s\n' "$cases" "$failed" >> "$evidence/target-delivery-v2-combined-status.txt"
((cases == 2 && failed == 0))
