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

source "$(dirname "${BASH_SOURCE[0]}")/exact_regression_case.sh"

failed=0
run_case issuance-old-owner server test artifacts_http '' mission_attempt::attempt_bound_legacy_credential_cannot_borrow_the_current_owner_on_upgrade || failed=$((failed+1))
run_case issuance-old-unbound server test artifacts_http '' mission_attempt::legacy_unbound_mission_is_preserved_but_requires_new_issuance_after_upgrade || failed=$((failed+1))
run_case issuance-doctor store test authority_invariants '' doctor_upgrade_requires_explicit_revocation_and_preserves_original_issuance || failed=$((failed+1))
run_case durable-owner server test durable_http '' owner_browser_and_device_required_before_any_upstream_call || failed=$((failed+1))
run_case durable-relay server test durable_http '' original_duplicates_and_native_snapshots_are_relayed_without_local_state || failed=$((failed+1))
run_case rebalance-release-retry store test experiment_compilations '' qualified_portfolio::automatic_rebalance::frozen_policy_rebalance_queues_one_original_bounded_build || failed=$((failed+1))
printf 'cases=6\nfailed=%s\n' "$failed" > "$evidence/release-regression-combined-status.txt"
if ((failed != 0)); then exit 1; fi
