-- DOCTOR_READ is not downstream or Agent authority. Keep the existing issuer,
-- epoch, active-Mission and project -> run -> principal lock checks unchanged.
-- Applied migrations remain immutable; this constraint is additive.
CREATE FUNCTION app.guard_doctor_scope() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p app.machine_principals;
BEGIN
  IF NEW.scope_codes && ARRAY['DOCTOR_READ']::text[] THEN
    SELECT * INTO STRICT p FROM app.machine_principals WHERE id=NEW.principal_id FOR SHARE;
    IF p.kind NOT IN ('CLI','AUTOMATION')
       OR NEW.scope_codes IS DISTINCT FROM ARRAY['DOCTOR_READ']::text[] THEN
      RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='doctor scope requires a read-only CLI or automation credential';
    END IF;
  END IF;
  RETURN NEW;
END $$;
-- Alphabetically after issuance_authority, so its existing lock order wins.
CREATE TRIGGER issuance_doctor_boundary BEFORE INSERT ON app.machine_credentials
FOR EACH ROW EXECUTE FUNCTION app.guard_doctor_scope();

-- Never rewrite an immutable historical issuance. An operator must explicitly
-- revoke any previously issued invalid doctor credential before this migration
-- can complete. Revoked evidence remains available for audit.
DO $$ BEGIN
  IF EXISTS (
    SELECT 1 FROM app.machine_credentials c
    JOIN app.machine_principals p ON p.id=c.principal_id
    WHERE c.scope_codes && ARRAY['DOCTOR_READ']::text[]
      AND (p.kind NOT IN ('CLI','AUTOMATION')
           OR c.scope_codes IS DISTINCT FROM ARRAY['DOCTOR_READ']::text[])
      AND NOT EXISTS (
        SELECT 1 FROM app.machine_credential_revocations r
        WHERE r.credential_id=c.id AND r.effective_at<=clock_timestamp()
      )
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='explicitly revoke invalid historical doctor credentials before migrating';
  END IF;
END $$;
