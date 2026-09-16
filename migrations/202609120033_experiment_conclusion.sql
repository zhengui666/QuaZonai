-- A research Alpha freezes inputs before evaluation; it must not freeze PENDING
-- forever. Publish the initial verdict WITH its new immutable evaluation aggregate.
CREATE OR REPLACE FUNCTION app.guard_consumed_experiment() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE consumed boolean;
BEGIN
 consumed := EXISTS(SELECT 1 FROM app.alpha_versions WHERE experiment_id=OLD.id)
   OR EXISTS(SELECT 1 FROM app.evaluations WHERE run_id=OLD.run_id);
 IF consumed AND ROW(NEW.code_artifact_id,NEW.parameter_artifact_id,NEW.run_id)
    IS DISTINCT FROM ROW(OLD.code_artifact_id,OLD.parameter_artifact_id,OLD.run_id) THEN
  RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='consumed experiment inputs are immutable';
 END IF;
 IF ROW(NEW.outcome,NEW.outcome_reason,NEW.conclusion_artifact_id)
    IS DISTINCT FROM ROW(OLD.outcome,OLD.outcome_reason,OLD.conclusion_artifact_id) THEN
  IF OLD.outcome<>'PENDING' THEN
   RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='published experiment verdict is immutable';
  END IF;
  IF consumed AND (OLD.outcome_reason IS NOT NULL OR OLD.conclusion_artifact_id IS NOT NULL
    OR NEW.outcome='PENDING' OR NEW.conclusion_artifact_id IS NULL
    OR nullif(btrim(NEW.outcome_reason),'') IS NULL
    OR EXISTS(SELECT 1 FROM app.qualifications q JOIN app.alpha_versions a ON a.id=q.alpha_version_id WHERE a.experiment_id=OLD.id)
    OR NOT EXISTS(SELECT 1 FROM app.evaluations e
      JOIN app.alpha_versions a ON a.id=e.subject_alpha_version_id AND a.experiment_id=OLD.id
      JOIN app.research_cycles c ON c.id=OLD.cycle_id
      JOIN app.research_briefs b ON b.id=c.brief_id AND b.state='FROZEN'
      WHERE e.project_id=OLD.project_id AND e.policy_id=b.evaluation_policy_id
        AND e.evaluation_kind IN ('DISCOVERY','WALK_FORWARD','SEALED')
        AND e.report_artifact_id=NEW.conclusion_artifact_id
        AND NOT EXISTS(SELECT 1 FROM app.evaluation_publications p WHERE p.evaluation_id=e.id)
        AND NEW.outcome=CASE
          WHEN e.execution_status='SUCCEEDED' AND e.evidence_status='VALID' AND e.decision='PASS' THEN 'SUPPORTED'
          WHEN e.execution_status='SUCCEEDED' AND e.evidence_status='VALID' AND e.decision='REJECT' THEN 'REJECTED'
          WHEN e.evidence_status='INVALID' THEN 'INVALID'
          ELSE 'INCONCLUSIVE' END)) THEN
   RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='initial consumed verdict requires its new evaluation aggregate';
  END IF;
 END IF;
 RETURN NEW;
END $$;
