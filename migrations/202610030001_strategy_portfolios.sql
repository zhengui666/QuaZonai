-- Add weight strategies to the existing aggregates. Legacy forecast rows keep
-- their meaning; no qualification, evaluation PASS or observed balance is made.
ALTER TABLE app.alpha_versions
 ADD COLUMN output_kind text NOT NULL DEFAULT 'FORECAST' CHECK(output_kind IN ('FORECAST','TARGET_WEIGHT')),
 ADD COLUMN strategy_policy app.document,
 ADD COLUMN source_evaluation_run_id app.identity REFERENCES app.runs,
 ADD COLUMN source_accepted_attempt_id app.identity REFERENCES app.run_attempts,
 ADD COLUMN source_report_artifact_id app.identity REFERENCES app.artifacts,
 ALTER COLUMN signal_kind DROP NOT NULL,
 ALTER COLUMN horizon_kind DROP NOT NULL,
 ALTER COLUMN forecast_unit DROP NOT NULL;
ALTER TABLE app.alpha_versions ADD CONSTRAINT alpha_output_shape CHECK (
 (output_kind='FORECAST' AND signal_kind IS NOT NULL AND horizon_kind IS NOT NULL
  AND forecast_unit IS NOT NULL AND strategy_policy IS NULL
  AND source_evaluation_run_id IS NULL AND source_accepted_attempt_id IS NULL
  AND source_report_artifact_id IS NULL)
 OR
 (output_kind='TARGET_WEIGHT' AND signal_kind IS NULL AND horizon_kind IS NULL
  AND horizon_value IS NULL AND forecast_unit IS NULL AND calibration_id IS NULL
  AND model_artifact_id IS NOT NULL AND strategy_policy IS NOT NULL
  AND source_evaluation_run_id IS NOT NULL AND source_accepted_attempt_id IS NOT NULL
  AND source_report_artifact_id IS NOT NULL));
CREATE UNIQUE INDEX strategy_alpha_source_once ON app.alpha_versions
 (experiment_id,source_report_artifact_id) WHERE output_kind='TARGET_WEIGHT';

CREATE FUNCTION app.guard_strategy_alpha_source() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.output_kind='TARGET_WEIGHT' AND NOT EXISTS (
  SELECT 1 FROM app.external_experiment_results result
  JOIN app.external_experiment_tasks task ON task.experiment_id=result.experiment_id AND task.run_id=result.run_id
  JOIN app.runs r ON r.id=result.run_id AND r.project_id=NEW.project_id AND r.state='SUCCEEDED'
  JOIN app.run_attempts attempt ON attempt.id=r.active_attempt_id AND attempt.dispatch_state='TERMINAL' AND attempt.accepted_at IS NOT NULL
  JOIN app.run_terminal_receipts terminal ON terminal.run_id=r.id AND terminal.attempt_id=attempt.id AND terminal.terminal_state='SUCCEEDED'
  JOIN app.run_native_outputs output ON output.attempt_id=attempt.id AND output.artifact_id=result.report_artifact_id
  JOIN app.artifacts report ON report.id=output.artifact_id AND report.project_id=NEW.project_id
   AND report.producer_run_id=r.id AND report.producer_attempt_id=attempt.id
   AND report.kind='REPORT' AND report.schema_name='qz.experiment_evaluation' AND report.schema_version='1'
  WHERE result.experiment_id=NEW.experiment_id
   AND result.run_id=NEW.source_evaluation_run_id AND attempt.id=NEW.source_accepted_attempt_id
   AND result.report_artifact_id=NEW.source_report_artifact_id AND task.model_artifact_id=NEW.model_artifact_id
   AND NEW.strategy_policy->>'output_kind'='TARGET_WEIGHT'
   AND NEW.strategy_policy->>'model_artifact_id'=NEW.model_artifact_id::text
   AND NEW.strategy_policy->>'code_artifact_id'=NEW.code_artifact_id::text
   AND NEW.strategy_policy->'source'->>'experiment_id'=NEW.experiment_id::text
   AND NEW.strategy_policy->'source'->>'evaluation_run_id'=r.id::text
   AND NEW.strategy_policy->'source'->>'accepted_attempt_id'=attempt.id::text
   AND NEW.strategy_policy->'source'->>'report_artifact_id'=report.id::text
 ) THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='strategy alpha requires its accepted external model and report';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER strategy_alpha_source BEFORE INSERT ON app.alpha_versions
 FOR EACH ROW EXECUTE FUNCTION app.guard_strategy_alpha_source();

