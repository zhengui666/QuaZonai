-- Field-level control authority. Historical immutable receipts/grants are not
-- rewritten. New commands contain their complete NONSECRET request and result.
ALTER TABLE app.command_receipts ADD COLUMN response_nonsecret_body app.document;
ALTER TABLE app.operator_command_grants ADD COLUMN normalized_nonsecret_request app.document;
ALTER TABLE app.operator_command_grants DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants ADD CONSTRAINT operator_command_grants_operation_check
CHECK(operation IN ('RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
 'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE','CREDENTIAL_ISSUE','CREDENTIAL_REVOKE'));

CREATE FUNCTION app.guard_principal_epoch() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.credential_epoch<OLD.credential_epoch OR
    (NEW.enabled IS DISTINCT FROM OLD.enabled AND NEW.credential_epoch<=OLD.credential_epoch) THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='principal disable/reenable requires a fresh monotonic credential epoch';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER epoch_monotonic BEFORE UPDATE ON app.machine_principals
FOR EACH ROW EXECUTE FUNCTION app.guard_principal_epoch();

CREATE FUNCTION app.lock_credential_revocation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM id FROM app.machine_credentials WHERE id=NEW.credential_id FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='23503',MESSAGE='credential is not registered'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER revocation_serialized BEFORE INSERT ON app.machine_credential_revocations
FOR EACH ROW EXECUTE FUNCTION app.lock_credential_revocation();

CREATE FUNCTION app.guard_operator_grant() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE a app.operator_auth_state; p app.machine_principals; c app.machine_credentials;
BEGIN
 SELECT * INTO STRICT a FROM app.operator_auth_state WHERE singleton FOR SHARE;
 SELECT * INTO STRICT c FROM app.machine_credentials WHERE id=NEW.credential_id;
 SELECT * INTO STRICT p FROM app.machine_principals WHERE id=c.principal_id FOR SHARE;
 PERFORM id FROM app.machine_credentials WHERE id=c.id FOR SHARE;
 IF NOT a.initialized OR NEW.auth_epoch<>a.session_epoch OR p.kind<>'CLI'
    OR NOT p.enabled OR c.principal_epoch<>p.credential_epoch
    OR c.expires_at<=clock_timestamp() OR NEW.expires_at>c.expires_at
    OR NEW.normalized_nonsecret_request IS NULL
    OR EXISTS(SELECT 1 FROM app.machine_credential_revocations r WHERE r.credential_id=c.id AND r.effective_at<=clock_timestamp()) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='operator grant requires current human epoch and active exact CLI credential';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER grant_authority BEFORE INSERT ON app.operator_command_grants
FOR EACH ROW EXECUTE FUNCTION app.guard_operator_grant();

CREATE FUNCTION app.guard_operator_consumption() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE g app.operator_command_grants; a app.operator_auth_state; r app.command_receipts;
BEGIN
 SELECT * INTO STRICT a FROM app.operator_auth_state WHERE singleton FOR SHARE;
 SELECT * INTO STRICT g FROM app.operator_command_grants WHERE id=NEW.grant_id FOR UPDATE;
 IF NOT a.initialized OR g.auth_epoch<>a.session_epoch OR g.expires_at<=clock_timestamp()
    OR g.normalized_nonsecret_request IS NULL THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='operator grant is stale or lacks the exact request snapshot';
 END IF;
 SELECT * INTO STRICT r FROM app.command_receipts WHERE id=NEW.command_receipt_id;
 IF r.principal_scope IS DISTINCT FROM 'CREDENTIAL:'||g.credential_id::text
    OR r.normalized_nonsecret_request IS DISTINCT FROM g.normalized_nonsecret_request THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='operator grant cannot authorize another credential or request body';
 END IF;
 -- Existing composite foreign keys bind the exact operation/target/receipt.
 RETURN NEW;
END $$;
CREATE TRIGGER consumption_authority BEFORE INSERT ON app.operator_command_consumptions
FOR EACH ROW EXECUTE FUNCTION app.guard_operator_consumption();

CREATE FUNCTION app.guard_control_receipt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.operation IN ('PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE',
      'CREDENTIAL_ISSUE','CREDENTIAL_REVOKE','OPERATOR_GRANT_ISSUE')
    AND NEW.response_nonsecret_body IS NULL THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='control command receipt requires the original nonsecret response';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER control_receipt BEFORE INSERT ON app.command_receipts
FOR EACH ROW EXECUTE FUNCTION app.guard_control_receipt();
CREATE INDEX credentials_by_principal ON app.machine_credentials(principal_id,id DESC);
CREATE INDEX credential_revocations_effective ON app.machine_credential_revocations(credential_id,effective_at);
