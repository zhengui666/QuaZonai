-- Add only new closed operator operations. Applied migration bytes and history
-- remain intact. Full request and public response use the existing receipts.
ALTER TABLE app.operator_command_grants DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants ADD CONSTRAINT operator_command_grants_operation_check
CHECK(operation IN ('RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
 'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE','CREDENTIAL_ISSUE','CREDENTIAL_REVOKE',
 'INPUT_SET_CREATE','EVALUATION_POLICY_CREATE'));

CREATE FUNCTION app.guard_research_receipt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.operation IN ('INPUT_SET_CREATE','EVALUATION_POLICY_CREATE') AND NEW.response_nonsecret_body IS NULL THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='research command requires its original public response';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER research_receipt BEFORE INSERT ON app.command_receipts
FOR EACH ROW EXECUTE FUNCTION app.guard_research_receipt();

-- A shared grant lock held by preparation/admission serializes with revocation.
-- Read effective revocations again AFTER any lock wait; presence of an InputSet
-- never authorizes a later execution to bypass this current-grant check.
CREATE FUNCTION app.lock_data_use_revocation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM id FROM app.data_use_grants WHERE id=NEW.grant_id FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='23503',MESSAGE='data use grant is not registered'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER revocation_serialized BEFORE INSERT ON app.data_use_revocations
FOR EACH ROW EXECUTE FUNCTION app.lock_data_use_revocation();
CREATE INDEX input_sets_by_project_id ON app.input_sets(project_id,id DESC);
CREATE INDEX evaluation_policies_by_project_id ON app.evaluation_policies(project_id,id DESC);
CREATE INDEX data_use_revocations_effective ON app.data_use_revocations(grant_id,effective_at);

-- Runtime and downstream have zero extra bindings; native TG_ARGV is NULL then.
CREATE OR REPLACE FUNCTION app.guard_revision() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE key text;
BEGIN
  IF NEW.id IS DISTINCT FROM OLD.id OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
    RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='immutable record identity';
  END IF;
  FOREACH key IN ARRAY COALESCE(TG_ARGV, ARRAY[]::text[]) LOOP
    IF to_jsonb(NEW)->key IS DISTINCT FROM to_jsonb(OLD)->key THEN
      RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='immutable domain binding';
    END IF;
  END LOOP;
  NEW.revision := OLD.revision + 1;
  NEW.updated_at := clock_timestamp();
  RETURN NEW;
END $$;
