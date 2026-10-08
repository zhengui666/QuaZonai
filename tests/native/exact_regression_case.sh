#!/usr/bin/env bash
# Shared strict selector used by the existing release gate and Capital exit gate.
# Callers supply the cargo array and evidence directory. Ignored tests are enabled
# only by an explicit seventh argument; the legacy six-argument calls stay normal.
run_case() {
  local tag=$1 package=$2 kind=$3 target=$4 features=$5 case=$6 mode=${7:-normal}
  local -a selection=(--lib) feature_flags=() ignored_flags=()
  case "$mode" in
    normal) ;;
    ignored) ignored_flags=(--ignored) ;;
    *) printf 'Unknown exact regression mode: %s\n' "$mode" >&2; return 2 ;;
  esac
  if [[ $kind == test ]]; then selection=(--test "$target"); fi
  if [[ -n $features ]]; then feature_flags=(--features "$features"); fi
  local list_status list_tee_status matched test_status=not_run test_tee_status=not_run executed=0 passed_one=0
  local -a pipeline
  set +e
  "${cargo[@]}" test --locked -p "$package" "${feature_flags[@]}" "${selection[@]}" "$case" -- --exact "${ignored_flags[@]}" --list 2>&1 | tee "$evidence/$tag-list.log"
  pipeline=("${PIPESTATUS[@]}")
  set -e
  list_status=${pipeline[0]}
  list_tee_status=${pipeline[1]}
  matched=$(grep -Fxc -- "$case: test" "$evidence/$tag-list.log" || true)
  if [[ $list_status == 0 && $list_tee_status == 0 && $matched == 1 ]]; then
    set +e
    "${cargo[@]}" test --locked -p "$package" "${feature_flags[@]}" "${selection[@]}" "$case" -- --exact "${ignored_flags[@]}" --nocapture --test-threads=1 2>&1 | tee "$evidence/$tag-test.log"
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

