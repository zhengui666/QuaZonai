-- Original admission identities for terminal publication and queue recovery.
CREATE TABLE app.candidate_simulation_tasks (
 run_id app.identity PRIMARY KEY REFERENCES app.runs,
 candidate_id app.identity NOT NULL REFERENCES app.portfolio_candidates,
 policy_id app.identity NOT NULL REFERENCES app.evaluation_policies,
 dataset_revision_id app.identity NOT NULL REFERENCES app.dataset_revisions,
 request app.document NOT NULL
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.candidate_simulation_tasks
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
