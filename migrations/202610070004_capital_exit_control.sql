-- Account-capital control only: no order/position/PnL/settlement ledger.
CREATE TABLE app.managed_capital_reservations (
 managed_account_key app.nonempty PRIMARY KEY,
 owner_binding_ref app.nonempty NOT NULL UNIQUE,
 downstream_id app.identity NOT NULL REFERENCES app.downstream_integrations,
 environment text NOT NULL CHECK(environment IN ('PAPER','LIVE')),
 venue app.nonempty NOT NULL,
 native_account_id app.nonempty NOT NULL,
 native_trader_id app.nonempty NOT NULL,
 native_client_id app.nonempty NOT NULL,
 collateral_currency app.nonempty NOT NULL,
 owner_registration app.document NOT NULL,
 revision app.revision NOT NULL DEFAULT 1,
 account_control_epoch bigint NOT NULL DEFAULT 0 CHECK(account_control_epoch>=0),
 reserved_amount numeric(38,18) NOT NULL DEFAULT 0 CHECK(reserved_amount>=0),
 active_intent_id app.identity,
 latest_assessment app.document,
 latest_assessment_artifact_id app.identity REFERENCES app.artifacts,
 observed_managed_capital_after app.document,
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(environment,venue,native_account_id),
 UNIQUE(downstream_id,environment,native_trader_id,native_account_id,native_client_id),
 CHECK((latest_assessment IS NULL)=(latest_assessment_artifact_id IS NULL)),
 CHECK(active_intent_id IS NOT NULL OR reserved_amount=0)
);
CREATE TABLE app.capital_exit_intents (
 id app.identity PRIMARY KEY,
 project_id app.identity NOT NULL REFERENCES app.projects,
 account_source_id app.identity NOT NULL REFERENCES app.native_account_sources,
 managed_account_key app.nonempty NOT NULL REFERENCES app.managed_capital_reservations,
 original_plan_artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 original_request app.document NOT NULL,
 revision app.revision NOT NULL DEFAULT 1,
 state text NOT NULL CHECK(state IN ('REQUESTED','FENCING','CANCELLING_OPENERS','REDUCING','WAITING_EVIDENCE','PAUSED','CANCELLING_EXIT','CANCELLED_RESERVED','RECONCILING_WITHDRAWAL','COMPLETED','BLOCKED')),
 command_id app.identity NOT NULL UNIQUE,
 command_action text NOT NULL DEFAULT 'START' CHECK(command_action IN ('START','PAUSE','CANCEL','RESUME','RECONCILE_WITHDRAWAL')),
 content app.document NOT NULL,
 fence_evidence_id app.identity,
 claim_credential_id app.identity REFERENCES app.machine_credentials,
 availability_observation_id app.identity REFERENCES app.native_account_observations,
 withdrawal_report app.document,
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(id,managed_account_key)
);
ALTER TABLE app.managed_capital_reservations ADD CONSTRAINT capital_exit_active_account
 FOREIGN KEY(active_intent_id,managed_account_key) REFERENCES app.capital_exit_intents(id,managed_account_key) DEFERRABLE INITIALLY DEFERRED;
