-- A formal native validation has one completed evaluation, including failures.
-- Do not rewrite historical records or add a second queue/publication state.
CREATE FUNCTION app.guard_native_alpha_evaluation() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE v app.experiment_validations; r app.runs;
BEGIN
 SELECT * INTO v FROM app.experiment_validations WHERE run_id=NEW.run_id FOR UPDATE;
 IF FOUND THEN
  -- Serialize on the original formal continuation, not unrelated evaluations.
  IF EXISTS(SELECT 1 FROM app.evaluations WHERE run_id=v.run_id) THEN
   RAISE EXCEPTION USING ERRCODE='23505', MESSAGE='formal validation already has an evaluation';
  END IF;
  SELECT * INTO STRICT r FROM app.runs WHERE id=v.run_id;
  IF NEW.subject_alpha_version_id IS DISTINCT FROM v.alpha_version_id
    OR NEW.policy_id<>v.policy_id OR NEW.input_set_id<>r.input_set_id
    OR NEW.project_id<>r.project_id OR NEW.evaluation_kind<>'WALK_FORWARD'
    OR NEW.execution_status<>r.state
    OR NOT EXISTS(SELECT 1 FROM app.run_terminal_receipts t
      WHERE t.run_id=r.id AND t.terminal_state=r.state
        AND t.attempt_id IS NOT DISTINCT FROM r.active_attempt_id)
    OR NOT EXISTS(SELECT 1 FROM app.artifacts a
      WHERE a.id=NEW.report_artifact_id AND a.id=NEW.method_versions_artifact_id
        AND a.schema_name='qz.alpha_evaluation' AND a.schema_version='1'
        AND a.producer_attempt_id IS NOT DISTINCT FROM r.active_attempt_id
        AND a.access_class='EVALUATOR_ONLY') THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='formal validation evaluation requires its exact terminal producer and frozen context';
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER native_alpha_evaluation BEFORE INSERT ON app.evaluations
 FOR EACH ROW EXECUTE FUNCTION app.guard_native_alpha_evaluation();
