-- Local operation replaces user enrollment. Existing records and encrypted
-- historical material stay intact; no legacy authority is resurrected.
ALTER TABLE app.operator_auth_state DROP CONSTRAINT operator_auth_state_check;
UPDATE app.operator_auth_state
 SET initialized=true, setup_completed_at=coalesce(setup_completed_at,clock_timestamp()),
     session_epoch=greatest(
       session_epoch,
       coalesce((SELECT max(auth_epoch) FROM app.browser_logins),0),
       coalesce((SELECT max(auth_epoch) FROM app.trusted_devices),0),
       coalesce((SELECT max(auth_epoch) FROM app.operator_command_grants),0)
     )+1
 WHERE singleton;
ALTER TABLE app.operator_auth_state ADD CONSTRAINT local_operator_ready
 CHECK (initialized AND setup_completed_at IS NOT NULL);

-- Fresh role identities cannot alias any user-selected pre-upgrade home label.
-- The nullable role distinguishes migrated native bindings from historical rows.
ALTER TABLE app.codex_profiles ADD COLUMN local_role text UNIQUE
 CHECK (local_role IN ('RESEARCHER','REVIEWER'));
ALTER TABLE app.codex_profiles ADD CONSTRAINT native_local_profile_binding CHECK (
 local_role IS NULL OR (
  connection_mode='SYSTEM' AND profile_origin='OPERATOR_MOUNT'
  AND codex_home_ref='local-' || lower(local_role) || '-' || id::text
 ));

WITH roles AS MATERIALIZED (
 SELECT uuidv7() AS id, role, label
 FROM (VALUES ('RESEARCHER','研究员'), ('REVIEWER','独立审阅员')) AS selected(role,label)
)
INSERT INTO app.codex_profiles(id,name,connection_mode,profile_origin,codex_home_ref,
 use_default_model_settings,saved_fast_mode,local_role)
SELECT id,label,'SYSTEM','OPERATOR_MOUNT','local-' || lower(role) || '-' || id::text,
 true,false,role FROM roles;

CREATE OR REPLACE FUNCTION app.guard_codex_profile_home() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.profile_origin IS DISTINCT FROM OLD.profile_origin
    OR NEW.local_role IS DISTINCT FROM OLD.local_role
    OR (OLD.local_role IS NOT NULL AND NEW.codex_home_ref IS DISTINCT FROM OLD.codex_home_ref) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native profile binding is immutable';
 END IF;
 RETURN NEW;
END $$;

-- Preserve the original account state machine, now scoped to native membership.
CREATE OR REPLACE FUNCTION app.guard_codex_account_operation() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE current_revision bigint; current_mode text;
BEGIN
 IF TG_OP='INSERT' THEN
  -- One shared native account has one active sender, across both local roles.
  PERFORM 1 FROM app.codex_profiles WHERE id IN (SELECT p.id FROM app.codex_profiles p WHERE p.id=NEW.profile_id OR (p.local_role IS NOT NULL AND EXISTS(SELECT 1 FROM app.codex_profiles selected WHERE selected.id=NEW.profile_id AND selected.local_role IS NOT NULL))) ORDER BY id FOR UPDATE;
  IF EXISTS(SELECT 1 FROM app.codex_account_operations WHERE profile_id IN (SELECT p.id FROM app.codex_profiles p WHERE p.id=NEW.profile_id OR (p.local_role IS NOT NULL AND EXISTS(SELECT 1 FROM app.codex_profiles selected WHERE selected.id=NEW.profile_id AND selected.local_role IS NOT NULL)))
    AND state IN ('REQUESTED','WAITING','CANCEL_REQUESTED')) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native account already has an active operation';
  END IF;
 END IF;
 SELECT revision,connection_mode INTO current_revision,current_mode FROM app.codex_profiles WHERE id=NEW.profile_id FOR SHARE;
 IF TG_OP='INSERT' THEN
  IF current_revision IS DISTINCT FROM NEW.profile_revision OR current_mode IS DISTINCT FROM 'SYSTEM'
    OR NEW.state<>'REQUESTED' OR NEW.dispatch_started_at IS NOT NULL OR NEW.native_login_id IS NOT NULL
    OR NEW.cancel_requested_at IS NOT NULL OR NEW.finished_at IS NOT NULL OR NEW.account_snapshot IS NOT NULL
    OR NEW.deadline_at<=clock_timestamp() THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native account operation requires an exact current system profile';
  END IF;
 ELSE
  IF OLD.state IN ('SUCCEEDED','CANCELLED','FAILED','UNKNOWN')
    OR (OLD.dispatch_started_at IS NOT NULL AND NEW.dispatch_started_at IS DISTINCT FROM OLD.dispatch_started_at)
    OR (OLD.native_login_id IS NOT NULL AND NEW.native_login_id IS DISTINCT FROM OLD.native_login_id)
    OR (OLD.cancel_requested_at IS NOT NULL AND NEW.cancel_requested_at IS DISTINCT FROM OLD.cancel_requested_at)
    OR (OLD.cancel_dispatch_started_at IS NOT NULL AND NEW.cancel_dispatch_started_at IS DISTINCT FROM OLD.cancel_dispatch_started_at)
    OR (OLD.state='WAITING' AND NEW.state='REQUESTED')
    OR (OLD.state='CANCEL_REQUESTED' AND NEW.state IN ('REQUESTED','WAITING')) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native account operation facts cannot be replaced or reversed';
  END IF;
 END IF;
 RETURN NEW;
END $$;

CREATE OR REPLACE FUNCTION app.guard_codex_account_profile_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM app.codex_account_operations WHERE profile_id IN (SELECT p.id FROM app.codex_profiles p WHERE p.id=OLD.id OR (p.local_role IS NOT NULL AND EXISTS(SELECT 1 FROM app.codex_profiles selected WHERE selected.id=OLD.id AND selected.local_role IS NOT NULL))) AND state IN ('REQUESTED','WAITING','CANCEL_REQUESTED')) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native account operation must end before profile replacement';
 END IF;
 RETURN NEW;
END $$;

CREATE OR REPLACE FUNCTION app.guard_codex_account_observation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 -- Profile row serialization is shared with account admission and configuration writes.
 PERFORM 1 FROM app.codex_profiles WHERE id IN (SELECT p.id FROM app.codex_profiles p WHERE p.id=NEW.profile_id OR (p.local_role IS NOT NULL AND EXISTS(SELECT 1 FROM app.codex_profiles selected WHERE selected.id=NEW.profile_id AND selected.local_role IS NOT NULL))) ORDER BY id FOR SHARE;
 IF EXISTS(SELECT 1 FROM app.codex_account_operations WHERE profile_id IN (SELECT p.id FROM app.codex_profiles p WHERE p.id=NEW.profile_id OR (p.local_role IS NOT NULL AND EXISTS(SELECT 1 FROM app.codex_profiles selected WHERE selected.id=NEW.profile_id AND selected.local_role IS NOT NULL)))
   AND (state IN ('REQUESTED','WAITING','CANCEL_REQUESTED') OR created_at>=NEW.observed_at OR finished_at>=NEW.observed_at)) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native account mutation invalidates an earlier configuration observation';
 END IF;
 RETURN NEW;
END $$;
