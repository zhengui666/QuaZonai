-- Native probe observations are immutable and independent of configuration CAS.
CREATE TABLE app.runtime_probe_observations (
 id app.identity PRIMARY KEY DEFAULT uuidv7(),
 runtime_id app.identity NOT NULL REFERENCES app.runtime_integrations,
 integration_revision app.revision NOT NULL,
 snapshot_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 observed_at app.instant NOT NULL,
 valid_until app.instant NOT NULL,
 outcome app.document NOT NULL,
 CHECK(valid_until > observed_at AND valid_until <= observed_at + interval '60 seconds'),
 CHECK(coalesce(jsonb_typeof(outcome->'result')='object'
   AND outcome->'result'->>'status' IN ('AVAILABLE','UNAVAILABLE'),false)),
 UNIQUE(snapshot_artifact_id),
 UNIQUE(runtime_id,snapshot_artifact_id)
);
CREATE INDEX runtime_probe_latest ON app.runtime_probe_observations(runtime_id,observed_at DESC,id DESC);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.runtime_probe_observations
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
ALTER TABLE app.runtime_integrations ADD CONSTRAINT runtime_probe_pointer
 FOREIGN KEY(id,last_capability_snapshot_artifact_id)
 REFERENCES app.runtime_probe_observations(runtime_id,snapshot_artifact_id)
 DEFERRABLE INITIALLY DEFERRED NOT VALID;

-- Refreshing observed capability metadata must not invalidate a frozen dispatch
-- configuration. Actual endpoint/credentials/TLS/capability changes still do.
CREATE FUNCTION app.guard_runtime_configuration_revision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (NEW.id,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.created_at) THEN
  RAISE EXCEPTION USING ERRCODE='23000',MESSAGE='immutable Runtime identity';
 END IF;
 IF (to_jsonb(NEW)-ARRAY['revision','updated_at','last_capability_snapshot_artifact_id'])
    IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['revision','updated_at','last_capability_snapshot_artifact_id']) THEN
  NEW.revision:=OLD.revision+1;
  NEW.last_capability_snapshot_artifact_id:=NULL;
 ELSE
  NEW.revision:=OLD.revision;
 END IF;
 NEW.updated_at:=clock_timestamp();
 RETURN NEW;
END $$;
DROP TRIGGER identity_revision ON app.runtime_integrations;
CREATE TRIGGER identity_revision BEFORE UPDATE ON app.runtime_integrations
 FOR EACH ROW EXECUTE FUNCTION app.guard_runtime_configuration_revision();

ALTER TABLE app.operator_command_grants DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants ADD CONSTRAINT operator_command_grants_operation_check CHECK(operation IN (
 'RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
 'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE',
 'CREDENTIAL_ISSUE','CREDENTIAL_REVOKE','INPUT_SET_CREATE','EVALUATION_POLICY_CREATE',
 'BRIEF_CREATE','BRIEF_UPDATE','INTEGRATION_SECRET_REGISTER',
 'RUNTIME_CREATE','RUNTIME_UPDATE','DOWNSTREAM_CREATE','DOWNSTREAM_UPDATE','RUNTIME_PROBE'
));
