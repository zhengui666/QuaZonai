-- Original automatic Build provenance; policies cannot reset the cutoff identity.
CREATE TABLE app.portfolio_rebalances (
 run_id app.identity PRIMARY KEY REFERENCES app.portfolio_build_tasks,
 project_id app.identity NOT NULL REFERENCES app.projects,
 mandate_id app.identity NOT NULL REFERENCES app.portfolio_mandates,
 downstream_id app.identity NOT NULL REFERENCES app.downstream_integrations,
 policy_id app.identity NOT NULL REFERENCES app.automation_policies,
 source_candidate_id app.identity NOT NULL REFERENCES app.portfolio_candidates,
 input_set_id app.identity NOT NULL REFERENCES app.input_sets,
 decision_cutoff app.instant NOT NULL,
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(project_id,mandate_id,downstream_id,decision_cutoff)
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.portfolio_rebalances
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE FUNCTION app.guard_portfolio_rebalance() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM app.portfolio_build_tasks t
  JOIN app.runs r ON r.id=t.run_id AND r.project_id=NEW.project_id AND r.input_set_id=NEW.input_set_id AND r.kind='PORTFOLIO_BUILD'
  JOIN app.input_sets i ON i.id=r.input_set_id AND i.project_id=r.project_id AND i.decision_cutoff>=NEW.decision_cutoff
  JOIN app.automation_policies p ON p.id=NEW.policy_id AND p.project_id=r.project_id AND p.mandate_id=t.mandate_id AND p.downstream_id=NEW.downstream_id
  JOIN app.portfolio_candidates c ON c.id=NEW.source_candidate_id AND c.project_id=r.project_id AND c.mandate_id=t.mandate_id AND c.decision_asof<i.decision_cutoff
  JOIN app.forward_weight_snapshots w ON w.id=t.snapshot_id AND w.project_id=r.project_id AND w.downstream_id=p.downstream_id AND w.environment=t.request->>'environment'
  WHERE t.run_id=NEW.run_id AND t.mandate_id=NEW.mandate_id) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='automatic Build must bind original policy, source and cutoff';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER original_rebalance BEFORE INSERT ON app.portfolio_rebalances
 FOR EACH ROW EXECUTE FUNCTION app.guard_portfolio_rebalance();
