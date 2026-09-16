-- Freeze the fitted model with its original native validation, not a new trial.
-- Historical rows/objects are never backfilled or relabelled as native fits.
CREATE FUNCTION app.guard_native_calibration() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE e app.evaluations; v app.alpha_versions; d app.dataset_revisions;
BEGIN
 SELECT * INTO STRICT e FROM app.evaluations WHERE id=NEW.validation_evaluation_id FOR UPDATE;
 SELECT * INTO STRICT v FROM app.alpha_versions WHERE id=e.subject_alpha_version_id;
 SELECT dataset.* INTO STRICT d FROM app.experiment_validations x
   JOIN app.dataset_revisions dataset ON dataset.id=x.dataset_revision_id
   WHERE x.run_id=e.run_id AND x.alpha_version_id=v.id AND x.policy_id=e.policy_id;
 IF EXISTS(SELECT 1 FROM app.calibrations WHERE validation_evaluation_id=e.id)
   OR EXISTS(SELECT 1 FROM app.evaluation_publications WHERE evaluation_id=e.id)
   OR e.execution_status<>'SUCCEEDED' OR e.evidence_status<>'VALID' OR e.evaluation_kind<>'WALK_FORWARD'
   OR e.valid_until IS NULL OR e.valid_until<=clock_timestamp()
   OR NEW.estimator_kind<>'linregress.affine_ols' OR NEW.estimator_version<>'0.5.4'
   OR NEW.train_input_set_id<>e.input_set_id
   OR NOT EXISTS(SELECT 1 FROM app.input_sets i WHERE i.id=NEW.train_input_set_id
     AND i.purpose='VALIDATION' AND i.frozen_at IS NOT NULL
     AND NEW.fit_end_available_at<=i.decision_cutoff)
   OR NEW.output_unit<>'RETURN_PER_HORIZON' OR NEW.horizon_kind<>'FIXED_BARS'
   OR NEW.horizon_value IS DISTINCT FROM v.horizon_value OR v.horizon_kind<>'FIXED_BARS'
   OR v.signal_kind<>'SCORE' OR v.calibration_id IS NOT NULL
   OR NOT EXISTS(SELECT 1 FROM app.artifacts a JOIN app.runs r ON r.id=e.run_id
     JOIN app.run_attempts attempt ON attempt.id=r.active_attempt_id
     WHERE a.id=NEW.model_artifact_id AND a.project_id=e.project_id AND a.producer_run_id=e.run_id
       AND a.producer_attempt_id=r.active_attempt_id AND attempt.accepted_at IS NOT NULL
       AND attempt.dispatch_state='TERMINAL' AND a.kind='MODEL' AND a.schema_name='qz.alpha_calibration'
       AND a.schema_version='1' AND a.access_class='EVALUATOR_ONLY' AND a.origin=d.origin
       AND a.media_type='application/json' AND a.storage_backend='LOCAL' AND a.storage_object_ref=a.id::text
       AND a.storage_version='1' AND a.created_by='RUNTIME' AND a.byte_count>0) THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='calibration requires its exact original native validation and model';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER native_calibration BEFORE INSERT ON app.calibrations
 FOR EACH ROW EXECUTE FUNCTION app.guard_native_calibration();
