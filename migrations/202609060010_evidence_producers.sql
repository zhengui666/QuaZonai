-- Add immutable producer and approval evidence bindings without rewriting any
-- applied migration or relabelling historical evidence.
-- Protect validation-to-installation even when executed by native SQLx alone.
LOCK TABLE app.evaluations, app.metric_values, app.approvals IN SHARE ROW EXCLUSIVE MODE;

CREATE FUNCTION app.evaluation_artifacts_valid(project uuid, producer uuid, report uuid, methods uuid)
RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT EXISTS (
    SELECT 1 FROM app.artifacts r JOIN app.artifacts m ON m.id=methods
    WHERE r.id=report AND r.project_id=project AND m.project_id=project
      AND r.producer_run_id=producer AND m.producer_run_id=producer
      AND r.kind='REPORT' AND m.kind='REPORT'
  )
$$;
CREATE FUNCTION app.metric_artifact_valid(evaluation uuid, artifact uuid)
RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT EXISTS (
    SELECT 1 FROM app.evaluations e JOIN app.artifacts a
      ON a.id=artifact AND a.project_id=e.project_id AND a.producer_run_id=e.run_id
    WHERE e.id=evaluation AND a.kind IN ('REPORT','METRICS')
  )
$$;
CREATE FUNCTION app.approval_evidence_valid(release uuid, inputs uuid)
RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT EXISTS (
    SELECT 1 FROM app.releases r
    JOIN app.portfolio_candidates c ON c.id=r.candidate_id
    JOIN app.evaluations e ON e.id=r.evaluation_id AND e.subject_candidate_id=c.id
    JOIN app.input_sets i ON i.id=inputs AND i.project_id=c.project_id
    WHERE r.id=release AND i.frozen_at IS NOT NULL AND i.purpose IN ('PORTFOLIO','FORWARD')
      AND EXISTS (SELECT 1 FROM app.input_set_items x WHERE x.input_set_id=i.id AND x.artifact_id=e.report_artifact_id)
      AND EXISTS (SELECT 1 FROM app.input_set_items x WHERE x.input_set_id=i.id AND x.artifact_id=e.method_versions_artifact_id)
  )
$$;

DO $$ BEGIN
  IF EXISTS (SELECT 1 FROM app.evaluations e
      WHERE NOT app.evaluation_artifacts_valid(e.project_id,e.run_id,e.report_artifact_id,e.method_versions_artifact_id))
     OR EXISTS (SELECT 1 FROM app.metric_values m WHERE NOT app.metric_artifact_valid(m.evaluation_id,m.source_artifact_id))
     OR EXISTS (SELECT 1 FROM app.approvals a WHERE NOT app.approval_evidence_valid(a.release_id,a.evidence_set_id)) THEN
    RAISE EXCEPTION USING ERRCODE='23514',
      MESSAGE='existing evaluation/approval provenance is incompatible; preserve history and resolve explicitly';
  END IF;
END $$;

CREATE FUNCTION app.guard_evaluation_artifacts() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT app.evaluation_artifacts_valid(NEW.project_id,NEW.run_id,NEW.report_artifact_id,NEW.method_versions_artifact_id) THEN
    RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='evaluation reports must be REPORT artifacts from this exact project and run';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER evaluation_artifact_binding BEFORE INSERT ON app.evaluations
FOR EACH ROW EXECUTE FUNCTION app.guard_evaluation_artifacts();

CREATE FUNCTION app.guard_metric_artifact() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT app.metric_artifact_valid(NEW.evaluation_id,NEW.source_artifact_id) THEN
    RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='metric source must be REPORT or METRICS from this evaluation project and run';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER metric_artifact_binding BEFORE INSERT ON app.metric_values
FOR EACH ROW EXECUTE FUNCTION app.guard_metric_artifact();

CREATE FUNCTION app.guard_approval_evidence() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  PERFORM id FROM app.input_sets WHERE id=NEW.evidence_set_id FOR SHARE;
  IF NOT app.approval_evidence_valid(NEW.release_id,NEW.evidence_set_id) THEN
    RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='approval requires frozen release-project evidence containing its exact reports';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER approval_evidence_binding BEFORE INSERT ON app.approvals
FOR EACH ROW EXECUTE FUNCTION app.guard_approval_evidence();
