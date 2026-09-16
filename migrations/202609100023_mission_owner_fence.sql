-- Preserve immutable historical issuances. An unknown historical owner epoch
-- cannot be inferred from the current lease; only new Mission issuances bind it.
ALTER TABLE app.machine_credentials
  ADD COLUMN issuer_owner_epoch app.revision;
ALTER TABLE app.machine_credentials ADD CONSTRAINT credential_owner_requires_attempt
  CHECK (issuer_owner_epoch IS NULL OR issuer_attempt_id IS NOT NULL);

CREATE OR REPLACE FUNCTION app.bind_credential_attempt() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p app.machine_principals; active_attempt uuid; owner_epoch bigint; lease_expires timestamptz;
BEGIN
  SELECT * INTO STRICT p FROM app.machine_principals WHERE id=NEW.principal_id;
  IF p.kind='MISSION' THEN
    PERFORM id FROM app.projects WHERE id=p.project_id FOR SHARE;
    SELECT active_attempt_id INTO STRICT active_attempt FROM app.runs
      WHERE id=p.run_id AND project_id=p.project_id FOR SHARE;
    IF active_attempt IS NULL OR
       (NEW.issuer_attempt_id IS NOT NULL AND NEW.issuer_attempt_id<>active_attempt) THEN
      RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='Mission credential must bind its current Attempt';
    END IF;
    SELECT a.owner_epoch,a.lease_expires_at INTO STRICT owner_epoch,lease_expires
      FROM app.run_attempts a WHERE a.id=active_attempt AND a.run_id=p.run_id FOR SHARE;
    PERFORM id FROM app.machine_principals WHERE id=p.id FOR SHARE;
    -- Read the real clock after every authority lock, including a waited principal.
    IF lease_expires<=clock_timestamp() OR
       (NEW.issuer_owner_epoch IS NOT NULL AND NEW.issuer_owner_epoch<>owner_epoch) THEN
      RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='Mission credential requires its current live owner lease';
    END IF;
    NEW.issuer_attempt_id:=active_attempt;
    NEW.issuer_owner_epoch:=owner_epoch;
  ELSIF NEW.issuer_attempt_id IS NOT NULL OR NEW.issuer_owner_epoch IS NOT NULL THEN
    RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='non-Mission credential cannot bind an Attempt owner';
  END IF;
  RETURN NEW;
END $$;
