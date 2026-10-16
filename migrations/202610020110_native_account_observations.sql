-- Read-only native account telemetry, independent of releases and evaluations.
CREATE TABLE app.native_account_sources (
 id app.identity PRIMARY KEY,
 project_id app.identity NOT NULL REFERENCES app.projects,
 downstream_id app.identity NOT NULL REFERENCES app.downstream_integrations,
 environment text NOT NULL CHECK(environment IN ('PAPER','LIVE')),
 native_trader_id app.nonempty NOT NULL,
 native_session_id app.nonempty NOT NULL,
 native_account_id app.nonempty NOT NULL,
 binding app.document NOT NULL,
 UNIQUE(project_id,downstream_id,environment,native_trader_id,native_session_id,native_account_id)
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.native_account_sources
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE TABLE app.native_account_observations (
 id app.identity PRIMARY KEY,
 source_id app.identity NOT NULL REFERENCES app.native_account_sources,
 sequence bigint NOT NULL CHECK(sequence > 0),
 native_event_id text,
 content app.document NOT NULL,
 gap_before boolean NOT NULL,
 received_at app.instant NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(source_id,sequence),
 UNIQUE(source_id,native_event_id),
 UNIQUE(source_id,id)
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.native_account_observations
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE INDEX native_account_observations_page ON app.native_account_observations(source_id,id DESC);
CREATE INDEX native_account_sources_page ON app.native_account_sources(project_id,id DESC);
CREATE TABLE app.native_account_cursors (
 source_id app.identity PRIMARY KEY REFERENCES app.native_account_sources,
 last_sequence bigint NOT NULL CHECK(last_sequence > 0),
 dropped_events bigint NOT NULL CHECK(dropped_events >= 0 AND dropped_events < last_sequence),
 has_gap boolean NOT NULL,
 last_observation_id app.identity NOT NULL,
 latest_snapshot_id app.identity,
 last_observed_at_ns bigint NOT NULL CHECK(last_observed_at_ns > 0),
 latest_snapshot_ns bigint,
 connection text NOT NULL CHECK(connection IN ('UNKNOWN','CONNECTED','DISCONNECTED')),
 FOREIGN KEY(source_id,last_observation_id) REFERENCES app.native_account_observations(source_id,id),
 FOREIGN KEY(source_id,latest_snapshot_id) REFERENCES app.native_account_observations(source_id,id),
 CHECK((latest_snapshot_id IS NULL) = (latest_snapshot_ns IS NULL))
);
