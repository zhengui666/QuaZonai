-- Driver ownership only; Run/Attempt/PGMQ and the model-turn ledger own execution.
CREATE TABLE app.run_missions (
 run_id app.identity PRIMARY KEY REFERENCES app.run_admissions(run_id),
 project_id app.identity NOT NULL,
 cycle_id app.identity NOT NULL,
 role text NOT NULL CHECK(role IN ('RESEARCHER','INDEPENDENT_REVIEWER')),
 profile_id app.identity NOT NULL REFERENCES app.codex_profiles,
 profile_revision app.revision NOT NULL,
 profile_snapshot app.document NOT NULL,
 credential_ref app.identity,
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(cycle_id,role),
 FOREIGN KEY(run_id,project_id,cycle_id) REFERENCES app.runs(id,project_id,cycle_id)
);
CREATE FUNCTION app.guard_run_mission() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE s app.cycle_startups; actual_kind text; actual_revision app.revision;
BEGIN
 SELECT * INTO STRICT s FROM app.cycle_startups WHERE cycle_id=NEW.cycle_id;
 SELECT kind INTO STRICT actual_kind FROM app.runs WHERE id=NEW.run_id;
 SELECT revision INTO STRICT actual_revision FROM app.codex_profiles WHERE id=NEW.profile_id FOR SHARE;
 IF actual_kind<>'AGENT_RESEARCH' OR NEW.project_id<>s.project_id
    OR NEW.profile_revision<>actual_revision
    OR NEW.profile_snapshot->'profile'->>'id' IS DISTINCT FROM NEW.profile_id::text
    OR NEW.profile_snapshot->'profile'->>'revision' IS DISTINCT FROM NEW.profile_revision::text
    OR (NEW.role='RESEARCHER' AND (NEW.profile_id IS DISTINCT FROM s.researcher_profile_id
         OR NEW.profile_revision IS DISTINCT FROM s.researcher_profile_revision))
    OR (NEW.role='INDEPENDENT_REVIEWER' AND (NEW.profile_id IS DISTINCT FROM s.reviewer_profile_id
         OR NEW.profile_revision IS DISTINCT FROM s.reviewer_profile_revision)) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='Mission must use the exact Cycle role and profile choice';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER run_mission BEFORE INSERT ON app.run_missions
 FOR EACH ROW EXECUTE FUNCTION app.guard_run_mission();
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.run_missions
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();

-- An observed native Thread must survive a concurrent Profile edit. Only the
-- exact immutable Mission selection grants this exception, never a fresh turn.
ALTER TABLE app.codex_sessions ADD COLUMN observed_service_tier text;
CREATE OR REPLACE FUNCTION app.guard_session_profile_revision() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE m app.run_missions; actual app.revision;
BEGIN
 SELECT * INTO m FROM app.run_missions WHERE run_id=NEW.run_id;
 IF FOUND THEN
  IF ROW(NEW.project_id,NEW.cycle_id,NEW.role,NEW.profile_id,NEW.profile_revision)
     IS DISTINCT FROM ROW(m.project_id,m.cycle_id,m.role,m.profile_id,m.profile_revision)
     OR NEW.requested_settings->>'profile_id' IS DISTINCT FROM m.profile_id::text
     OR NEW.requested_settings->>'profile_revision' IS DISTINCT FROM m.profile_revision::text
     OR NEW.requested_settings->'profile_origin' IS DISTINCT FROM m.profile_snapshot->'profile'->'profile_origin'
     OR NEW.requested_settings->'connection_mode' IS DISTINCT FROM m.profile_snapshot->'profile'->'connection_mode' THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native session must retain the exact Mission selection';
  END IF;
 ELSE
  SELECT revision INTO actual FROM app.codex_profiles WHERE id=NEW.profile_id FOR SHARE;
  IF actual IS NULL OR actual<>NEW.profile_revision THEN
   RAISE EXCEPTION USING ERRCODE='23503',MESSAGE='session must freeze an existing current profile revision';
  END IF;
 END IF;
 RETURN NEW;
END $$;
