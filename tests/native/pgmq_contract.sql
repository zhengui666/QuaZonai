-- W0 compatibility evidence only. Run in an isolated, disposable PostgreSQL.
-- This is not a job implementation or a claim of exactly-once execution.
\set ON_ERROR_STOP on
CREATE EXTENSION IF NOT EXISTS pgmq;
DO $$ BEGIN
    IF (SELECT extversion FROM pg_extension WHERE extname = 'pgmq') <> '1.10.0' THEN
        RAISE EXCEPTION 'PGMQ_VERSION_MISMATCH';
    END IF;
END $$;
SELECT pgmq.create('qz_w0_probe');
CREATE TEMP TABLE probe_facts (id integer PRIMARY KEY, state text NOT NULL);
BEGIN;
INSERT INTO probe_facts VALUES (1, 'QUEUED');
SELECT pgmq.send('qz_w0_probe', '{"origin":"FIXTURE","run_id":"rollback"}'::jsonb);
ROLLBACK;
DO $$ BEGIN
    IF EXISTS (SELECT FROM probe_facts) OR EXISTS (SELECT FROM pgmq.read('qz_w0_probe', 0, 10)) THEN
        RAISE EXCEPTION 'BUSINESS_AND_QUEUE_ROLLBACK_MISMATCH';
    END IF;
END $$;
BEGIN;
INSERT INTO probe_facts VALUES (2, 'QUEUED');
SELECT pgmq.send('qz_w0_probe', '{"origin":"FIXTURE","run_id":"committed"}'::jsonb);
COMMIT;
DO $$
DECLARE first_read record; second_read record;
BEGIN
    SELECT * INTO STRICT first_read FROM pgmq.read('qz_w0_probe', 0, 1);
    SELECT * INTO STRICT second_read FROM pgmq.read('qz_w0_probe', 0, 1);
    IF first_read.msg_id <> second_read.msg_id
       OR second_read.read_ct <> first_read.read_ct + 1
       OR first_read.message->>'origin' <> 'FIXTURE'
       OR first_read.message->>'run_id' <> 'committed' THEN
        RAISE EXCEPTION 'AT_LEAST_ONCE_REDELIVERY_NOT_OBSERVED';
    END IF;
    -- A native archive in a rolled-back savepoint must not destroy the message.
    BEGIN
        UPDATE probe_facts SET state = 'SUCCEEDED' WHERE id = 2;
        PERFORM pgmq.archive('qz_w0_probe', first_read.msg_id);
        RAISE EXCEPTION USING ERRCODE = 'P0004', MESSAGE = 'intentional rollback';
    EXCEPTION WHEN SQLSTATE 'P0004' THEN NULL;
    END;
    IF (SELECT state FROM probe_facts WHERE id = 2) <> 'QUEUED'
       OR NOT EXISTS (SELECT FROM pgmq.read('qz_w0_probe', 0, 1)) THEN
        RAISE EXCEPTION 'RESULT_AND_ACK_ROLLBACK_MISMATCH';
    END IF;
    UPDATE probe_facts SET state = 'SUCCEEDED' WHERE id = 2;
    IF NOT pgmq.archive('qz_w0_probe', first_read.msg_id) THEN
        RAISE EXCEPTION 'NATIVE_ACK_FAILED';
    END IF;
    IF EXISTS (SELECT FROM pgmq.read('qz_w0_probe', 0, 1)) THEN
        RAISE EXCEPTION 'ARCHIVED_MESSAGE_REDELIVERED';
    END IF;
END $$;
SELECT pgmq.drop_queue('qz_w0_probe');
SELECT 'PGMQ_TX_ROLLBACK_REDELIVERY_AND_ACK_PASSED; origin=FIXTURE' AS evidence;
