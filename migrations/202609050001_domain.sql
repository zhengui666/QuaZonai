-- First deployment into a NEW database only. No legacy schema is altered.
-- SQLx executes this migration transactionally. Native PostgreSQL constraints are
-- authoritative for identities and relationships; qualification/permissions and
-- time-dependent release decisions still belong to the locked domain service.
DO $$ BEGIN
  IF current_setting('server_version_num')::integer / 10000 <> 18 THEN
    RAISE EXCEPTION USING ERRCODE='0A000', MESSAGE='this schema requires PostgreSQL major 18';
  END IF;
END $$;
CREATE SCHEMA app;
CREATE EXTENSION IF NOT EXISTS pgmq VERSION '1.10.0';
DO $$ BEGIN
  IF (SELECT extversion FROM pg_extension WHERE extname='pgmq') IS DISTINCT FROM '1.10.0' THEN
    RAISE EXCEPTION USING ERRCODE='0A000', MESSAGE='this schema requires PGMQ 1.10.0';
  END IF;
END $$;
CREATE DOMAIN app.identity AS uuid CHECK (VALUE IS NULL OR (uuid_extract_version(VALUE) = 7) IS TRUE);
CREATE DOMAIN app.counter AS bigint CHECK (VALUE >= 0);
CREATE DOMAIN app.revision AS bigint CHECK (VALUE >= 1);
CREATE DOMAIN app.uint16 AS integer CHECK (VALUE BETWEEN 0 AND 65535);
CREATE DOMAIN app.uint32 AS bigint CHECK (VALUE BETWEEN 0 AND 4294967295);
-- An unconstrained numeric domain avoids PostgreSQL silently rounding a value
-- before a NUMERIC(38,18) check. Native trim_scale enforces the equivalent range.
CREATE DOMAIN app.decimal_value AS numeric CHECK (
  VALUE > -100000000000000000000 AND VALUE < 100000000000000000000
  AND scale(trim_scale(VALUE)) <= 18);
CREATE DOMAIN app.instant AS timestamptz CHECK (isfinite(VALUE));
CREATE DOMAIN app.nonempty AS text CHECK (length(btrim(VALUE)) > 0);
CREATE DOMAIN app.document AS jsonb CHECK (
  jsonb_typeof(VALUE) = 'object' AND VALUE ? 'schema_version'
  AND VALUE->'schema_version' = '1'::jsonb);

CREATE TABLE app.research_lineages (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
origin text NOT NULL CHECK (origin IN ('NEW','FORK','LEGACY_IMPORT')),
 parent_lineage_id app.identity REFERENCES app.research_lineages,
 legacy_reference text,
 reason app.nonempty NOT NULL,
 CHECK ((origin = 'FORK') = (parent_lineage_id IS NOT NULL))
);

CREATE TABLE app.projects (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
root_lineage_id app.identity NOT NULL REFERENCES app.research_lineages,
 name varchar(120) NOT NULL CHECK (length(btrim(name)) > 0),
 description text NOT NULL DEFAULT '',
 state text NOT NULL CHECK (state IN ('DRAFT','ACTIVE','PAUSED','ARCHIVED')),
 current_brief_id app.identity,
 current_automation_policy_id app.identity,
 created_by text NOT NULL CHECK (created_by IN ('OPERATOR','IMPORT')),
 archived_at app.instant,
 CHECK ((state = 'ARCHIVED') = (archived_at IS NOT NULL))
);

CREATE TABLE app.runtime_integrations (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
name app.nonempty NOT NULL,
 endpoint app.nonempty NOT NULL,
 tls_policy text NOT NULL CHECK (tls_policy IN ('SYSTEM_CA','PINNED_CA')),
 credential_ref app.nonempty NOT NULL,
 allowed_capabilities text[] NOT NULL,
 protocol_version app.nonempty NOT NULL,
 last_capability_snapshot_artifact_id app.identity,
 enabled boolean NOT NULL
);

CREATE TABLE app.downstream_integrations (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
name app.nonempty NOT NULL,
 endpoint app.nonempty NOT NULL,
 credential_ref app.nonempty NOT NULL,
 accepted_package_versions text[] NOT NULL,
 environments text NOT NULL CHECK (environments IN ('PAPER','LIVE','BOTH')),
 enabled boolean NOT NULL
);

CREATE TABLE app.codex_profiles (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
name app.nonempty NOT NULL,
 connection_mode text NOT NULL CHECK (connection_mode IN ('SYSTEM','CUSTOM_PROVIDER')),
 profile_origin text NOT NULL CHECK (profile_origin IN ('MANAGED_VOLUME','OPERATOR_MOUNT')),
 codex_home_ref app.nonempty NOT NULL UNIQUE,
 custom_base_url text,
 custom_api_key_ref text,
 custom_provider_options app.document,
 use_default_model_settings boolean NOT NULL,
 saved_model text,
 saved_reasoning_effort text,
 saved_fast_mode boolean NOT NULL,
 CHECK (connection_mode <> 'CUSTOM_PROVIDER' OR (custom_base_url IS NOT NULL AND custom_api_key_ref IS NOT NULL))
);

CREATE TABLE app.artifacts (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
project_id app.identity REFERENCES app.projects,
 producer_run_id app.identity,
 producer_attempt_id app.identity,
 kind text NOT NULL CHECK (kind IN ('CODE','PARAMETERS','SIGNALS','TARGETS','REPORT','METRICS','DATA_QUALITY','MODEL','PACKAGE','LOG','MIGRATION')),
 media_type app.nonempty NOT NULL,
 schema_name app.nonempty NOT NULL,
 schema_version app.nonempty NOT NULL,
 storage_backend text NOT NULL CHECK (storage_backend IN ('LOCAL','OBJECT_STORE','NATIVE_CATALOG')),
 storage_object_ref app.nonempty NOT NULL,
 storage_version app.nonempty NOT NULL,
 byte_count app.counter NOT NULL,
 access_class text NOT NULL CHECK (access_class IN ('OPERATOR','RESEARCH','EVALUATOR_ONLY','DELIVERY')),
 origin text NOT NULL CHECK (origin IN ('REAL','SYNTHETIC','FIXTURE','LEGACY_UNKNOWN')),
 created_by text NOT NULL CHECK (created_by IN ('OPERATOR','RUNTIME','AGENT','IMPORT')),
 retention_class text NOT NULL CHECK (retention_class IN ('REFERENCED','TEMPORARY','AUDIT')),
 UNIQUE(storage_backend,storage_object_ref,storage_version),
 CHECK (producer_attempt_id IS NULL OR producer_run_id IS NOT NULL),
 CHECK (producer_run_id IS NULL OR project_id IS NOT NULL)
);

CREATE TABLE app.data_sources (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
name app.nonempty NOT NULL,
 runtime_id app.identity NOT NULL REFERENCES app.runtime_integrations,
 native_catalog_ref app.nonempty NOT NULL,
 provider_kind app.nonempty NOT NULL,
 enabled boolean NOT NULL,
 UNIQUE(runtime_id,native_catalog_ref)
);

CREATE TABLE app.data_use_grants (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
source_id app.identity NOT NULL REFERENCES app.data_sources,
 version integer NOT NULL CHECK(version > 0),
 license_reference app.nonempty NOT NULL,
 evidence_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 allowed_uses text NOT NULL CHECK(allowed_uses IN ('RESEARCH','RESEARCH_AND_PAPER','RESEARCH_PAPER_LIVE')),
 valid_from app.instant NOT NULL,
 valid_until app.instant,
 authorized_by text NOT NULL CHECK(authorized_by = 'OPERATOR'),
 CHECK(valid_until IS NULL OR valid_until > valid_from),
 UNIQUE(source_id,version), UNIQUE(id,source_id)
);

