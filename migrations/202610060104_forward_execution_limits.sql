-- New forward measurements inherit the exact immutable candidate admission.
-- Historical admission rows, native task rows and replay snapshots are untouched.
CREATE FUNCTION app.forward_execution_limits(parent_limits jsonb) RETURNS jsonb
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
 memory := LEAST((parent_limits->>'memory_mib')::bigint,512);
 wall := CASE WHEN parent_limits->>'wall_seconds' IS NULL THEN NULL
              ELSE LEAST((parent_limits->>'wall_seconds')::bigint,60) END;
 cpu := (parent_limits->>'cpu_seconds')::numeric;
 output := (parent_limits->>'output_bytes')::numeric;
 IF wall IS NULL AND cpu IS NOT NULL THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='forward finite CPU requires independent enforcement or a wall bound';
 END IF;
 RETURN jsonb_build_object(
  'schema_version',1,'experiments',0,
  'cpu_seconds',CASE WHEN cpu IS NULL THEN NULL::text ELSE LEAST(cpu,30,wall)::bigint::text END,
  'wall_seconds',wall,'memory_mib',memory,
  'output_bytes',CASE WHEN output IS NULL THEN NULL::text ELSE LEAST(output,1048576)::bigint::text END);
END $$;

-- This remains an INSERT guard. Existing finite Runs replay their original
-- immutable receipt; there is no legacy-tuple OR inherited-tuple bypass for new Runs.
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
      AND h.state IN ('CLAIMED','ACKNOWLEDGED') AND released.environment='REAL'
      AND transfer.provenance='RECORDED_TRANSITION';
   IF expected_limits IS NULL OR NEW.limits IS DISTINCT FROM expected_limits
      OR NEW.normalized_request->>'max_parallel_runs' IS DISTINCT FROM '2' THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='forward admission requires its exact inherited protected input';
   END IF;
  ELSIF r.kind NOT IN ('IMPORT','EXPORT','DATA_VALIDATE') OR NEW.limits->>'experiments' IS DISTINCT FROM '0' THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='run admission must bind the exact project and optional cycle';
  END IF;
 END IF;
 RETURN NEW;
END $$;
-- guard_native_task_binding from 068 retains one CPU, REAL/EVALUATOR_ONLY,
-- the exact protected parameter artifact, capability receipt and output schema.