ALTER TABLE app.portfolio_mandates
 ADD COLUMN allocation_method text NOT NULL DEFAULT 'NATIVE_OPTIMIZER' CHECK(allocation_method IN ('NATIVE_OPTIMIZER','FIXED_TARGET_WEIGHTS')),
 ADD COLUMN strategy_content app.document,
 ALTER COLUMN objective DROP NOT NULL,
 ALTER COLUMN risk_measure DROP NOT NULL,
 ALTER COLUMN covariance_estimator DROP NOT NULL,
 ALTER COLUMN alpha_ensemble DROP NOT NULL,
 ALTER COLUMN optimizer DROP NOT NULL,
 ALTER COLUMN required_evaluation_policy_id DROP NOT NULL;
ALTER TABLE app.portfolio_mandates ADD CONSTRAINT mandate_allocation_shape CHECK (
 (allocation_method='NATIVE_OPTIMIZER' AND strategy_content IS NULL
  AND objective IS NOT NULL AND risk_measure IS NOT NULL AND covariance_estimator IS NOT NULL
  AND alpha_ensemble IS NOT NULL AND optimizer IS NOT NULL AND required_evaluation_policy_id IS NOT NULL)
 OR
 (allocation_method='FIXED_TARGET_WEIGHTS' AND strategy_content IS NOT NULL
  AND objective IS NULL AND risk_measure IS NULL AND covariance_estimator IS NULL
  AND alpha_ensemble IS NULL AND optimizer IS NULL AND required_evaluation_policy_id IS NULL));

ALTER TABLE app.portfolio_build_tasks
 ADD COLUMN source_kind text NOT NULL DEFAULT 'FORECAST' CHECK(source_kind IN ('FORECAST','STRATEGY_ALPHA')),
 ADD COLUMN purpose text CHECK(purpose IN ('HISTORICAL_REPLAY','CURRENT_DECISION')),
 DROP CONSTRAINT one_original_weight_source;
ALTER TABLE app.portfolio_build_tasks ADD CONSTRAINT portfolio_build_source_shape CHECK (
 (source_kind='FORECAST' AND purpose IS NULL
  AND (snapshot_id IS NOT NULL)::integer+(last_target_candidate_id IS NOT NULL)::integer=1)
 OR
 (source_kind='STRATEGY_ALPHA' AND purpose IS NOT NULL
  AND snapshot_id IS NULL AND last_target_candidate_id IS NULL
  AND (request->>'source_kind'='STRATEGY_ALPHA') IS TRUE
  AND (request->'purpose'->>'purpose'=purpose) IS TRUE));

ALTER TABLE app.portfolio_candidates
 ADD COLUMN source_kind text NOT NULL DEFAULT 'FORECAST' CHECK(source_kind IN ('FORECAST','STRATEGY_ALPHA')),
 ADD COLUMN purpose text CHECK(purpose IN ('HISTORICAL_REPLAY','CURRENT_DECISION')),
 ADD COLUMN strategy_detail app.document,
 ALTER COLUMN solver_status DROP NOT NULL;
ALTER TABLE app.portfolio_candidates ADD CONSTRAINT candidate_source_shape CHECK (
 (source_kind='FORECAST' AND purpose IS NULL AND solver_status IS NOT NULL AND strategy_detail IS NULL)
 OR
 (source_kind='STRATEGY_ALPHA' AND purpose IS NOT NULL AND solver_status IS NULL AND strategy_detail IS NOT NULL
  AND forecast_artifact_id IS NULL AND covariance_artifact_id IS NULL AND allocation_evaluation_id IS NULL
  AND current_weights_source='NONE' AND current_weights_artifact_id IS NULL
  AND target_artifact_id IS NOT NULL AND cash_weight IS NOT NULL));

ALTER TABLE app.candidate_alphas
 ADD COLUMN source_kind text NOT NULL DEFAULT 'FORECAST' CHECK(source_kind IN ('FORECAST','STRATEGY_ALPHA')),
 ALTER COLUMN qualification_id DROP NOT NULL,
 ALTER COLUMN forecast_unit DROP NOT NULL;
ALTER TABLE app.candidate_alphas ADD CONSTRAINT candidate_alpha_source_shape CHECK (
 (source_kind='FORECAST' AND qualification_id IS NOT NULL AND forecast_unit IS NOT NULL)
 OR (source_kind='STRATEGY_ALPHA' AND qualification_id IS NULL AND calibration_id IS NULL AND forecast_unit IS NULL));


