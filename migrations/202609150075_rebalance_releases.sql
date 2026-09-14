CREATE TABLE app.portfolio_rebalance_releases (
 build_run_id app.identity PRIMARY KEY REFERENCES app.portfolio_rebalance_studies,
 release_id app.identity NOT NULL UNIQUE REFERENCES app.releases,
 created_at app.instant NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.portfolio_rebalance_releases
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE FUNCTION app.guard_rebalance_release() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM app.portfolio_rebalance_studies child
  JOIN app.portfolio_study_tasks s ON s.run_id=child.study_run_id
  JOIN app.evaluations e ON e.run_id=s.run_id AND e.subject_candidate_id=s.candidate_id
  JOIN app.evaluation_publications p ON p.evaluation_id=e.id
  JOIN app.releases r ON r.candidate_id=s.candidate_id AND r.evaluation_id=e.id
  WHERE child.build_run_id=NEW.build_run_id AND r.id=NEW.release_id
   AND e.evaluation_kind='PORTFOLIO' AND e.execution_status='SUCCEEDED'
   AND e.evidence_status='VALID' AND e.decision='PASS') THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='automatic Release must bind the original Study PASS';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER original_release BEFORE INSERT ON app.portfolio_rebalance_releases
 FOR EACH ROW EXECUTE FUNCTION app.guard_rebalance_release();
