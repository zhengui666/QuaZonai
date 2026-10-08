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
