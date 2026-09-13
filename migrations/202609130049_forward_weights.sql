CREATE TABLE app.forward_weight_snapshots (
 id app.identity PRIMARY KEY,
 project_id app.identity NOT NULL REFERENCES app.projects,
 downstream_id app.identity NOT NULL REFERENCES app.downstream_integrations,
 environment text NOT NULL CHECK(environment IN ('PAPER','LIVE')),
 external_message_id app.nonempty NOT NULL,
 report_artifact_id app.identity NOT NULL UNIQUE REFERENCES app.artifacts,
 content app.document NOT NULL,
 received_at app.instant NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(project_id,downstream_id,environment,external_message_id)
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.forward_weight_snapshots
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
