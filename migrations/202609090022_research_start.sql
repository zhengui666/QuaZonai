-- A frozen Brief owns one immutable execution context. The existing input and
-- license services remain the validation authority; no schema-level fake READY.
CREATE TABLE app.brief_execution_contexts (
 brief_id app.identity PRIMARY KEY,
 project_id app.identity NOT NULL,
 runtime_id app.identity NOT NULL REFERENCES app.runtime_integrations,
 runtime_revision app.revision NOT NULL,
 discovery_input_set_id app.identity NOT NULL,
 validation_input_set_id app.identity NOT NULL,
 sealed_input_set_id app.identity NOT NULL,
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(brief_id,project_id) REFERENCES app.research_briefs(id,project_id),
 FOREIGN KEY(discovery_input_set_id,project_id) REFERENCES app.input_sets(id,project_id),
 FOREIGN KEY(validation_input_set_id,project_id) REFERENCES app.input_sets(id,project_id),
 FOREIGN KEY(sealed_input_set_id,project_id) REFERENCES app.input_sets(id,project_id),
 CHECK(discovery_input_set_id<>validation_input_set_id
   AND discovery_input_set_id<>sealed_input_set_id
   AND validation_input_set_id<>sealed_input_set_id)
);
CREATE FUNCTION app.guard_brief_execution_context() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE b app.research_briefs; i app.input_sets; expected_role text; input_id uuid;
BEGIN
 SELECT * INTO STRICT b FROM app.research_briefs WHERE id=NEW.brief_id FOR UPDATE;
 IF b.state<>'DRAFT' OR b.project_id<>NEW.project_id THEN
  RAISE EXCEPTION USING ERRCODE='23000',MESSAGE='execution context requires the exact unfrozen Brief';
 END IF;
 FOR input_id,expected_role IN SELECT * FROM (VALUES
   (NEW.discovery_input_set_id,'DISCOVERY'),(NEW.validation_input_set_id,'VALIDATION'),
   (NEW.sealed_input_set_id,'SEALED')) AS inputs(id,role)
 LOOP
  SELECT * INTO STRICT i FROM app.input_sets WHERE id=input_id FOR SHARE;
  IF i.project_id<>NEW.project_id OR i.purpose<>expected_role OR i.frozen_at IS NULL THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='execution context input mismatch';
  END IF;
 END LOOP;
 RETURN NEW;
END $$;
CREATE TRIGGER execution_context BEFORE INSERT ON app.brief_execution_contexts
 FOR EACH ROW EXECUTE FUNCTION app.guard_brief_execution_context();
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.brief_execution_contexts
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE FUNCTION app.require_context_freeze_commit() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM app.research_briefs WHERE id=NEW.brief_id AND state='FROZEN' AND frozen_at IS NOT NULL) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='execution context and Brief freeze must commit together';
 END IF;
 RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER execution_context_commit AFTER INSERT ON app.brief_execution_contexts
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION app.require_context_freeze_commit();

CREATE TABLE app.cycle_startups (
 cycle_id app.identity PRIMARY KEY,
 project_id app.identity NOT NULL,
 initial_run_id app.identity NOT NULL UNIQUE,
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(cycle_id,project_id) REFERENCES app.research_cycles(id,project_id),
 FOREIGN KEY(initial_run_id,project_id,cycle_id) REFERENCES app.runs(id,project_id,cycle_id)
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.cycle_startups
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE INDEX cycles_by_project_id ON app.research_cycles(project_id,id DESC);
CREATE INDEX cycles_per_day ON app.research_cycles(project_id,created_at);

ALTER TABLE app.operator_command_grants DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants ADD CONSTRAINT operator_command_grants_operation_check CHECK(operation IN (
 'RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
 'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE',
 'CREDENTIAL_ISSUE','CREDENTIAL_REVOKE','INPUT_SET_CREATE','EVALUATION_POLICY_CREATE',
 'BRIEF_CREATE','BRIEF_UPDATE','BRIEF_FREEZE','CYCLE_START','INTEGRATION_SECRET_REGISTER',
 'RUNTIME_CREATE','RUNTIME_UPDATE','DOWNSTREAM_CREATE','DOWNSTREAM_UPDATE','RUNTIME_PROBE'
));
CREATE FUNCTION app.guard_research_start_receipt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.operation IN ('BRIEF_FREEZE','CYCLE_START') AND NEW.response_nonsecret_body IS NULL THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='research start requires original public response';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER research_start_receipt BEFORE INSERT ON app.command_receipts
 FOR EACH ROW EXECUTE FUNCTION app.guard_research_start_receipt();
