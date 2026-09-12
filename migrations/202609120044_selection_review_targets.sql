-- Preserve the exact native-calibrated target alongside the original validation
-- version. Old immutable selections stay unknown; never infer from active Alpha.
ALTER TABLE app.cycle_selection_trials
 ADD COLUMN review_alpha_version_id app.identity REFERENCES app.alpha_versions,
 ADD CHECK(review_alpha_version_id IS NULL OR selected);

CREATE FUNCTION app.guard_selection_review_target() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.review_alpha_version_id IS NULL THEN RETURN NEW; END IF;
 IF NOT EXISTS(
   SELECT 1 FROM app.evaluations e
   JOIN app.evaluation_publications published ON published.evaluation_id=e.id
   JOIN app.alpha_versions original ON original.id=e.subject_alpha_version_id
   JOIN app.alpha_versions target ON target.id=NEW.review_alpha_version_id
   LEFT JOIN app.calibrations c ON c.id=target.calibration_id
   WHERE e.id=NEW.evaluation_id AND e.evaluation_kind='WALK_FORWARD'
     AND e.execution_status='SUCCEEDED' AND e.evidence_status='VALID' AND e.decision='PASS'
     AND e.valid_until>clock_timestamp() AND original.id=NEW.alpha_version_id
     AND original.experiment_id=NEW.experiment_id
     AND target.alpha_id=original.alpha_id AND target.experiment_id=original.experiment_id
     AND ((original.signal_kind='EXPECTED_RETURN' AND target.id=original.id AND target.calibration_id IS NULL)
       OR (original.signal_kind='SCORE' AND target.version=original.version+1
         AND c.validation_evaluation_id=e.id))
 ) THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='review target must retain the original passed validation output';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER selection_review_target BEFORE INSERT ON app.cycle_selection_trials
 FOR EACH ROW EXECUTE FUNCTION app.guard_selection_review_target();
