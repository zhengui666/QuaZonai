-- Widen only the native positive CPU grant representation. Preserve every
-- existing immutable task/spec and explicit numeric budget unchanged.
ALTER TABLE app.run_native_tasks DROP CONSTRAINT run_native_tasks_cpu_check;
ALTER TABLE app.run_native_tasks ALTER COLUMN cpu TYPE bigint;
ALTER TABLE app.run_native_tasks ADD CONSTRAINT run_native_tasks_cpu_check
 CHECK(cpu BETWEEN 1 AND 4294967295);

-- New forward measurements inherit the exact immutable candidate admission.
-- Historical admission rows, native task rows and replay snapshots are untouched.
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
    OR parent_limits - ARRAY['schema_version','experiments','cpu_seconds','wall_seconds','memory_mib','output_bytes'] <> '{}'::jsonb THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='forward source execution limits are invalid';
 END IF;
 FOR field IN SELECT unnest(ARRAY['experiments','memory_mib','wall_seconds']) LOOP
  value := parent_limits->field;
  IF field='wall_seconds' AND (value IS NULL OR value='null'::jsonb) THEN CONTINUE; END IF;
  IF jsonb_typeof(value) IS DISTINCT FROM 'number'
     OR parent_limits->>field !~ '^(0|[1-9][0-9]*)$'
     OR (parent_limits->>field)::numeric > 4294967295
     OR (field<>'experiments' AND (parent_limits->>field)::numeric=0) THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='forward source execution limits are invalid';
  END IF;
 END LOOP;
 FOR field IN SELECT unnest(ARRAY['cpu_seconds','output_bytes']) LOOP
  value := parent_limits->field;
  IF value IS NULL OR value='null'::jsonb THEN CONTINUE; END IF;
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


-- Preserve the complete Paper/Real provenance and exact normalized identity.
CREATE OR REPLACE FUNCTION app.guard_run_admission_identity() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE r app.runs; expected_limits jsonb;
BEGIN
 SELECT * INTO STRICT r FROM app.runs WHERE id=NEW.run_id FOR UPDATE;
 IF NEW.project_id IS DISTINCT FROM r.project_id OR NEW.cycle_id IS DISTINCT FROM r.cycle_id THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='run admission must bind the exact project and optional cycle';
 END IF;
 IF NEW.cycle_id IS NULL THEN
  IF r.kind='FORWARD_EVALUATE' THEN
   SELECT app.forward_execution_limits(a.limits) INTO expected_limits
    FROM app.forward_evaluation_inputs f
    JOIN app.input_sets i ON i.id=f.input_set_id AND i.project_id=f.project_id
    JOIN app.handoff_offers h ON h.id=f.handoff_id
    JOIN app.releases released ON released.id=h.release_id
    JOIN app.portfolio_candidates candidate ON candidate.id=released.candidate_id AND candidate.project_id=f.project_id
    JOIN app.run_admissions a ON a.run_id=candidate.run_id AND a.project_id=candidate.project_id
    JOIN app.handoff_transfers transfer ON transfer.handoff_id=h.id AND transfer.downstream_id=h.downstream_id
       AND transfer.external_claim_id=h.external_claim_id AND transfer.claimed_at=h.claimed_at
    WHERE f.input_set_id=r.input_set_id AND f.project_id=r.project_id AND f.runtime_id=NEW.runtime_id
      AND a.runtime_id=NEW.runtime_id AND f.request->>'operation'='EVALUATE_FORWARD'
      AND i.purpose='FORWARD' AND i.frozen_at IS NOT NULL
      AND h.state IN ('CLAIMED','ACKNOWLEDGED') AND app.forward_delivery_origin(h.id) IS NOT NULL
      AND transfer.provenance='RECORDED_TRANSITION';
   IF expected_limits IS NULL OR NEW.limits IS DISTINCT FROM expected_limits
      OR NOT (NEW.normalized_request ? 'max_parallel_runs')
      OR (NEW.normalized_request->'max_parallel_runs' IS DISTINCT FROM 'null'::jsonb AND
          (jsonb_typeof(NEW.normalized_request->'max_parallel_runs') IS DISTINCT FROM 'number'
           OR NEW.normalized_request->>'max_parallel_runs' !~ '^[1-9][0-9]*$'
           OR (NEW.normalized_request->>'max_parallel_runs')::numeric > 4294967295))
      OR NEW.normalized_request IS DISTINCT FROM jsonb_build_object(
          'schema_version',1,'project_id',r.project_id,'input_set_id',r.input_set_id,
          'runtime_id',NEW.runtime_id,'runtime_revision',NEW.runtime_revision::text,
          'kind',r.kind,'limits',NEW.limits,'max_parallel_runs',NEW.normalized_request->'max_parallel_runs') THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='forward admission requires its exact inherited protected input';
   END IF;
  ELSIF r.kind NOT IN ('IMPORT','EXPORT','DATA_VALIDATE') OR NEW.limits->>'experiments' IS DISTINCT FROM '0' THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='run admission must bind the exact project and optional cycle';
  END IF;
 END IF;
 RETURN NEW;
END $$;


-- Finite grants use the exact inherited CPU/wall allocation rather than a fixed core.
CREATE OR REPLACE FUNCTION app.guard_native_task_binding() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE valid boolean;
BEGIN
 PERFORM id FROM app.runs WHERE id=NEW.run_id FOR UPDATE;
 SELECT r.state='QUEUED' AND r.active_attempt_id IS NULL
   AND (r.kind IN ('DATA_VALIDATE','ALPHA_EVALUATE','PORTFOLIO_BUILD','PORTFOLIO_SIMULATE')
     OR (r.kind='FORWARD_EVALUATE' AND NEW.cpu=CASE WHEN a.limits->>'cpu_seconds' IS NULL THEN 1
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
