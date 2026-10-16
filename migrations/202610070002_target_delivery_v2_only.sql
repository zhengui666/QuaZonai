-- Upgrade delivery writers only. Existing artifacts, releases, approvals,
-- transfers and command receipts remain immutable, with their original IDs and
-- schema versions. In particular, no V1 package is relabelled or republished.
LOCK TABLE app.artifacts, app.releases, app.approvals, app.handoff_offers
 IN SHARE ROW EXCLUSIVE MODE;

ALTER TABLE app.releases DROP CONSTRAINT releases_execution_environment_check;
ALTER TABLE app.releases ADD CONSTRAINT releases_execution_environment_check
 CHECK(execution_environment IN ('PAPER','LIVE'));
ALTER TABLE app.releases DROP CONSTRAINT release_source_shape;
ALTER TABLE app.releases ADD CONSTRAINT release_source_shape CHECK (
 (source_kind='FORECAST_EVALUATION' AND evaluation_id IS NOT NULL
  AND decision_run_id IS NULL AND decision_attempt_id IS NULL AND decision_report_artifact_id IS NULL
  AND ((package_schema_version='1' AND execution_environment IS NULL)
    OR (package_schema_version='2' AND execution_environment IS NOT NULL)))
 OR
 (source_kind='NATIVE_TARGET_DECISION' AND evaluation_id IS NULL
  AND decision_run_id IS NOT NULL AND decision_attempt_id IS NOT NULL AND decision_report_artifact_id IS NOT NULL
  AND execution_environment IS NOT NULL AND execution_environment='PAPER' AND package_schema_version='2'));

-- This predicate still describes historical identity; new-write policy is a
-- separate trigger so historical checks never pretend V1 was originally V2.
CREATE OR REPLACE FUNCTION app.release_package_valid(candidate uuid, package uuid, version text, environment text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS (
  SELECT 1 FROM app.portfolio_candidates c JOIN app.artifacts a ON a.id=package
  WHERE c.id=candidate AND a.project_id=c.project_id AND a.kind='PACKAGE'
   AND a.media_type='application/json' AND a.schema_name='qz.target_package' AND a.byte_count>0
   AND (
    (c.source_kind='FORECAST' AND a.schema_version=version AND version IN ('1','2')
     AND (environment='DEMO' OR (environment='REAL' AND a.origin='REAL' AND a.access_class='DELIVERY')))
    OR
    (c.source_kind='STRATEGY_ALPHA' AND c.purpose='CURRENT_DECISION'
     AND a.schema_version='2' AND version='2' AND a.access_class='DELIVERY'
     AND ((environment='DEMO' AND a.origin IN ('SYNTHETIC','FIXTURE','LEGACY_UNKNOWN'))
      OR (environment='REAL' AND a.origin='REAL')))
   )
 )
$$;

CREATE FUNCTION app.guard_target_package_v2_write() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.schema_name='qz.target_package' AND NEW.schema_version<>'2' THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='TARGET_PACKAGE_V2_REQUIRED';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER target_package_v2_write BEFORE INSERT ON app.artifacts
 FOR EACH ROW EXECUTE FUNCTION app.guard_target_package_v2_write();

CREATE FUNCTION app.guard_target_release_v2_write() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.package_schema_version<>'2' THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='TARGET_PACKAGE_V2_REQUIRED';
 END IF;
 IF NOT EXISTS (
  SELECT 1 FROM app.portfolio_candidates c WHERE c.id=NEW.candidate_id
   AND ((NEW.source_kind='FORECAST_EVALUATION' AND c.source_kind='FORECAST')
    OR (NEW.source_kind='NATIVE_TARGET_DECISION' AND c.source_kind='STRATEGY_ALPHA'))
 ) THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='release source kind must match its candidate';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER target_release_v2_write BEFORE INSERT ON app.releases
 FOR EACH ROW EXECUTE FUNCTION app.guard_target_release_v2_write();

CREATE FUNCTION app.guard_target_delivery_v2_write() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM app.releases WHERE id=NEW.release_id AND package_schema_version='2') THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='TARGET_PACKAGE_V2_REQUIRED';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER target_delivery_v2_write BEFORE INSERT ON app.approvals
 FOR EACH ROW EXECUTE FUNCTION app.guard_target_delivery_v2_write();
CREATE TRIGGER target_delivery_v2_write BEFORE INSERT ON app.handoff_offers
 FOR EACH ROW EXECUTE FUNCTION app.guard_target_delivery_v2_write();

-- Scope the fence to a new transfer. Historical rejection, acknowledgement,
-- revocation and expiry remain possible and never rewrite the package bytes.
CREATE FUNCTION app.guard_target_claim_v2_write() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.state='OFFERED' AND NEW.state='CLAIMED'
  AND NOT EXISTS(SELECT 1 FROM app.releases WHERE id=NEW.release_id AND package_schema_version='2') THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='TARGET_PACKAGE_V2_REQUIRED';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER target_claim_v2_write BEFORE UPDATE ON app.handoff_offers
 FOR EACH ROW EXECUTE FUNCTION app.guard_target_claim_v2_write();
