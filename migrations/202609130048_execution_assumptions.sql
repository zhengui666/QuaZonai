-- Preserve historical assumptions. Only explicitly bound native sources enter the new API.
CREATE TABLE app.execution_assumption_sources (
 assumptions_id app.identity PRIMARY KEY REFERENCES app.execution_assumptions,
 project_id app.identity NOT NULL REFERENCES app.projects,
 input_set_id app.identity NOT NULL,
 dataset_revision_id app.identity NOT NULL REFERENCES app.dataset_revisions,
 runtime_id app.identity NOT NULL REFERENCES app.runtime_integrations,
 capability_snapshot_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 settings app.document NOT NULL,
 FOREIGN KEY(input_set_id,project_id) REFERENCES app.input_sets(id,project_id)
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.execution_assumption_sources
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();

ALTER TABLE app.operator_command_grants DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants ADD CONSTRAINT operator_command_grants_operation_check CHECK(operation IN (
 'RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
 'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE',
 'CREDENTIAL_ISSUE','CREDENTIAL_REVOKE','INPUT_SET_CREATE','EVALUATION_POLICY_CREATE',
 'BRIEF_CREATE','BRIEF_UPDATE','BRIEF_FREEZE','CYCLE_START','INTEGRATION_SECRET_REGISTER',
 'RUNTIME_CREATE','RUNTIME_UPDATE','DOWNSTREAM_CREATE','DOWNSTREAM_UPDATE','RUNTIME_PROBE',
 'DATA_SOURCE_CREATE','DATA_SOURCE_UPDATE','DATA_GRANT_CREATE','DATA_GRANT_REVOKE','DATASET_REGISTER','DATA_VALIDATE',
 'CODEX_PROFILE_CREATE','CODEX_PROFILE_UPDATE','CODEX_PROBE','CODEX_LOGIN_START','CODEX_LOGIN_CANCEL','CODEX_LOGOUT',
 'ALPHA_EVALUATE','MANDATE_CREATE','EXECUTION_ASSUMPTIONS_CREATE'
));
