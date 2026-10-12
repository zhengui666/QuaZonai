-- Preserve the parent table identity and every dependent foreign key. SQLite
-- rewrites the old column internally; no jobs, children or tombstones are deleted.
DROP TRIGGER runtime_jobs_immutable;
ALTER TABLE runtime_jobs RENAME COLUMN deadline_us TO finite_deadline_us;
ALTER TABLE runtime_jobs ADD COLUMN deadline_us INTEGER;
UPDATE runtime_jobs SET deadline_us=finite_deadline_us;
ALTER TABLE runtime_jobs DROP COLUMN finite_deadline_us;
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
