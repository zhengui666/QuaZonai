#!/usr/bin/env bash
# Exact no-order native producer and real isolated PostgreSQL/HTTP regressions.
# The Sandbox fixture and generated client binding are not venue/account evidence.
set -euo pipefail
: "${DATABASE_URL:?Use only a disposable PostgreSQL 18 + PGMQ 1.10.0 test instance}"
if [[ ${QZ_TEST_PG_CONTAINER:-} != store-database || $DATABASE_URL != postgres://postgres:*@127.0.0.1:55432/postgres ]]; then
  printf 'Use only the explicitly injected store-postgres CI database.\n' >&2
  exit 2
fi
if (($# == 0)); then set -- cargo; fi
cargo=("$@")
evidence=${QZ_TEST_EVIDENCE:-$(mktemp -d)}
mkdir -p "$evidence"
phase=build-fixture
trap 'status=$?; printf "phase=%s\nexit=%s\n" "$phase" "$status" > "$evidence/native-client-combined-status.txt"' EXIT

"${cargo[@]}" build --locked -p job --features native-sandbox-test \
  --example native_client_account_snapshots --message-format=json-render-diagnostics \
  2> >(tee "$evidence/native-client-fixture-build.stderr" >&2) \
  | tee "$evidence/native-client-fixture-build.jsonl"
QUAZONAI_NATIVE_CLIENT_ACCOUNT_FIXTURE_BIN=$(python3 - "$evidence/native-client-fixture-build.jsonl" <<'PY'
import json
import pathlib
import sys
paths = set()
for line in pathlib.Path(sys.argv[1]).read_text().splitlines():
    try:
        item = json.loads(line)
    except json.JSONDecodeError:
        continue
    target = item.get("target", {})
    if (item.get("reason") == "compiler-artifact"
            and target.get("name") == "native_client_account_snapshots"
            and "example" in target.get("kind", []) and item.get("executable")):
        paths.add(item["executable"])
if len(paths) != 1:
    raise SystemExit("expected exactly one compiled native client fixture artifact")
path = pathlib.Path(paths.pop())
if not path.is_file():
    raise SystemExit("compiled native client fixture is not present")
print(path.resolve())
PY
)
export QUAZONAI_NATIVE_CLIENT_ACCOUNT_FIXTURE_BIN

run_exact() {
  local tag=$1 package=$2 features=$3 target=$4 case=$5 mode=${6:-normal}
  local -a flags=() selection=()
  if [[ -n "$features" ]]; then flags=(--features "$features"); fi
  if [[ "$mode" == ignored ]]; then selection=(--ignored); fi
  phase="$tag-list"
  "${cargo[@]}" test --locked -p "$package" "${flags[@]}" --test "$target" "$case" -- \
    --exact "${selection[@]}" --list 2>&1 | tee "$evidence/$tag-list.log"
  test "$(grep -Fxc -- "$case: test" "$evidence/$tag-list.log" || true)" -eq 1
  phase="$tag-execute"
  "${cargo[@]}" test --locked -p "$package" "${flags[@]}" --test "$target" "$case" -- \
    --exact "${selection[@]}" --nocapture --test-threads=1 2>&1 | tee "$evidence/$tag-test.log"
  test "$(grep -Fxc -- 'running 1 test' "$evidence/$tag-test.log" || true)" -eq 1
  grep -Eq -- '^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured;' "$evidence/$tag-test.log"
  printf 'phase=verified\nexit=0\nlisted=1\nexecuted=1\npassed=1\nignored=0\n' > "$evidence/$tag-status.txt"
}

run_exact native-client-original job native-sandbox-test native_sandbox_observer \
  native_sandbox_lifecycle_fills_and_portfolio_events_reach_retained_stream
run_exact native-client-producer job native-sandbox-test native_sandbox_observer \
  client_bound_observer_uses_actual_native_identity_without_submitting_orders
run_exact native-client-store-replay store '' account_observations \
  client_binding_is_immutable_before_replay_and_preserves_v1_read_values
run_exact native-client-store-legacy store '' account_observations \
  legacy_source_cannot_be_backfilled_with_client_evidence
run_exact native-client-store-race store '' account_observations \
  concurrent_protocols_cannot_mix_the_same_original_source
run_exact native-client-http server native-codex account_observations_http \
  client_bound_http_retains_original_values_rejects_protocol_changes_and_keeps_read_scope
run_exact native-client-http-legacy server native-codex account_observations_http \
  legacy_http_source_cannot_gain_client_evidence_from_a_new_wrapper
run_exact native-client-pipeline server native-codex account_observations_http \
  actual_native_client_observations_reach_http_sql_and_original_readback ignored
phase=verified
