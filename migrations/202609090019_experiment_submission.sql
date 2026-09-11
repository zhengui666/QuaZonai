-- Keep the proposal's author separate from the experiment's scientific Run.
-- Historical experiments remain without an invented author record.
CREATE TABLE app.experiment_authorship (
  experiment_id app.identity PRIMARY KEY,
  project_id app.identity NOT NULL,
  actor_kind text NOT NULL CHECK (actor_kind IN ('OPERATOR','CLI','MISSION')),
  credential_id app.identity REFERENCES app.machine_credentials,
  author_run_id app.identity,
  author_attempt_id app.identity,
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  FOREIGN KEY (experiment_id,project_id) REFERENCES app.experiments(id,project_id),
  FOREIGN KEY (author_run_id,project_id) REFERENCES app.runs(id,project_id),
  FOREIGN KEY (author_attempt_id,author_run_id) REFERENCES app.run_attempts(id,run_id),
  CHECK (
    (actor_kind='OPERATOR' AND credential_id IS NULL AND author_run_id IS NULL AND author_attempt_id IS NULL)
    OR (actor_kind='CLI' AND credential_id IS NOT NULL AND author_run_id IS NULL AND author_attempt_id IS NULL)
    OR (actor_kind='MISSION' AND credential_id IS NOT NULL AND author_run_id IS NOT NULL AND author_attempt_id IS NOT NULL)
  )
);
CREATE INDEX experiment_authorship_by_mission
  ON app.experiment_authorship(author_run_id,experiment_id) WHERE author_run_id IS NOT NULL;
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.experiment_authorship
  FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE FUNCTION app.guard_experiment_author() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE proposal app.experiments%ROWTYPE;
BEGIN
  SELECT * INTO STRICT proposal FROM app.experiments WHERE id=NEW.experiment_id;
  IF NEW.actor_kind='OPERATOR' AND proposal.trial_source<>'OPERATOR' THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='experiment author/source mismatch';
  END IF;
  IF NEW.actor_kind='CLI' AND (
    proposal.trial_source<>'OPERATOR' OR NOT EXISTS (
      SELECT 1 FROM app.machine_credentials c JOIN app.machine_principals p ON p.id=c.principal_id
      WHERE c.id=NEW.credential_id AND p.kind='CLI' AND p.project_id=NEW.project_id
    )
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='experiment CLI author mismatch';
  END IF;
  IF NEW.actor_kind='MISSION' AND (
    proposal.trial_source<>'CODEX' OR NOT EXISTS (
      SELECT 1 FROM app.machine_credentials c
      JOIN app.machine_principals p ON p.id=c.principal_id
      JOIN app.runs r ON r.id=p.run_id AND r.project_id=p.project_id
      WHERE c.id=NEW.credential_id AND p.kind='MISSION' AND p.project_id=NEW.project_id
        AND p.run_id=NEW.author_run_id AND c.issuer_attempt_id=NEW.author_attempt_id
        AND r.cycle_id=proposal.cycle_id AND r.kind='AGENT_RESEARCH'
    )
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='experiment Mission author mismatch';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER exact_author BEFORE INSERT ON app.experiment_authorship
  FOR EACH ROW EXECUTE FUNCTION app.guard_experiment_author();

CREATE FUNCTION app.guard_experiment_receipt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.operation='EXPERIMENT_PROPOSE' AND (
    NEW.response_status<>201 OR NEW.response_nonsecret_body IS NULL
    OR NOT EXISTS(SELECT 1 FROM app.experiment_authorship a WHERE a.experiment_id=NEW.resource_id)
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='experiment receipt requires its published author';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER experiment_receipt BEFORE INSERT ON app.command_receipts
  FOR EACH ROW EXECUTE FUNCTION app.guard_experiment_receipt();
