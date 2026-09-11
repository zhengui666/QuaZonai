-- Native Codex profile observations are configuration evidence, not research,
-- OAuth tokens or a second canonical conversation database. Old rows are retained.
CREATE FUNCTION app.guard_codex_profile_home() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.profile_origin IS DISTINCT FROM OLD.profile_origin THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native profile origin is immutable';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER codex_profile_home BEFORE UPDATE ON app.codex_profiles
 FOR EACH ROW EXECUTE FUNCTION app.guard_codex_profile_home();

CREATE TABLE app.codex_profile_observations (
 id app.identity PRIMARY KEY DEFAULT uuidv7(),
 profile_id app.identity NOT NULL REFERENCES app.codex_profiles,
 profile_revision app.revision NOT NULL,
 observed_at app.instant NOT NULL,
 valid_until app.instant NOT NULL,
 outcome app.document NOT NULL,
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 CHECK(valid_until>observed_at AND valid_until<=observed_at+interval '60 seconds'),
 CHECK(jsonb_typeof(outcome->'result')='object'
   AND (outcome->'result'->>'status') IN ('AVAILABLE','UNAVAILABLE'))
);
CREATE INDEX codex_profile_observations_latest ON app.codex_profile_observations
 (profile_id,profile_revision,observed_at DESC,id DESC);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.codex_profile_observations
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE FUNCTION app.guard_codex_profile_observation() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE actual bigint;
BEGIN
 SELECT revision INTO actual FROM app.codex_profiles WHERE id=NEW.profile_id FOR SHARE;
 IF actual IS DISTINCT FROM NEW.profile_revision OR NEW.observed_at>clock_timestamp()
    OR NEW.valid_until<=clock_timestamp() THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native profile observation revision or time mismatch';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER codex_profile_observation BEFORE INSERT ON app.codex_profile_observations
 FOR EACH ROW EXECUTE FUNCTION app.guard_codex_profile_observation();

ALTER TABLE app.operator_command_grants DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants ADD CONSTRAINT operator_command_grants_operation_check CHECK(operation IN (
 'RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
 'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE',
 'CREDENTIAL_ISSUE','CREDENTIAL_REVOKE','INPUT_SET_CREATE','EVALUATION_POLICY_CREATE',
 'BRIEF_CREATE','BRIEF_UPDATE','BRIEF_FREEZE','CYCLE_START','INTEGRATION_SECRET_REGISTER',
 'RUNTIME_CREATE','RUNTIME_UPDATE','DOWNSTREAM_CREATE','DOWNSTREAM_UPDATE','RUNTIME_PROBE',
 'DATA_SOURCE_CREATE','DATA_SOURCE_UPDATE','DATA_GRANT_CREATE','DATA_GRANT_REVOKE','DATASET_REGISTER','DATA_VALIDATE',
 'CODEX_PROFILE_CREATE','CODEX_PROFILE_UPDATE','CODEX_PROBE'
));
CREATE FUNCTION app.guard_codex_profile_receipt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.operation IN ('CODEX_PROFILE_CREATE','CODEX_PROFILE_UPDATE','CODEX_PROBE')
    AND NEW.response_nonsecret_body IS NULL THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='Codex settings require the original nonsecret response';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER codex_profile_receipt BEFORE INSERT ON app.command_receipts
 FOR EACH ROW EXECUTE FUNCTION app.guard_codex_profile_receipt();