CREATE TABLE app.data_use_revocations (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
grant_id app.identity NOT NULL REFERENCES app.data_use_grants,
 effective_at app.instant NOT NULL,
 reason_code app.nonempty NOT NULL,
 reason app.nonempty NOT NULL
);

CREATE TABLE app.universe_versions (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
name app.nonempty NOT NULL,
 membership_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 instrument_definition_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 calendar_ref app.nonempty NOT NULL,
 calendar_version app.nonempty NOT NULL,
 selection_asof app.instant NOT NULL,
 has_historical_membership boolean NOT NULL,
 coverage_start app.instant NOT NULL,
 coverage_end app.instant NOT NULL,
 CHECK(coverage_end > coverage_start)
);

CREATE TABLE app.dataset_revisions (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
source_id app.identity NOT NULL REFERENCES app.data_sources,
 data_use_grant_id app.identity NOT NULL,
 native_snapshot_ref app.nonempty NOT NULL,
 native_storage_version app.nonempty NOT NULL,
 universe_version_id app.identity NOT NULL REFERENCES app.universe_versions,
 schema_version app.nonempty NOT NULL,
 data_kind text NOT NULL CHECK(data_kind IN ('BAR','QUOTE','TRADE','ORDER_BOOK','FUNDAMENTAL','EVENT','DERIVED_FEATURE')),
 partition_role text NOT NULL CHECK(partition_role IN ('DISCOVERY','VALIDATION','SEALED','FORWARD')),
 event_start app.instant NOT NULL,
 event_end app.instant NOT NULL,
 available_through app.instant NOT NULL,
 row_count app.counter NOT NULL,
 timezone app.nonempty NOT NULL,
 quality_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 pit_status text NOT NULL CHECK(pit_status IN ('VERIFIED','UNVERIFIED','INVALID')),
 revision_policy text NOT NULL CHECK(revision_policy IN ('AS_KNOWN_THEN','RESTATED','UNKNOWN')),
 origin text NOT NULL CHECK(origin IN ('REAL','SYNTHETIC','FIXTURE','LEGACY_UNKNOWN')),
 CHECK(event_end > event_start),
 FOREIGN KEY(data_use_grant_id,source_id) REFERENCES app.data_use_grants(id,source_id),
 UNIQUE(source_id,native_snapshot_ref,native_storage_version)
);

CREATE TABLE app.benchmark_versions (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
name app.nonempty NOT NULL,
 dataset_revision_id app.identity NOT NULL REFERENCES app.dataset_revisions,
 return_kind text NOT NULL CHECK(return_kind IN ('TOTAL_RETURN','PRICE_RETURN','CASH')),
 currency char(3) NOT NULL,
 frequency app.nonempty NOT NULL
);

CREATE TABLE app.execution_assumptions (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
venue_capability_ref app.nonempty NOT NULL,
 engine_image_ref app.nonempty NOT NULL,
 price_type text NOT NULL CHECK(price_type IN ('MID','BID_ASK','TRADE','BAR')),
 starting_capital app.decimal_value NOT NULL CHECK(starting_capital > 0),
 base_currency char(3) NOT NULL,
 fee_schedule_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 slippage_model app.document NOT NULL,
 fill_model app.document NOT NULL,
 latency_model app.document,
 liquidity_artifact_id app.identity REFERENCES app.artifacts,
 cost_assumption_status text NOT NULL CHECK(cost_assumption_status IN ('DATA_BACKED','CONSERVATIVE_ASSUMPTION','INSUFFICIENT')),
 participation_limit app.decimal_value CHECK(participation_limit > 0 AND participation_limit <= 1),
 calendar_version app.nonempty NOT NULL,
 settlement_rule_ref app.nonempty NOT NULL
);

CREATE TABLE app.input_sets (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
project_id app.identity NOT NULL REFERENCES app.projects,
 purpose text NOT NULL CHECK(purpose IN ('DISCOVERY','VALIDATION','SEALED','PORTFOLIO','FORWARD')),
 decision_cutoff app.instant NOT NULL,
 frozen_at app.instant,
 UNIQUE(id,project_id)
);

CREATE TABLE app.input_set_items (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
input_set_id app.identity NOT NULL REFERENCES app.input_sets,
 dataset_revision_id app.identity REFERENCES app.dataset_revisions,
 artifact_id app.identity REFERENCES app.artifacts,
 role app.nonempty NOT NULL,
 ordinal integer NOT NULL CHECK(ordinal >= 0),
 CHECK(num_nonnulls(dataset_revision_id,artifact_id) = 1),
 UNIQUE(input_set_id,ordinal)
);

CREATE UNIQUE INDEX input_dataset_once ON app.input_set_items(input_set_id,dataset_revision_id) WHERE dataset_revision_id IS NOT NULL;
CREATE UNIQUE INDEX input_artifact_once ON app.input_set_items(input_set_id,artifact_id) WHERE artifact_id IS NOT NULL;

CREATE TABLE app.evaluation_policies (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
project_id app.identity NOT NULL REFERENCES app.projects,
 version integer NOT NULL CHECK(version > 0),
 selection_rule app.document NOT NULL,
 split_policy app.document NOT NULL,
 metric_requirements jsonb NOT NULL CHECK(jsonb_typeof(metric_requirements) = 'array'),
 minimum_observations integer NOT NULL CHECK(minimum_observations > 0),
 maximum_missing_fraction app.decimal_value NOT NULL CHECK(maximum_missing_fraction BETWEEN 0 AND 1),
 require_real_data boolean NOT NULL,
 required_capabilities text[] NOT NULL,
 maximum_sealed_uses_per_lineage integer NOT NULL CHECK(maximum_sealed_uses_per_lineage > 0),
 validity_seconds bigint NOT NULL CHECK(validity_seconds > 0),
 UNIQUE(project_id,version), UNIQUE(id,project_id)
);

CREATE TABLE app.research_briefs (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
project_id app.identity NOT NULL REFERENCES app.projects,
 version integer NOT NULL CHECK(version > 0),
 hypothesis app.nonempty NOT NULL,
 economic_rationale app.nonempty NOT NULL,
 universe_version_id app.identity NOT NULL REFERENCES app.universe_versions,
 target_kind text NOT NULL CHECK(target_kind IN ('SCORE','EXPECTED_RETURN')),
 horizon_kind text NOT NULL CHECK(horizon_kind IN ('FIXED_BARS','FIXED_DURATION','VARIABLE_INTERVAL')),
 horizon_value bigint CHECK(horizon_value > 0),
 base_currency char(3) NOT NULL,
 benchmark_ref app.identity REFERENCES app.benchmark_versions,
 evaluation_policy_id app.identity NOT NULL,
 execution_assumptions_id app.identity NOT NULL REFERENCES app.execution_assumptions,
 budget app.document NOT NULL,
 stop_rule app.document NOT NULL,
 state text NOT NULL CHECK(state IN ('DRAFT','FROZEN')),
 frozen_at app.instant,
 supersedes_id app.identity,
 CHECK((state = 'FROZEN') = (frozen_at IS NOT NULL)),
 CHECK((horizon_kind = 'VARIABLE_INTERVAL') = (horizon_value IS NULL)),
 UNIQUE(project_id,version), UNIQUE(id,project_id),
 FOREIGN KEY(evaluation_policy_id,project_id) REFERENCES app.evaluation_policies(id,project_id),
 FOREIGN KEY(supersedes_id,project_id) REFERENCES app.research_briefs(id,project_id)
);

CREATE TABLE app.brief_data_bindings (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
brief_id app.identity NOT NULL REFERENCES app.research_briefs,
 dataset_revision_id app.identity NOT NULL REFERENCES app.dataset_revisions,
 role text NOT NULL CHECK(role IN ('DISCOVERY','VALIDATION','SEALED','FORWARD')),
 access_policy text NOT NULL CHECK(access_policy IN ('METADATA_ONLY','RESEARCH_READ','EVALUATOR_ONLY')),
 UNIQUE(brief_id,dataset_revision_id)
);

