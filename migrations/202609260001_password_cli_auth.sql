-- The password replaces automatic browser admission. Preserve old history while
-- invalidating every pre-password browser session and single-use grant.
ALTER TABLE app.operator_auth_state ADD COLUMN password_verifier text
 CHECK (password_verifier IS NULL OR octet_length(password_verifier) BETWEEN 1 AND 256);
UPDATE app.operator_auth_state SET session_epoch=session_epoch+1 WHERE singleton;

-- Owner CLI devices are independent of scoped Mission/Downstream capabilities.
-- They have no deadline; deletion in the UI records irreversible revocation.
CREATE TABLE app.cli_devices (
 id app.identity PRIMARY KEY DEFAULT uuidv7(),
 name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
 verifier text NOT NULL CHECK (octet_length(verifier) BETWEEN 1 AND 256),
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 last_used_at app.instant NOT NULL DEFAULT clock_timestamp(),
 revoked_at app.instant
);
CREATE FUNCTION app.guard_cli_device() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (to_jsonb(NEW)-ARRAY['last_used_at','revoked_at']) IS DISTINCT FROM
    (to_jsonb(OLD)-ARRAY['last_used_at','revoked_at'])
    OR NEW.last_used_at < OLD.last_used_at
    OR (OLD.revoked_at IS NOT NULL AND NEW.revoked_at IS DISTINCT FROM OLD.revoked_at) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='CLI device cannot be rebound or resurrected';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER immutable_identity BEFORE UPDATE ON app.cli_devices
 FOR EACH ROW EXECUTE FUNCTION app.guard_cli_device();
CREATE TRIGGER no_delete BEFORE DELETE ON app.cli_devices
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
