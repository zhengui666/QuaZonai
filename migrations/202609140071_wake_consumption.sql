-- One original Wake and one native Cycle commit together. No historical repair.
CREATE FUNCTION app.guard_wake_terminal() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.state IN ('CONSUMED','CANCELLED') THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='terminal Wake is immutable';
 END IF;
 IF (NEW.state='CONSUMED') <> (NEW.consumed_cycle_id IS NOT NULL) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='Wake consumption requires exact Cycle';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER terminal BEFORE UPDATE ON app.wake_events
 FOR EACH ROW EXECUTE FUNCTION app.guard_wake_terminal();

CREATE FUNCTION app.guard_native_wake_cycle() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE wake app.wake_events;
BEGIN
 IF TG_TABLE_NAME='research_cycles' THEN
  IF NEW.wake_id IS NULL THEN RETURN NEW; END IF;
  SELECT * INTO wake FROM app.wake_events WHERE id=NEW.wake_id;
 ELSE
  SELECT * INTO wake FROM app.wake_events WHERE id=NEW.id;
 END IF;
 IF wake.trigger='DEGRADATION' AND wake.state='CONSUMED' THEN
  IF NOT EXISTS(SELECT 1 FROM app.research_cycles c
    JOIN app.cycle_startups s ON s.cycle_id=c.id AND s.project_id=c.project_id
    JOIN app.degradation_observations o ON o.id=wake.observation_id
      AND o.project_id=wake.project_id AND o.classification='DEGRADED'
    JOIN app.forward_observation_publications p ON p.observation_id=o.id
    WHERE c.id=wake.consumed_cycle_id AND c.wake_id=wake.id
      AND c.project_id=wake.project_id AND c.trigger='DEGRADATION') THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native Wake and Cycle must commit together';
  END IF;
 END IF;
 IF TG_TABLE_NAME='research_cycles' THEN
  IF NEW.wake_id IS NOT NULL AND (wake.state<>'CONSUMED' OR wake.consumed_cycle_id IS DISTINCT FROM NEW.id) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='Cycle must consume its original Wake';
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER native_cycle AFTER INSERT ON app.research_cycles
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION app.guard_native_wake_cycle();
CREATE CONSTRAINT TRIGGER native_cycle AFTER INSERT OR UPDATE ON app.wake_events
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION app.guard_native_wake_cycle();
