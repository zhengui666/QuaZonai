-- Preserve old approval history; never infer a missing authorization binding.
ALTER TABLE app.approvals
 ADD COLUMN downstream_revision app.revision,
 ADD COLUMN decision_ordinal integer CHECK(decision_ordinal>=0),
 ADD COLUMN readiness_observation_id app.identity REFERENCES app.downstream_probe_observations,
 ADD CONSTRAINT approval_admission_complete CHECK(
  (downstream_revision IS NULL AND decision_ordinal IS NULL AND readiness_observation_id IS NULL)
  OR (downstream_revision IS NOT NULL AND decision_ordinal IS NOT NULL AND readiness_observation_id IS NOT NULL));
CREATE FUNCTION app.guard_approval_admission_binding() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.readiness_observation_id IS NOT NULL AND NOT EXISTS (
  SELECT 1 FROM app.downstream_probe_observations p
  WHERE p.id=NEW.readiness_observation_id AND p.downstream_id=NEW.downstream_id
   AND p.integration_revision=NEW.downstream_revision
 ) THEN
  RAISE EXCEPTION USING ERRCODE='23503',MESSAGE='approval observation must belong to its exact downstream revision';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER approval_admission_binding BEFORE INSERT ON app.approvals
 FOR EACH ROW EXECUTE FUNCTION app.guard_approval_admission_binding();
