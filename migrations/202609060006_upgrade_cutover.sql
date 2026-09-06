-- Repair a prior 0005 upgrade window without editing any applied checksum.
-- Store::migrate already holds an outer write cutover. These locks also protect
-- this corrective backfill when the file is used by native new-DB test runners.
LOCK TABLE app.operator_auth_state, app.browser_logins, app.trusted_devices,
 app.operator_command_grants, app.evaluations, app.metric_values,
 app.evaluation_publications, app.degradation_observations
 IN SHARE ROW EXCLUSIVE MODE;

INSERT INTO app.evaluation_publications(evaluation_id)
SELECT id FROM app.evaluations ON CONFLICT DO NOTHING;
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM app.degradation_observations o
    WHERE NOT app.degradation_binding_valid(o.project_id,o.release_id,o.evaluation_id,o.policy_id)) THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='invalid historical degradation binding; preserve evidence and abort upgrade';
 END IF;
END $$;

-- A previously rolled-back epoch cannot be repaired by merely adding one to its
-- current value. Invalidate every recorded older authority, without erasing it.
DO $$
DECLARE auth app.operator_auth_state%ROWTYPE; high_epoch bigint; next_epoch bigint;
BEGIN
 SELECT * INTO STRICT auth FROM app.operator_auth_state WHERE singleton FOR UPDATE;
 IF auth.initialized THEN
   SELECT greatest(auth.session_epoch,
     coalesce((SELECT max(auth_epoch) FROM app.browser_logins),1),
     coalesce((SELECT max(auth_epoch) FROM app.trusted_devices),1),
     coalesce((SELECT max(auth_epoch) FROM app.operator_command_grants),1)) INTO high_epoch;
   next_epoch := high_epoch + 1; -- Native bigint overflow aborts, never wraps.
   UPDATE app.operator_auth_state SET session_epoch=next_epoch WHERE id=auth.id;
   INSERT INTO app.command_receipts(principal_scope,operation,idempotency_key,
       normalized_nonsecret_request,resource_id,response_status)
   VALUES('SYSTEM_MIGRATOR','AUTH_UPGRADE_INVALIDATE','202609060006',
     jsonb_build_object('schema_version',1,'previous_epoch',auth.session_epoch::text,
       'historical_max_epoch',high_epoch::text,'new_epoch',next_epoch::text,
       'reason_code','UPGRADE_CUTOVER'),auth.id,200);
 END IF;
END $$;
