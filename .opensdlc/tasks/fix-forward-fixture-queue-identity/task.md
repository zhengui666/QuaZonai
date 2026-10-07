# Allocate distinct native queue identities for Forward fixtures

## Failure and scope

PR 176, head cf1416f0284ce16374126deed2b80d84640c0bba, CI run 37578114286 / store-postgres job 112651460625: 11 of the 12 new PostgreSQL cases passed. The remaining migration_103_to_104_preserves_existing_receipt_and_rejects_new_legacy_bypass case failed while creating its second relational source fixture. PostgreSQL returned 23505 for run_admissions_initial_queue_message_id_key because tests/support/forward.rs used 100000 for every source admission.

The original 103 receipt, real migration to 104 and unchanged replay assertions precede this failure. This failure does not establish a business migration defect.

## Minimal correction

Use nextval(pg_get_serial_sequence('pgmq.q_runs','msg_id')) only in the terminal source fixture. This atomically reserves from the same native queue identity sequence as normal PGMQ inserts. It neither invents a separate ID allocator nor adds pending work for a historical terminal fixture. A reserved sequence gap is deliberate. PostgreSQL's unique/positive identity constraints stay installed.

Keep both source fixtures and every original receipt/replay/bypass assertion. Add an explicit assertion that the two source admissions own distinct identities. No production code, migration, thresholds, execution budgets, receipt contents or CI selector is changed.

## Verification

Original masked CI job log inspected. The isolated correction is based on the frozen V5 tree; its archived bytes are untouched. Rust syntax parsing is performed without Cargo. The exclusive Cargo window belongs to another candidate. Real PostgreSQL/PGMQ execution remains required on the corrected PR head; source inspection or compilation is not reported as a PostgreSQL pass.
