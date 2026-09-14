-- Preserve history; incompatible duplicate versions fail migration, never rewrite.
ALTER TABLE app.handoff_offers ADD COLUMN supersedes_handoff_id app.identity REFERENCES app.handoff_offers;
CREATE UNIQUE INDEX handoff_original_version ON app.handoff_offers(release_id,downstream_id,environment);
DROP TRIGGER identity_revision ON app.handoff_offers;
CREATE TRIGGER identity_revision BEFORE UPDATE ON app.handoff_offers FOR EACH ROW
 EXECUTE FUNCTION app.guard_revision('release_id','approval_id','downstream_id','environment','delivery_sequence','offered_at','expires_at','supersedes_handoff_id');
ALTER TABLE app.operator_command_grants DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants ADD CONSTRAINT operator_command_grants_operation_check CHECK(operation IN (
 'HANDOFF_OFFER','RELEASE_CREATE','RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
 'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE',
 'CREDENTIAL_ISSUE','CREDENTIAL_REVOKE','INPUT_SET_CREATE','EVALUATION_POLICY_CREATE',
 'BRIEF_CREATE','BRIEF_UPDATE','BRIEF_FREEZE','CYCLE_START','INTEGRATION_SECRET_REGISTER',
 'RUNTIME_CREATE','RUNTIME_UPDATE','DOWNSTREAM_CREATE','DOWNSTREAM_UPDATE','RUNTIME_PROBE','DOWNSTREAM_PROBE',
 'DATA_SOURCE_CREATE','DATA_SOURCE_UPDATE','DATA_GRANT_CREATE','DATA_GRANT_REVOKE','DATASET_REGISTER','DATA_VALIDATE',
 'CODEX_PROFILE_CREATE','CODEX_PROFILE_UPDATE','CODEX_PROBE','CODEX_LOGIN_START','CODEX_LOGIN_CANCEL','CODEX_LOGOUT',
 'ALPHA_EVALUATE','MANDATE_CREATE','EXECUTION_ASSUMPTIONS_CREATE','PORTFOLIO_BUILD','PORTFOLIO_SIMULATE'
));