CREATE TABLE app.research_cycles (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
project_id app.identity NOT NULL REFERENCES app.projects,
 brief_id app.identity NOT NULL,
 ordinal integer NOT NULL CHECK(ordinal > 0),
 trigger text NOT NULL CHECK(trigger IN ('OPERATOR','SCHEDULE','DEGRADATION','NEW_DATA')),
 wake_id app.identity,
 state text NOT NULL CHECK(state IN ('QUEUED','RUNNING','WAITING_INPUT','PAUSING','PAUSED','COMPLETED','CANCELLED','FAILED')),
 outcome text CHECK(outcome IN ('QUALIFIED_CANDIDATES','NO_SUPPORTED_CANDIDATE','BUDGET_EXHAUSTED','INCONCLUSIVE')),
 budget_snapshot app.document NOT NULL,
 reserved_experiments app.uint32 NOT NULL DEFAULT 0,
 used_experiments app.uint32 NOT NULL DEFAULT 0,
 reserved_cpu_seconds app.counter NOT NULL DEFAULT 0,
 started_at app.instant,
 ended_at app.instant,
 next_action text,
 UNIQUE(project_id,ordinal), UNIQUE(id,project_id), UNIQUE(wake_id),
 FOREIGN KEY(brief_id,project_id) REFERENCES app.research_briefs(id,project_id)
);

CREATE TABLE app.runs (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
project_id app.identity NOT NULL REFERENCES app.projects,
 cycle_id app.identity,
 kind text NOT NULL CHECK(kind IN ('AGENT_RESEARCH','DATA_VALIDATE','ALPHA_EVALUATE','PORTFOLIO_BUILD','PORTFOLIO_SIMULATE','FORWARD_EVALUATE','EXPORT','IMPORT')),
 input_set_id app.identity NOT NULL,
 state text NOT NULL CHECK(state IN ('QUEUED','DISPATCHING','RUNNING','RECONCILING','CANCEL_REQUESTED','SUCCEEDED','FAILED','CANCELLED')),
 current_attempt_no app.uint32 NOT NULL DEFAULT 0,
 active_attempt_id app.identity,
 last_event_seq app.counter NOT NULL DEFAULT 0,
 deadline_at app.instant NOT NULL,
 cancellation_requested_at app.instant,
 terminal_reason_code text,
 queued_at app.instant NOT NULL,
 started_at app.instant,
 finished_at app.instant,
 UNIQUE(id,project_id), UNIQUE(id,project_id,cycle_id),
 FOREIGN KEY(cycle_id,project_id) REFERENCES app.research_cycles(id,project_id),
 FOREIGN KEY(input_set_id,project_id) REFERENCES app.input_sets(id,project_id),
 CHECK((state IN ('SUCCEEDED','FAILED','CANCELLED')) = (finished_at IS NOT NULL)),
 CHECK(state <> 'CANCEL_REQUESTED' OR cancellation_requested_at IS NOT NULL),
 CHECK((current_attempt_no = 0) = (active_attempt_id IS NULL)),
 CHECK(deadline_at > queued_at)
);

CREATE TABLE app.run_attempts (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
run_id app.identity NOT NULL REFERENCES app.runs,
 attempt_no app.uint32 NOT NULL CHECK(attempt_no > 0),
 worker_owner_id app.nonempty NOT NULL,
 owner_epoch app.revision NOT NULL,
 lease_expires_at app.instant NOT NULL,
 runtime_id app.identity REFERENCES app.runtime_integrations,
 external_job_id text,
 dispatch_state text NOT NULL CHECK(dispatch_state IN ('NOT_SENT','SENT_UNKNOWN','ACKNOWLEDGED','TERMINAL')),
 runtime_state text NOT NULL CHECK(runtime_state IN ('UNKNOWN','PENDING','RUNNING','SUCCEEDED','FAILED','CANCELLED')),
 result_manifest_artifact_id app.identity REFERENCES app.artifacts,
 accepted_at app.instant,
 error_class text CHECK(error_class IN ('RETRYABLE_INFRA','PERMANENT_CONFIG','INVALID_INPUT','CANCELLED','RESOURCE_LIMIT')),
 error_code text,
 UNIQUE(run_id,attempt_no), UNIQUE(id,run_id), UNIQUE(id,run_id,attempt_no),
 UNIQUE(runtime_id,external_job_id),
 CHECK(external_job_id IS NULL OR runtime_id IS NOT NULL)
);

CREATE TABLE app.run_events (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
run_id app.identity NOT NULL REFERENCES app.runs,
 seq app.counter NOT NULL CHECK(seq > 0),
 attempt_id app.identity,
 event_type app.nonempty NOT NULL,
 schema_version integer NOT NULL CHECK(schema_version = 1),
 payload app.document NOT NULL,
 occurred_at app.instant NOT NULL,
 UNIQUE(run_id,seq),
 FOREIGN KEY(attempt_id,run_id) REFERENCES app.run_attempts(id,run_id)
);

CREATE TABLE app.experiment_families (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
project_id app.identity NOT NULL REFERENCES app.projects,
 root_lineage_id app.identity NOT NULL REFERENCES app.research_lineages,
 question app.nonempty NOT NULL,
 selection_policy_id app.identity NOT NULL,
 UNIQUE(id,project_id),
 FOREIGN KEY(selection_policy_id,project_id) REFERENCES app.evaluation_policies(id,project_id)
);

CREATE TABLE app.experiments (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
project_id app.identity NOT NULL REFERENCES app.projects,
 cycle_id app.identity NOT NULL,
 family_id app.identity NOT NULL,
 parent_experiment_id app.identity,
 ordinal integer NOT NULL CHECK(ordinal > 0),
 hypothesis app.nonempty NOT NULL,
 expected_failure_modes text NOT NULL,
 proposal_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 code_artifact_id app.identity REFERENCES app.artifacts,
 parameter_artifact_id app.identity REFERENCES app.artifacts,
 trial_source text NOT NULL CHECK(trial_source IN ('CODEX','OPTUNA','OPERATOR')),
 native_study_ref text,
 native_trial_id text,
 run_id app.identity,
 outcome text NOT NULL CHECK(outcome IN ('PENDING','SUPPORTED','REJECTED','INVALID','INCONCLUSIVE')),
 outcome_reason text,
 conclusion_artifact_id app.identity REFERENCES app.artifacts,
 UNIQUE(cycle_id,ordinal), UNIQUE(id,project_id),
 FOREIGN KEY(cycle_id,project_id) REFERENCES app.research_cycles(id,project_id),
 FOREIGN KEY(family_id,project_id) REFERENCES app.experiment_families(id,project_id),
 FOREIGN KEY(parent_experiment_id,project_id) REFERENCES app.experiments(id,project_id),
 FOREIGN KEY(run_id,project_id) REFERENCES app.runs(id,project_id)
);

CREATE TABLE app.alphas (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
project_id app.identity NOT NULL REFERENCES app.projects,
 name app.nonempty NOT NULL,
 lifecycle text NOT NULL CHECK(lifecycle IN ('RESEARCH','QUALIFIED','SUSPENDED','RETIRED')),
 active_version_id app.identity,
 UNIQUE(id,project_id)
);

