-- First-stage trial accounting, without rewriting or refunding historical rows.
CREATE FUNCTION app.guard_experiment_trial_charge() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE valid boolean;
BEGIN
 IF TG_TABLE_NAME='experiment_compilations' THEN
  SELECT EXISTS(SELECT 1 FROM app.run_admissions WHERE run_id=NEW.compile_run_id AND limits->>'experiments'='1') INTO valid;
 ELSE
  SELECT EXISTS(SELECT 1 FROM app.experiment_compilations c
    JOIN app.run_admissions first_stage ON first_stage.run_id=c.compile_run_id
    JOIN app.run_admissions prediction ON prediction.run_id=NEW.run_id
    WHERE c.experiment_id=NEW.experiment_id
      AND first_stage.limits->>'experiments'='1' AND prediction.limits->>'experiments'='0') INTO valid;
 END IF;
 IF NOT valid THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='experiment trial must be charged once at compilation';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER trial_charge BEFORE INSERT ON app.experiment_compilations
 FOR EACH ROW EXECUTE FUNCTION app.guard_experiment_trial_charge();
CREATE TRIGGER trial_charge BEFORE INSERT ON app.experiment_forecasts
 FOR EACH ROW EXECUTE FUNCTION app.guard_experiment_trial_charge();
