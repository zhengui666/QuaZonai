-- Absent Mission/repair/daily Cycle caps do not rewrite frozen historical budgets.
-- Existing numeric JSON caps remain explicit and retain their exact meaning.
-- Remove the unrelated 16-bit ordinal ceiling while preserving positive,
-- append-only, uniquely correlated reservations and all ledger-stage triggers.
ALTER TABLE app.model_turn_reservations
  DROP CONSTRAINT model_turn_reservations_ordinal_check;
ALTER TABLE app.model_turn_reservations
  ALTER COLUMN ordinal TYPE bigint;
ALTER TABLE app.model_turn_reservations
  ADD CONSTRAINT model_turn_reservations_ordinal_check CHECK (ordinal > 0);
