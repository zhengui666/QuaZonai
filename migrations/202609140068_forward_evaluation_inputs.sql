-- Native protected source membership, not a generic grant to read evaluator objects.
CREATE TABLE app.forward_evaluation_inputs (
 input_set_id app.identity PRIMARY KEY,
 project_id app.identity NOT NULL REFERENCES app.projects,
 handoff_id app.identity NOT NULL REFERENCES app.handoff_offers,
 policy_id app.identity NOT NULL,
 runtime_id app.identity NOT NULL REFERENCES app.runtime_integrations,
 parameters_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 request app.document NOT NULL,
 valid_until app.instant NOT NULL,
 FOREIGN KEY(input_set_id,project_id) REFERENCES app.input_sets(id,project_id),
 FOREIGN KEY(policy_id,project_id) REFERENCES app.automation_policies(id,project_id)
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.forward_evaluation_inputs
FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE UNIQUE INDEX one_forward_run_per_input ON app.runs(input_set_id) WHERE kind='FORWARD_EVALUATE';

-- Preserve the original generic boundary; only the protected fixed feedback intent
-- can enter this additional standalone kind. Every other research kind still needs a Cycle.
CREATE OR REPLACE FUNCTION app.guard_run_admission_identity() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE r app.runs;
BEGIN
 SELECT * INTO STRICT r FROM app.runs WHERE id=NEW.run_id FOR UPDATE;
 IF NEW.project_id IS DISTINCT FROM r.project_id OR NEW.cycle_id IS DISTINCT FROM r.cycle_id THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='run admission must bind the exact project and optional cycle';
 END IF;
 IF NEW.cycle_id IS NULL THEN
  IF r.kind='FORWARD_EVALUATE' THEN
   IF NEW.limits <> '{"schema_version":1,"experiments":0,"cpu_seconds":"30","wall_seconds":60,"memory_mib":512,"output_bytes":"1048576"}'::jsonb
      OR NEW.normalized_request->>'max_parallel_runs' IS DISTINCT FROM '2'
      OR NOT EXISTS(SELECT 1 FROM app.forward_evaluation_inputs f JOIN app.input_sets i ON i.id=f.input_set_id AND i.project_id=f.project_id
        WHERE f.input_set_id=r.input_set_id AND f.project_id=r.project_id AND f.runtime_id=NEW.runtime_id
        AND f.request->>'operation'='EVALUATE_FORWARD' AND i.purpose='FORWARD' AND i.frozen_at IS NOT NULL) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='forward admission requires its exact protected fixed input';
   END IF;
  ELSIF r.kind NOT IN ('IMPORT','EXPORT','DATA_VALIDATE') OR NEW.limits->>'experiments' IS DISTINCT FROM '0' THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='run admission must bind the exact project and optional cycle';
  END IF;
 END IF;
 RETURN NEW;
END $$;

CREATE OR REPLACE FUNCTION app.guard_native_task_binding() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE valid boolean;
BEGIN
 PERFORM id FROM app.runs WHERE id=NEW.run_id FOR UPDATE;
 SELECT r.state='QUEUED' AND r.active_attempt_id IS NULL
   AND (r.kind IN ('DATA_VALIDATE','ALPHA_EVALUATE','PORTFOLIO_BUILD','PORTFOLIO_SIMULATE')
     OR (r.kind='FORWARD_EVALUATE' AND NEW.cpu=1 AND NEW.origin='REAL' AND NEW.access_class='EVALUATOR_ONLY'
       AND NEW.output_schemas='[{"name":"qz.forward_evaluation","version":"1"}]'::jsonb
       AND EXISTS(SELECT 1 FROM app.forward_evaluation_inputs f WHERE f.input_set_id=r.input_set_id
          AND f.project_id=r.project_id AND f.runtime_id=a.runtime_id AND f.parameters_artifact_id=p.id)))
   AND p.project_id=r.project_id AND p.kind='PARAMETERS'
   AND p.media_type='application/json' AND p.schema_name='qz.native_task' AND p.schema_version='1'
   AND p.storage_backend='LOCAL' AND p.storage_object_ref=p.id::text AND p.storage_version='1'
   AND p.byte_count BETWEEN 1 AND 8388608 AND p.access_class=NEW.access_class
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
