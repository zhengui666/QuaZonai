-- Preserve existing source associations and historical column values while new
-- registrations identify original content by their immutable artifact and part.
-- The prior migration remains unchanged for installations that already applied it.
ALTER TABLE app.feature_artifact_sources
 ALTER COLUMN content_sha256 DROP NOT NULL;
