-- Bounded failed/in-flight native capability verification, shared by processes.
-- NULL is the single global bucket; known credentials have one bounded row each.
CREATE TABLE app.machine_auth_rate_windows (
    credential_id app.identity REFERENCES app.machine_credentials,
    window_started_at app.instant NOT NULL,
    attempts integer NOT NULL CHECK (
        attempts >= 0 AND attempts <= CASE WHEN credential_id IS NULL THEN 32 ELSE 5 END
    ),
    UNIQUE NULLS NOT DISTINCT(credential_id)
);
