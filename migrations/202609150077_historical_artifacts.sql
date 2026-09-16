-- Historical public copies never enter app.artifacts or active scientific authority.
CREATE TABLE app.historical_artifact_copies (
 record_id app.identity PRIMARY KEY REFERENCES app.historical_records,
 byte_count app.counter NOT NULL CHECK(byte_count>0 AND byte_count<=67108864),
 first_import_id app.identity NOT NULL REFERENCES app.historical_import_reports DEFERRABLE INITIALLY DEFERRED
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.historical_artifact_copies
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE TABLE app.historical_artifact_results (
 id app.identity PRIMARY KEY DEFAULT uuidv7(),
 report_id app.identity NOT NULL REFERENCES app.historical_import_reports DEFERRABLE INITIALLY DEFERRED,
 source_table app.nonempty NOT NULL CHECK(source_table IN ('mission_artifacts','alpha_signal_artifacts')),
 source_id uuid NOT NULL,
 record_id app.identity REFERENCES app.historical_records,
 source_outcome text CHECK(source_outcome IN ('COPIED','MISSING','UNSUPPORTED','UNREADABLE','SEALED_RETAINED','MANUAL_REVIEW_REQUIRED')),
 verified_readable boolean NOT NULL,
 stored boolean NOT NULL,
 byte_count app.counter,
 UNIQUE(report_id,source_table,source_id),
 CHECK(verified_readable=(source_outcome IS NOT DISTINCT FROM 'COPIED')),
 CHECK((verified_readable AND byte_count IS NOT NULL AND byte_count>0 AND byte_count<=67108864) OR (NOT verified_readable AND byte_count IS NULL)),
 CHECK(NOT stored OR (verified_readable AND record_id IS NOT NULL))
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.historical_artifact_results
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE TABLE app.historical_artifact_reports (
 report_id app.identity PRIMARY KEY REFERENCES app.historical_import_reports DEFERRABLE INITIALLY DEFERRED,
 summary app.document NOT NULL,
 export_report app.document
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.historical_artifact_reports
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
