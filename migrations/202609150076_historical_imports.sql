ALTER TABLE app.operator_command_grants DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants ADD CONSTRAINT operator_command_grants_operation_check CHECK(operation IN ('MIGRATION_IMPORT',
 'APPROVAL_REVOKE','HANDOFF_OFFER','RELEASE_CREATE','RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
 'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE',
 'CREDENTIAL_ISSUE','CREDENTIAL_REVOKE','INPUT_SET_CREATE','EVALUATION_POLICY_CREATE',
 'BRIEF_CREATE','BRIEF_UPDATE','BRIEF_FREEZE','CYCLE_START','INTEGRATION_SECRET_REGISTER',
 'RUNTIME_CREATE','RUNTIME_UPDATE','DOWNSTREAM_CREATE','DOWNSTREAM_UPDATE','RUNTIME_PROBE','DOWNSTREAM_PROBE',
 'DATA_SOURCE_CREATE','DATA_SOURCE_UPDATE','DATA_GRANT_CREATE','DATA_GRANT_REVOKE','DATASET_REGISTER','DATA_VALIDATE',
 'CODEX_PROFILE_CREATE','CODEX_PROFILE_UPDATE','CODEX_PROBE','CODEX_LOGIN_START','CODEX_LOGIN_CANCEL','CODEX_LOGOUT',
 'ALPHA_EVALUATE','MANDATE_CREATE','EXECUTION_ASSUMPTIONS_CREATE','PORTFOLIO_BUILD','PORTFOLIO_SIMULATE'
));

CREATE TABLE app.historical_import_reports (
 id app.identity PRIMARY KEY,
 export_ref app.identity NOT NULL,
 source_installation_id app.identity NOT NULL,
 dry_run boolean NOT NULL,
 source_report app.document NOT NULL,
 result app.document NOT NULL,
 created_at app.instant NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.historical_import_reports
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE TABLE app.historical_records (
 id app.identity PRIMARY KEY DEFAULT uuidv7(),
 source_installation_id app.identity NOT NULL,
 source_table app.nonempty NOT NULL,
 original_key jsonb NOT NULL CHECK(jsonb_typeof(original_key)='object' AND original_key<>'{}'::jsonb),
 fields jsonb NOT NULL CHECK(jsonb_typeof(fields)='object'),
 first_import_id app.identity NOT NULL REFERENCES app.historical_import_reports DEFERRABLE INITIALLY DEFERRED,
 disposition text NOT NULL CHECK(disposition IN ('READ_ONLY_HISTORY','LEGACY_REVALIDATION_REQUIRED')),
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(source_installation_id,source_table,original_key)
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.historical_records
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE TABLE app.historical_import_members (
 report_id app.identity NOT NULL REFERENCES app.historical_import_reports DEFERRABLE INITIALLY DEFERRED,
 record_id app.identity NOT NULL REFERENCES app.historical_records,
 PRIMARY KEY(report_id,record_id)
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.historical_import_members
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
