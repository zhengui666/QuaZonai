-- Native cache copies consume disk in addition to their original SQLite objects.
-- These are execution-file reservations, not QZ research budgets or a second queue.
CREATE TABLE materialization_reservations (
    external_id TEXT PRIMARY KEY REFERENCES runtime_jobs(external_id),
    byte_count INTEGER NOT NULL CHECK (byte_count > 0),
    reserved_us INTEGER NOT NULL
) STRICT;
CREATE TRIGGER materialization_reservations_no_update
BEFORE UPDATE ON materialization_reservations
BEGIN SELECT RAISE(ABORT, 'native materialization reservation is immutable'); END;
CREATE TRIGGER materialization_reservations_delete_terminal_only
BEFORE DELETE ON materialization_reservations
WHEN NOT EXISTS (SELECT 1 FROM runtime_jobs WHERE external_id=OLD.external_id AND phase='TERMINAL')
BEGIN SELECT RAISE(ABORT, 'active native materialization cannot be released'); END;

-- Capture an actual stopped native process before deleting it to install a
-- cancellation barrier. Reopening the gateway must not erase a prior failure.
CREATE TABLE native_exit_observations (
    external_id TEXT PRIMARY KEY REFERENCES runtime_jobs(external_id),
    container_id TEXT NOT NULL CHECK (length(container_id)=64),
    started_us INTEGER NOT NULL,
    finished_us INTEGER NOT NULL CHECK (finished_us >= started_us),
    exit_code INTEGER NOT NULL,
    oom_killed INTEGER NOT NULL CHECK (oom_killed IN (0,1)),
    failure_code TEXT CHECK (failure_code IN ('MEMORY_LIMIT','DEADLINE_EXCEEDED','NATIVE_JOB_FAILED')),
    observed_us INTEGER NOT NULL CHECK (observed_us >= finished_us-5000000)
) STRICT;
CREATE TRIGGER native_exit_observations_no_update
BEFORE UPDATE ON native_exit_observations
BEGIN SELECT RAISE(ABORT, 'native exit observation is immutable'); END;
CREATE TRIGGER native_exit_observations_no_delete
BEFORE DELETE ON native_exit_observations
BEGIN SELECT RAISE(ABORT, 'native exit observation is retained evidence'); END;
