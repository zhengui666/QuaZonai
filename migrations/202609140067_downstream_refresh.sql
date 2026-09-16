-- Worker-owned short reservation; observations remain immutable historical evidence.
CREATE TABLE app.downstream_probe_refresh (
 downstream_id app.identity PRIMARY KEY REFERENCES app.downstream_integrations,
 lease_id app.identity NOT NULL,
 lease_until app.instant NOT NULL,
 next_attempt_at app.instant NOT NULL,
 CHECK(next_attempt_at >= lease_until)
);
