-- A feature's immutable source relationship complements the existing source,
-- license, Dataset and native metadata registry. It never expands license scope.
-- Only a checked same-source paired attachment can be registered in this slice.
CREATE TABLE app.feature_artifact_sources (
 artifact_id app.identity PRIMARY KEY REFERENCES app.artifacts,
 project_id app.identity NOT NULL REFERENCES app.projects,
 dataset_revision_id app.identity NOT NULL REFERENCES app.dataset_registration_evidence(dataset_revision_id),
 native_metadata_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 feature_part_key text NOT NULL CHECK (
   length(feature_part_key) BETWEEN 1 AND 120
   AND feature_part_key ~ '^[A-Za-z0-9][A-Za-z0-9._-]{0,119}$'
 ),
 content_sha256 text NOT NULL CHECK (
   length(content_sha256)=64 AND content_sha256 ~ '^[0-9a-f]{64}$'
 ),
 UNIQUE(project_id,dataset_revision_id,feature_part_key)
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.feature_artifact_sources
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();

-- Byte content and the descriptor hash are checked against original buffers by
-- Store. SQL enforces the immutable identity and the artifact publication shape.
-- No current license gate is added to historical reads or exports.
CREATE FUNCTION app.guard_feature_artifact_source() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE valid boolean;
BEGIN
 SELECT a.project_id=NEW.project_id
   AND a.kind='PARAMETERS' AND a.schema_name='qz.feature_observations' AND a.schema_version='1'
   AND a.media_type='application/json' AND a.access_class='RESEARCH' AND a.created_by='IMPORT'
   AND a.storage_backend='LOCAL' AND a.storage_object_ref=a.id::text AND a.storage_version='1'
   AND a.byte_count BETWEEN 1 AND 2097152
   AND a.producer_run_id IS NULL AND a.producer_attempt_id IS NULL
   AND a.origin=d.origin AND d.partition_role<>'SEALED'
   AND e.native_metadata_artifact_id=NEW.native_metadata_artifact_id
 INTO valid
 FROM app.artifacts a
 JOIN app.dataset_revisions d ON d.id=NEW.dataset_revision_id
 JOIN app.dataset_registration_evidence e ON e.dataset_revision_id=d.id
 WHERE a.id=NEW.artifact_id;
 IF valid IS DISTINCT FROM true THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='recorded feature source binding mismatch';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER feature_source_binding BEFORE INSERT ON app.feature_artifact_sources
 FOR EACH ROW EXECUTE FUNCTION app.guard_feature_artifact_source();

-- Publishing this new schema requires its source relationship in the same
-- transaction. Ordinary legacy PARAMETERS artifacts retain their old contract.
CREATE FUNCTION app.require_feature_artifact_source() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS (SELECT 1 FROM app.feature_artifact_sources WHERE artifact_id=NEW.id) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='recorded feature artifact requires source binding';
 END IF;
 RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER feature_source_required AFTER INSERT ON app.artifacts
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
 WHEN (NEW.schema_name='qz.feature_observations')
 EXECUTE FUNCTION app.require_feature_artifact_source();

ALTER TABLE app.operator_command_grants DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants ADD CONSTRAINT operator_command_grants_operation_check CHECK(operation IN (
 'EXPERIMENT_ADOPT_ALPHA','CYCLE_START_EXTERNAL','CYCLE_FINISH_EXTERNAL','EXPERIMENT_EVALUATE','MIGRATION_IMPORT',
 'APPROVAL_REVOKE','HANDOFF_OFFER','RELEASE_CREATE','RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
 'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE','CREDENTIAL_ISSUE','CREDENTIAL_REVOKE',
 'INPUT_SET_CREATE','EVALUATION_POLICY_CREATE','BRIEF_CREATE','BRIEF_UPDATE','BRIEF_FREEZE','CYCLE_START','INTEGRATION_SECRET_REGISTER',
 'RUNTIME_CREATE','RUNTIME_UPDATE','DOWNSTREAM_CREATE','DOWNSTREAM_UPDATE','RUNTIME_PROBE','DOWNSTREAM_PROBE',
 'RECORDED_FEATURE_REGISTER','DATA_SOURCE_CREATE','DATA_SOURCE_UPDATE','DATA_GRANT_CREATE','DATA_GRANT_REVOKE','DATASET_REGISTER','DATA_VALIDATE',
 'CODEX_PROFILE_CREATE','CODEX_PROFILE_UPDATE','CODEX_PROBE','CODEX_LOGIN_START','CODEX_LOGIN_CANCEL','CODEX_LOGOUT',
 'ALPHA_EVALUATE','MANDATE_CREATE','EXECUTION_ASSUMPTIONS_CREATE','PORTFOLIO_BUILD','PORTFOLIO_SIMULATE'
));

CREATE OR REPLACE FUNCTION app.guard_data_management_receipt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.operation IN ('DATA_SOURCE_CREATE','DATA_SOURCE_UPDATE','DATA_GRANT_CREATE','DATA_GRANT_REVOKE','DATASET_REGISTER','RECORDED_FEATURE_REGISTER')
    AND NEW.response_nonsecret_body IS NULL THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='data management requires original public response';
 END IF;
 RETURN NEW;
END $$;
