-- Native downstream observations are immutable, ordered by probe start rather
-- than response completion. No configuration pointer or revision mutation needed.
CREATE TABLE app.downstream_probe_observations (
 id app.identity PRIMARY KEY DEFAULT uuidv7(),
 downstream_id app.identity NOT NULL REFERENCES app.downstream_integrations,
 integration_revision app.revision NOT NULL,
 snapshot_artifact_id app.identity NOT NULL UNIQUE REFERENCES app.artifacts,
 started_at app.instant NOT NULL,
 observed_at app.instant NOT NULL,
 valid_until app.instant NOT NULL,
 outcome app.document NOT NULL,
 CHECK(observed_at >= started_at AND observed_at <= started_at + interval '20 seconds'),
 CHECK(valid_until = started_at + interval '60 seconds'),
 CHECK(coalesce(jsonb_typeof(outcome->'result')='object'
   AND outcome->'result'->>'status' IN ('AVAILABLE','UNAVAILABLE'),false))
);
CREATE INDEX downstream_probe_latest ON app.downstream_probe_observations(downstream_id,started_at DESC,id DESC);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.downstream_probe_observations
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();

ALTER TABLE app.operator_command_grants DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants ADD CONSTRAINT operator_command_grants_operation_check CHECK(operation IN (
 'RELEASE_CREATE','RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
 'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE',
 'CREDENTIAL_ISSUE','CREDENTIAL_REVOKE','INPUT_SET_CREATE','EVALUATION_POLICY_CREATE',
 'BRIEF_CREATE','BRIEF_UPDATE','BRIEF_FREEZE','CYCLE_START','INTEGRATION_SECRET_REGISTER',
 'RUNTIME_CREATE','RUNTIME_UPDATE','DOWNSTREAM_CREATE','DOWNSTREAM_UPDATE','RUNTIME_PROBE','DOWNSTREAM_PROBE',
 'DATA_SOURCE_CREATE','DATA_SOURCE_UPDATE','DATA_GRANT_CREATE','DATA_GRANT_REVOKE','DATASET_REGISTER','DATA_VALIDATE',
 'CODEX_PROFILE_CREATE','CODEX_PROFILE_UPDATE','CODEX_PROBE','CODEX_LOGIN_START','CODEX_LOGIN_CANCEL','CODEX_LOGOUT',
 'ALPHA_EVALUATE','MANDATE_CREATE','EXECUTION_ASSUMPTIONS_CREATE','PORTFOLIO_BUILD','PORTFOLIO_SIMULATE'
));
