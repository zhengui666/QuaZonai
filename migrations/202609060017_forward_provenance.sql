-- Preserve transfer chronology and bind Forward reports to their actual project.
-- These locks precede every audit and remain held through installation.
LOCK TABLE app.handoff_offers, app.forward_messages IN SHARE ROW EXCLUSIVE MODE;

ALTER TABLE app.handoff_offers ADD CONSTRAINT handoff_claim_tuple
  UNIQUE(id,downstream_id,external_claim_id,claimed_at);
CREATE TABLE app.handoff_transfers (
  id app.identity PRIMARY KEY DEFAULT uuidv7(),
  created_at app.instant NOT NULL DEFAULT clock_timestamp(),
  handoff_id app.identity NOT NULL UNIQUE,
  downstream_id app.identity NOT NULL,
  external_claim_id app.nonempty NOT NULL,
  claimed_at app.instant NOT NULL,
  provenance text NOT NULL CHECK(provenance IN ('RECORDED_TRANSITION','LEGACY_CLAIMED_STATE')),
  UNIQUE(handoff_id,downstream_id),
  FOREIGN KEY(handoff_id,downstream_id,external_claim_id,claimed_at)
    REFERENCES app.handoff_offers(id,downstream_id,external_claim_id,claimed_at)
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.handoff_transfers
FOR EACH ROW EXECUTE FUNCTION app.reject_change();

DO $$ BEGIN
  IF EXISTS(SELECT 1 FROM app.handoff_offers
      WHERE state='REJECTED' AND (claimed_at IS NOT NULL OR external_claim_id IS NOT NULL)) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='rejected historical handoff has no independently recorded transfer; preserve for explicit native reconciliation';
  END IF;
END $$;
-- A present claimed/acknowledged state proves only this legacy state, not a
-- fabricated historical transition event. Mark that distinction explicitly.
INSERT INTO app.handoff_transfers(handoff_id,downstream_id,external_claim_id,claimed_at,provenance)
SELECT id,downstream_id,external_claim_id,claimed_at,'LEGACY_CLAIMED_STATE'
FROM app.handoff_offers WHERE state IN ('CLAIMED','ACKNOWLEDGED');

CREATE FUNCTION app.forward_report_valid(handoff uuid, report uuid)
RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT EXISTS(
    SELECT 1 FROM app.handoff_offers h
      JOIN app.releases r ON r.id=h.release_id
      JOIN app.portfolio_candidates c ON c.id=r.candidate_id
      JOIN app.artifacts a ON a.id=report
    WHERE h.id=handoff AND r.environment='REAL' AND a.project_id=c.project_id
      AND a.kind='REPORT' AND a.media_type='application/json'
      AND a.schema_name='qz.forward_report' AND a.schema_version='1'
      AND a.origin='REAL' AND a.access_class='EVALUATOR_ONLY' AND a.byte_count>0
  )
$$;
DO $$ BEGIN
  IF EXISTS(
    SELECT 1 FROM app.forward_messages m LEFT JOIN app.handoff_transfers t
      ON t.handoff_id=m.handoff_id AND t.downstream_id=m.downstream_id
    WHERE t.id IS NULL OR m.created_at<t.claimed_at OR m.received_at<t.claimed_at
      OR NOT app.forward_report_valid(m.handoff_id,m.report_artifact_id)
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='historical Forward chronology or report provenance is invalid; preserve original feedback';
  END IF;
END $$;
ALTER TABLE app.forward_messages ADD CONSTRAINT forward_exact_transfer
  FOREIGN KEY(handoff_id,downstream_id) REFERENCES app.handoff_transfers(handoff_id,downstream_id);

CREATE FUNCTION app.guard_transfer_record() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.provenance<>'RECORDED_TRANSITION' OR NOT EXISTS(
    SELECT 1 FROM app.handoff_offers WHERE id=NEW.handoff_id
      AND downstream_id=NEW.downstream_id AND external_claim_id=NEW.external_claim_id
      AND claimed_at=NEW.claimed_at AND state='CLAIMED' FOR SHARE
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='new transfer must record the exact claimed transition';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER transfer_record BEFORE INSERT ON app.handoff_transfers
FOR EACH ROW EXECUTE FUNCTION app.guard_transfer_record();

CREATE FUNCTION app.record_handoff_transfer() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.state='OFFERED' AND NEW.state='CLAIMED' THEN
    INSERT INTO app.handoff_transfers(handoff_id,downstream_id,external_claim_id,claimed_at,provenance)
    VALUES(NEW.id,NEW.downstream_id,NEW.external_claim_id,NEW.claimed_at,'RECORDED_TRANSITION');
  END IF;
  RETURN NULL;
END $$;
CREATE TRIGGER record_transfer AFTER UPDATE ON app.handoff_offers
FOR EACH ROW EXECUTE FUNCTION app.record_handoff_transfer();

CREATE FUNCTION app.guard_rejection_transfer() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.state='OFFERED' AND NEW.state='REJECTED'
     AND (NEW.claimed_at IS NOT NULL OR NEW.external_claim_id IS NOT NULL) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='rejecting an unclaimed offer cannot fabricate transfer fields';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER rejection_transfer BEFORE UPDATE ON app.handoff_offers
FOR EACH ROW EXECUTE FUNCTION app.guard_rejection_transfer();

CREATE OR REPLACE FUNCTION app.guard_forward_transfer() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE transferred app.handoff_offers%ROWTYPE;
BEGIN
  SELECT * INTO transferred FROM app.handoff_offers
    WHERE id=NEW.handoff_id AND downstream_id=NEW.downstream_id FOR SHARE;
  IF NOT FOUND THEN
    RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='forward message requires its exact handoff/downstream';
  END IF;
  IF transferred.state NOT IN ('CLAIMED','ACKNOWLEDGED') OR transferred.claimed_at IS NULL
     OR NOT EXISTS(SELECT 1 FROM app.handoff_transfers t WHERE t.handoff_id=transferred.id
       AND t.downstream_id=transferred.downstream_id AND t.external_claim_id=transferred.external_claim_id
       AND t.claimed_at=transferred.claimed_at)
     OR NEW.created_at<transferred.claimed_at OR NEW.received_at<transferred.claimed_at THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='Forward feedback must follow its recorded eligible transfer';
  END IF;
  IF NOT app.forward_report_valid(NEW.handoff_id,NEW.report_artifact_id) THEN
    RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='Forward requires the exact project real evaluator-only report and schema';
  END IF;
  RETURN NEW;
END $$;
