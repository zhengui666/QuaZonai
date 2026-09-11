-- Preserve existing issuance history. Legacy NULL Attempt bindings are audit
-- records, not valid Mission authority for any scope; never guess a new binding.
ALTER TABLE app.machine_credentials
  ADD COLUMN issuer_attempt_id app.identity REFERENCES app.run_attempts(id);

CREATE FUNCTION app.bind_credential_attempt() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p app.machine_principals; active_attempt uuid;
BEGIN
  SELECT * INTO STRICT p FROM app.machine_principals WHERE id=NEW.principal_id;
  IF p.kind='MISSION' THEN
    PERFORM id FROM app.projects WHERE id=p.project_id FOR SHARE;
    SELECT active_attempt_id INTO STRICT active_attempt FROM app.runs
      WHERE id=p.run_id AND project_id=p.project_id FOR SHARE;
    PERFORM id FROM app.machine_principals WHERE id=p.id FOR SHARE;
    IF active_attempt IS NULL OR
       (NEW.issuer_attempt_id IS NOT NULL AND NEW.issuer_attempt_id<>active_attempt) THEN
      RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='Mission credential must bind its current Attempt';
    END IF;
    NEW.issuer_attempt_id:=active_attempt;
  ELSIF NEW.issuer_attempt_id IS NOT NULL THEN
    RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='non-Mission credential cannot bind an Attempt';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER issuance_attempt BEFORE INSERT ON app.machine_credentials
FOR EACH ROW EXECUTE FUNCTION app.bind_credential_attempt();

CREATE FUNCTION app.guard_artifact_submit_receipt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.operation='ARTIFACT_SUBMIT' AND (
    NEW.response_nonsecret_body IS NULL OR NEW.normalized_nonsecret_request ? 'content'
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='artifact receipt requires metadata and original public response';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER artifact_submit_receipt BEFORE INSERT ON app.command_receipts
FOR EACH ROW EXECUTE FUNCTION app.guard_artifact_submit_receipt();
