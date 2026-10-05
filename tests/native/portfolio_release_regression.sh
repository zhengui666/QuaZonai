#!/usr/bin/env bash
# Run the unchanged real-PG stack regression in both original integration targets.
# Neither a list-only pass nor zero matched/ignored tests is successful evidence.
set -euo pipefail
: "${DATABASE_URL:?Use only a disposable PostgreSQL 18 + PGMQ 1.10.0 test instance}"
if (($# == 0)); then set -- cargo; fi
cargo=("$@")
case=qualified_portfolio::release_freezes_original_package_and_replays_without_republishing
evidence=${QZ_TEST_EVIDENCE:-target/portfolio-release-evidence}
mkdir -p "$evidence"

run_target() {
  local label=$1
  shift
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
  printf 'target=%s\nlist_status=%s\nlist_tee_status=%s\nmatched=%s\ntest_status=%s\ntest_tee_status=%s\nexecuted=%s\npassed_one=%s\n' \
    "$label" "$list_status" "$list_tee_status" "$matched" "$test_status" "$test_tee_status" "$executed" "$passed_one" \
    > "$evidence/$label-status.txt" || return 1
  cat "$evidence/$label-status.txt" || return 1
  [[ $list_status == 0 && $list_tee_status == 0 && $matched == 1 &&
     $test_status == 0 && $test_tee_status == 0 && $executed == 1 && $passed_one == 1 ]]
}

# Capture separately: even a failed Store test must not prevent the Server target.
store_status=0
run_target store -p store --test experiment_compilations || store_status=$?
server_status=0
run_target server -p server --features native-codex --test portfolio_study_http || server_status=$?
printf 'store=%s\nserver=%s\n' "$store_status" "$server_status" > "$evidence/combined-status.txt"
if ((store_status != 0 || server_status != 0)); then exit 1; fi
