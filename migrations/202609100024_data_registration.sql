-- Native registration evidence complements the existing immutable dataset identity.
-- Historical rows are preserved without inventing a native metadata artifact or authority.
CREATE FUNCTION app.guard_data_source_identity() RETURNS trigger LANGUAGE plpgsql AS $native_identity$
BEGIN
 IF NEW.id IS DISTINCT FROM OLD.id
    OR NEW.runtime_id IS DISTINCT FROM OLD.runtime_id
    OR NEW.native_catalog_ref IS DISTINCT FROM OLD.native_catalog_ref
    OR NEW.provider_kind IS DISTINCT FROM OLD.provider_kind THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native source identity is immutable';
 END IF;
 RETURN NEW;
END $native_identity$;
CREATE TRIGGER native_source_identity BEFORE UPDATE ON app.data_sources
 FOR EACH ROW EXECUTE FUNCTION app.guard_data_source_identity();

CREATE TABLE app.dataset_registration_evidence (
 dataset_revision_id app.identity PRIMARY KEY REFERENCES app.dataset_revisions,
 native_metadata_artifact_id app.identity NOT NULL UNIQUE REFERENCES app.artifacts,
 source_revision app.revision NOT NULL,
 runtime_revision app.revision NOT NULL,
 observed_at app.instant NOT NULL,
 created_at app.instant NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.dataset_registration_evidence
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE FUNCTION app.guard_native_dataset_registration() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE valid boolean;
BEGIN
 SELECT m.kind='REPORT' AND m.schema_name='qz.native_catalog_metadata' AND m.schema_version='1'
   AND m.media_type='application/json' AND m.storage_backend='LOCAL' AND m.storage_version='1'
   AND m.storage_object_ref=m.id::text AND m.byte_count BETWEEN 1 AND 1048576
   AND m.project_id IS NULL AND m.producer_run_id IS NULL AND m.producer_attempt_id IS NULL
   AND m.access_class='OPERATOR' AND m.created_by='RUNTIME' AND m.origin=d.origin
   AND q.kind='DATA_QUALITY' AND q.schema_name='qz.data_quality' AND q.schema_version='1'
   AND q.media_type='application/json' AND q.access_class='OPERATOR' AND q.created_by='RUNTIME'
   AND q.storage_backend='LOCAL' AND q.storage_object_ref=q.id::text AND q.storage_version='1'
   AND q.byte_count BETWEEN 1 AND 1049600 AND q.origin=d.origin
   AND s.revision=NEW.source_revision AND r.revision=NEW.runtime_revision
   AND NEW.observed_at<=clock_timestamp() AND NEW.observed_at>=d.created_at-interval '20 seconds'
 INTO valid
 FROM app.dataset_revisions d
 JOIN app.data_sources s ON s.id=d.source_id
 JOIN app.runtime_integrations r ON r.id=s.runtime_id
 JOIN app.artifacts m ON m.id=NEW.native_metadata_artifact_id
 JOIN app.artifacts q ON q.id=d.quality_artifact_id
 WHERE d.id=NEW.dataset_revision_id;
 IF valid IS DISTINCT FROM true THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native dataset registration evidence mismatch';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER native_registration BEFORE INSERT ON app.dataset_registration_evidence
 FOR EACH ROW EXECUTE FUNCTION app.guard_native_dataset_registration();

ALTER TABLE app.operator_command_grants DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants ADD CONSTRAINT operator_command_grants_operation_check CHECK(operation IN (
 'RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
 'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE',
 'CREDENTIAL_ISSUE','CREDENTIAL_REVOKE','INPUT_SET_CREATE','EVALUATION_POLICY_CREATE',
 'BRIEF_CREATE','BRIEF_UPDATE','BRIEF_FREEZE','CYCLE_START','INTEGRATION_SECRET_REGISTER',
 'RUNTIME_CREATE','RUNTIME_UPDATE','DOWNSTREAM_CREATE','DOWNSTREAM_UPDATE','RUNTIME_PROBE',
 'DATA_SOURCE_CREATE','DATA_SOURCE_UPDATE','DATA_GRANT_CREATE','DATA_GRANT_REVOKE','DATASET_REGISTER'
));
CREATE FUNCTION app.guard_data_management_receipt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.operation IN ('DATA_SOURCE_CREATE','DATA_SOURCE_UPDATE','DATA_GRANT_CREATE','DATA_GRANT_REVOKE','DATASET_REGISTER')
    AND NEW.response_nonsecret_body IS NULL THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='data management requires original public response';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER data_management_receipt BEFORE INSERT ON app.command_receipts
 FOR EACH ROW EXECUTE FUNCTION app.guard_data_management_receipt();
