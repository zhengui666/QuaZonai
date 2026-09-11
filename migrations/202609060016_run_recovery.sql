-- Preserve every already-applied migration. Management tasks have an explicit
-- NULL Cycle identity; they never create a counterfeit research budget.
ALTER TABLE app.run_admissions ALTER COLUMN cycle_id DROP NOT NULL;
ALTER TABLE app.run_admissions ADD CONSTRAINT admission_run_project
  FOREIGN KEY(run_id,project_id) REFERENCES app.runs(id,project_id);
CREATE UNIQUE INDEX admission_standalone_key ON app.run_admissions(project_id,command_key)
  WHERE cycle_id IS NULL;
CREATE INDEX run_standalone_activity ON app.runs(project_id,state) WHERE cycle_id IS NULL;

CREATE FUNCTION app.guard_run_admission_identity() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE r app.runs;
BEGIN
  SELECT * INTO STRICT r FROM app.runs WHERE id=NEW.run_id FOR UPDATE;
  IF NEW.project_id IS DISTINCT FROM r.project_id
     OR NEW.cycle_id IS DISTINCT FROM r.cycle_id
     OR (NEW.cycle_id IS NULL AND
         (r.kind NOT IN ('IMPORT','EXPORT','DATA_VALIDATE')
          OR NEW.limits->>'experiments' IS DISTINCT FROM '0')) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='run admission must bind the exact project and optional cycle';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER admission_identity BEFORE INSERT ON app.run_admissions
  FOR EACH ROW EXECUTE FUNCTION app.guard_run_admission_identity();

-- The owning Run, not accepted_at (which may legitimately be NULL), freezes
-- every attempt field. This shares the Store's parent-before-child lock order.
CREATE FUNCTION app.guard_terminal_run_attempt() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE parent_id uuid; parent_state text;
BEGIN
  parent_id := CASE WHEN TG_OP='DELETE' THEN OLD.run_id ELSE NEW.run_id END;
  SELECT state INTO STRICT parent_state FROM app.runs WHERE id=parent_id FOR UPDATE;
  IF parent_state IN ('SUCCEEDED','FAILED','CANCELLED') THEN
    RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='terminal run attempts are immutable';
  END IF;
  IF TG_OP='DELETE' THEN RETURN OLD; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER terminal_attempt BEFORE INSERT OR UPDATE OR DELETE ON app.run_attempts
  FOR EACH ROW EXECUTE FUNCTION app.guard_terminal_run_attempt();
