-- Preserve historical starts without inventing which native account was chosen.
-- Every new start must freeze both choices; the existing immutable trigger seals them.
ALTER TABLE app.cycle_startups
 ADD COLUMN researcher_profile_id app.identity REFERENCES app.codex_profiles,
 ADD COLUMN researcher_profile_revision app.revision,
 ADD COLUMN reviewer_profile_id app.identity REFERENCES app.codex_profiles,
 ADD COLUMN reviewer_profile_revision app.revision,
 ADD CONSTRAINT cycle_profiles_complete CHECK (
   num_nonnulls(researcher_profile_id,researcher_profile_revision,
     reviewer_profile_id,reviewer_profile_revision) IN (0,4));

CREATE FUNCTION app.guard_cycle_profiles() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p app.codex_profiles;
BEGIN
 IF num_nonnulls(NEW.researcher_profile_id,NEW.researcher_profile_revision,
     NEW.reviewer_profile_id,NEW.reviewer_profile_revision)<>4 THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='new Cycle requires explicit native profile choices';
 END IF;
 -- Deterministic lock order even if two projects choose the roles in reverse.
 FOR p IN SELECT * FROM app.codex_profiles
   WHERE id IN (NEW.researcher_profile_id,NEW.reviewer_profile_id) ORDER BY id FOR SHARE
 LOOP
  IF (p.id=NEW.researcher_profile_id AND p.revision<>NEW.researcher_profile_revision)
     OR (p.id=NEW.reviewer_profile_id AND p.revision<>NEW.reviewer_profile_revision) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='Cycle native profile revision mismatch';
  END IF;
 END LOOP;
 RETURN NEW;
END $$;
CREATE TRIGGER cycle_profiles BEFORE INSERT ON app.cycle_startups
 FOR EACH ROW EXECUTE FUNCTION app.guard_cycle_profiles();
