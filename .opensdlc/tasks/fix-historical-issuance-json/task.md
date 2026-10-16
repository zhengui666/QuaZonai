# Preserve exact historical issuance assertions after the lease column migration

## Failure and source

Release workflow 37582752653 at dev 3018f10b189fb912fea93da20840bc7eada0cdc1 failed three migration tests in store job 112672750127. The two Server artifacts_http cases and the Store authority_invariants case compare PostgreSQL to_jsonb of original machine_credentials rows before and after the actual committed migrations. The new schema includes lease_bound=false, while their expected JSON accounts only for older nullable provenance additions.

Migration 202610060101 explicitly adds lease_bound boolean NOT NULL DEFAULT false. Existing finite credentials must retain this false value; only newly issued, exact-owner, no-deadline Mission credentials can use renewable lease authority. The original failure log shows the new false key as the sole JSON difference.

## Minimal correction

In each affected old-schema fixture, first assert that lease_bound is absent. After the real upgrade, add only Value::Bool(false) to the complete expected row, separately assert the migrated value is false, and retain the original whole-row equality. The first Server case also has a later old-CLI comparison; apply the same explicit expected addition there rather than waiting for another failure.

Preserve old Mission rejection, old CLI survival, current exact-owner issuance, the Doctor migration's required explicit revocation, and every existing scope/epoch/expiry assertion. No field is removed from a comparison. No production serializer, authorization rule, migration, stored record or receipt is changed.

## Verification

Source inspection covers every machine_credentials to_jsonb migration comparison in Server and Store tests. The independent source copy starts from the exact dev 3018f tree, not the newer four-slice PR 177. Rust syntax parsing and minimal patch replay are checked without Cargo while another candidate owns the build window. The corrected release head must execute all three real PostgreSQL/HTTP migration tests; static review is not a runtime pass.