CREATE TABLE app.alpha_versions (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
project_id app.identity NOT NULL REFERENCES app.projects,
 alpha_id app.identity NOT NULL,
 version integer NOT NULL CHECK(version > 0),
 experiment_id app.identity NOT NULL,
 root_lineage_id app.identity NOT NULL REFERENCES app.research_lineages,
 code_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 model_artifact_id app.identity REFERENCES app.artifacts,
 signal_contract_version app.nonempty NOT NULL,
 signal_kind text NOT NULL CHECK(signal_kind IN ('SCORE','EXPECTED_RETURN')),
 horizon_kind text NOT NULL CHECK(horizon_kind IN ('FIXED_BARS','FIXED_DURATION','VARIABLE_INTERVAL')),
 horizon_value bigint CHECK(horizon_value > 0),
 forecast_unit text NOT NULL CHECK(forecast_unit IN ('RETURN_PER_HORIZON','RESIDUAL_RETURN_PER_HORIZON','UNITLESS_SCORE')),
 calibration_id app.identity,
 runtime_image_ref app.nonempty NOT NULL,
 UNIQUE(alpha_id,version), UNIQUE(id,project_id), UNIQUE(id,alpha_id),
 FOREIGN KEY(alpha_id,project_id) REFERENCES app.alphas(id,project_id),
 FOREIGN KEY(experiment_id,project_id) REFERENCES app.experiments(id,project_id),
 CHECK((horizon_kind = 'VARIABLE_INTERVAL') = (horizon_value IS NULL))
);

CREATE TABLE app.portfolio_mandates (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
project_id app.identity NOT NULL REFERENCES app.projects,
 version integer NOT NULL CHECK(version > 0),
 objective text NOT NULL CHECK(objective IN ('MIN_RISK','MAX_UTILITY','RISK_BUDGETING')),
 risk_measure text NOT NULL CHECK(risk_measure IN ('VARIANCE','CVAR')),
 base_currency char(3) NOT NULL,
 capital_assumption app.decimal_value NOT NULL CHECK(capital_assumption > 0),
 universe_version_id app.identity NOT NULL REFERENCES app.universe_versions,
 covariance_estimator app.document NOT NULL,
 alpha_ensemble app.document NOT NULL,
 optimizer app.document NOT NULL,
 constraints app.document NOT NULL,
 rebalance_schedule app.document NOT NULL,
 required_evaluation_policy_id app.identity NOT NULL,
 execution_assumptions_id app.identity NOT NULL REFERENCES app.execution_assumptions,
 exposure_tolerance app.decimal_value NOT NULL CHECK(exposure_tolerance > 0),
 UNIQUE(project_id,version), UNIQUE(id,project_id),
 FOREIGN KEY(required_evaluation_policy_id,project_id) REFERENCES app.evaluation_policies(id,project_id)
);

CREATE TABLE app.portfolio_candidates (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
project_id app.identity NOT NULL REFERENCES app.projects,
 mandate_id app.identity NOT NULL,
 input_set_id app.identity NOT NULL,
 decision_asof app.instant NOT NULL,
 run_id app.identity NOT NULL,
 solver_status text NOT NULL CHECK(solver_status IN ('OPTIMAL','ACCEPTABLE_INACCURATE','INFEASIBLE','UNBOUNDED','FAILED')),
 evidence_status text NOT NULL CHECK(evidence_status IN ('VALID','INCOMPLETE','INVALID')),
 reason_code text,
 forecast_artifact_id app.identity REFERENCES app.artifacts,
 covariance_artifact_id app.identity REFERENCES app.artifacts,
 diagnostics_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 target_artifact_id app.identity REFERENCES app.artifacts,
 allocation_evaluation_id app.identity,
 cash_weight app.decimal_value,
 current_weights_source text NOT NULL CHECK(current_weights_source IN ('FORWARD_SNAPSHOT','LAST_TARGET','NONE')),
 current_weights_artifact_id app.identity REFERENCES app.artifacts,
 UNIQUE(id,project_id), UNIQUE(id,mandate_id),
 FOREIGN KEY(mandate_id,project_id) REFERENCES app.portfolio_mandates(id,project_id),
 FOREIGN KEY(input_set_id,project_id) REFERENCES app.input_sets(id,project_id),
 FOREIGN KEY(run_id,project_id) REFERENCES app.runs(id,project_id),
 CHECK(solver_status IN ('OPTIMAL','ACCEPTABLE_INACCURATE') OR (target_artifact_id IS NULL AND cash_weight IS NULL)),
 CHECK((current_weights_source = 'NONE') = (current_weights_artifact_id IS NULL))
);

CREATE TABLE app.evaluations (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
project_id app.identity NOT NULL REFERENCES app.projects,
 subject_alpha_version_id app.identity,
 subject_candidate_id app.identity,
 input_set_id app.identity NOT NULL,
 policy_id app.identity NOT NULL,
 run_id app.identity NOT NULL,
 evaluation_kind text NOT NULL CHECK(evaluation_kind IN ('DISCOVERY','WALK_FORWARD','SEALED','PORTFOLIO','FORWARD')),
 execution_status text NOT NULL CHECK(execution_status IN ('SUCCEEDED','FAILED','CANCELLED')),
 evidence_status text NOT NULL CHECK(evidence_status IN ('VALID','INVALID','INCOMPLETE','UNSUPPORTED')),
 decision text NOT NULL CHECK(decision IN ('PASS','REJECT','INCONCLUSIVE')),
 report_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 method_versions_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 concluded_at app.instant NOT NULL,
 valid_until app.instant,
 UNIQUE(id,project_id), UNIQUE(id,subject_candidate_id), UNIQUE(id,subject_alpha_version_id,policy_id),
 FOREIGN KEY(subject_alpha_version_id,project_id) REFERENCES app.alpha_versions(id,project_id),
 FOREIGN KEY(subject_candidate_id,project_id) REFERENCES app.portfolio_candidates(id,project_id),
 FOREIGN KEY(input_set_id,project_id) REFERENCES app.input_sets(id,project_id),
 FOREIGN KEY(policy_id,project_id) REFERENCES app.evaluation_policies(id,project_id),
 FOREIGN KEY(run_id,project_id) REFERENCES app.runs(id,project_id),
 CHECK(num_nonnulls(subject_alpha_version_id,subject_candidate_id)=1),
 CHECK(decision <> 'PASS' OR (execution_status='SUCCEEDED' AND evidence_status='VALID' AND valid_until IS NOT NULL)),
 CHECK(valid_until IS NULL OR valid_until > concluded_at)
);

CREATE TABLE app.metric_values (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
evaluation_id app.identity NOT NULL REFERENCES app.evaluations,
 metric_code app.nonempty NOT NULL,
 scope app.nonempty NOT NULL,
 value double precision,
 status text NOT NULL CHECK(status IN ('OK','INSUFFICIENT_DATA','UNSUPPORTED','INVALID_INPUT','FAILED')),
 reason_code text,
 unit app.nonempty NOT NULL,
 period_start app.instant NOT NULL,
 period_end app.instant NOT NULL,
 observation_count app.counter NOT NULL,
 frequency app.nonempty NOT NULL,
 annualization_factor double precision,
 method_id app.nonempty NOT NULL,
 method_version app.nonempty NOT NULL,
 source_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 higher_is_better boolean,
 UNIQUE(evaluation_id,metric_code,scope),
 CHECK(period_end > period_start),
 CHECK(value IS NULL OR (value > '-Infinity'::float8 AND value < 'Infinity'::float8)),
 CHECK(annualization_factor IS NULL OR (annualization_factor > 0 AND annualization_factor < 'Infinity'::float8)),
 CHECK((status='OK' AND value IS NOT NULL AND reason_code IS NULL) OR
       (status<>'OK' AND value IS NULL AND length(btrim(reason_code)) > 0 AND reason_code IS NOT NULL))
);

CREATE TABLE app.calibrations (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
estimator_kind app.nonempty NOT NULL,
 estimator_version app.nonempty NOT NULL,
 model_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 train_input_set_id app.identity NOT NULL REFERENCES app.input_sets,
 fit_end_available_at app.instant NOT NULL,
 output_unit app.nonempty NOT NULL,
 horizon_kind app.nonempty NOT NULL,
 horizon_value bigint,
 validation_evaluation_id app.identity NOT NULL REFERENCES app.evaluations
);

