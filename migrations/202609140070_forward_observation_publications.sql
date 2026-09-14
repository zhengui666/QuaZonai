-- Native provenance only. Existing historical observations remain unchanged.
CREATE TABLE app.forward_observation_publications (
    run_id app.identity PRIMARY KEY REFERENCES app.runs,
    observation_id app.identity NOT NULL UNIQUE REFERENCES app.degradation_observations
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.forward_observation_publications
FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE FUNCTION app.guard_forward_observation_publication() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM app.runs r
        JOIN app.forward_evaluation_inputs f ON f.input_set_id=r.input_set_id AND f.project_id=r.project_id
        JOIN app.handoff_offers h ON h.id=f.handoff_id
        JOIN app.degradation_observations o ON o.id=NEW.observation_id AND o.policy_id=f.policy_id AND o.project_id=r.project_id AND o.release_id=h.release_id
        JOIN app.evaluations e ON e.id=o.evaluation_id AND e.run_id=r.id AND e.input_set_id=r.input_set_id AND e.evaluation_kind='FORWARD'
        JOIN app.evaluation_publications published ON published.evaluation_id=e.id
        JOIN app.run_terminal_receipts terminal ON terminal.run_id=r.id AND terminal.terminal_state=r.state
        WHERE r.id=NEW.run_id AND r.kind='FORWARD_EVALUATE'
    ) THEN
        RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='native observation requires its original frozen forward terminal evaluation';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER forward_observation_publication BEFORE INSERT ON app.forward_observation_publications
FOR EACH ROW EXECUTE FUNCTION app.guard_forward_observation_publication();
