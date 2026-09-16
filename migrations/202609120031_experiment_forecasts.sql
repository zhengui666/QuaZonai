-- Discovery prediction binds the accepted compiler producer to one scientific Run.
CREATE TABLE app.experiment_forecasts (
 experiment_id app.identity PRIMARY KEY REFERENCES app.experiment_compilations(experiment_id),
 run_id app.identity NOT NULL UNIQUE REFERENCES app.run_native_tasks(run_id),
 model_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 dataset_revision_id app.identity NOT NULL REFERENCES app.dataset_revisions,
 created_at app.instant NOT NULL DEFAULT clock_timestamp()
);
CREATE FUNCTION app.guard_experiment_forecast() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE e app.experiments; c app.experiment_compilations; r app.runs; t app.run_native_tasks;
BEGIN
 SELECT * INTO STRICT e FROM app.experiments WHERE id=NEW.experiment_id FOR UPDATE;
 SELECT * INTO STRICT c FROM app.experiment_compilations WHERE experiment_id=e.id;
 SELECT * INTO STRICT r FROM app.runs WHERE id=NEW.run_id;
 SELECT * INTO STRICT t FROM app.run_native_tasks WHERE run_id=r.id;
 IF e.outcome<>'PENDING' OR e.run_id IS NOT NULL
    OR r.project_id<>e.project_id OR r.cycle_id IS DISTINCT FROM e.cycle_id
    OR r.kind<>'ALPHA_EVALUATE' OR t.access_class<>'RESEARCH'
    OR NOT EXISTS(SELECT 1 FROM app.research_cycles y JOIN app.brief_execution_contexts b ON b.brief_id=y.brief_id WHERE y.id=e.cycle_id AND b.discovery_input_set_id=r.input_set_id)
    OR NOT EXISTS(SELECT 1 FROM app.input_set_items i JOIN app.dataset_revisions d ON d.id=i.dataset_revision_id WHERE i.input_set_id=r.input_set_id AND d.id=NEW.dataset_revision_id AND i.role='DISCOVERY' AND d.partition_role='DISCOVERY' AND d.origin=t.origin)
    OR NOT EXISTS(SELECT 1 FROM app.runs compiler
      JOIN app.run_attempts a ON a.id=compiler.active_attempt_id AND a.run_id=compiler.id
      JOIN app.run_terminal_receipts receipt ON receipt.run_id=compiler.id AND receipt.attempt_id=a.id
      JOIN app.run_native_outputs o ON o.attempt_id=a.id
      JOIN app.artifacts model ON model.id=o.artifact_id
      WHERE compiler.id=c.compile_run_id AND compiler.state='SUCCEEDED' AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL
        AND receipt.terminal_state='SUCCEEDED' AND model.id=NEW.model_artifact_id
        AND model.producer_run_id=compiler.id AND model.producer_attempt_id=a.id
        AND model.kind='MODEL' AND model.schema_name='qz.wasm_model' AND model.schema_version='1' AND model.access_class='RESEARCH')
    OR jsonb_array_length(t.input_bindings)<>3
    OR NOT t.input_bindings @> jsonb_build_array(jsonb_build_object('kind','DATASET','revision_id',NEW.dataset_revision_id,'role','DISCOVERY'))
    OR NOT t.input_bindings @> jsonb_build_array(jsonb_build_object('kind','ARTIFACT','artifact_id',NEW.model_artifact_id,'role','MODEL'))
    OR NOT t.input_bindings @> jsonb_build_array(jsonb_build_object('kind','ARTIFACT','artifact_id',t.parameters_artifact_id,'role','PARAMETERS'))
    OR t.output_schemas IS DISTINCT FROM '[{"name":"qz.native_forecast","version":"1"}]'::jsonb THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='forecast requires the original accepted model and frozen Discovery inputs';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER forecast BEFORE INSERT ON app.experiment_forecasts
 FOR EACH ROW EXECUTE FUNCTION app.guard_experiment_forecast();
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.experiment_forecasts
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE FUNCTION app.guard_forecast_experiment_run() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM app.experiment_forecasts WHERE experiment_id=OLD.id AND run_id IS DISTINCT FROM NEW.run_id) THEN
  RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='forecast experiment Run is immutable';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER forecast_experiment_run BEFORE UPDATE ON app.experiments
 FOR EACH ROW EXECUTE FUNCTION app.guard_forecast_experiment_run();
CREATE FUNCTION app.require_forecast_experiment_run() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM app.experiments WHERE id=NEW.experiment_id AND run_id=NEW.run_id) THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='forecast and experiment Run must commit together';
 END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER forecast_experiment_publication AFTER INSERT ON app.experiment_forecasts
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION app.require_forecast_experiment_run();