CREATE TABLE app.qualifications (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
alpha_version_id app.identity NOT NULL REFERENCES app.alpha_versions,
 policy_id app.identity NOT NULL REFERENCES app.evaluation_policies,
 qualifying_evaluation_id app.identity NOT NULL,
 granted_at app.instant NOT NULL,
 valid_until app.instant NOT NULL,
 CHECK(valid_until > granted_at),
 UNIQUE(id,alpha_version_id), UNIQUE(alpha_version_id,policy_id,qualifying_evaluation_id),
 FOREIGN KEY(qualifying_evaluation_id,alpha_version_id,policy_id)
 REFERENCES app.evaluations(id,subject_alpha_version_id,policy_id)
);

CREATE TABLE app.qualification_revocations (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
qualification_id app.identity NOT NULL REFERENCES app.qualifications,
 reason_code app.nonempty NOT NULL,
 evidence_evaluation_id app.identity REFERENCES app.evaluations,
 effective_at app.instant NOT NULL
);

CREATE TABLE app.evidence_exposures (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
root_lineage_id app.identity NOT NULL REFERENCES app.research_lineages,
 dataset_revision_id app.identity NOT NULL REFERENCES app.dataset_revisions,
 evaluation_id app.identity REFERENCES app.evaluations,
 actor_kind text NOT NULL CHECK(actor_kind IN ('OPERATOR','RESEARCH_AGENT','EVALUATOR','IMPORT')),
 actor_session_ref text,
 exposure_kind text NOT NULL CHECK(exposure_kind IN ('RAW','SAMPLE','METRIC','PLOT','SUMMARY','LEGACY_UNKNOWN')),
 exposed_at app.instant NOT NULL,
 purpose app.nonempty NOT NULL
);

CREATE TABLE app.candidate_alphas (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
candidate_id app.identity NOT NULL REFERENCES app.portfolio_candidates,
 alpha_version_id app.identity NOT NULL REFERENCES app.alpha_versions,
 qualification_id app.identity NOT NULL,
 ensemble_weight app.decimal_value NOT NULL,
 calibration_id app.identity REFERENCES app.calibrations,
 forecast_unit app.nonempty NOT NULL,
 coverage_fraction app.decimal_value NOT NULL CHECK(coverage_fraction BETWEEN 0 AND 1),
 UNIQUE(candidate_id,alpha_version_id),
 FOREIGN KEY(qualification_id,alpha_version_id) REFERENCES app.qualifications(id,alpha_version_id)
);

CREATE TABLE app.candidate_targets (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
candidate_id app.identity NOT NULL REFERENCES app.portfolio_candidates,
 instrument_id app.nonempty NOT NULL,
 target_weight app.decimal_value NOT NULL,
 currency char(3) NOT NULL,
 asof app.instant NOT NULL,
 valid_until app.instant NOT NULL,
 CHECK(valid_until > asof),
 UNIQUE(candidate_id,instrument_id)
);

CREATE TABLE app.releases (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
candidate_id app.identity NOT NULL,
 package_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 package_schema_version app.nonempty NOT NULL,
 mandate_id app.identity NOT NULL REFERENCES app.portfolio_mandates,
 evaluation_id app.identity NOT NULL,
 market_capability_version app.nonempty NOT NULL,
 asof app.instant NOT NULL,
 valid_from app.instant NOT NULL,
 valid_until app.instant NOT NULL,
 environment text NOT NULL CHECK(environment IN ('DEMO','REAL')),
 CHECK(valid_until > valid_from AND valid_from >= asof),
 UNIQUE(id,candidate_id),
 FOREIGN KEY(candidate_id,mandate_id) REFERENCES app.portfolio_candidates(id,mandate_id),
 FOREIGN KEY(evaluation_id,candidate_id) REFERENCES app.evaluations(id,subject_candidate_id)
);

CREATE TABLE app.automation_policies (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
project_id app.identity NOT NULL REFERENCES app.projects,
 mode text NOT NULL CHECK(mode IN ('MANUAL','AUTO_PAPER','AUTO_HANDOFF')),
 mandate_id app.identity NOT NULL,
 downstream_id app.identity NOT NULL REFERENCES app.downstream_integrations,
 required_paper_observations integer NOT NULL CHECK(required_paper_observations > 0),
 minimum_paper_elapsed_seconds bigint NOT NULL CHECK(minimum_paper_elapsed_seconds > 0),
 max_feedback_age_seconds bigint NOT NULL CHECK(max_feedback_age_seconds > 0),
 promotion_metric_requirements jsonb NOT NULL CHECK(jsonb_typeof(promotion_metric_requirements)='array'),
 degradation_metric_requirements jsonb NOT NULL CHECK(jsonb_typeof(degradation_metric_requirements)='array'),
 authorized_at app.instant NOT NULL,
 valid_until app.instant NOT NULL,
 enabled_for_new_rebalances boolean NOT NULL,
 max_rebalances_per_day integer NOT NULL CHECK(max_rebalances_per_day > 0),
 UNIQUE(id,project_id),
 FOREIGN KEY(mandate_id,project_id) REFERENCES app.portfolio_mandates(id,project_id),
 CHECK(valid_until > authorized_at)
);

CREATE TABLE app.policy_revocations (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
automation_policy_id app.identity NOT NULL REFERENCES app.automation_policies,
 effective_at app.instant NOT NULL,
 reason app.nonempty NOT NULL
);

CREATE TABLE app.approvals (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
release_id app.identity NOT NULL REFERENCES app.releases,
 environment text NOT NULL CHECK(environment IN ('PAPER','LIVE')),
 downstream_id app.identity NOT NULL REFERENCES app.downstream_integrations,
 authority_kind text NOT NULL CHECK(authority_kind IN ('OPERATOR','FROZEN_POLICY')),
 automation_policy_id app.identity REFERENCES app.automation_policies,
 evidence_set_id app.identity NOT NULL REFERENCES app.input_sets,
 granted_at app.instant NOT NULL,
 valid_until app.instant NOT NULL,
 CHECK((authority_kind='FROZEN_POLICY') = (automation_policy_id IS NOT NULL)),
 CHECK(valid_until > granted_at),
 UNIQUE(id,release_id,downstream_id,environment)
);

CREATE TABLE app.approval_revocations (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
approval_id app.identity NOT NULL REFERENCES app.approvals,
 effective_at app.instant NOT NULL,
 reason app.nonempty NOT NULL
);

CREATE TABLE app.release_decisions (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
release_id app.identity NOT NULL,
 candidate_id app.identity NOT NULL REFERENCES app.portfolio_candidates,
 downstream_id app.identity NOT NULL REFERENCES app.downstream_integrations,
 environment text NOT NULL CHECK(environment IN ('PAPER','LIVE')),
 ordinal integer NOT NULL CHECK(ordinal > 0),
 decision text NOT NULL CHECK(decision IN ('REJECT','REOPEN')),
 supersedes_decision_id app.identity,
 reason_code app.nonempty NOT NULL,
 reason app.nonempty NOT NULL,
 decided_at app.instant NOT NULL,
 decided_by text NOT NULL CHECK(decided_by='OPERATOR'),
 FOREIGN KEY(release_id,candidate_id) REFERENCES app.releases(id,candidate_id),
 UNIQUE(candidate_id,downstream_id,environment,ordinal),
 UNIQUE(id,candidate_id,downstream_id,environment),
 FOREIGN KEY(supersedes_decision_id,candidate_id,downstream_id,environment)
 REFERENCES app.release_decisions(id,candidate_id,downstream_id,environment)
);

