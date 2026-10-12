-- This journal owns native remote-job identities, never research budgets or approvals.
CREATE TABLE runtime_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    instance_id TEXT NOT NULL UNIQUE
);

CREATE TABLE input_objects (
    id TEXT PRIMARY KEY,
    storage_version TEXT NOT NULL CHECK (length(storage_version) BETWEEN 1 AND 120),
    bytes BLOB NOT NULL,
    byte_count INTEGER NOT NULL CHECK (byte_count BETWEEN 1 AND 67108864),
    created_us INTEGER NOT NULL,
    CHECK (length(bytes) = byte_count)
) STRICT;

CREATE TABLE runtime_jobs (
    external_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    attempt_no INTEGER NOT NULL CHECK (attempt_no BETWEEN 1 AND 4294967295),
    owner_epoch INTEGER NOT NULL CHECK (owner_epoch > 0),
    spec_json TEXT CHECK (spec_json IS NULL OR json_valid(spec_json)),
    submitted_us INTEGER NOT NULL,
    deadline_us INTEGER NOT NULL,
    phase TEXT NOT NULL CHECK (phase IN ('QUEUED','CREATING','CREATED','STARTING','RUNNING','TERMINAL')),
    launch_json TEXT CHECK (launch_json IS NULL OR json_valid(launch_json)),
    container_id TEXT,
    start_intent_us INTEGER,
    started_us INTEGER,
    cancel_requested_us INTEGER,
    cancel_owner_epoch INTEGER CHECK (cancel_owner_epoch IS NULL OR cancel_owner_epoch > 0),
    stop_code TEXT,
    barrier_id TEXT,
    terminal_state TEXT CHECK (terminal_state IN ('SUCCEEDED','FAILED','CANCELLED')),
    finished_us INTEGER,
    manifest_json TEXT CHECK (manifest_json IS NULL OR json_valid(manifest_json)),
    output_reservation INTEGER NOT NULL CHECK (output_reservation BETWEEN 0 AND 67108864),
    UNIQUE (run_id, attempt_no),
    CHECK ((phase = 'TERMINAL') = (terminal_state IS NOT NULL)),
    CHECK ((terminal_state IS NOT NULL) = (finished_us IS NOT NULL)),
    CHECK ((cancel_requested_us IS NULL) = (cancel_owner_epoch IS NULL)),
    CHECK (cancel_owner_epoch IS NULL OR cancel_owner_epoch >= owner_epoch),
    CHECK (started_us IS NULL OR start_intent_us IS NOT NULL),
    CHECK (start_intent_us IS NULL OR container_id IS NOT NULL),
    CHECK (container_id IS NULL OR launch_json IS NOT NULL),
    CHECK (spec_json IS NOT NULL OR (phase = 'TERMINAL' AND terminal_state = 'CANCELLED' AND cancel_requested_us IS NOT NULL AND launch_json IS NULL)),
    CHECK (phase != 'TERMINAL' OR terminal_state = 'CANCELLED' OR manifest_json IS NOT NULL),
    CHECK (manifest_json IS NULL OR phase = 'TERMINAL'),
    CHECK (phase != 'TERMINAL' OR output_reservation = 0)
) STRICT;
CREATE INDEX runtime_jobs_pending ON runtime_jobs(submitted_us, external_id) WHERE phase != 'TERMINAL';

CREATE TABLE job_outputs (
    external_id TEXT NOT NULL REFERENCES runtime_jobs(external_id),
    storage_ref TEXT NOT NULL,
    metadata_json TEXT NOT NULL CHECK (json_valid(metadata_json)),
    bytes BLOB NOT NULL,
    byte_count INTEGER NOT NULL CHECK (byte_count BETWEEN 1 AND 67108864),
    PRIMARY KEY (external_id, storage_ref),
    CHECK (length(bytes) = byte_count)
) STRICT;

CREATE TRIGGER input_objects_no_update BEFORE UPDATE ON input_objects
BEGIN SELECT RAISE(ABORT, 'native object is immutable'); END;
CREATE TRIGGER input_objects_no_delete BEFORE DELETE ON input_objects
BEGIN SELECT RAISE(ABORT, 'native object retention requires explicit maintenance'); END;
CREATE TRIGGER job_outputs_no_update BEFORE UPDATE ON job_outputs
BEGIN SELECT RAISE(ABORT, 'native output is immutable'); END;
CREATE TRIGGER job_outputs_no_delete BEFORE DELETE ON job_outputs
BEGIN SELECT RAISE(ABORT, 'referenced native output cannot be deleted'); END;
CREATE TRIGGER runtime_jobs_no_delete BEFORE DELETE ON runtime_jobs
BEGIN SELECT RAISE(ABORT, 'native identity tombstone cannot be deleted'); END;
CREATE TRIGGER runtime_jobs_immutable BEFORE UPDATE ON runtime_jobs
WHEN OLD.phase = 'TERMINAL'
  OR NEW.external_id IS NOT OLD.external_id
  OR NEW.run_id IS NOT OLD.run_id
  OR NEW.attempt_no IS NOT OLD.attempt_no
  OR NEW.owner_epoch IS NOT OLD.owner_epoch
  OR NEW.spec_json IS NOT OLD.spec_json
  OR NEW.submitted_us IS NOT OLD.submitted_us
  OR NEW.deadline_us IS NOT OLD.deadline_us
  OR (OLD.launch_json IS NOT NULL AND NEW.launch_json IS NOT OLD.launch_json)
  OR (OLD.container_id IS NOT NULL AND NEW.container_id IS NOT OLD.container_id)
  OR (OLD.start_intent_us IS NOT NULL AND NEW.start_intent_us IS NOT OLD.start_intent_us)
  OR (OLD.started_us IS NOT NULL AND NEW.started_us IS NOT OLD.started_us)
  OR (OLD.cancel_requested_us IS NOT NULL AND NEW.cancel_requested_us IS NOT OLD.cancel_requested_us)
  OR (OLD.cancel_owner_epoch IS NOT NULL AND (NEW.cancel_owner_epoch IS NULL OR NEW.cancel_owner_epoch < OLD.cancel_owner_epoch))
  OR (OLD.stop_code IS NOT NULL AND NEW.stop_code IS NOT OLD.stop_code)
  OR (OLD.barrier_id IS NOT NULL AND NEW.barrier_id IS NOT OLD.barrier_id)
BEGIN SELECT RAISE(ABORT, 'native job identity or terminal fact is immutable'); END;
