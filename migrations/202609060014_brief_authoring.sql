-- Draft editing and frozen membership serialize through the same parent row.
-- Do not change any applied migration or rewrite a historical Brief.
DROP TRIGGER immutable ON app.brief_data_bindings;
DROP TRIGGER frozen_brief_members ON app.brief_data_bindings;
CREATE OR REPLACE FUNCTION app.guard_brief_binding() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE parent_id uuid; parent_state text;
BEGIN
 parent_id:=CASE WHEN TG_OP='DELETE' THEN OLD.brief_id ELSE NEW.brief_id END;
 IF TG_OP='UPDATE' AND (NEW.id,NEW.created_at,NEW.brief_id) IS DISTINCT FROM (OLD.id,OLD.created_at,OLD.brief_id) THEN
  RAISE EXCEPTION USING ERRCODE='23000',MESSAGE='immutable Brief member identity';
 END IF;
 SELECT state INTO STRICT parent_state FROM app.research_briefs WHERE id=parent_id FOR UPDATE;
 IF parent_state<>'DRAFT' THEN RAISE EXCEPTION USING ERRCODE='23000',MESSAGE='frozen Brief membership'; END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER frozen_brief_members BEFORE INSERT OR UPDATE OR DELETE ON app.brief_data_bindings
 FOR EACH ROW EXECUTE FUNCTION app.guard_brief_binding();
ALTER TABLE app.operator_command_grants DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants ADD CONSTRAINT operator_command_grants_operation_check CHECK(operation IN (
 'RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
 'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE','CREDENTIAL_ISSUE','CREDENTIAL_REVOKE',
 'INPUT_SET_CREATE','EVALUATION_POLICY_CREATE','BRIEF_CREATE','BRIEF_UPDATE'));
CREATE FUNCTION app.guard_brief_receipt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.operation IN ('BRIEF_CREATE','BRIEF_UPDATE') AND NEW.response_nonsecret_body IS NULL THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='Brief command requires original public response';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER brief_receipt BEFORE INSERT ON app.command_receipts FOR EACH ROW EXECUTE FUNCTION app.guard_brief_receipt();
CREATE INDEX briefs_by_project_id ON app.research_briefs(project_id,id DESC);