CREATE TABLE app.capital_exit_evidence (
 id app.identity PRIMARY KEY,
 intent_id app.identity REFERENCES app.capital_exit_intents,
 managed_account_key app.nonempty NOT NULL REFERENCES app.managed_capital_reservations,
 owner_binding_ref app.nonempty NOT NULL,
 account_source_id app.identity NOT NULL REFERENCES app.native_account_sources,
 record_kind text NOT NULL CHECK(record_kind IN ('ASSESSMENT','INTENT')),
 external_message_id app.nonempty NOT NULL,
 sequence bigint NOT NULL CHECK(sequence>0),
 artifact_id app.identity NOT NULL REFERENCES app.artifacts,
 content app.document NOT NULL,
 cash_movement_ref app.nonempty,
 received_at app.instant NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(owner_binding_ref,external_message_id),
 CHECK((record_kind='INTENT')=(intent_id IS NOT NULL)),
 FOREIGN KEY(intent_id,managed_account_key) REFERENCES app.capital_exit_intents(id,managed_account_key)
);
ALTER TABLE app.capital_exit_intents ADD FOREIGN KEY(fence_evidence_id) REFERENCES app.capital_exit_evidence DEFERRABLE INITIALLY DEFERRED;
CREATE UNIQUE INDEX capital_exit_native_movement_once ON app.capital_exit_evidence(managed_account_key,cash_movement_ref) WHERE cash_movement_ref IS NOT NULL;
CREATE UNIQUE INDEX capital_exit_intent_sequence ON app.capital_exit_evidence(intent_id,account_source_id,sequence) WHERE intent_id IS NOT NULL;
CREATE UNIQUE INDEX capital_exit_assessment_sequence ON app.capital_exit_evidence(owner_binding_ref,account_source_id,sequence) WHERE intent_id IS NULL;
CREATE INDEX capital_exit_project_page ON app.capital_exit_intents(project_id,id DESC);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.capital_exit_evidence FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE FUNCTION app.guard_managed_capital_identity() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION USING ERRCODE='23000',MESSAGE='managed account reservation cannot be deleted'; END IF;
 IF NEW.managed_account_key IS DISTINCT FROM OLD.managed_account_key OR NEW.owner_registration IS DISTINCT FROM OLD.owner_registration
   OR NEW.owner_binding_ref IS DISTINCT FROM OLD.owner_binding_ref OR NEW.downstream_id IS DISTINCT FROM OLD.downstream_id
   OR NEW.environment IS DISTINCT FROM OLD.environment OR NEW.venue IS DISTINCT FROM OLD.venue
   OR NEW.native_account_id IS DISTINCT FROM OLD.native_account_id OR NEW.native_trader_id IS DISTINCT FROM OLD.native_trader_id
   OR NEW.native_client_id IS DISTINCT FROM OLD.native_client_id OR NEW.collateral_currency IS DISTINCT FROM OLD.collateral_currency
   OR NEW.created_at IS DISTINCT FROM OLD.created_at OR NEW.revision<OLD.revision OR NEW.account_control_epoch<OLD.account_control_epoch THEN
  RAISE EXCEPTION USING ERRCODE='23000',MESSAGE='managed account identity and epoch are durable';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER capital_identity BEFORE UPDATE OR DELETE ON app.managed_capital_reservations FOR EACH ROW EXECUTE FUNCTION app.guard_managed_capital_identity();
CREATE FUNCTION app.guard_capital_exit_identity() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION USING ERRCODE='23000',MESSAGE='capital exit intent cannot be deleted'; END IF;
 IF NEW.id IS DISTINCT FROM OLD.id OR NEW.project_id IS DISTINCT FROM OLD.project_id OR NEW.account_source_id IS DISTINCT FROM OLD.account_source_id
   OR NEW.managed_account_key IS DISTINCT FROM OLD.managed_account_key OR NEW.original_plan_artifact_id IS DISTINCT FROM OLD.original_plan_artifact_id
   OR NEW.original_request IS DISTINCT FROM OLD.original_request OR NEW.created_at IS DISTINCT FROM OLD.created_at OR NEW.revision<OLD.revision THEN
  RAISE EXCEPTION USING ERRCODE='23000',MESSAGE='capital exit original plan and identity are immutable';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER capital_exit_identity BEFORE UPDATE OR DELETE ON app.capital_exit_intents FOR EACH ROW EXECUTE FUNCTION app.guard_capital_exit_identity();
SELECT pgmq.create('capital_exits');
COMMENT ON TABLE app.managed_capital_reservations IS 'Managed-owner reinvestment budget only. This is not a wallet lock, venue balance or execution ledger.';
COMMENT ON TABLE app.capital_exit_evidence IS 'Original authenticated native evidence. User-reported withdrawals and isolated balances are not transfer proof.';
