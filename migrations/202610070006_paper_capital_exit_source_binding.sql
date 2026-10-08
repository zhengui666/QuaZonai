-- Add source-bound Paper owner identity without rewriting 004, its rows or receipts.
-- NULL/NULL on an existing Paper row means legacy identity, never new authority.
ALTER TABLE app.managed_capital_reservations
 ADD COLUMN paper_account_source_id app.identity REFERENCES app.native_account_sources,
 ADD COLUMN paper_native_session_id app.nonempty;
ALTER TABLE app.managed_capital_reservations ADD CONSTRAINT capital_paper_source_shape
 CHECK ((paper_account_source_id IS NULL)=(paper_native_session_id IS NULL)
    AND (environment='PAPER' OR paper_account_source_id IS NULL));

-- Remove exactly the two old label-wide unique constraints. PostgreSQL may
-- truncate their generated names; select by their exact ordered columns.
DO $$
DECLARE old_constraint record; removed integer := 0;
BEGIN
 FOR old_constraint IN
  SELECT c.conname FROM pg_constraint c
  WHERE c.conrelid='app.managed_capital_reservations'::regclass AND c.contype='u'
   AND (SELECT array_agg(a.attname::text ORDER BY k.ordinality)
        FROM unnest(c.conkey) WITH ORDINALITY k(attnum,ordinality)
        JOIN pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=k.attnum)
       IN (ARRAY['environment','venue','native_account_id']::text[],
           ARRAY['downstream_id','environment','native_trader_id','native_account_id','native_client_id']::text[])
 LOOP
  EXECUTE format('ALTER TABLE app.managed_capital_reservations DROP CONSTRAINT %I',old_constraint.conname);
  removed := removed + 1;
 END LOOP;
 IF removed<>2 THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='expected original capital identity constraints';
 END IF;
END $$;
CREATE UNIQUE INDEX capital_live_account_identity ON app.managed_capital_reservations(environment,venue,native_account_id) WHERE environment='LIVE';
CREATE UNIQUE INDEX capital_live_owner_identity ON app.managed_capital_reservations(downstream_id,environment,native_trader_id,native_account_id,native_client_id) WHERE environment='LIVE';
CREATE UNIQUE INDEX capital_paper_source_identity ON app.managed_capital_reservations(paper_account_source_id) WHERE environment='PAPER' AND paper_account_source_id IS NOT NULL;
-- A source is scoped by project/downstream, so another source may describe the
-- SAME engine/account. Such an alias must not create a second Paper allowance.
CREATE UNIQUE INDEX capital_paper_engine_account ON app.managed_capital_reservations(paper_native_session_id,native_account_id) WHERE environment='PAPER' AND paper_account_source_id IS NOT NULL;

CREATE FUNCTION app.guard_paper_capital_exit_source() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='UPDATE' THEN
  IF NEW.paper_account_source_id IS DISTINCT FROM OLD.paper_account_source_id
   OR NEW.paper_native_session_id IS DISTINCT FROM OLD.paper_native_session_id THEN
   RAISE EXCEPTION USING ERRCODE='23000',MESSAGE='original Paper capital source is immutable';
  END IF;
  RETURN NEW;
 END IF;
 IF NEW.environment='PAPER' THEN
  IF NEW.paper_account_source_id IS NULL OR NEW.paper_native_session_id IS NULL
   OR NEW.managed_account_key IS DISTINCT FROM 'paper-native:'||NEW.paper_account_source_id::text
   OR NEW.owner_binding_ref IS DISTINCT FROM 'nautilus-paper:'||NEW.paper_account_source_id::text
   OR NOT EXISTS(SELECT 1 FROM app.native_account_sources s
     WHERE s.id=NEW.paper_account_source_id AND s.environment='PAPER'
      AND s.downstream_id=NEW.downstream_id AND s.native_session_id=NEW.paper_native_session_id
      AND s.native_account_id=NEW.native_account_id AND s.native_trader_id=NEW.native_trader_id
      AND s.native_client_id=NEW.native_client_id
      AND NEW.owner_registration->'paper_source'->>'source_id'=s.id::text
      AND NEW.owner_registration->'paper_source'->'schema_version'='1'::jsonb
      AND NEW.owner_registration->'paper_source'->'binding'=s.binding) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='Paper capital registration requires its exact authenticated source';
  END IF;
  IF EXISTS(SELECT 1 FROM app.capital_exit_intents i
    JOIN app.managed_capital_reservations legacy ON legacy.managed_account_key=i.managed_account_key
    JOIN app.native_account_sources original ON original.id=i.account_source_id
    WHERE legacy.environment='PAPER' AND legacy.paper_account_source_id IS NULL AND original.environment='PAPER'
      AND original.native_session_id=NEW.paper_native_session_id
      AND original.native_account_id=NEW.native_account_id) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='legacy Paper capital intent requires explicit source recovery';
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER capital_paper_source_binding BEFORE INSERT OR UPDATE ON app.managed_capital_reservations FOR EACH ROW EXECUTE FUNCTION app.guard_paper_capital_exit_source();

CREATE FUNCTION app.guard_paper_capital_exit_intent() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM app.managed_capital_reservations r WHERE r.managed_account_key=NEW.managed_account_key AND r.environment='PAPER'
  AND r.paper_account_source_id IS DISTINCT FROM NEW.account_source_id) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='new Paper capital work requires exact source binding';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER capital_paper_intent_source BEFORE INSERT ON app.capital_exit_intents FOR EACH ROW EXECUTE FUNCTION app.guard_paper_capital_exit_intent();
CREATE TRIGGER capital_paper_evidence_source BEFORE INSERT ON app.capital_exit_evidence FOR EACH ROW EXECUTE FUNCTION app.guard_paper_capital_exit_intent();
COMMENT ON COLUMN app.managed_capital_reservations.paper_account_source_id IS 'Exact authorized original Paper account source. Legacy NULL is historical only, never a label-based fallback.';
COMMENT ON COLUMN app.managed_capital_reservations.paper_native_session_id IS 'Original native engine instance/session. Used with native AccountId to prevent cross-project/downstream source aliases from duplicating a budget.';
