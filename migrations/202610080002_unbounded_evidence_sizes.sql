-- Remove application payload/collection ceilings, not source, role, producer,
-- immutable identity, positivity, or native binding checks. Applied migrations
-- and every existing evidence row remain unchanged.


CREATE OR REPLACE FUNCTION app.guard_mission_summary() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM app.model_turn_reservations r
   JOIN app.codex_sessions session ON session.id=r.session_id AND session.run_id=r.run_id
   JOIN app.model_turn_receipts t ON t.reservation_id=r.id AND t.outcome='SUCCEEDED'
   JOIN app.model_turn_bindings b ON b.reservation_id=r.id
   JOIN app.model_turn_terminals terminal ON terminal.reservation_id=r.id AND terminal.outcome=t.outcome AND terminal.native_turn_id=b.native_turn_id
   JOIN app.artifacts a ON a.id=NEW.artifact_id
   WHERE r.id=NEW.reservation_id AND a.project_id=r.project_id AND a.producer_run_id=r.run_id
    AND a.producer_attempt_id=r.attempt_id AND a.kind='REPORT' AND a.schema_name='qz.mission_summary'
    AND a.schema_version='1' AND a.media_type='application/json'
    AND a.access_class=CASE session.role WHEN 'RESEARCHER' THEN 'RESEARCH' WHEN 'INDEPENDENT_REVIEWER' THEN 'EVALUATOR_ONLY' END
    AND a.origin='SYNTHETIC' AND a.created_by='RUNTIME' AND a.storage_backend='LOCAL'
    AND a.storage_object_ref=a.id::text AND a.storage_version='1' AND a.byte_count>0) THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='public summary requires the exact successful native Turn, role and producer';
 END IF;
 RETURN NEW;
END $$;

CREATE OR REPLACE FUNCTION app.guard_native_dataset_registration() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE valid boolean;
BEGIN
 SELECT m.kind='REPORT' AND m.schema_name='qz.native_catalog_metadata' AND m.schema_version='1'
   AND m.media_type='application/json' AND m.storage_backend='LOCAL' AND m.storage_version='1'
   AND m.storage_object_ref=m.id::text AND m.byte_count>0
   AND m.project_id IS NULL AND m.producer_run_id IS NULL AND m.producer_attempt_id IS NULL
   AND m.access_class='OPERATOR' AND m.created_by='RUNTIME' AND m.origin=d.origin
   AND q.kind='DATA_QUALITY' AND q.schema_name='qz.data_quality' AND q.schema_version='1'
   AND q.media_type='application/json' AND q.access_class='OPERATOR' AND q.created_by='RUNTIME'
   AND q.storage_backend='LOCAL' AND q.storage_object_ref=q.id::text AND q.storage_version='1'
   AND q.byte_count>0 AND q.origin=d.origin
   AND s.revision=NEW.source_revision AND r.revision=NEW.runtime_revision
   AND NEW.observed_at<=clock_timestamp() AND NEW.observed_at>=d.created_at-interval '20 seconds'
 INTO valid
 FROM app.dataset_revisions d
 JOIN app.data_sources s ON s.id=d.source_id
 JOIN app.runtime_integrations r ON r.id=s.runtime_id
 JOIN app.artifacts m ON m.id=NEW.native_metadata_artifact_id
 JOIN app.artifacts q ON q.id=d.quality_artifact_id
 WHERE d.id=NEW.dataset_revision_id;
 IF valid IS DISTINCT FROM true THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native dataset registration evidence mismatch';
 END IF;
 RETURN NEW;
END $$;

CREATE OR REPLACE FUNCTION app.guard_feature_artifact_source() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE valid boolean;
BEGIN
 SELECT a.project_id=NEW.project_id
   AND a.kind='PARAMETERS' AND a.schema_name='qz.feature_observations' AND a.schema_version='1'
   AND a.media_type='application/json' AND a.access_class='RESEARCH' AND a.created_by='IMPORT'
   AND a.storage_backend='LOCAL' AND a.storage_object_ref=a.id::text AND a.storage_version='1'
   AND a.byte_count>0
   AND a.producer_run_id IS NULL AND a.producer_attempt_id IS NULL
   AND a.origin=d.origin AND d.partition_role<>'SEALED'
   AND e.native_metadata_artifact_id=NEW.native_metadata_artifact_id
 INTO valid
 FROM app.artifacts a
 JOIN app.dataset_revisions d ON d.id=NEW.dataset_revision_id
 JOIN app.dataset_registration_evidence e ON e.dataset_revision_id=d.id
 WHERE a.id=NEW.artifact_id;
 IF valid IS DISTINCT FROM true THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='recorded feature source binding mismatch';
 END IF;
 RETURN NEW;
END $$;

-- Column check names are fixed by the original table definitions.
ALTER TABLE app.run_native_tasks DROP CONSTRAINT run_native_tasks_input_bindings_check;
ALTER TABLE app.run_native_tasks ADD CONSTRAINT run_native_tasks_input_bindings_check
 CHECK(jsonb_typeof(input_bindings)='array' AND jsonb_array_length(input_bindings)>=1);
ALTER TABLE app.run_native_tasks DROP CONSTRAINT run_native_tasks_output_schemas_check;
ALTER TABLE app.run_native_tasks ADD CONSTRAINT run_native_tasks_output_schemas_check
 CHECK(jsonb_typeof(output_schemas)='array' AND jsonb_array_length(output_schemas)>=1);
ALTER TABLE app.historical_artifact_copies DROP CONSTRAINT historical_artifact_copies_byte_count_check;
ALTER TABLE app.historical_artifact_copies ADD CONSTRAINT historical_artifact_copies_byte_count_check
 CHECK(byte_count>0);

-- The original table-level check received an automatically allocated name.
-- Resolve only its known old byte ceiling, and refuse an unexpected schema.
DO $$
DECLARE names text[];
BEGIN
 SELECT array_agg(conname ORDER BY conname) INTO names FROM pg_constraint
 WHERE conrelid='app.historical_artifact_results'::regclass AND contype='c'
   AND pg_get_constraintdef(oid) LIKE '%67108864%';
 IF cardinality(names) IS DISTINCT FROM 1 THEN
  RAISE EXCEPTION 'expected the original historical artifact byte-count check';
 END IF;
 EXECUTE format('ALTER TABLE app.historical_artifact_results DROP CONSTRAINT %I',names[1]);
END $$;
ALTER TABLE app.historical_artifact_results ADD CONSTRAINT historical_artifact_results_readable_bytes_check
 CHECK((verified_readable AND byte_count IS NOT NULL AND byte_count>0)
       OR (NOT verified_readable AND byte_count IS NULL));

-- Complete supplied features and review reasons are data, not a tuning budget.
ALTER TABLE app.external_experiment_requests DROP CONSTRAINT external_experiment_requests_feature_artifact_ids_check;
ALTER TABLE app.external_experiment_requests ADD CONSTRAINT external_experiment_requests_feature_artifact_ids_check
 CHECK(jsonb_typeof(feature_artifact_ids)='array' AND jsonb_array_length(feature_artifact_ids)>=1);
ALTER TABLE app.mission_reviews DROP CONSTRAINT mission_reviews_reasons_check;
ALTER TABLE app.mission_reviews ADD CONSTRAINT mission_reviews_reasons_check
 CHECK(jsonb_typeof(reasons)='array' AND jsonb_array_length(reasons)>=1);
