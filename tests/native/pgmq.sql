\set ON_ERROR_STOP on
DO $$
BEGIN
  IF current_database() <> 'qz_native_probe' THEN
    RAISE EXCEPTION 'This destructive fixture is restricted to qz_native_probe';
  END IF;
END $$;

-- Invoke twice with separate connections: -v observe=false, then -v observe=true.
\if :observe
DO $$
DECLARE delivered record;
BEGIN
  IF (SELECT count(*) FROM qz_w0_runs) <> 1
     OR (SELECT count(*) FROM qz_w0_events) <> 1 THEN
    RAISE EXCEPTION 'Business/event transaction was not committed exactly once';
  END IF;
  SELECT * INTO STRICT delivered FROM pgmq.read('qz_w0', 30, 1);
  IF delivered.message <> '{"run_id": "committed"}'::jsonb THEN
    RAISE EXCEPTION 'Observed a rolled-back or unrelated message';
  END IF;
  IF NOT pgmq.archive('qz_w0', delivered.msg_id) THEN
    RAISE EXCEPTION 'Native message archival failed';
  END IF;
  IF (SELECT queue_length FROM pgmq.metrics('qz_w0')) <> 0 THEN
    RAISE EXCEPTION 'Unexpected duplicate message remains';
  END IF;
END $$;
SELECT 'PGMQ_COMMIT_ROLLBACK_SECOND_SESSION_OK' AS evidence;
\else
CREATE EXTENSION IF NOT EXISTS pgmq VERSION '1.10.0';
SELECT pgmq.create('qz_w0');
CREATE TABLE qz_w0_runs (id text PRIMARY KEY);
CREATE TABLE qz_w0_events (run_id text NOT NULL REFERENCES qz_w0_runs(id));
BEGIN;
INSERT INTO qz_w0_runs VALUES ('rolled-back');
INSERT INTO qz_w0_events VALUES ('rolled-back');
SELECT pgmq.send('qz_w0', '{"run_id":"rolled-back"}'::jsonb);
ROLLBACK;

DO $$
BEGIN
  BEGIN
    INSERT INTO qz_w0_runs VALUES ('crashed');
    INSERT INTO qz_w0_events VALUES ('crashed');
    PERFORM pgmq.send('qz_w0', '{"run_id":"crashed"}'::jsonb);
    RAISE EXCEPTION USING ERRCODE = 'P0002', MESSAGE = 'injected-after-send';
  EXCEPTION WHEN no_data_found THEN
    NULL; -- PostgreSQL rolls back this entire subtransaction, including pgmq.send.
  END;
  IF EXISTS (SELECT FROM qz_w0_runs) OR EXISTS (SELECT FROM qz_w0_events)
     OR (SELECT queue_length FROM pgmq.metrics('qz_w0')) <> 0 THEN
    RAISE EXCEPTION 'Rollback leaked business facts, events, or messages';
  END IF;
END $$;

BEGIN;
INSERT INTO qz_w0_runs VALUES ('committed');
INSERT INTO qz_w0_events VALUES ('committed');
SELECT pgmq.send('qz_w0', '{"run_id":"committed"}'::jsonb);
COMMIT;
\endif
