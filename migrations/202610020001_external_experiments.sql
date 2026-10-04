-- External research uses the same frozen cycle, charged trial, Runs and PGMQ.
-- These immutable producer edges are not another scheduler or Agent authority.
ALTER TABLE app.operator_command_grants DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants ADD CONSTRAINT operator_command_grants_operation_check CHECK(operation IN (
 'CYCLE_START_EXTERNAL','CYCLE_FINISH_EXTERNAL','EXPERIMENT_EVALUATE','MIGRATION_IMPORT',
 'APPROVAL_REVOKE','HANDOFF_OFFER','RELEASE_CREATE','RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
 'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE',
 'CREDENTIAL_ISSUE','CREDENTIAL_REVOKE','INPUT_SET_CREATE','EVALUATION_POLICY_CREATE',
 'BRIEF_CREATE','BRIEF_UPDATE','BRIEF_FREEZE','CYCLE_START','INTEGRATION_SECRET_REGISTER',
 'RUNTIME_CREATE','RUNTIME_UPDATE','DOWNSTREAM_CREATE','DOWNSTREAM_UPDATE','RUNTIME_PROBE','DOWNSTREAM_PROBE',
 'DATA_SOURCE_CREATE','DATA_SOURCE_UPDATE','DATA_GRANT_CREATE','DATA_GRANT_REVOKE','DATASET_REGISTER','DATA_VALIDATE',
 'CODEX_PROFILE_CREATE','CODEX_PROFILE_UPDATE','CODEX_PROBE','CODEX_LOGIN_START','CODEX_LOGIN_CANCEL','CODEX_LOGOUT',
 'ALPHA_EVALUATE','MANDATE_CREATE','EXECUTION_ASSUMPTIONS_CREATE','PORTFOLIO_BUILD','PORTFOLIO_SIMULATE'
));
CREATE TABLE app.external_research_cycles (
 cycle_id app.identity PRIMARY KEY REFERENCES app.research_cycles,
 created_at app.instant NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.external_research_cycles FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE TABLE app.external_experiment_requests (
 experiment_id app.identity PRIMARY KEY REFERENCES app.experiments,
 compile_run_id app.identity NOT NULL UNIQUE REFERENCES app.run_native_tasks,
 dataset_revision_id app.identity NOT NULL REFERENCES app.dataset_revisions,
 feature_artifact_ids jsonb NOT NULL CHECK(jsonb_typeof(feature_artifact_ids)='array' AND jsonb_array_length(feature_artifact_ids) BETWEEN 1 AND 16),
 request app.document NOT NULL,
 evaluation_request app.document NOT NULL,
 created_at app.instant NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.external_experiment_requests FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE TABLE app.external_experiment_tasks (
 experiment_id app.identity PRIMARY KEY REFERENCES app.external_experiment_requests,
 run_id app.identity NOT NULL UNIQUE REFERENCES app.run_native_tasks,
 model_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 created_at app.instant NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.external_experiment_tasks FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE TABLE app.external_experiment_results (
 experiment_id app.identity PRIMARY KEY REFERENCES app.external_experiment_requests,
 run_id app.identity NOT NULL UNIQUE REFERENCES app.runs,
 report_artifact_id app.identity UNIQUE REFERENCES app.artifacts,
 reason app.nonempty NOT NULL,
 created_at app.instant NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.external_experiment_results FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE FUNCTION app.guard_external_experiment_request() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE e app.experiments; r app.runs; n app.run_native_tasks;
BEGIN
 SELECT * INTO STRICT e FROM app.experiments WHERE id=NEW.experiment_id FOR UPDATE;
 SELECT * INTO STRICT r FROM app.runs WHERE id=NEW.compile_run_id;
 SELECT * INTO STRICT n FROM app.run_native_tasks WHERE run_id=r.id;
 IF e.outcome<>'PENDING' OR e.run_id IS NOT NULL OR e.code_artifact_id IS NULL
    OR NOT EXISTS(SELECT 1 FROM app.external_research_cycles WHERE cycle_id=e.cycle_id)
    OR NOT EXISTS(SELECT 1 FROM app.experiment_authorship WHERE experiment_id=e.id AND actor_kind IN ('OPERATOR','CLI'))
    OR r.project_id<>e.project_id OR r.cycle_id IS DISTINCT FROM e.cycle_id
    OR r.kind<>'DATA_VALIDATE' OR r.state<>'QUEUED' OR n.access_class<>'RESEARCH'
    OR NOT EXISTS(SELECT 1 FROM app.run_admissions WHERE run_id=r.id AND limits->>'experiments'='1')
    OR jsonb_array_length(n.input_bindings)<>2
    OR NOT n.input_bindings @> jsonb_build_array(jsonb_build_object('kind','ARTIFACT','artifact_id',e.code_artifact_id,'role','CODE'))
    OR NOT n.input_bindings @> jsonb_build_array(jsonb_build_object('kind','ARTIFACT','artifact_id',n.parameters_artifact_id,'role','PARAMETERS'))
    OR EXISTS(SELECT 1 FROM jsonb_array_elements_text(NEW.feature_artifact_ids) feature(id) WHERE NOT EXISTS(SELECT 1 FROM app.artifacts WHERE id=feature.id::uuid AND project_id=e.project_id AND kind='PARAMETERS' AND access_class='RESEARCH')) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='external experiment requires its owned paid compilation and immutable inputs';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER external_request BEFORE INSERT ON app.external_experiment_requests FOR EACH ROW EXECUTE FUNCTION app.guard_external_experiment_request();
CREATE FUNCTION app.guard_external_experiment_task() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE e app.experiments; r app.runs; n app.run_native_tasks; original app.external_experiment_requests;
BEGIN
 SELECT * INTO STRICT e FROM app.experiments WHERE id=NEW.experiment_id FOR UPDATE;
 SELECT * INTO STRICT original FROM app.external_experiment_requests WHERE experiment_id=e.id;
 SELECT * INTO STRICT r FROM app.runs WHERE id=NEW.run_id;
 SELECT * INTO STRICT n FROM app.run_native_tasks WHERE run_id=r.id;
 IF e.outcome<>'PENDING' OR e.run_id IS DISTINCT FROM original.compile_run_id
    OR r.project_id<>e.project_id OR r.cycle_id IS DISTINCT FROM e.cycle_id OR r.kind<>'PORTFOLIO_SIMULATE' OR r.state<>'QUEUED'
    OR n.access_class<>'RESEARCH' OR NOT EXISTS(SELECT 1 FROM app.run_admissions WHERE run_id=r.id AND limits->>'experiments'='0')
    OR NOT EXISTS(SELECT 1 FROM app.research_cycles c JOIN app.brief_execution_contexts x ON x.brief_id=c.brief_id WHERE c.id=e.cycle_id AND x.validation_input_set_id=r.input_set_id)
    OR NOT EXISTS(SELECT 1 FROM app.runs compiled JOIN app.run_terminal_receipts receipt ON receipt.run_id=compiled.id AND receipt.attempt_id=compiled.active_attempt_id AND receipt.terminal_state='SUCCEEDED' JOIN app.run_attempts a ON a.id=receipt.attempt_id AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL JOIN app.run_native_outputs o ON o.attempt_id=a.id JOIN app.artifacts model ON model.id=o.artifact_id WHERE compiled.id=original.compile_run_id AND compiled.state='SUCCEEDED' AND model.id=NEW.model_artifact_id AND model.kind='MODEL' AND model.schema_name='qz.wasm_model' AND model.producer_run_id=compiled.id AND model.producer_attempt_id=a.id)
    OR jsonb_array_length(n.input_bindings)<>3+jsonb_array_length(original.feature_artifact_ids)
    OR NOT n.input_bindings @> jsonb_build_array(jsonb_build_object('kind','DATASET','revision_id',original.dataset_revision_id,'role','VALIDATION'))
    OR NOT n.input_bindings @> jsonb_build_array(jsonb_build_object('kind','ARTIFACT','artifact_id',NEW.model_artifact_id,'role','MODEL'))
    OR EXISTS(SELECT 1 FROM jsonb_array_elements_text(original.feature_artifact_ids) feature(id) WHERE NOT n.input_bindings @> jsonb_build_array(jsonb_build_object('kind','ARTIFACT','artifact_id',feature.id::uuid,'role','PARAMETERS')))
    OR NOT n.input_bindings @> jsonb_build_array(jsonb_build_object('kind','ARTIFACT','artifact_id',n.parameters_artifact_id,'role','PARAMETERS'))
    OR n.output_schemas IS DISTINCT FROM '[{"name":"qz.experiment_evaluation","version":"1"}]'::jsonb THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='external evaluation must retain its accepted compiler, original trial and validation inputs';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER external_task BEFORE INSERT ON app.external_experiment_tasks FOR EACH ROW EXECUTE FUNCTION app.guard_external_experiment_task();
CREATE FUNCTION app.guard_external_experiment_result() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE e app.experiments; r app.runs;
BEGIN
 SELECT * INTO STRICT e FROM app.experiments WHERE id=NEW.experiment_id FOR UPDATE;
 SELECT * INTO STRICT r FROM app.runs WHERE id=NEW.run_id;
 IF e.outcome<>'PENDING' OR e.run_id IS DISTINCT FROM r.id OR r.project_id<>e.project_id
    OR r.state NOT IN ('SUCCEEDED','FAILED','CANCELLED')
    OR NOT EXISTS(SELECT 1 FROM app.run_terminal_receipts WHERE run_id=r.id AND terminal_state=r.state)
    OR (NEW.report_artifact_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM app.external_experiment_tasks t JOIN app.run_attempts a ON a.id=r.active_attempt_id AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL JOIN app.run_native_outputs o ON o.attempt_id=a.id JOIN app.artifacts report ON report.id=o.artifact_id WHERE t.experiment_id=e.id AND t.run_id=r.id AND r.state='SUCCEEDED' AND report.id=NEW.report_artifact_id AND report.producer_run_id=r.id AND report.producer_attempt_id=a.id AND report.schema_name='qz.experiment_evaluation' AND report.schema_version='1' AND report.access_class='RESEARCH')) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='external result requires its original terminal receipt and native report';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER external_result BEFORE INSERT ON app.external_experiment_results FOR EACH ROW EXECUTE FUNCTION app.guard_external_experiment_result();
CREATE FUNCTION app.guard_external_experiment() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM app.external_experiment_requests WHERE experiment_id=OLD.id) THEN
  IF ROW(NEW.code_artifact_id,NEW.parameter_artifact_id) IS DISTINCT FROM ROW(OLD.code_artifact_id,OLD.parameter_artifact_id)
     OR (NEW.run_id IS DISTINCT FROM OLD.run_id AND NOT (OLD.run_id IS NULL AND EXISTS(SELECT 1 FROM app.external_experiment_requests WHERE experiment_id=OLD.id AND compile_run_id=NEW.run_id)) AND NOT EXISTS(SELECT 1 FROM app.external_experiment_tasks WHERE experiment_id=OLD.id AND run_id=NEW.run_id)) THEN
   RAISE EXCEPTION USING ERRCODE='23000',MESSAGE='external experiment inputs and native producer are immutable';
  END IF;
  IF ROW(NEW.outcome,NEW.outcome_reason,NEW.conclusion_artifact_id) IS DISTINCT FROM ROW(OLD.outcome,OLD.outcome_reason,OLD.conclusion_artifact_id)
     AND NOT EXISTS(SELECT 1 FROM app.external_experiment_results WHERE experiment_id=OLD.id AND run_id=NEW.run_id AND reason=NEW.outcome_reason AND report_artifact_id IS NOT DISTINCT FROM NEW.conclusion_artifact_id AND NEW.outcome='INCONCLUSIVE') THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='external metrics do not establish hypothesis support or qualification';
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER external_experiment BEFORE UPDATE ON app.experiments FOR EACH ROW EXECUTE FUNCTION app.guard_external_experiment();