-- A member uses the same output branch as its parent candidate and actual Alpha.
-- Preserve the original publication lock and same-project membership invariant.
CREATE OR REPLACE FUNCTION app.guard_candidate_member() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE owner app.identity; candidate_source text;
BEGIN
  SELECT project_id,source_kind INTO owner,candidate_source
  FROM app.portfolio_candidates WHERE id=NEW.candidate_id FOR UPDATE;
  IF owner IS NULL THEN
    RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='candidate must exist before its members';
  END IF;
  IF EXISTS(SELECT 1 FROM app.candidate_publications WHERE candidate_id=NEW.candidate_id) THEN
    RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='published candidate membership is immutable';
  END IF;
  IF TG_TABLE_NAME='candidate_alphas' THEN
    IF NOT EXISTS(SELECT 1 FROM app.alpha_versions WHERE id=NEW.alpha_version_id AND project_id=owner) THEN
      RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='candidate alpha must belong to the same project';
    END IF;
    IF NEW.source_kind<>candidate_source OR NOT EXISTS (
      SELECT 1 FROM app.alpha_versions WHERE id=NEW.alpha_version_id
      AND output_kind=CASE NEW.source_kind WHEN 'FORECAST' THEN 'FORECAST' ELSE 'TARGET_WEIGHT' END
    ) THEN
      RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='candidate member output branch must match its candidate and alpha';
    END IF;
  END IF;
  RETURN NEW;
END $$;

ALTER TABLE app.releases
 ADD COLUMN source_kind text NOT NULL DEFAULT 'FORECAST_EVALUATION' CHECK(source_kind IN ('FORECAST_EVALUATION','NATIVE_TARGET_DECISION')),
 ADD COLUMN decision_run_id app.identity REFERENCES app.runs,
 ADD COLUMN decision_attempt_id app.identity REFERENCES app.run_attempts,
 ADD COLUMN decision_report_artifact_id app.identity REFERENCES app.artifacts,
 ADD COLUMN execution_environment text CHECK(execution_environment='PAPER'),
 ALTER COLUMN evaluation_id DROP NOT NULL;
ALTER TABLE app.releases ADD CONSTRAINT release_source_shape CHECK (
 (source_kind='FORECAST_EVALUATION' AND evaluation_id IS NOT NULL
  AND decision_run_id IS NULL AND decision_attempt_id IS NULL AND decision_report_artifact_id IS NULL
  AND execution_environment IS NULL AND package_schema_version='1')
 OR
 (source_kind='NATIVE_TARGET_DECISION' AND evaluation_id IS NULL
  AND decision_run_id IS NOT NULL AND decision_attempt_id IS NOT NULL AND decision_report_artifact_id IS NOT NULL
  AND execution_environment IS NOT NULL AND execution_environment='PAPER' AND package_schema_version='2'));

CREATE FUNCTION app.strategy_candidate_producer_valid(candidate uuid, producer uuid, attempt uuid, report uuid)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS (
  SELECT 1 FROM app.portfolio_candidates c
  JOIN app.portfolio_build_tasks task ON task.run_id=c.run_id AND task.mandate_id=c.mandate_id
   AND task.source_kind='STRATEGY_ALPHA' AND task.purpose=c.purpose
  JOIN app.runs r ON r.id=c.run_id AND r.project_id=c.project_id AND r.state='SUCCEEDED'
  JOIN app.run_attempts a ON a.id=r.active_attempt_id AND a.run_id=r.id AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL
  JOIN app.run_terminal_receipts terminal ON terminal.run_id=r.id AND terminal.attempt_id=a.id AND terminal.terminal_state='SUCCEEDED'
  JOIN app.run_native_outputs output ON output.attempt_id=a.id AND output.artifact_id=c.diagnostics_artifact_id
  JOIN app.artifacts original ON original.id=output.artifact_id AND original.project_id=c.project_id
   AND original.producer_run_id=r.id AND original.producer_attempt_id=a.id
   AND original.kind='REPORT' AND original.schema_name='qz.strategy_portfolio' AND original.schema_version='1'
  WHERE c.id=candidate AND c.source_kind='STRATEGY_ALPHA' AND c.purpose='CURRENT_DECISION'
   AND r.id=producer AND a.id=attempt AND original.id=report
 )
$$;
CREATE FUNCTION app.guard_strategy_release_source() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.source_kind='NATIVE_TARGET_DECISION' AND NOT app.strategy_candidate_producer_valid(
  NEW.candidate_id,NEW.decision_run_id,NEW.decision_attempt_id,NEW.decision_report_artifact_id) THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='strategy release requires its accepted current decision';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER strategy_release_source BEFORE INSERT ON app.releases
 FOR EACH ROW EXECUTE FUNCTION app.guard_strategy_release_source();

