-- A calibrated version extends its original Alpha. It never changes the trial
-- or borrows an unrelated evaluation, and never backfills historical versions.
CREATE FUNCTION app.guard_calibrated_alpha_version() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE source app.alpha_versions;
BEGIN
 IF NEW.calibration_id IS NULL THEN RETURN NEW; END IF;
 SELECT v.* INTO STRICT source FROM app.calibrations c
   JOIN app.evaluations e ON e.id=c.validation_evaluation_id
   JOIN app.evaluation_publications p ON p.evaluation_id=e.id
   JOIN app.alpha_versions v ON v.id=e.subject_alpha_version_id
   JOIN app.artifacts model ON model.id=c.model_artifact_id
   WHERE c.id=NEW.calibration_id AND c.estimator_kind='linregress.affine_ols'
     AND c.estimator_version='0.5.4' AND e.execution_status='SUCCEEDED' AND e.evidence_status='VALID'
     AND model.kind='MODEL' AND model.schema_name='qz.alpha_calibration' AND model.schema_version='1'
     AND model.producer_run_id=e.run_id AND model.project_id=e.project_id;
 PERFORM id FROM app.alphas WHERE id=source.alpha_id FOR UPDATE;
 IF source.calibration_id IS NOT NULL OR source.signal_kind<>'SCORE'
   OR NEW.version<>source.version+1
   OR ROW(NEW.project_id,NEW.alpha_id,NEW.experiment_id,NEW.root_lineage_id,NEW.code_artifact_id,
     NEW.model_artifact_id,NEW.signal_contract_version,NEW.signal_kind,NEW.horizon_kind,
     NEW.horizon_value,NEW.forecast_unit,NEW.runtime_image_ref)
     IS DISTINCT FROM ROW(source.project_id,source.alpha_id,source.experiment_id,source.root_lineage_id,
     source.code_artifact_id,source.model_artifact_id,source.signal_contract_version,source.signal_kind,
     source.horizon_kind,source.horizon_value,source.forecast_unit,source.runtime_image_ref)
   OR EXISTS(SELECT 1 FROM app.alpha_versions WHERE calibration_id=NEW.calibration_id) THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='calibrated version must retain its exact original Alpha and trial';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER calibrated_alpha_version BEFORE INSERT ON app.alpha_versions
 FOR EACH ROW EXECUTE FUNCTION app.guard_calibrated_alpha_version();
