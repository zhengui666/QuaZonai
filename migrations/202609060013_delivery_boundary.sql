-- Native publication/transfer constraints. No legacy row is relabelled or erased.
-- Lock before validating and retain the locks through the entire installation.
LOCK TABLE app.research_lineages, app.releases, app.approvals,
  app.handoff_offers, app.forward_messages IN SHARE ROW EXCLUSIVE MODE;

DO $$ BEGIN
  IF EXISTS (
    WITH RECURSIVE ancestors(root,id,parent_id) AS (
      SELECT id,id,parent_lineage_id FROM app.research_lineages
      UNION ALL
      SELECT a.root,l.id,l.parent_lineage_id FROM ancestors a
        JOIN app.research_lineages l ON l.id=a.parent_id
    ) CYCLE id SET cyclic USING path
    SELECT 1 FROM ancestors WHERE cyclic
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='existing cyclic research lineage requires explicit resolution';
  END IF;
END $$;
ALTER TABLE app.research_lineages ADD CONSTRAINT lineage_not_own_parent
  CHECK(parent_lineage_id IS DISTINCT FROM id);
CREATE FUNCTION app.guard_research_ancestry() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF EXISTS (
    WITH RECURSIVE ancestors(id,parent_id) AS (
      SELECT id,parent_lineage_id FROM app.research_lineages WHERE id=NEW.id
      UNION ALL
      SELECT l.id,l.parent_lineage_id FROM app.research_lineages l
        JOIN ancestors a ON l.id=a.parent_id
    ) CYCLE id SET cyclic USING path
    SELECT 1 FROM ancestors WHERE cyclic
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='cyclic research lineage';
  END IF;
  RETURN NULL;
END $$;
-- Immutable parents plus the immediate FK exclude cross-transaction cycles;
-- queued AFTER ROW checks see all members of a multi-row INSERT.
CREATE CONSTRAINT TRIGGER research_ancestry AFTER INSERT ON app.research_lineages
FOR EACH ROW EXECUTE FUNCTION app.guard_research_ancestry();

CREATE FUNCTION app.release_package_valid(candidate uuid, package uuid, version text, environment text)
RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT EXISTS (
    SELECT 1 FROM app.portfolio_candidates c JOIN app.artifacts a ON a.id=package
    WHERE c.id=candidate AND a.project_id=c.project_id AND a.kind='PACKAGE'
      AND a.media_type='application/json' AND a.schema_name='qz.target_package'
      AND a.schema_version='1' AND version='1' AND a.byte_count>0
      AND (environment='DEMO' OR (environment='REAL' AND a.origin='REAL' AND a.access_class='DELIVERY'))
  )
$$;
DO $$ BEGIN
  IF EXISTS (SELECT 1 FROM app.releases r WHERE NOT app.release_package_valid(
      r.candidate_id,r.package_artifact_id,r.package_schema_version,r.environment))
    OR EXISTS(SELECT 1 FROM app.approvals a JOIN app.releases r ON r.id=a.release_id WHERE r.environment<>'REAL')
    OR EXISTS(SELECT 1 FROM app.handoff_offers h JOIN app.releases r ON r.id=h.release_id WHERE r.environment<>'REAL')
    OR EXISTS(SELECT 1 FROM app.forward_messages m JOIN app.handoff_offers h ON h.id=m.handoff_id
      WHERE h.claimed_at IS NULL OR h.state NOT IN ('CLAIMED','ACKNOWLEDGED','REJECTED')) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='existing delivery identity or transfer provenance is incompatible; preserve history for explicit resolution';
  END IF;
END $$;
CREATE FUNCTION app.guard_release_package() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT app.release_package_valid(NEW.candidate_id,NEW.package_artifact_id,NEW.package_schema_version,NEW.environment) THEN
    RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='release requires its project immutable PACKAGE and exact schema/origin';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER release_package BEFORE INSERT ON app.releases
FOR EACH ROW EXECUTE FUNCTION app.guard_release_package();

CREATE FUNCTION app.guard_real_delivery() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS(SELECT 1 FROM app.releases WHERE id=NEW.release_id AND environment='REAL') THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='DEMO_NOT_DELIVERABLE';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER real_delivery BEFORE INSERT ON app.approvals
FOR EACH ROW EXECUTE FUNCTION app.guard_real_delivery();
CREATE TRIGGER real_delivery BEFORE INSERT ON app.handoff_offers
FOR EACH ROW EXECUTE FUNCTION app.guard_real_delivery();

CREATE FUNCTION app.guard_claim_clock() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE accepted app.instant;
BEGIN
  IF OLD.state='OFFERED' AND NEW.state='CLAIMED' THEN
    -- UPDATE has acquired the row lock before running this trigger.
    accepted:=clock_timestamp();
    IF accepted<OLD.offered_at OR accepted>=OLD.expires_at THEN
      RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='RELEASE_EXPIRED';
    END IF;
    IF NEW.claimed_at IS NULL THEN
      RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='claim requires the explicit transition shape';
    END IF;
    NEW.claimed_at:=accepted;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER claim_clock BEFORE UPDATE ON app.handoff_offers
FOR EACH ROW EXECUTE FUNCTION app.guard_claim_clock();

CREATE FUNCTION app.guard_forward_transfer() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE transferred app.handoff_offers%ROWTYPE;
BEGIN
  SELECT * INTO transferred FROM app.handoff_offers
    WHERE id=NEW.handoff_id AND downstream_id=NEW.downstream_id FOR SHARE;
  IF NOT FOUND THEN
    RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='forward message requires its exact handoff/downstream';
  END IF;
  IF transferred.state NOT IN ('CLAIMED','ACKNOWLEDGED') OR transferred.claimed_at IS NULL THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='forward message requires an eligible transferred handoff';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER forward_transfer BEFORE INSERT ON app.forward_messages
FOR EACH ROW EXECUTE FUNCTION app.guard_forward_transfer();
