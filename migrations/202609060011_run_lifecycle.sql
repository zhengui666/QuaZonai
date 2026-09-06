-- Zero-argument native triggers are legal; PostgreSQL supplies null TG_ARGV.
CREATE OR REPLACE FUNCTION app.guard_revision() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE key text;
BEGIN
  IF NEW.id IS DISTINCT FROM OLD.id OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
    RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='immutable record identity';
  END IF;
  FOREACH key IN ARRAY COALESCE(TG_ARGV, ARRAY[]::text[]) LOOP
    IF to_jsonb(NEW)->key IS DISTINCT FROM to_jsonb(OLD)->key THEN
      RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='immutable domain binding';
    END IF;
  END LOOP;
  NEW.revision := OLD.revision + 1;
  NEW.updated_at := clock_timestamp();
  RETURN NEW;
END $$;

-- Native queue identity and QZ resource/terminal facts. No second queue/outbox.
CREATE TABLE app.run_admissions (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  run_id app.identity NOT NULL UNIQUE,
  project_id app.identity NOT NULL,
  cycle_id app.identity NOT NULL,
  command_key app.nonempty NOT NULL CHECK(length(command_key)<=200),
  normalized_request app.document NOT NULL,
  initial_snapshot app.document NOT NULL,
  limits app.document NOT NULL,
  runtime_id app.identity NOT NULL REFERENCES app.runtime_integrations,
  runtime_revision app.revision NOT NULL,
  runtime_snapshot app.document NOT NULL,
  initial_queue_message_id bigint NOT NULL UNIQUE CHECK(initial_queue_message_id>0),
  UNIQUE(cycle_id,command_key),
  FOREIGN KEY(run_id,project_id,cycle_id) REFERENCES app.runs(id,project_id,cycle_id)
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.run_admissions
FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TABLE app.run_terminal_receipts (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  run_id app.identity NOT NULL UNIQUE REFERENCES app.runs,
  attempt_id app.identity,
  observation app.document NOT NULL,
  terminal_state text NOT NULL CHECK(terminal_state IN ('SUCCEEDED','FAILED','CANCELLED')),
  result_snapshot app.document NOT NULL,
  FOREIGN KEY(attempt_id,run_id) REFERENCES app.run_attempts(id,run_id)
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.run_terminal_receipts
FOR EACH ROW EXECUTE FUNCTION app.reject_change();

-- A terminal run cannot be resurrected by a later worker, even with a valid row
-- revision. The event trigger may still project the terminal event's cursor.
CREATE FUNCTION app.guard_run_terminal_identity() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.state IN ('SUCCEEDED','FAILED','CANCELLED') AND
    ROW(NEW.state,NEW.current_attempt_no,NEW.active_attempt_id,NEW.finished_at,
        NEW.started_at,NEW.cancellation_requested_at,NEW.terminal_reason_code)
    IS DISTINCT FROM
    ROW(OLD.state,OLD.current_attempt_no,OLD.active_attempt_id,OLD.finished_at,
        OLD.started_at,OLD.cancellation_requested_at,OLD.terminal_reason_code) THEN
    RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='terminal run facts are immutable';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER terminal_run BEFORE UPDATE ON app.runs
FOR EACH ROW EXECUTE FUNCTION app.guard_run_terminal_identity();

CREATE INDEX run_cycle_activity ON app.runs(cycle_id,state);

-- Do not let a standalone existence-only receipt masquerade as a terminal
-- adoption. The same native Run row is locked by the Store's result transaction.
CREATE FUNCTION app.guard_run_terminal_receipt() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE r app.runs;
BEGIN
  SELECT * INTO STRICT r FROM app.runs WHERE id=NEW.run_id FOR UPDATE;
  IF NEW.terminal_state IS DISTINCT FROM r.state
     OR NEW.attempt_id IS DISTINCT FROM r.active_attempt_id
     OR NEW.result_snapshot->>'id' IS DISTINCT FROM r.id::text
     OR NEW.result_snapshot->>'project_id' IS DISTINCT FROM r.project_id::text
     OR NEW.result_snapshot->>'state' IS DISTINCT FROM r.state
     OR NEW.result_snapshot->>'active_attempt_id' IS DISTINCT FROM r.active_attempt_id::text
     OR NEW.result_snapshot->>'revision' IS DISTINCT FROM r.revision::text
     OR NEW.result_snapshot->>'last_event_seq' IS DISTINCT FROM r.last_event_seq::text
     OR r.finished_at IS NULL
  THEN RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='terminal receipt must match the adopted Run';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER terminal_receipt_binding BEFORE INSERT ON app.run_terminal_receipts
FOR EACH ROW EXECUTE FUNCTION app.guard_run_terminal_receipt();