CREATE OR REPLACE FUNCTION app.approval_evidence_valid(release uuid, inputs uuid)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS (
  SELECT 1 FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id
  JOIN app.evaluations e ON e.id=r.evaluation_id AND e.subject_candidate_id=c.id
  JOIN app.input_sets i ON i.id=inputs AND i.project_id=c.project_id
  WHERE r.id=release AND r.source_kind='FORECAST_EVALUATION'
   AND i.frozen_at IS NOT NULL AND i.purpose IN ('PORTFOLIO','FORWARD')
   AND EXISTS(SELECT 1 FROM app.input_set_items x WHERE x.input_set_id=i.id AND x.artifact_id=e.report_artifact_id)
   AND EXISTS(SELECT 1 FROM app.input_set_items x WHERE x.input_set_id=i.id AND x.artifact_id=e.method_versions_artifact_id)
 ) OR EXISTS (
  SELECT 1 FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id
  JOIN app.input_sets i ON i.id=inputs AND i.project_id=c.project_id
  WHERE r.id=release AND r.source_kind='NATIVE_TARGET_DECISION' AND r.execution_environment='PAPER'
   AND i.frozen_at IS NOT NULL AND i.purpose IN ('PORTFOLIO','FORWARD')
   AND app.strategy_candidate_producer_valid(c.id,r.decision_run_id,r.decision_attempt_id,r.decision_report_artifact_id)
   AND EXISTS(SELECT 1 FROM app.input_set_items x WHERE x.input_set_id=i.id AND x.artifact_id=r.decision_report_artifact_id)
 )
$$;

CREATE OR REPLACE FUNCTION app.release_package_valid(candidate uuid, package uuid, version text, environment text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS (
  SELECT 1 FROM app.portfolio_candidates c JOIN app.artifacts a ON a.id=package
  WHERE c.id=candidate AND a.project_id=c.project_id AND a.kind='PACKAGE'
   AND a.media_type='application/json' AND a.schema_name='qz.target_package' AND a.byte_count>0
   AND (
    (c.source_kind='FORECAST' AND a.schema_version='1' AND version='1'
     AND (environment='DEMO' OR (environment='REAL' AND a.origin='REAL' AND a.access_class='DELIVERY')))
    OR
    (c.source_kind='STRATEGY_ALPHA' AND c.purpose='CURRENT_DECISION'
     AND a.schema_version='2' AND version='2' AND a.access_class='DELIVERY'
     AND ((environment='DEMO' AND a.origin IN ('SYNTHETIC','FIXTURE','LEGACY_UNKNOWN'))
      OR (environment='REAL' AND a.origin='REAL')))
   )
 )
$$;

CREATE OR REPLACE FUNCTION app.guard_real_delivery() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS (
  SELECT 1 FROM app.releases r WHERE r.id=NEW.release_id
   AND ((r.source_kind='FORECAST_EVALUATION' AND r.environment='REAL')
    OR (r.source_kind='NATIVE_TARGET_DECISION' AND r.execution_environment='PAPER' AND NEW.environment='PAPER'))
 ) THEN
  IF EXISTS(SELECT 1 FROM app.releases WHERE id=NEW.release_id AND source_kind='NATIVE_TARGET_DECISION') THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='STRATEGY_PAPER_ONLY';
  END IF;
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='DEMO_NOT_DELIVERABLE';
 END IF;
 RETURN NEW;
END $$;

ALTER TABLE app.operator_command_grants DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants ADD CONSTRAINT operator_command_grants_operation_check CHECK(operation IN (
 'EXPERIMENT_ADOPT_ALPHA','CYCLE_START_EXTERNAL','CYCLE_FINISH_EXTERNAL','EXPERIMENT_EVALUATE','MIGRATION_IMPORT',
 'APPROVAL_REVOKE','HANDOFF_OFFER','RELEASE_CREATE','RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
 'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE','CREDENTIAL_ISSUE','CREDENTIAL_REVOKE',
 'INPUT_SET_CREATE','EVALUATION_POLICY_CREATE','BRIEF_CREATE','BRIEF_UPDATE','BRIEF_FREEZE','CYCLE_START','INTEGRATION_SECRET_REGISTER',
 'RUNTIME_CREATE','RUNTIME_UPDATE','DOWNSTREAM_CREATE','DOWNSTREAM_UPDATE','RUNTIME_PROBE','DOWNSTREAM_PROBE',
 'DATA_SOURCE_CREATE','DATA_SOURCE_UPDATE','DATA_GRANT_CREATE','DATA_GRANT_REVOKE','DATASET_REGISTER','DATA_VALIDATE',
 'CODEX_PROFILE_CREATE','CODEX_PROFILE_UPDATE','CODEX_PROBE','CODEX_LOGIN_START','CODEX_LOGIN_CANCEL','CODEX_LOGOUT',
 'ALPHA_EVALUATE','MANDATE_CREATE','EXECUTION_ASSUMPTIONS_CREATE','PORTFOLIO_BUILD','PORTFOLIO_SIMULATE'
));
