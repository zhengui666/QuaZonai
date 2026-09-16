CREATE TABLE app.portfolio_rebalance_studies (
 build_run_id app.identity PRIMARY KEY REFERENCES app.portfolio_rebalances,
 study_run_id app.identity NOT NULL UNIQUE REFERENCES app.portfolio_study_tasks,
 created_at app.instant NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.portfolio_rebalance_studies
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE FUNCTION app.guard_rebalance_study() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM app.portfolio_rebalances b
  JOIN app.portfolio_build_tasks build ON build.run_id=b.run_id
  JOIN app.portfolio_candidates c ON c.run_id=b.run_id AND c.project_id=b.project_id AND c.mandate_id=b.mandate_id
  JOIN app.candidate_publications p ON p.candidate_id=c.id
  JOIN app.portfolio_study_tasks s ON s.candidate_id=c.id
  JOIN app.runs r ON r.id=s.run_id AND r.project_id=b.project_id AND r.kind='PORTFOLIO_SIMULATE'
  WHERE b.run_id=NEW.build_run_id AND s.run_id=NEW.study_run_id
    AND s.request->>'cycle_id'=build.request->>'cycle_id'
    AND s.request->>'runtime_id'=build.request->>'runtime_id'
    AND s.request->>'expected_runtime_revision'=build.request->>'expected_runtime_revision'
    AND s.request->'limits'=build.request->'limits') THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='automatic Study must bind the original Build and limits';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER original_study BEFORE INSERT ON app.portfolio_rebalance_studies
 FOR EACH ROW EXECUTE FUNCTION app.guard_rebalance_study();
