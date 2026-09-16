-- Human account-operation facts only. Native Codex owns OAuth tokens and auth files.
CREATE TABLE app.codex_account_operations (
 id app.identity PRIMARY KEY DEFAULT uuidv7(),
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 updated_at app.instant NOT NULL DEFAULT clock_timestamp(),
 revision app.revision NOT NULL DEFAULT 1,
 profile_id app.identity NOT NULL REFERENCES app.codex_profiles,
 profile_revision app.revision NOT NULL,
 action text NOT NULL CHECK(action IN ('LOGIN','LOGOUT')),
 state text NOT NULL CHECK(state IN ('REQUESTED','WAITING','CANCEL_REQUESTED','SUCCEEDED','CANCELLED','FAILED','UNKNOWN')),
 deadline_at app.instant NOT NULL,
 dispatch_started_at app.instant,
 native_login_id text CHECK(native_login_id IS NULL OR (length(native_login_id) BETWEEN 1 AND 200 AND native_login_id !~ '[[:cntrl:]]')),
 cancel_requested_at app.instant,
 cancel_dispatch_started_at app.instant,
 finished_at app.instant,
 reason_code text CHECK(reason_code IS NULL OR reason_code IN (
  'NATIVE_LOGIN_COMPLETED','NATIVE_LOGIN_REJECTED','NATIVE_LOGOUT_COMPLETED','NATIVE_CANCEL_CONFIRMED',
  'CONFIRMED_NOT_SENT','NATIVE_RESPONSE_UNKNOWN','NATIVE_CONTRACT_UNSUPPORTED','NATIVE_VERSION_UNSUPPORTED',
  'DEPLOYMENT_UNAVAILABLE','PROFILE_CHANGED','WAIT_WINDOW_ENDED','OWNER_UNAVAILABLE')),
 account_snapshot app.document,
 CHECK(deadline_at>created_at AND deadline_at<=created_at+interval '15 minutes'),
 CHECK(dispatch_started_at IS NULL OR dispatch_started_at>=created_at),
 CHECK(cancel_requested_at IS NULL OR (action='LOGIN' AND cancel_requested_at>=created_at)),
 CHECK(cancel_dispatch_started_at IS NULL OR (native_login_id IS NOT NULL AND cancel_requested_at IS NOT NULL AND cancel_dispatch_started_at>=cancel_requested_at)),
 CHECK(native_login_id IS NULL OR (action='LOGIN' AND dispatch_started_at IS NOT NULL)),
 CHECK(state<>'WAITING' OR native_login_id IS NOT NULL),
 CHECK(state<>'CANCEL_REQUESTED' OR cancel_requested_at IS NOT NULL),
 CHECK((state IN ('SUCCEEDED','CANCELLED','FAILED','UNKNOWN'))=(finished_at IS NOT NULL)),
 CHECK((finished_at IS NOT NULL)=(reason_code IS NOT NULL)),
 CHECK(finished_at IS NULL OR finished_at>=created_at),
 CHECK(account_snapshot IS NULL OR (finished_at IS NOT NULL AND jsonb_typeof(account_snapshot->'account')='object')),
 CHECK(state<>'SUCCEEDED' OR (dispatch_started_at IS NOT NULL AND account_snapshot IS NOT NULL
   AND ((action='LOGIN' AND native_login_id IS NOT NULL AND reason_code='NATIVE_LOGIN_COMPLETED')
     OR (action='LOGOUT' AND reason_code='NATIVE_LOGOUT_COMPLETED')))),
 CHECK(state<>'CANCELLED' OR (action='LOGIN' AND
   ((dispatch_started_at IS NULL AND reason_code='CONFIRMED_NOT_SENT')
    OR (cancel_dispatch_started_at IS NOT NULL AND reason_code='NATIVE_CANCEL_CONFIRMED'))))
);
CREATE UNIQUE INDEX codex_account_one_active ON app.codex_account_operations(profile_id)
 WHERE state IN ('REQUESTED','WAITING','CANCEL_REQUESTED');
CREATE INDEX codex_account_latest ON app.codex_account_operations(profile_id,created_at DESC,id DESC);
CREATE TRIGGER no_delete BEFORE DELETE ON app.codex_account_operations
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE TRIGGER identity_revision BEFORE UPDATE ON app.codex_account_operations
 FOR EACH ROW EXECUTE FUNCTION app.guard_revision('profile_id','profile_revision','action','deadline_at');

CREATE FUNCTION app.guard_codex_account_operation() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE current_revision bigint; current_mode text;
BEGIN
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
CREATE TRIGGER account_operation BEFORE INSERT OR UPDATE ON app.codex_account_operations
 FOR EACH ROW EXECUTE FUNCTION app.guard_codex_account_operation();

CREATE FUNCTION app.guard_codex_account_profile_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM app.codex_account_operations WHERE profile_id=OLD.id AND state IN ('REQUESTED','WAITING','CANCEL_REQUESTED')) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native account operation must end before profile replacement';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER account_operation_in_progress BEFORE UPDATE ON app.codex_profiles
 FOR EACH ROW EXECUTE FUNCTION app.guard_codex_account_profile_change();

CREATE FUNCTION app.guard_codex_account_observation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 -- Profile row serialization is shared with account admission and configuration writes.
 PERFORM 1 FROM app.codex_profiles WHERE id=NEW.profile_id FOR SHARE;
 IF EXISTS(SELECT 1 FROM app.codex_account_operations WHERE profile_id=NEW.profile_id
   AND (state IN ('REQUESTED','WAITING','CANCEL_REQUESTED') OR created_at>=NEW.observed_at OR finished_at>=NEW.observed_at)) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native account mutation invalidates an earlier configuration observation';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER account_observation BEFORE INSERT ON app.codex_profile_observations
 FOR EACH ROW EXECUTE FUNCTION app.guard_codex_account_observation();

ALTER TABLE app.operator_command_grants DROP CONSTRAINT operator_command_grants_operation_check;
ALTER TABLE app.operator_command_grants ADD CONSTRAINT operator_command_grants_operation_check CHECK(operation IN (
 'RELEASE_APPROVE','RELEASE_REJECT','RELEASE_REOPEN','POLICY_AUTHORIZE','POLICY_REVOKE',
 'PROJECT_CREATE','PROJECT_UPDATE','PRINCIPAL_CREATE','PRINCIPAL_UPDATE',
 'CREDENTIAL_ISSUE','CREDENTIAL_REVOKE','INPUT_SET_CREATE','EVALUATION_POLICY_CREATE',
 'BRIEF_CREATE','BRIEF_UPDATE','BRIEF_FREEZE','CYCLE_START','INTEGRATION_SECRET_REGISTER',
 'RUNTIME_CREATE','RUNTIME_UPDATE','DOWNSTREAM_CREATE','DOWNSTREAM_UPDATE','RUNTIME_PROBE',
 'DATA_SOURCE_CREATE','DATA_SOURCE_UPDATE','DATA_GRANT_CREATE','DATA_GRANT_REVOKE','DATASET_REGISTER','DATA_VALIDATE',
 'CODEX_PROFILE_CREATE','CODEX_PROFILE_UPDATE','CODEX_PROBE','CODEX_LOGIN_START','CODEX_LOGIN_CANCEL','CODEX_LOGOUT'
));
