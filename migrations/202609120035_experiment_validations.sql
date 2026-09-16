-- One continuation of the original paid trial, never a second queue or verdict.
CREATE TABLE app.experiment_validations (
 experiment_id app.identity PRIMARY KEY REFERENCES app.experiment_forecasts(experiment_id),
 run_id app.identity NOT NULL UNIQUE REFERENCES app.run_native_tasks(run_id),
 alpha_version_id app.identity NOT NULL UNIQUE REFERENCES app.alpha_versions,
 policy_id app.identity NOT NULL REFERENCES app.evaluation_policies,
 dataset_revision_id app.identity NOT NULL REFERENCES app.dataset_revisions,
 created_at app.instant NOT NULL DEFAULT clock_timestamp()
);
CREATE FUNCTION app.guard_experiment_validation() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE e app.experiments; r app.runs; t app.run_native_tasks; v app.alpha_versions;
BEGIN
 SELECT * INTO STRICT e FROM app.experiments WHERE id=NEW.experiment_id FOR UPDATE;
 SELECT * INTO STRICT r FROM app.runs WHERE id=NEW.run_id;
 SELECT * INTO STRICT t FROM app.run_native_tasks WHERE run_id=r.id;
 SELECT * INTO STRICT v FROM app.alpha_versions WHERE id=NEW.alpha_version_id;
 IF e.outcome<>'PENDING' OR r.state<>'QUEUED' OR r.kind<>'ALPHA_EVALUATE'
    OR r.project_id<>e.project_id OR r.cycle_id IS DISTINCT FROM e.cycle_id
    OR v.project_id<>e.project_id OR v.experiment_id<>e.id
    OR t.access_class<>'EVALUATOR_ONLY' OR t.image_ref<>v.runtime_image_ref
    OR NOT EXISTS(SELECT 1 FROM app.research_cycles c
      JOIN app.research_briefs b ON b.id=c.brief_id AND b.state='FROZEN'
      JOIN app.brief_execution_contexts x ON x.brief_id=b.id
      JOIN app.execution_assumptions a ON a.id=b.execution_assumptions_id
      WHERE c.id=e.cycle_id AND b.evaluation_policy_id=NEW.policy_id
        AND x.validation_input_set_id=r.input_set_id AND a.engine_image_ref=t.image_ref)
    OR (SELECT count(*) FROM app.input_set_items WHERE input_set_id=r.input_set_id AND dataset_revision_id IS NOT NULL)<>1
    OR NOT EXISTS(SELECT 1 FROM app.input_set_items i JOIN app.dataset_revisions d ON d.id=i.dataset_revision_id
      WHERE i.input_set_id=r.input_set_id AND d.id=NEW.dataset_revision_id
        AND i.role='VALIDATION' AND d.partition_role='VALIDATION' AND d.origin=t.origin)
    OR NOT EXISTS(SELECT 1 FROM app.experiment_compilations c
      JOIN app.run_admissions compiled ON compiled.run_id=c.compile_run_id AND compiled.limits->>'experiments'='1'
      JOIN app.experiment_forecasts f ON f.experiment_id=c.experiment_id AND f.model_artifact_id=v.model_artifact_id
      JOIN app.runs predicted ON predicted.id=f.run_id AND predicted.id=e.run_id AND predicted.state='SUCCEEDED'
      JOIN app.run_attempts a ON a.id=predicted.active_attempt_id AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL
      JOIN app.run_terminal_receipts receipt ON receipt.run_id=predicted.id AND receipt.attempt_id=a.id AND receipt.terminal_state='SUCCEEDED'
      JOIN app.command_receipts created ON created.resource_id=v.id
        AND created.principal_scope='MISSION:'||c.mission_run_id::text
        AND created.operation='RESEARCH_ALPHA_CREATE' AND created.idempotency_key=e.id::text
      WHERE c.experiment_id=e.id)
    OR NOT EXISTS(SELECT 1 FROM app.run_admissions WHERE run_id=r.id AND limits->>'experiments'='0')
    OR jsonb_array_length(t.input_bindings)<>3
    OR NOT t.input_bindings @> jsonb_build_array(jsonb_build_object('kind','DATASET','revision_id',NEW.dataset_revision_id,'role','VALIDATION'))
    OR NOT t.input_bindings @> jsonb_build_array(jsonb_build_object('kind','ARTIFACT','artifact_id',v.model_artifact_id,'role','MODEL'))
    OR NOT t.input_bindings @> jsonb_build_array(jsonb_build_object('kind','ARTIFACT','artifact_id',t.parameters_artifact_id,'role','PARAMETERS'))
    OR t.output_schemas IS DISTINCT FROM '[{"name":"qz.alpha_validation","version":"1"}]'::jsonb THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='validation requires the original paid trial, Alpha, policy and complete frozen Validation input';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER validation BEFORE INSERT ON app.experiment_validations
 FOR EACH ROW EXECUTE FUNCTION app.guard_experiment_validation();
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.experiment_validations
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
