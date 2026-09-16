-- Browser authority stays outside middleware session state, so stale parallel
-- session saves cannot undo logout or device revocation. No raw secrets here.
CREATE TABLE app.bootstrap_capabilities (
 id app.identity PRIMARY KEY DEFAULT uuidv7(),
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 verifier app.nonempty NOT NULL CHECK(octet_length(verifier) <= 256),
 expires_at app.instant NOT NULL,
 consumed_at app.instant,
 CHECK(expires_at > created_at AND expires_at <= created_at + interval '15 minutes')
);
CREATE TABLE app.auth_enrollments (
 id app.identity PRIMARY KEY DEFAULT uuidv7(),
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 capability_id app.identity NOT NULL UNIQUE REFERENCES app.bootstrap_capabilities,
 secret_ref app.identity NOT NULL,
 browser_binding app.nonempty NOT NULL CHECK(octet_length(browser_binding) = 43),
 expires_at app.instant NOT NULL,
 confirmed_at app.instant,
 CHECK(expires_at > created_at AND expires_at <= created_at + interval '10 minutes')
);
CREATE TABLE app.browser_logins (
 id app.identity PRIMARY KEY DEFAULT uuidv7(),
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 auth_epoch app.revision NOT NULL,
 authenticated_at app.instant NOT NULL,
 expires_at app.instant NOT NULL,
 device_id app.identity REFERENCES app.trusted_devices,
 revoked_at app.instant,
 CHECK(expires_at > created_at AND expires_at <= created_at + interval '30 days')
);
CREATE TABLE app.auth_rate_windows (
 operation text PRIMARY KEY CHECK(operation IN ('BOOTSTRAP','LOGIN','REAUTH')),
 window_started_at app.instant NOT NULL,
 attempts integer NOT NULL CHECK(attempts BETWEEN 1 AND 5)
);
CREATE INDEX login_device ON app.browser_logins(device_id) WHERE revoked_at IS NULL;
CREATE INDEX login_expiry ON app.browser_logins(expires_at);
INSERT INTO app.operator_auth_state(initialized,session_epoch) VALUES(false,1);
-- The native session table is created by PostgresStore::migrate under the
-- migration role; runtime service startup never attempts DDL.

CREATE FUNCTION app.guard_browser_auth_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='bootstrap_capabilities' THEN
   IF (to_jsonb(NEW)-'consumed_at') IS DISTINCT FROM (to_jsonb(OLD)-'consumed_at')
      OR OLD.consumed_at IS NOT NULL OR NEW.consumed_at IS NULL THEN
     RAISE EXCEPTION USING ERRCODE='23000',MESSAGE='bootstrap capability is single use';
   END IF;
 ELSIF TG_TABLE_NAME='auth_enrollments' THEN
   IF (to_jsonb(NEW)-'confirmed_at') IS DISTINCT FROM (to_jsonb(OLD)-'confirmed_at')
      OR OLD.confirmed_at IS NOT NULL OR NEW.confirmed_at IS NULL THEN
     RAISE EXCEPTION USING ERRCODE='23000',MESSAGE='enrollment is immutable after confirmation';
   END IF;
 ELSIF TG_TABLE_NAME='browser_logins' THEN
   IF (to_jsonb(NEW)-ARRAY['authenticated_at','revoked_at']) IS DISTINCT FROM
      (to_jsonb(OLD)-ARRAY['authenticated_at','revoked_at'])
      OR (OLD.revoked_at IS NOT NULL AND NEW.revoked_at IS DISTINCT FROM OLD.revoked_at)
      OR NEW.authenticated_at < OLD.authenticated_at THEN
     RAISE EXCEPTION USING ERRCODE='23000',MESSAGE='browser authority cannot be rebound or resurrected';
   END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER auth_transition BEFORE UPDATE ON app.bootstrap_capabilities FOR EACH ROW EXECUTE FUNCTION app.guard_browser_auth_change();
CREATE TRIGGER auth_transition BEFORE UPDATE ON app.auth_enrollments FOR EACH ROW EXECUTE FUNCTION app.guard_browser_auth_change();
CREATE TRIGGER auth_transition BEFORE UPDATE ON app.browser_logins FOR EACH ROW EXECUTE FUNCTION app.guard_browser_auth_change();
CREATE TRIGGER no_delete BEFORE DELETE ON app.bootstrap_capabilities FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE TRIGGER no_delete BEFORE DELETE ON app.auth_enrollments FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE TRIGGER no_delete BEFORE DELETE ON app.browser_logins FOR EACH ROW EXECUTE FUNCTION app.reject_change();
