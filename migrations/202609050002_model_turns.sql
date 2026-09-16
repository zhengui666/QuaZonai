-- Domain writers use project -> cycle -> run -> session locking. These triggers
-- also preserve the ledger stage invariants for accidental direct SQL writes.
CREATE FUNCTION app.guard_turn_stage() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE item app.model_turn_reservations; sid uuid;
BEGIN
  IF TG_TABLE_NAME = 'model_turn_reservations' THEN
    sid := NEW.session_id;
  ELSE
    SELECT * INTO STRICT item FROM app.model_turn_reservations WHERE id=NEW.reservation_id;
    sid := item.session_id;
  END IF;
  PERFORM 1 FROM app.codex_sessions WHERE id=sid FOR UPDATE;
  IF TG_TABLE_NAME = 'model_turn_reservations' THEN
    IF EXISTS(SELECT 1 FROM app.model_turn_reservations r
              WHERE r.session_id=sid AND NOT EXISTS(
                SELECT 1 FROM app.model_turn_receipts t WHERE t.reservation_id=r.id)) THEN
      RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='mission already has an unresolved model turn';
    END IF;
  ELSIF TG_TABLE_NAME = 'model_turn_dispatches' THEN
    IF EXISTS(SELECT 1 FROM app.model_turn_terminals WHERE reservation_id=NEW.reservation_id) THEN
      RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='cannot dispatch a settled model turn';
    END IF;
  ELSIF TG_TABLE_NAME = 'model_turn_terminals' THEN
    IF NEW.outcome='NOT_SENT' AND EXISTS(
      SELECT 1 FROM app.model_turn_dispatches WHERE reservation_id=NEW.reservation_id) THEN
      RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='dispatch intent prevents unproven not-sent terminal';
    END IF;
  ELSIF TG_TABLE_NAME = 'model_turn_receipts' THEN
    IF NEW.cost_currency IS DISTINCT FROM item.cost_currency OR
       (NEW.actual_cost IS NULL) <> (item.reserved_cost IS NULL) THEN
      RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='receipt cost accounting differs from reservation';
    END IF;
    IF NEW.outcome='NOT_SENT' THEN
      IF EXISTS(SELECT 1 FROM app.model_turn_dispatches WHERE reservation_id=NEW.reservation_id) THEN
        RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='dispatch intent prevents unproven refund';
      END IF;
    ELSIF NOT EXISTS(SELECT 1 FROM app.model_turn_bindings WHERE reservation_id=NEW.reservation_id) THEN
      RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='native settlement requires an observed turn';
    END IF;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER ledger_stage BEFORE INSERT ON app.model_turn_reservations
FOR EACH ROW EXECUTE FUNCTION app.guard_turn_stage();
CREATE TRIGGER ledger_stage BEFORE INSERT ON app.model_turn_dispatches
FOR EACH ROW EXECUTE FUNCTION app.guard_turn_stage();
CREATE TRIGGER ledger_stage BEFORE INSERT ON app.model_turn_terminals
FOR EACH ROW EXECUTE FUNCTION app.guard_turn_stage();
CREATE TRIGGER ledger_stage BEFORE INSERT ON app.model_turn_receipts
FOR EACH ROW EXECUTE FUNCTION app.guard_turn_stage();

CREATE VIEW app.model_turn_accounting AS
SELECT r.id AS reservation_id, r.project_id, r.cycle_id, r.run_id, r.session_id,
  CASE WHEN t.id IS NULL THEN r.reserved_tokens ELSE 0 END AS reserved_tokens,
  CASE WHEN t.id IS NOT NULL THEN t.actual_tokens ELSE 0 END AS used_tokens,
  CASE WHEN t.id IS NULL THEN r.reserved_cost ELSE 0::numeric END AS reserved_cost,
  CASE WHEN t.id IS NOT NULL THEN t.actual_cost ELSE 0::numeric END AS used_cost,
  r.cost_currency,
  CASE WHEN t.id IS NULL THEN 1 ELSE 0 END AS reserved_turns,
  CASE WHEN t.id IS NOT NULL AND t.outcome<>'NOT_SENT' THEN 1 ELSE 0 END AS used_turns,
  CASE WHEN t.id IS NULL AND r.turn_kind='REPAIR' THEN 1 ELSE 0 END AS reserved_repair_turns,
  CASE WHEN t.id IS NOT NULL AND t.outcome<>'NOT_SENT' AND r.turn_kind='REPAIR' THEN 1 ELSE 0 END AS used_repair_turns
FROM app.model_turn_reservations r LEFT JOIN app.model_turn_receipts t ON t.reservation_id=r.id;
