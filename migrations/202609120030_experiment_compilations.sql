-- One concrete producer edge, not another task state machine.
ALTER TABLE app.experiments ADD CONSTRAINT experiment_cycle_identity UNIQUE(id,project_id,cycle_id);
CREATE TABLE app.experiment_compilations (
 experiment_id app.identity PRIMARY KEY,
 project_id app.identity NOT NULL,
 cycle_id app.identity NOT NULL,
 mission_run_id app.identity NOT NULL REFERENCES app.run_missions(run_id),
 compile_run_id app.identity NOT NULL UNIQUE REFERENCES app.run_native_tasks(run_id),
 code_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 parameter_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(experiment_id,project_id,cycle_id) REFERENCES app.experiments(id,project_id,cycle_id),
 FOREIGN KEY(mission_run_id,project_id,cycle_id) REFERENCES app.runs(id,project_id,cycle_id),
 FOREIGN KEY(compile_run_id,project_id,cycle_id) REFERENCES app.runs(id,project_id,cycle_id)
);
CREATE FUNCTION app.guard_experiment_compilation() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE e app.experiments; t app.run_native_tasks;
BEGIN
 SELECT * INTO STRICT e FROM app.experiments WHERE id=NEW.experiment_id FOR UPDATE;
 SELECT * INTO STRICT t FROM app.run_native_tasks WHERE run_id=NEW.compile_run_id;
 IF e.outcome<>'PENDING' OR e.run_id IS NOT NULL
    OR e.code_artifact_id IS DISTINCT FROM NEW.code_artifact_id
    OR e.parameter_artifact_id IS DISTINCT FROM NEW.parameter_artifact_id
    OR NOT EXISTS(SELECT 1 FROM app.experiment_authorship WHERE experiment_id=e.id)
    OR NOT EXISTS(SELECT 1 FROM app.run_missions WHERE run_id=NEW.mission_run_id AND role='RESEARCHER')
    OR NOT EXISTS(SELECT 1 FROM app.runs WHERE id=NEW.compile_run_id AND kind='DATA_VALIDATE')
    OR t.access_class<>'RESEARCH' OR t.origin<>'SYNTHETIC'
    OR jsonb_array_length(t.input_bindings)<>2
    OR NOT t.input_bindings @> jsonb_build_array(jsonb_build_object('kind','ARTIFACT','artifact_id',NEW.code_artifact_id,'role','CODE'))
    OR NOT t.input_bindings @> jsonb_build_array(jsonb_build_object('kind','ARTIFACT','artifact_id',t.parameters_artifact_id,'role','PARAMETERS'))
    OR t.output_schemas IS DISTINCT FROM '[{"name":"qz.wasm_model","version":"1"},{"name":"qz.model_compilation","version":"1"}]'::jsonb THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='compilation must retain the exact proposal and native producer';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER compilation BEFORE INSERT ON app.experiment_compilations
 FOR EACH ROW EXECUTE FUNCTION app.guard_experiment_compilation();
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.experiment_compilations
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE FUNCTION app.guard_compiling_experiment() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF ROW(NEW.code_artifact_id,NEW.parameter_artifact_id) IS DISTINCT FROM ROW(OLD.code_artifact_id,OLD.parameter_artifact_id)
    AND EXISTS(SELECT 1 FROM app.experiment_compilations WHERE experiment_id=OLD.id) THEN
  RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='compiled experiment inputs are immutable';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER compiling_experiment BEFORE UPDATE ON app.experiments
 FOR EACH ROW EXECUTE FUNCTION app.guard_compiling_experiment();
