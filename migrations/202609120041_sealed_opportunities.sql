-- Original native execution identities, not another queue or a mutable budget.
CREATE TABLE app.sealed_evaluation_tasks (
 run_id app.identity PRIMARY KEY REFERENCES app.run_native_tasks,
 alpha_version_id app.identity NOT NULL REFERENCES app.alpha_versions,
 policy_id app.identity NOT NULL REFERENCES app.evaluation_policies,
 validation_evaluation_id app.identity NOT NULL REFERENCES app.evaluations,
 dataset_revision_id app.identity NOT NULL REFERENCES app.dataset_revisions,
 created_at app.instant NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.sealed_evaluation_tasks
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TABLE app.sealed_opportunities (
 attempt_id app.identity PRIMARY KEY REFERENCES app.run_native_attempts,
 exposure_id app.identity NOT NULL UNIQUE REFERENCES app.evidence_exposures,
 created_at app.instant NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.sealed_opportunities
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE FUNCTION app.guard_sealed_opportunity() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE valid boolean;
BEGIN
 SELECT e.root_lineage_id=v.root_lineage_id AND e.dataset_revision_id=t.dataset_revision_id
    AND e.evaluation_id IS NULL AND e.actor_kind='EVALUATOR' AND e.exposure_kind='RAW'
    AND e.actor_session_ref=NEW.attempt_id::text AND e.purpose='NATIVE_SEALED_CAPABILITY_RESERVED'
 INTO valid FROM app.run_native_attempts a
 JOIN app.sealed_evaluation_tasks t ON t.run_id=a.run_id
 JOIN app.alpha_versions v ON v.id=t.alpha_version_id
 JOIN app.evidence_exposures e ON e.id=NEW.exposure_id
 WHERE a.attempt_id=NEW.attempt_id;
 IF valid IS DISTINCT FROM true THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='sealed opportunity must bind the original native Attempt and exposure';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER sealed_opportunity BEFORE INSERT ON app.sealed_opportunities
 FOR EACH ROW EXECUTE FUNCTION app.guard_sealed_opportunity();
