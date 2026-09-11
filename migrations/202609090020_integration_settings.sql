-- Add transport configuration without changing applied migrations or credential bytes.
ALTER TABLE app.runtime_integrations
 ADD COLUMN development_http boolean NOT NULL DEFAULT false,
 ADD COLUMN ca_certificate_ref app.nonempty,
 ADD CONSTRAINT runtime_ca_binding CHECK (
   (tls_policy='PINNED_CA') = (ca_certificate_ref IS NOT NULL)
 ) NOT VALID,
 ADD CONSTRAINT runtime_development_transport CHECK (
   NOT development_http OR tls_policy='SYSTEM_CA'
 );
ALTER TABLE app.downstream_integrations
 ADD COLUMN development_http boolean NOT NULL DEFAULT false;

ALTER TABLE app.operator_command_grants
 DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants
 ADD CONSTRAINT operator_command_grants_operation_check CHECK(operation IN (
  'RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
  'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE',
  'CREDENTIAL_ISSUE','CREDENTIAL_REVOKE','INPUT_SET_CREATE','EVALUATION_POLICY_CREATE',
  'BRIEF_CREATE','BRIEF_UPDATE','INTEGRATION_SECRET_REGISTER',
  'RUNTIME_CREATE','RUNTIME_UPDATE','DOWNSTREAM_CREATE','DOWNSTREAM_UPDATE'
 ));

CREATE FUNCTION app.guard_integration_receipt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.operation IN ('INTEGRATION_SECRET_REGISTER','RUNTIME_CREATE','RUNTIME_UPDATE',
                       'DOWNSTREAM_CREATE','DOWNSTREAM_UPDATE')
    AND NEW.response_nonsecret_body IS NULL THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='integration command requires original response';
 END IF;
 IF NEW.operation='INTEGRATION_SECRET_REGISTER' AND (
   NOT (NEW.normalized_nonsecret_request ?& ARRAY['schema_version','purpose','label'])
   OR EXISTS(SELECT 1 FROM jsonb_object_keys(NEW.normalized_nonsecret_request) AS k(key)
             WHERE key NOT IN ('schema_version','purpose','label'))
   OR NEW.response_nonsecret_body->'resource' ? 'value'
 ) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='secret receipt contains non-intent fields';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER integration_receipt BEFORE INSERT ON app.command_receipts
 FOR EACH ROW EXECUTE FUNCTION app.guard_integration_receipt();
