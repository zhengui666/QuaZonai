-- Optional per-job execution quotas, not a reservation or a host allocation.
-- Existing task definitions, admissions, specs and receipts remain immutable.
-- In particular, an old stored CPU=1 remains 1 even if its CPU budget was null.
ALTER TABLE app.run_native_tasks ALTER COLUMN cpu DROP NOT NULL;
ALTER TABLE app.run_native_tasks DROP CONSTRAINT run_native_tasks_cpu_check;
ALTER TABLE app.run_native_tasks ADD CONSTRAINT run_native_tasks_cpu_check
 CHECK(cpu IS NULL OR cpu BETWEEN 1 AND 4294967295);

-- New Forward admissions inherit explicit null or positive memory/CPU budgets.
-- Missing fields and wrong JSON types do not stand in for an explicit null.
CREATE OR REPLACE FUNCTION app.forward_execution_limits(parent_limits jsonb) RETURNS jsonb
LANGUAGE plpgsql IMMUTABLE STRICT AS $$
DECLARE
 field text;
 value jsonb;
 memory bigint;
 wall bigint;
 cpu numeric;
 output numeric;
BEGIN
 IF jsonb_typeof(parent_limits) IS DISTINCT FROM 'object'
    OR jsonb_typeof(parent_limits->'schema_version') IS DISTINCT FROM 'number'
    OR parent_limits->>'schema_version' IS DISTINCT FROM '1'
    OR NOT (parent_limits ?& ARRAY['schema_version','experiments','cpu_seconds','wall_seconds','memory_mib','output_bytes'])
    OR parent_limits - ARRAY['schema_version','experiments','cpu_seconds','wall_seconds','memory_mib','output_bytes'] <> '{}'::jsonb THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='forward source execution limits are invalid';
 END IF;
 FOR field IN SELECT unnest(ARRAY['experiments','memory_mib','wall_seconds']) LOOP
  value := parent_limits->field;
  IF field IN ('memory_mib','wall_seconds') AND value='null'::jsonb THEN CONTINUE; END IF;
  IF jsonb_typeof(value) IS DISTINCT FROM 'number'
     OR parent_limits->>field !~ '^(0|[1-9][0-9]*)$'
     OR (parent_limits->>field)::numeric > 4294967295
     OR (field<>'experiments' AND (parent_limits->>field)::numeric=0) THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='forward source execution limits are invalid';
  END IF;
 END LOOP;
 FOR field IN SELECT unnest(ARRAY['cpu_seconds','output_bytes']) LOOP
  value := parent_limits->field;
  IF value='null'::jsonb THEN CONTINUE; END IF;
  IF jsonb_typeof(value) IS DISTINCT FROM 'string'
     OR parent_limits->>field !~ '^[1-9][0-9]*$'
     OR (parent_limits->>field)::numeric > 9223372036854775807 THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='forward source execution limits are invalid';
  END IF;
 END LOOP;
 memory := (parent_limits->>'memory_mib')::bigint;
 wall := CASE WHEN parent_limits->>'wall_seconds' IS NULL THEN NULL
              ELSE (parent_limits->>'wall_seconds')::bigint END;
 cpu := (parent_limits->>'cpu_seconds')::numeric;
 output := (parent_limits->>'output_bytes')::numeric;
 IF wall IS NULL AND cpu IS NOT NULL THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='forward finite CPU requires independent enforcement or a wall bound';
 END IF;
 RETURN jsonb_build_object(
  'schema_version',1,'experiments',0,
  'cpu_seconds',CASE WHEN cpu IS NULL THEN NULL::text ELSE cpu::bigint::text END,
  'wall_seconds',wall,'memory_mib',memory,
  'output_bytes',CASE WHEN output IS NULL THEN NULL::text ELSE output::bigint::text END);
END $$;

