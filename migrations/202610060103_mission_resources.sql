-- Host/container CPU counters belong to one exact native launch, never a PID or
-- reusable Run name. Closed launches remain in the cumulative Mission ledger.
CREATE TABLE app.mission_resources (
 id uuid PRIMARY KEY,
 run_id uuid NOT NULL REFERENCES app.runs(id),
 attempt_id uuid NOT NULL REFERENCES app.run_attempts(id),
 owner_epoch bigint NOT NULL CHECK (owner_epoch > 0),
 backend text NOT NULL CHECK (backend IN ('HOST','DOCKER')),
 physical_id text,
 launch_requested boolean NOT NULL DEFAULT false,
 execution_requested boolean NOT NULL DEFAULT false,
 cpu_nanoseconds bigint CHECK (cpu_nanoseconds >= 0),
 accounting_unknown boolean NOT NULL DEFAULT true,
 history_unknown boolean NOT NULL DEFAULT false,
 final_accounted boolean NOT NULL DEFAULT false,
 closed boolean NOT NULL DEFAULT false,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 CHECK (NOT execution_requested OR launch_requested),
 CHECK (NOT final_accounted OR (cpu_nanoseconds IS NOT NULL AND NOT accounting_unknown)),
 CHECK (NOT closed OR final_accounted OR accounting_unknown)
);
CREATE UNIQUE INDEX mission_resources_one_open ON app.mission_resources(run_id) WHERE NOT closed;

-- A receipt/summary is not process-tree cleanup evidence. Reserve/checkpoint
-- lock this same Run first, so a concurrent new launch cannot cross terminality.
CREATE FUNCTION app.guard_mission_resource_terminal() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.kind='AGENT_RESEARCH' AND NEW.state IN ('SUCCEEDED','FAILED','CANCELLED')
    AND EXISTS(SELECT 1 FROM app.mission_resources WHERE run_id=NEW.id AND NOT closed) THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='mission terminal requires confirmed resource cleanup';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER mission_resource_terminal BEFORE UPDATE OF state ON app.runs
 FOR EACH ROW EXECUTE FUNCTION app.guard_mission_resource_terminal();

-- A control-only recovery allowance is created once, never renewed by retries.
CREATE TABLE app.mission_reconciliation_limits (
 run_id uuid PRIMARY KEY REFERENCES app.runs(id),
 attempt_id uuid NOT NULL REFERENCES app.run_attempts(id),
 session_id uuid NOT NULL REFERENCES app.codex_sessions(id),
 limits app.document NOT NULL,
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 deadline_at app.instant NOT NULL,
 CHECK(deadline_at>created_at AND deadline_at<=created_at+interval '110 seconds')
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.mission_reconciliation_limits
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
ALTER TABLE app.mission_resources
 ADD COLUMN purpose text NOT NULL DEFAULT 'RESEARCH' CHECK(purpose IN ('RESEARCH','RECONCILE')),
 ADD COLUMN effective_limits app.document NOT NULL,
 ADD COLUMN deadline_at app.instant;

CREATE FUNCTION app.guard_mission_resource_identity() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (NEW.id,NEW.run_id,NEW.attempt_id,NEW.owner_epoch,NEW.backend,NEW.purpose,NEW.effective_limits,NEW.deadline_at,NEW.history_unknown,NEW.created_at)
       IS DISTINCT FROM
    (OLD.id,OLD.run_id,OLD.attempt_id,OLD.owner_epoch,OLD.backend,OLD.purpose,OLD.effective_limits,OLD.deadline_at,OLD.history_unknown,OLD.created_at)
    OR (OLD.physical_id IS NOT NULL AND NEW.physical_id IS DISTINCT FROM OLD.physical_id)
    OR (OLD.launch_requested AND NOT NEW.launch_requested)
    OR (OLD.execution_requested AND NOT NEW.execution_requested)
    OR (OLD.closed AND NOT NEW.closed)
    OR (OLD.final_accounted AND (NOT NEW.final_accounted OR NEW.cpu_nanoseconds IS DISTINCT FROM OLD.cpu_nanoseconds)) THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='mission resource identity and final facts are immutable';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER identity BEFORE UPDATE ON app.mission_resources
 FOR EACH ROW EXECUTE FUNCTION app.guard_mission_resource_identity();
CREATE TRIGGER retain_ledger BEFORE DELETE ON app.mission_resources
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();

-- Separate research and control measurements; total remains NULL if either
-- component is unknown, including a legacy session without research history.
CREATE VIEW app.mission_resource_accounting AS
WITH totals AS (
 SELECT r.run_id,
   CASE WHEN count(*) FILTER(WHERE purpose='RESEARCH')=0
          AND EXISTS(SELECT 1 FROM app.codex_sessions s WHERE s.run_id=r.run_id)
        THEN NULL
        WHEN bool_or(accounting_unknown OR history_unknown) FILTER(WHERE purpose='RESEARCH')
        THEN NULL ELSE coalesce(sum(cpu_nanoseconds) FILTER(WHERE purpose='RESEARCH'),0) END AS research_cpu_nanoseconds,
   CASE WHEN bool_or(accounting_unknown OR history_unknown) FILTER(WHERE purpose='RECONCILE')
        THEN NULL ELSE coalesce(sum(cpu_nanoseconds) FILTER(WHERE purpose='RECONCILE'),0) END AS reconciliation_cpu_nanoseconds
 FROM app.mission_resources r GROUP BY r.run_id
)
SELECT *, research_cpu_nanoseconds+reconciliation_cpu_nanoseconds AS total_cpu_nanoseconds FROM totals;
