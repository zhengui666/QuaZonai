-- Original review Turn/answer associations, not an Agent workflow engine.
CREATE TABLE app.mission_review_turns (
 reservation_id app.identity PRIMARY KEY REFERENCES app.model_turn_reservations,
 run_id app.identity NOT NULL REFERENCES app.run_missions,
 cycle_id app.identity NOT NULL,
 experiment_id app.identity NOT NULL,
 alpha_version_id app.identity NOT NULL REFERENCES app.alpha_versions,
 validation_evaluation_id app.identity NOT NULL REFERENCES app.evaluations,
 UNIQUE(run_id,experiment_id),
 FOREIGN KEY(cycle_id,experiment_id) REFERENCES app.cycle_selection_trials
);
CREATE FUNCTION app.guard_mission_review_turn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM app.model_turn_reservations r
   JOIN app.codex_sessions s ON s.id=r.session_id AND s.run_id=r.run_id AND s.role='INDEPENDENT_REVIEWER'
   JOIN app.run_missions m ON m.run_id=r.run_id AND m.role=s.role
   JOIN app.cycle_selection_trials t ON t.cycle_id=m.cycle_id AND t.experiment_id=NEW.experiment_id
   JOIN app.cycle_selections selected ON selected.cycle_id=t.cycle_id AND selected.status='COMPLETE'
   WHERE r.id=NEW.reservation_id AND r.run_id=NEW.run_id AND m.cycle_id=NEW.cycle_id
     AND r.command_key='mission/review/'||t.experiment_id::text
     AND t.selected AND t.review_alpha_version_id=NEW.alpha_version_id
     AND t.evaluation_id=NEW.validation_evaluation_id) THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='review Turn must retain original independent Mission and selected target';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER review_turn BEFORE INSERT ON app.mission_review_turns
 FOR EACH ROW EXECUTE FUNCTION app.guard_mission_review_turn();
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.mission_review_turns
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE TABLE app.mission_reviews (
 reservation_id app.identity PRIMARY KEY REFERENCES app.mission_review_turns,
 summary_artifact_id app.identity NOT NULL UNIQUE REFERENCES app.artifacts,
 decision text NOT NULL CHECK(decision IN ('PASS','REJECT','INCONCLUSIVE')),
 reasons jsonb NOT NULL CHECK(jsonb_typeof(reasons)='array' AND jsonb_array_length(reasons) BETWEEN 1 AND 32),
 created_at app.instant NOT NULL DEFAULT clock_timestamp()
);
CREATE FUNCTION app.guard_mission_review_answer() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM app.mission_review_turns review
   JOIN app.model_turn_summaries summary ON summary.reservation_id=review.reservation_id
   JOIN app.model_turn_terminals terminal ON terminal.reservation_id=review.reservation_id AND terminal.outcome='SUCCEEDED'
   JOIN app.model_turn_receipts receipt ON receipt.reservation_id=review.reservation_id AND receipt.outcome='SUCCEEDED'
   JOIN app.artifacts artifact ON artifact.id=summary.artifact_id AND artifact.access_class='EVALUATOR_ONLY'
   WHERE review.reservation_id=NEW.reservation_id AND summary.artifact_id=NEW.summary_artifact_id) THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='review answer requires its original native settled public summary';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER review_answer BEFORE INSERT ON app.mission_reviews
 FOR EACH ROW EXECUTE FUNCTION app.guard_mission_review_answer();
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.mission_reviews
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
