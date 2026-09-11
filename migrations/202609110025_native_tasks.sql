-- Bind the existing durable Run/Attempt lifecycle to actual fixed native jobs.
-- These are definitions and native object references, not a second task queue.
CREATE TABLE app.run_native_tasks (
 run_id app.identity PRIMARY KEY REFERENCES app.run_admissions(run_id),
 parameters_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 input_bindings jsonb NOT NULL CHECK(jsonb_typeof(input_bindings)='array' AND jsonb_array_length(input_bindings) BETWEEN 1 AND 256),
 image_ref app.nonempty NOT NULL,
 cpu smallint NOT NULL CHECK(cpu BETWEEN 1 AND 1024),
 capability_snapshot_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 output_schemas jsonb NOT NULL CHECK(jsonb_typeof(output_schemas)='array' AND jsonb_array_length(output_schemas) BETWEEN 1 AND 64),
 origin text NOT NULL CHECK(origin IN ('REAL','SYNTHETIC','FIXTURE','LEGACY_UNKNOWN')),
 access_class text NOT NULL CHECK(access_class IN ('RESEARCH','EVALUATOR_ONLY')),
 created_at app.instant NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.run_native_tasks
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE FUNCTION app.guard_native_task_binding() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE valid boolean;
BEGIN
 PERFORM id FROM app.runs WHERE id=NEW.run_id FOR UPDATE;
 SELECT r.state='QUEUED' AND r.active_attempt_id IS NULL
   AND r.kind IN ('DATA_VALIDATE','ALPHA_EVALUATE','PORTFOLIO_BUILD','PORTFOLIO_SIMULATE')
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
CREATE TRIGGER native_task_binding BEFORE INSERT ON app.run_native_tasks
 FOR EACH ROW EXECUTE FUNCTION app.guard_native_task_binding();

CREATE TABLE app.run_native_attempts (
 attempt_id app.identity PRIMARY KEY REFERENCES app.run_attempts,
 run_id app.identity NOT NULL REFERENCES app.run_native_tasks,
 spec_json app.document NOT NULL,
 created_at app.instant NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.run_native_attempts
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE FUNCTION app.guard_native_attempt_spec() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE valid boolean;
BEGIN
 SELECT a.run_id=NEW.run_id AND r.active_attempt_id=a.id AND a.dispatch_state='NOT_SENT'
   AND r.state IN ('DISPATCHING','RECONCILING')
   AND a.lease_expires_at>clock_timestamp() AND r.deadline_at>clock_timestamp()
   AND NEW.spec_json->>'run_id'=r.id::text
   AND NEW.spec_json->>'attempt_no'=a.attempt_no::text
   AND NEW.spec_json->>'owner_epoch'=a.owner_epoch::text
   AND NEW.spec_json->>'external_job_id'=a.external_job_id
   AND NEW.spec_json->>'input_set_id'=r.input_set_id::text
   AND NEW.spec_json->>'job_kind'=r.kind
   AND NEW.spec_json->>'image_ref'=d.image_ref
   AND NEW.spec_json->>'parameters_artifact_id'=d.parameters_artifact_id::text
   AND NEW.spec_json->'limits'->>'cpu'=d.cpu::text
   AND NEW.spec_json->'limits'->'cpu_seconds'=admission.limits->'cpu_seconds'
   AND NEW.spec_json->'limits'->'memory_mib'=admission.limits->'memory_mib'
   AND NEW.spec_json->'limits'->'wall_seconds'=admission.limits->'wall_seconds'
   AND NEW.spec_json->'limits'->'output_bytes'=admission.limits->'output_bytes'
   AND NEW.spec_json->'inputs'=d.input_bindings
   AND (NEW.spec_json->>'deadline_at')::timestamptz=r.deadline_at
   AND NEW.spec_json->'requested_output_schemas'=d.output_schemas
 INTO valid FROM app.run_attempts a JOIN app.runs r ON r.id=a.run_id
 JOIN app.run_admissions admission ON admission.run_id=r.id
 JOIN app.run_native_tasks d ON d.run_id=r.id WHERE a.id=NEW.attempt_id;
 IF valid IS DISTINCT FROM true THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native JobSpec must bind the current unsent Attempt';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER native_attempt_spec BEFORE INSERT ON app.run_native_attempts
 FOR EACH ROW EXECUTE FUNCTION app.guard_native_attempt_spec();

CREATE TABLE app.run_native_outputs (
 attempt_id app.identity NOT NULL REFERENCES app.run_native_attempts,
 remote_storage_ref app.identity NOT NULL,
 artifact_id app.identity NOT NULL UNIQUE REFERENCES app.artifacts,
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(attempt_id,remote_storage_ref)
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.run_native_outputs
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE FUNCTION app.guard_native_output_binding() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE valid boolean;
BEGIN
 SELECT a.producer_attempt_id=n.attempt_id AND a.producer_run_id=n.run_id
   AND a.project_id=r.project_id AND a.created_by='RUNTIME'
   AND a.access_class=d.access_class AND a.origin=d.origin
   AND a.storage_backend='LOCAL' AND a.storage_object_ref=a.id::text AND a.storage_version='1'
   AND a.byte_count>0
 INTO valid FROM app.run_native_attempts n JOIN app.runs r ON r.id=n.run_id
 JOIN app.run_native_tasks d ON d.run_id=r.id JOIN app.artifacts a ON a.id=NEW.artifact_id
 WHERE n.attempt_id=NEW.attempt_id;
 IF valid IS DISTINCT FROM true THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native output must bind its exact producer and source authority';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER native_output_binding BEFORE INSERT ON app.run_native_outputs
 FOR EACH ROW EXECUTE FUNCTION app.guard_native_output_binding();

-- DATA_VALIDATE is an Operator operation targeting its existing frozen InputSet.
-- The actual new Run remains server-assigned and is preserved by the original receipt.
ALTER TABLE app.operator_command_grants DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants ADD CONSTRAINT operator_command_grants_operation_check CHECK(operation IN (
 'RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
 'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE',
 'CREDENTIAL_ISSUE','CREDENTIAL_REVOKE','INPUT_SET_CREATE','EVALUATION_POLICY_CREATE',
 'BRIEF_CREATE','BRIEF_UPDATE','BRIEF_FREEZE','CYCLE_START','INTEGRATION_SECRET_REGISTER',
 'RUNTIME_CREATE','RUNTIME_UPDATE','DOWNSTREAM_CREATE','DOWNSTREAM_UPDATE','RUNTIME_PROBE',
 'DATA_SOURCE_CREATE','DATA_SOURCE_UPDATE','DATA_GRANT_CREATE','DATA_GRANT_REVOKE','DATASET_REGISTER','DATA_VALIDATE'
));
CREATE FUNCTION app.guard_data_validate_receipt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.operation='DATA_VALIDATE' AND NEW.response_nonsecret_body IS NULL THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='data validation requires its original Run response';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER data_validate_receipt BEFORE INSERT ON app.command_receipts
 FOR EACH ROW EXECUTE FUNCTION app.guard_data_validate_receipt();
