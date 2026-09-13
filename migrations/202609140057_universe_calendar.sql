-- Optional original Runtime calendar data; existing immutable Universes remain unchanged.
ALTER TABLE app.universe_versions
 ADD COLUMN calendar_artifact_id app.identity REFERENCES app.artifacts;
