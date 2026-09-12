CREATE TABLE app.mission_sealed_evaluations (
 review_reservation_id app.identity PRIMARY KEY REFERENCES app.mission_reviews,
 run_id app.identity NOT NULL UNIQUE REFERENCES app.sealed_evaluation_tasks
);
CREATE FUNCTION app.guard_mission_sealed_evaluation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM app.mission_reviews answer
   JOIN app.mission_review_turns review ON review.reservation_id=answer.reservation_id
   JOIN app.run_missions mission ON mission.run_id=review.run_id AND mission.role='INDEPENDENT_REVIEWER'
   JOIN app.cycle_selections selection ON selection.cycle_id=review.cycle_id
   JOIN app.sealed_evaluation_tasks task ON task.run_id=NEW.run_id
   JOIN app.runs run ON run.id=task.run_id AND run.cycle_id=review.cycle_id AND run.project_id=mission.project_id
   WHERE answer.reservation_id=NEW.review_reservation_id AND answer.decision='PASS'
     AND task.alpha_version_id=review.alpha_version_id
     AND task.validation_evaluation_id=review.validation_evaluation_id
     AND task.policy_id=selection.policy_id) THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='Sealed continuation requires its original independent PASS target';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER original_review BEFORE INSERT ON app.mission_sealed_evaluations
 FOR EACH ROW EXECUTE FUNCTION app.guard_mission_sealed_evaluation();
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.mission_sealed_evaluations
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
