-- Add provenance only, not an account ledger or a venue-connection assertion.
-- Existing V1 sources remain NULL. The existing immutable-source trigger forbids
-- updates, so an old source cannot be backfilled with a guessed client identity.
ALTER TABLE app.native_account_sources
 ADD COLUMN native_client_id app.nonempty;

COMMENT ON COLUMN app.native_account_sources.native_client_id IS
 'Original authorized producer client binding; NULL for legacy V1. Not venue authentication proof.';

-- Preserve the complete envelope's protocol identity even though native values
-- remain in the original V1 projection. Older writers default to V1 and cannot
-- append unbound observations to a client-bound source after a rollback.
ALTER TABLE app.native_account_observations
 ADD COLUMN source_schema_version smallint NOT NULL DEFAULT 1
 CHECK(source_schema_version IN (1,2));

CREATE FUNCTION app.guard_native_client_observation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS (
  SELECT 1 FROM app.native_account_sources s WHERE s.id=NEW.source_id
   AND ((s.native_client_id IS NULL AND NEW.source_schema_version=1)
     OR (s.native_client_id IS NOT NULL AND NEW.source_schema_version=2))
 ) THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='native observation protocol must match original client binding';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER native_client_observation_binding BEFORE INSERT ON app.native_account_observations
 FOR EACH ROW EXECUTE FUNCTION app.guard_native_client_observation();