CREATE TABLE app.handoff_offers (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
release_id app.identity NOT NULL REFERENCES app.releases,
 approval_id app.identity NOT NULL,
 downstream_id app.identity NOT NULL REFERENCES app.downstream_integrations,
 environment text NOT NULL CHECK(environment IN ('PAPER','LIVE')),
 delivery_sequence app.counter NOT NULL CHECK(delivery_sequence > 0),
 state text NOT NULL CHECK(state IN ('OFFERED','CLAIMED','ACKNOWLEDGED','REJECTED','REVOKED','EXPIRED')),
 external_claim_id text,
 offered_at app.instant NOT NULL,
 expires_at app.instant NOT NULL,
 claimed_at app.instant,
 acknowledged_at app.instant,
 UNIQUE(id,downstream_id), UNIQUE(downstream_id,environment,delivery_sequence),
 FOREIGN KEY(approval_id,release_id,downstream_id,environment)
 REFERENCES app.approvals(id,release_id,downstream_id,environment),
 CHECK(expires_at > offered_at),
 CHECK((external_claim_id IS NULL) = (claimed_at IS NULL))
);

CREATE TABLE app.forward_messages (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
downstream_id app.identity NOT NULL REFERENCES app.downstream_integrations,
 external_message_id app.nonempty NOT NULL,
 handoff_id app.identity NOT NULL,
 stream_id app.nonempty NOT NULL,
 sequence app.counter NOT NULL,
 message_revision integer NOT NULL CHECK(message_revision > 0),
 supersedes_message_id app.identity,
 window_start app.instant NOT NULL,
 window_end app.instant NOT NULL,
 coverage_status text NOT NULL CHECK(coverage_status IN ('COMPLETE','PARTIAL','CORRECTION')),
 observation_count app.counter NOT NULL,
 report_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 issued_at app.instant NOT NULL,
 received_at app.instant NOT NULL,
 UNIQUE(downstream_id,external_message_id), UNIQUE(handoff_id,stream_id,sequence,message_revision),
 UNIQUE(id,handoff_id,stream_id,sequence),
 FOREIGN KEY(handoff_id,downstream_id) REFERENCES app.handoff_offers(id,downstream_id),
 FOREIGN KEY(supersedes_message_id,handoff_id,stream_id,sequence)
 REFERENCES app.forward_messages(id,handoff_id,stream_id,sequence),
 CHECK(window_end > window_start),
 CHECK((coverage_status='CORRECTION') = (supersedes_message_id IS NOT NULL))
);

CREATE TABLE app.forward_evidence_windows (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
release_id app.identity NOT NULL REFERENCES app.releases,
 input_set_id app.identity NOT NULL REFERENCES app.input_sets,
 evaluation_id app.identity NOT NULL REFERENCES app.evaluations,
 window_start app.instant NOT NULL,
 window_end app.instant NOT NULL,
 complete_observations app.counter NOT NULL,
 is_contiguous boolean NOT NULL,
 freshness_deadline app.instant NOT NULL,
 CHECK(window_end > window_start)
);

CREATE TABLE app.degradation_observations (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
project_id app.identity NOT NULL REFERENCES app.projects,
 release_id app.identity NOT NULL REFERENCES app.releases,
 evaluation_id app.identity NOT NULL REFERENCES app.evaluations,
 policy_id app.identity NOT NULL,
 classification text NOT NULL CHECK(classification IN ('HEALTHY','WATCH','DEGRADED','INSUFFICIENT_DATA')),
 reason_codes text[] NOT NULL,
 observed_at app.instant NOT NULL,
 UNIQUE(id,project_id),
 FOREIGN KEY(policy_id,project_id) REFERENCES app.automation_policies(id,project_id)
);

