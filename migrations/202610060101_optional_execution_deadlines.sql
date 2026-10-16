-- Explicit absent application deadlines. Existing immutable rows retain their values.
ALTER TABLE app.runs ALTER COLUMN deadline_at DROP NOT NULL;
ALTER TABLE app.model_turn_reservations ALTER COLUMN deadline_at DROP NOT NULL;

CREATE OR REPLACE FUNCTION app.guard_native_attempt_spec() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE valid boolean;
BEGIN
 SELECT a.run_id=NEW.run_id AND r.active_attempt_id=a.id AND a.dispatch_state='NOT_SENT'
   AND r.state IN ('DISPATCHING','RECONCILING')
   AND a.lease_expires_at>clock_timestamp() AND (r.deadline_at IS NULL OR r.deadline_at>clock_timestamp())
   AND NEW.spec_json ? 'deadline_at'
   AND NEW.spec_json->'limits' ? 'wall_seconds'
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

-- Existing credentials remain finite and immutable. Only new no-deadline
-- Missions may use their exact current owner lease as the effective expiry.
ALTER TABLE app.machine_credentials ADD COLUMN lease_bound boolean NOT NULL DEFAULT false;
CREATE OR REPLACE FUNCTION app.guard_machine_issuance() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p app.machine_principals; mission app.runs; project_state text;
BEGIN
  IF NOT app.valid_machine_scopes_v1(NEW.scope_codes) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='invalid machine scope set';
  END IF;
  -- Immutable binding lookup determines the project -> run -> principal lock
  -- order. Authority is reread under the last lock, never granted by this read.
  SELECT * INTO STRICT p FROM app.machine_principals WHERE id=NEW.principal_id;
  IF p.project_id IS NOT NULL THEN
    SELECT state INTO STRICT project_state FROM app.projects WHERE id=p.project_id FOR SHARE;
  END IF;
  IF p.kind='MISSION' THEN
    SELECT * INTO STRICT mission FROM app.runs
      WHERE id=p.run_id AND project_id=p.project_id FOR SHARE;
  END IF;
  SELECT * INTO STRICT p FROM app.machine_principals WHERE id=NEW.principal_id FOR SHARE;
  IF NOT p.enabled OR NEW.principal_epoch<>p.credential_epoch THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='inactive or stale principal issuance';
  END IF;
  IF p.project_id IS NULL AND NEW.scope_codes IS DISTINCT FROM ARRAY['DOCTOR_READ']::text[] THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='non-doctor credential requires a project';
  END IF;
  IF NEW.issued_by='MISSION_SERVICE' AND p.kind<>'MISSION' THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='mission service cannot issue another identity kind';
  END IF;
  IF p.kind='DOWNSTREAM' THEN
    IF NOT NEW.scope_codes <@ ARRAY['DOWNSTREAM_CLAIM','DOWNSTREAM_ACK','FORWARD_SUBMIT','DOCTOR_READ']::text[] THEN
      RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='downstream scope exceeds delivery boundary';
    END IF;
  ELSIF NEW.scope_codes && ARRAY['DOWNSTREAM_CLAIM','DOWNSTREAM_ACK','FORWARD_SUBMIT']::text[] THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='non-downstream principal cannot act as a downstream';
  END IF;
  IF p.kind='MISSION' AND (
      project_state<>'ACTIVE' OR mission.state NOT IN ('DISPATCHING','RUNNING','RECONCILING')
      OR (mission.deadline_at IS NOT NULL AND NEW.expires_at>mission.deadline_at) OR NEW.expires_at<=clock_timestamp()
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='mission credential outlives active work';
  END IF;
  IF NEW.lease_bound AND (p.kind<>'MISSION' OR NEW.issued_by<>'MISSION_SERVICE'
      OR mission.deadline_at IS NOT NULL OR NOT EXISTS (
        SELECT 1 FROM app.run_attempts a WHERE a.id=mission.active_attempt_id
          AND a.run_id=mission.id AND a.id=NEW.issuer_attempt_id
          AND a.owner_epoch=NEW.issuer_owner_epoch AND a.lease_expires_at>clock_timestamp()
          AND NEW.expires_at<=a.lease_expires_at)) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='lease-bound credential requires the exact live Mission owner';
  END IF;
  RETURN NEW;
END $$;