-- A newly bound Forward task has no CPU quota exactly when its frozen budget
-- has none. Finite ceil(CPU seconds / wall seconds) is unchanged.
CREATE OR REPLACE FUNCTION app.guard_native_task_binding() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE valid boolean;
BEGIN
 PERFORM id FROM app.runs WHERE id=NEW.run_id FOR UPDATE;
 SELECT r.state='QUEUED' AND r.active_attempt_id IS NULL
   AND (r.kind IN ('DATA_VALIDATE','ALPHA_EVALUATE','PORTFOLIO_BUILD','PORTFOLIO_SIMULATE')
     OR (r.kind='FORWARD_EVALUATE' AND NEW.cpu IS NOT DISTINCT FROM CASE WHEN a.limits->>'cpu_seconds' IS NULL THEN NULL
          ELSE ceil((a.limits->>'cpu_seconds')::numeric / NULLIF((a.limits->>'wall_seconds')::numeric,0)) END AND NEW.access_class='EVALUATOR_ONLY' AND p.origin=NEW.origin
       AND NEW.output_schemas='[{"name":"qz.forward_evaluation","version":"1"}]'::jsonb
       AND EXISTS(SELECT 1 FROM app.forward_evaluation_inputs f WHERE f.input_set_id=r.input_set_id
          AND f.project_id=r.project_id AND f.runtime_id=a.runtime_id AND f.parameters_artifact_id=p.id
          AND NEW.origin=app.forward_delivery_origin(f.handoff_id))))
   AND p.project_id=r.project_id AND p.kind='PARAMETERS'
   AND p.media_type='application/json' AND p.schema_name='qz.native_task' AND p.schema_version='1'
   AND p.storage_backend='LOCAL' AND p.storage_object_ref=p.id::text AND p.storage_version='1'
   AND p.byte_count > 0 AND p.access_class=NEW.access_class
   AND p.created_by IN ('OPERATOR','RUNTIME') AND p.producer_run_id IS NULL AND p.producer_attempt_id IS NULL
   AND c.schema_name='qz.runtime_probe' AND c.schema_version='1' AND c.kind='REPORT'
   AND EXISTS(SELECT 1 FROM app.runtime_probe_observations o WHERE o.snapshot_artifact_id=c.id
       AND o.runtime_id=a.runtime_id AND o.integration_revision=a.runtime_revision
       AND o.valid_until>clock_timestamp() AND o.outcome->'result'->>'status'='AVAILABLE')
 INTO valid FROM app.runs r JOIN app.run_admissions a ON a.run_id=r.id
 JOIN app.artifacts p ON p.id=NEW.parameters_artifact_id
 JOIN app.artifacts c ON c.id=NEW.capability_snapshot_artifact_id
 WHERE r.id=NEW.run_id;
 IF valid IS DISTINCT FROM true THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native task definition must bind a new authorized Run';
 END IF;
 RETURN NEW;
END $$;

-- Bind every present limit to the frozen admission and task. JSON numeric and
-- null values are compared without coercing strings or treating missing as null.
-- A historical numeric task is authoritative; never recompute it on a retry.
CREATE OR REPLACE FUNCTION app.guard_native_attempt_spec() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE valid boolean;
BEGIN
 SELECT a.run_id=NEW.run_id AND r.active_attempt_id=a.id AND a.dispatch_state='NOT_SENT'
   AND r.state IN ('DISPATCHING','RECONCILING')
   AND a.lease_expires_at>clock_timestamp() AND (r.deadline_at IS NULL OR r.deadline_at>clock_timestamp())
   AND NEW.spec_json ? 'deadline_at'
   AND jsonb_typeof(NEW.spec_json->'limits')='object'
   AND NEW.spec_json->'limits' ?& ARRAY['cpu','cpu_seconds','memory_mib','wall_seconds','output_bytes']
   AND NEW.spec_json->>'run_id'=r.id::text
   AND NEW.spec_json->>'attempt_no'=a.attempt_no::text
   AND NEW.spec_json->>'owner_epoch'=a.owner_epoch::text
   AND NEW.spec_json->>'external_job_id'=a.external_job_id
   AND NEW.spec_json->>'input_set_id'=r.input_set_id::text
   AND NEW.spec_json->>'job_kind'=r.kind
   AND NEW.spec_json->>'image_ref'=d.image_ref
   AND NEW.spec_json->>'parameters_artifact_id'=d.parameters_artifact_id::text
   AND NEW.spec_json->'limits'->'cpu' IS NOT DISTINCT FROM coalesce(to_jsonb(d.cpu),'null'::jsonb)
   -- JSONB numeric equality accepts 1.0=1, but Rust's u32 wire does not.
   AND (NEW.spec_json->'limits'->'cpu'='null'::jsonb
        OR NEW.spec_json->'limits'->>'cpu' ~ '^[1-9][0-9]*$')
   AND jsonb_typeof(NEW.spec_json->'limits'->'memory_mib') IN ('number','null')
   AND (NEW.spec_json->'limits'->'memory_mib'='null'::jsonb
        OR NEW.spec_json->'limits'->>'memory_mib' ~ '^[1-9][0-9]*$')
   AND NEW.spec_json->'limits'->'cpu_seconds'=admission.limits->'cpu_seconds'
   AND NEW.spec_json->'limits'->'memory_mib'=admission.limits->'memory_mib'
   AND NEW.spec_json->'limits'->'wall_seconds'=admission.limits->'wall_seconds'
   AND NEW.spec_json->'limits'->'output_bytes'=admission.limits->'output_bytes'
   AND NEW.spec_json->'inputs'=d.input_bindings
   AND (NEW.spec_json->>'deadline_at')::timestamptz IS NOT DISTINCT FROM r.deadline_at
   AND NEW.spec_json->'requested_output_schemas'=d.output_schemas
 INTO valid FROM app.run_attempts a JOIN app.runs r ON r.id=a.run_id
 JOIN app.run_admissions admission ON admission.run_id=r.id
 JOIN app.run_native_tasks d ON d.run_id=r.id WHERE a.id=NEW.attempt_id;
 IF valid IS DISTINCT FROM true THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native JobSpec must bind the current unsent Attempt';
 END IF;
 RETURN NEW;
END $$;