CREATE TABLE app.wake_events (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
project_id app.identity NOT NULL REFERENCES app.projects,
 observation_id app.identity,
 trigger text NOT NULL CHECK(trigger IN ('DEGRADATION','DATA_AVAILABLE','OPERATOR','SCHEDULE')),
 state text NOT NULL CHECK(state IN ('PENDING','SUPPRESSED','CONSUMED','CANCELLED')),
 not_before app.instant NOT NULL,
 consumed_cycle_id app.identity,
 reason app.nonempty NOT NULL,
 UNIQUE(observation_id,trigger), UNIQUE(id,project_id), UNIQUE(consumed_cycle_id),
 FOREIGN KEY(observation_id,project_id) REFERENCES app.degradation_observations(id,project_id),
 FOREIGN KEY(consumed_cycle_id,project_id) REFERENCES app.research_cycles(id,project_id) DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE app.codex_sessions (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
project_id app.identity NOT NULL REFERENCES app.projects,
 cycle_id app.identity NOT NULL,
 run_id app.identity NOT NULL UNIQUE,
 role text NOT NULL CHECK(role IN ('RESEARCHER','INDEPENDENT_REVIEWER')),
 profile_id app.identity NOT NULL REFERENCES app.codex_profiles,
 profile_revision app.revision NOT NULL,
 thread_id app.nonempty NOT NULL,
 active_turn_id text,
 codex_version app.nonempty NOT NULL,
 protocol_schema_version app.nonempty NOT NULL,
 requested_settings app.document NOT NULL,
 observed_model text,
 observed_effort text,
 observed_provider text,
 native_history_ref app.nonempty NOT NULL,
 public_summary_artifact_id app.identity REFERENCES app.artifacts,
 UNIQUE(profile_id,thread_id), UNIQUE(id,run_id,project_id,cycle_id),
 UNIQUE(id,profile_revision),
 FOREIGN KEY(run_id,project_id,cycle_id) REFERENCES app.runs(id,project_id,cycle_id)
);

CREATE TABLE app.model_turn_reservations (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
project_id app.identity NOT NULL,
 cycle_id app.identity NOT NULL,
 run_id app.identity NOT NULL,
 session_id app.identity NOT NULL,
 attempt_id app.identity NOT NULL,
 owner_epoch app.revision NOT NULL,
 profile_revision app.revision NOT NULL,
 ordinal integer NOT NULL CHECK(ordinal BETWEEN 1 AND 65535),
 command_key app.nonempty NOT NULL CHECK(octet_length(command_key) <= 200),
 turn_kind text NOT NULL CHECK(turn_kind IN ('RESEARCH','REPAIR')),
 reserved_tokens app.counter NOT NULL CHECK(reserved_tokens > 0),
 reserved_cost app.decimal_value CHECK(reserved_cost >= 0),
 cost_currency char(3),
 request_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 deadline_at app.instant NOT NULL,
 CHECK((reserved_cost IS NULL) = (cost_currency IS NULL)),
 UNIQUE(session_id,command_key), UNIQUE(id,session_id),
 UNIQUE(run_id,ordinal), UNIQUE(session_id,ordinal),
 FOREIGN KEY(session_id,profile_revision) REFERENCES app.codex_sessions(id,profile_revision),
 FOREIGN KEY(session_id,run_id,project_id,cycle_id)
 REFERENCES app.codex_sessions(id,run_id,project_id,cycle_id),
 FOREIGN KEY(attempt_id,run_id) REFERENCES app.run_attempts(id,run_id)
);

CREATE TABLE app.model_turn_dispatches (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
reservation_id app.identity NOT NULL UNIQUE REFERENCES app.model_turn_reservations,
 owner_epoch app.revision NOT NULL,
 rpc_request_id app.nonempty NOT NULL CHECK(octet_length(rpc_request_id) <= 200),
 UNIQUE(reservation_id,rpc_request_id)
);

CREATE TABLE app.model_turn_bindings (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
reservation_id app.identity NOT NULL UNIQUE REFERENCES app.model_turn_dispatches(reservation_id),
 session_id app.identity NOT NULL REFERENCES app.codex_sessions,
 native_turn_id app.nonempty NOT NULL CHECK(octet_length(native_turn_id) <= 200),
 UNIQUE(session_id,native_turn_id), UNIQUE(reservation_id,native_turn_id),
 FOREIGN KEY(reservation_id,session_id) REFERENCES app.model_turn_reservations(id,session_id)
);

CREATE TABLE app.model_turn_terminals (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  reservation_id app.identity NOT NULL UNIQUE REFERENCES app.model_turn_reservations,
  native_turn_id app.nonempty,
  outcome text NOT NULL CHECK(outcome IN ('SUCCEEDED','FAILED','CANCELLED','NOT_SENT')),
  reason_code app.nonempty NOT NULL CHECK(octet_length(reason_code)<=120),
  observed_at app.instant NOT NULL,
  CHECK((outcome='NOT_SENT') = (native_turn_id IS NULL)),
  UNIQUE(reservation_id,outcome),
  FOREIGN KEY(reservation_id,native_turn_id)
    REFERENCES app.model_turn_bindings(reservation_id,native_turn_id)
);

CREATE TABLE app.model_turn_receipts (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
reservation_id app.identity NOT NULL UNIQUE REFERENCES app.model_turn_reservations,
 outcome text NOT NULL CHECK(outcome IN ('SUCCEEDED','FAILED','CANCELLED','NOT_SENT')),
 actual_tokens app.counter NOT NULL,
 actual_cost app.decimal_value CHECK(actual_cost >= 0),
 cost_currency char(3),
 usage_source text NOT NULL CHECK(usage_source IN ('NATIVE_REPORT','CONFIRMED_NOT_SENT')),
 reason_code app.nonempty NOT NULL CHECK(octet_length(reason_code) <= 120),
 FOREIGN KEY(reservation_id,outcome) REFERENCES app.model_turn_terminals(reservation_id,outcome),
 CHECK((actual_cost IS NULL) = (cost_currency IS NULL)),
 CHECK((outcome='NOT_SENT') = (usage_source='CONFIRMED_NOT_SENT')),
 CHECK(outcome<>'NOT_SENT' OR (actual_tokens=0 AND (actual_cost IS NULL OR actual_cost=0)))
);

CREATE TABLE app.operator_auth_state (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
singleton boolean NOT NULL DEFAULT true UNIQUE CHECK(singleton),
 initialized boolean NOT NULL,
 totp_secret_ref text,
 last_accepted_totp_step bigint,
 session_epoch app.revision NOT NULL,
 setup_completed_at app.instant,
 CHECK((initialized AND totp_secret_ref IS NOT NULL AND setup_completed_at IS NOT NULL) OR
       (NOT initialized AND totp_secret_ref IS NULL AND setup_completed_at IS NULL AND last_accepted_totp_step IS NULL))
);

CREATE TABLE app.trusted_devices (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
token_verifier_ref app.nonempty NOT NULL UNIQUE,
 label app.nonempty NOT NULL,
 last_used_at app.instant,
 expires_at app.instant NOT NULL,
 revoked_at app.instant,
 auth_epoch app.revision NOT NULL
);

CREATE TABLE app.machine_principals (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
  revision app.revision NOT NULL DEFAULT 1,
name app.nonempty NOT NULL,
 kind text NOT NULL CHECK(kind IN ('CLI','DOWNSTREAM','AUTOMATION','MISSION')),
 project_id app.identity REFERENCES app.projects,
 downstream_id app.identity REFERENCES app.downstream_integrations,
 run_id app.identity,
 enabled boolean NOT NULL,
 credential_epoch app.revision NOT NULL,
 FOREIGN KEY(run_id,project_id) REFERENCES app.runs(id,project_id),
 CHECK(kind<>'MISSION' OR (run_id IS NOT NULL AND project_id IS NOT NULL)),
 CHECK(kind<>'DOWNSTREAM' OR downstream_id IS NOT NULL)
);

CREATE TABLE app.machine_credentials (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
principal_id app.identity NOT NULL REFERENCES app.machine_principals,
 public_token_id app.nonempty NOT NULL UNIQUE,
 verifier_ref app.nonempty NOT NULL,
 principal_epoch app.revision NOT NULL,
 scope_codes text[] NOT NULL CHECK(cardinality(scope_codes) > 0),
 issued_at app.instant NOT NULL,
 expires_at app.instant NOT NULL,
 issued_by text NOT NULL CHECK(issued_by IN ('OPERATOR','MISSION_SERVICE')),
 CHECK(expires_at > issued_at)
);

CREATE TABLE app.machine_credential_revocations (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
credential_id app.identity NOT NULL REFERENCES app.machine_credentials,
 effective_at app.instant NOT NULL,
 reason app.nonempty NOT NULL
);

CREATE TABLE app.operator_command_grants (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
credential_id app.identity NOT NULL REFERENCES app.machine_credentials,
 operation text NOT NULL CHECK(operation IN ('RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE')),
 target_id app.identity NOT NULL,
 auth_epoch app.revision NOT NULL,
 authenticated_at app.instant NOT NULL,
 expires_at app.instant NOT NULL,
 CHECK(expires_at > authenticated_at AND expires_at <= authenticated_at + interval '300 seconds'),
 UNIQUE(id,operation,target_id)
);

CREATE TABLE app.command_receipts (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
principal_scope app.nonempty NOT NULL,
 operation app.nonempty NOT NULL,
 idempotency_key app.nonempty NOT NULL CHECK(octet_length(idempotency_key) <= 200),
 normalized_nonsecret_request app.document NOT NULL,
 resource_id app.identity NOT NULL,
 response_status integer NOT NULL CHECK(response_status BETWEEN 100 AND 599),
 expires_at app.instant,
 UNIQUE(principal_scope,operation,idempotency_key),
 UNIQUE(id,operation,resource_id)
);

CREATE TABLE app.operator_command_consumptions (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  grant_id app.identity NOT NULL UNIQUE,
  command_receipt_id app.identity NOT NULL UNIQUE,
  operation app.nonempty NOT NULL,
  target_id app.identity NOT NULL,
  FOREIGN KEY(grant_id,operation,target_id) REFERENCES app.operator_command_grants(id,operation,target_id),
  FOREIGN KEY(command_receipt_id,operation,target_id) REFERENCES app.command_receipts(id,operation,resource_id)
);

-- Circular references are deferred only where an atomic publication needs it.
ALTER TABLE app.projects ADD FOREIGN KEY(current_brief_id,id) REFERENCES app.research_briefs(id,project_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE app.projects ADD FOREIGN KEY(current_automation_policy_id,id) REFERENCES app.automation_policies(id,project_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE app.runtime_integrations ADD FOREIGN KEY(last_capability_snapshot_artifact_id) REFERENCES app.artifacts;
ALTER TABLE app.research_cycles ADD FOREIGN KEY(wake_id,project_id) REFERENCES app.wake_events(id,project_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE app.runs ADD FOREIGN KEY(active_attempt_id,id,current_attempt_no) REFERENCES app.run_attempts(id,run_id,attempt_no) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE app.artifacts ADD FOREIGN KEY(producer_run_id,project_id) REFERENCES app.runs(id,project_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE app.artifacts ADD FOREIGN KEY(producer_attempt_id,producer_run_id) REFERENCES app.run_attempts(id,run_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE app.alphas ADD FOREIGN KEY(active_version_id,id) REFERENCES app.alpha_versions(id,alpha_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE app.alpha_versions ADD FOREIGN KEY(calibration_id) REFERENCES app.calibrations DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE app.portfolio_candidates ADD FOREIGN KEY(allocation_evaluation_id,id) REFERENCES app.evaluations(id,subject_candidate_id) DEFERRABLE INITIALLY DEFERRED;

CREATE INDEX cycles_by_project ON app.research_cycles(project_id,ordinal DESC);
CREATE INDEX runs_admission ON app.runs(project_id,state,queued_at);
CREATE INDEX attempts_lease ON app.run_attempts(lease_expires_at) WHERE accepted_at IS NULL;
CREATE INDEX exposures_by_lineage ON app.evidence_exposures(root_lineage_id,dataset_revision_id);
CREATE INDEX wakes_due ON app.wake_events(state,not_before);
CREATE INDEX turn_cycle_accounting ON app.model_turn_reservations(cycle_id);
CREATE INDEX turn_session_accounting ON app.model_turn_reservations(session_id);

-- Reject immutable rewrites even from a mistaken trusted service command.
-- Deployment must also use a non-owner application role, without DDL/TRUNCATE.
CREATE FUNCTION app.reject_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='immutable domain record';
END $$;

CREATE FUNCTION app.guard_revision() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE key text;
BEGIN
  IF NEW.id IS DISTINCT FROM OLD.id OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
    RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='immutable record identity';
  END IF;
  FOREACH key IN ARRAY TG_ARGV LOOP
    IF to_jsonb(NEW)->key IS DISTINCT FROM to_jsonb(OLD)->key THEN
      RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='immutable domain binding';
    END IF;
  END LOOP;
  NEW.revision := OLD.revision + 1;
  NEW.updated_at := clock_timestamp();
  RETURN NEW;
END $$;

CREATE FUNCTION app.guard_frozen_parent() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.frozen_at IS NOT NULL THEN
    RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='frozen domain record';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER frozen_brief BEFORE UPDATE ON app.research_briefs FOR EACH ROW EXECUTE FUNCTION app.guard_frozen_parent();
CREATE TRIGGER frozen_inputs BEFORE UPDATE ON app.input_sets FOR EACH ROW EXECUTE FUNCTION app.guard_frozen_parent();

CREATE FUNCTION app.guard_input_insert() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE frozen app.instant;
BEGIN
  SELECT frozen_at INTO frozen FROM app.input_sets WHERE id=NEW.input_set_id FOR UPDATE;
  IF frozen IS NOT NULL THEN
    RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='cannot extend frozen input set';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER frozen_input_members BEFORE INSERT ON app.input_set_items FOR EACH ROW EXECUTE FUNCTION app.guard_input_insert();
CREATE FUNCTION app.guard_brief_binding() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE frozen app.instant;
BEGIN
  SELECT frozen_at INTO frozen FROM app.research_briefs WHERE id=NEW.brief_id FOR UPDATE;
  IF frozen IS NOT NULL THEN
    RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='cannot extend frozen brief';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER frozen_brief_members BEFORE INSERT ON app.brief_data_bindings FOR EACH ROW EXECUTE FUNCTION app.guard_brief_binding();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.research_lineages FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.projects FOR EACH ROW EXECUTE FUNCTION app.guard_revision('root_lineage_id','created_by');
CREATE TRIGGER no_delete BEFORE DELETE ON app.projects FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.runtime_integrations FOR EACH ROW EXECUTE FUNCTION app.guard_revision();
CREATE TRIGGER no_delete BEFORE DELETE ON app.runtime_integrations FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.downstream_integrations FOR EACH ROW EXECUTE FUNCTION app.guard_revision();
CREATE TRIGGER no_delete BEFORE DELETE ON app.downstream_integrations FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.codex_profiles FOR EACH ROW EXECUTE FUNCTION app.guard_revision('codex_home_ref');
CREATE TRIGGER no_delete BEFORE DELETE ON app.codex_profiles FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.artifacts FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.data_sources FOR EACH ROW EXECUTE FUNCTION app.guard_revision('runtime_id','native_catalog_ref','provider_kind');
CREATE TRIGGER no_delete BEFORE DELETE ON app.data_sources FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.data_use_grants FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.data_use_revocations FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.universe_versions FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.dataset_revisions FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.benchmark_versions FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.execution_assumptions FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.input_sets FOR EACH ROW EXECUTE FUNCTION app.guard_revision('project_id','purpose','decision_cutoff');
CREATE TRIGGER no_delete BEFORE DELETE ON app.input_sets FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.input_set_items FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.evaluation_policies FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.research_briefs FOR EACH ROW EXECUTE FUNCTION app.guard_revision('project_id','version','supersedes_id');
CREATE TRIGGER no_delete BEFORE DELETE ON app.research_briefs FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.brief_data_bindings FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.research_cycles FOR EACH ROW EXECUTE FUNCTION app.guard_revision('project_id','brief_id','ordinal','trigger','wake_id','budget_snapshot');
CREATE TRIGGER no_delete BEFORE DELETE ON app.research_cycles FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.runs FOR EACH ROW EXECUTE FUNCTION app.guard_revision('project_id','cycle_id','kind','input_set_id','deadline_at','queued_at');
CREATE TRIGGER no_delete BEFORE DELETE ON app.runs FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.run_attempts FOR EACH ROW EXECUTE FUNCTION app.guard_revision('run_id','attempt_no','runtime_id');
CREATE TRIGGER no_delete BEFORE DELETE ON app.run_attempts FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.run_events FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.experiment_families FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.experiments FOR EACH ROW EXECUTE FUNCTION app.guard_revision('project_id','cycle_id','family_id','parent_experiment_id','ordinal','hypothesis','expected_failure_modes','proposal_artifact_id','trial_source','native_study_ref','native_trial_id');
CREATE TRIGGER no_delete BEFORE DELETE ON app.experiments FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.alphas FOR EACH ROW EXECUTE FUNCTION app.guard_revision('project_id');
CREATE TRIGGER no_delete BEFORE DELETE ON app.alphas FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.alpha_versions FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.portfolio_mandates FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.portfolio_candidates FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.evaluations FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.metric_values FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.calibrations FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.qualifications FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.qualification_revocations FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.evidence_exposures FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.candidate_alphas FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.candidate_targets FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.releases FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.automation_policies FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.policy_revocations FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.approvals FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.approval_revocations FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.release_decisions FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.handoff_offers FOR EACH ROW EXECUTE FUNCTION app.guard_revision('release_id','approval_id','downstream_id','environment','delivery_sequence','offered_at','expires_at');
CREATE TRIGGER no_delete BEFORE DELETE ON app.handoff_offers FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.forward_messages FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.forward_evidence_windows FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.degradation_observations FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.wake_events FOR EACH ROW EXECUTE FUNCTION app.guard_revision('project_id','observation_id','trigger');
CREATE TRIGGER no_delete BEFORE DELETE ON app.wake_events FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.codex_sessions FOR EACH ROW EXECUTE FUNCTION app.guard_revision('project_id','cycle_id','run_id','role','profile_id','profile_revision','thread_id','codex_version','protocol_schema_version','requested_settings','native_history_ref');
CREATE TRIGGER no_delete BEFORE DELETE ON app.codex_sessions FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.model_turn_reservations FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.model_turn_dispatches FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.model_turn_bindings FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.model_turn_terminals FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.model_turn_receipts FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.operator_auth_state FOR EACH ROW EXECUTE FUNCTION app.guard_revision('singleton');
CREATE TRIGGER no_delete BEFORE DELETE ON app.operator_auth_state FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.trusted_devices FOR EACH ROW EXECUTE FUNCTION app.guard_revision('token_verifier_ref','auth_epoch','expires_at');
CREATE TRIGGER no_delete BEFORE DELETE ON app.trusted_devices FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER identity_revision BEFORE UPDATE ON app.machine_principals FOR EACH ROW EXECUTE FUNCTION app.guard_revision('kind','project_id','downstream_id','run_id');
CREATE TRIGGER no_delete BEFORE DELETE ON app.machine_principals FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.machine_credentials FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.machine_credential_revocations FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.operator_command_grants FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.operator_command_consumptions FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.command_receipts FOR EACH ROW EXECUTE FUNCTION app.reject_change();

SELECT pgmq.create('runs');
SELECT pgmq.create('model_turns');
