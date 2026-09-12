-- Preserve the original native Mission role without widening artifact reads.
CREATE OR REPLACE FUNCTION app.guard_mission_summary() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM app.model_turn_reservations r
   JOIN app.codex_sessions session ON session.id=r.session_id AND session.run_id=r.run_id
   JOIN app.model_turn_receipts t ON t.reservation_id=r.id AND t.outcome='SUCCEEDED'
   JOIN app.model_turn_bindings b ON b.reservation_id=r.id
   JOIN app.model_turn_terminals terminal ON terminal.reservation_id=r.id AND terminal.outcome=t.outcome AND terminal.native_turn_id=b.native_turn_id
   JOIN app.artifacts a ON a.id=NEW.artifact_id
   WHERE r.id=NEW.reservation_id AND a.project_id=r.project_id AND a.producer_run_id=r.run_id
    AND a.producer_attempt_id=r.attempt_id AND a.kind='REPORT' AND a.schema_name='qz.mission_summary'
    AND a.schema_version='1' AND a.media_type='application/json'
    AND a.access_class=CASE session.role WHEN 'RESEARCHER' THEN 'RESEARCH' WHEN 'INDEPENDENT_REVIEWER' THEN 'EVALUATOR_ONLY' END
    AND a.origin='SYNTHETIC' AND a.created_by='RUNTIME' AND a.storage_backend='LOCAL'
    AND a.storage_object_ref=a.id::text AND a.storage_version='1' AND a.byte_count BETWEEN 1 AND 1048576) THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='public summary requires the exact successful native Turn, role and producer';
 END IF;
 RETURN NEW;
END $$;
