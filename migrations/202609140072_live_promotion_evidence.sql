-- Exact original observation set, not an application hash or a mutable qualification.
CREATE TABLE app.live_promotion_evidence (
 approval_id app.identity PRIMARY KEY REFERENCES app.approvals,
 observation_ids uuid[] NOT NULL CHECK(cardinality(observation_ids) BETWEEN 1 AND 255)
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.live_promotion_evidence
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE FUNCTION app.guard_live_promotion_evidence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE a app.approvals; ids uuid[];
BEGIN
 IF TG_TABLE_NAME='approvals' THEN
  a:=NEW;
 ELSE
  SELECT * INTO a FROM app.approvals WHERE id=NEW.approval_id;
 END IF;
 SELECT observation_ids INTO ids FROM app.live_promotion_evidence WHERE approval_id=a.id;
 IF a.authority_kind='FROZEN_POLICY' AND a.environment='LIVE' THEN
  IF ids IS NULL OR ids IS DISTINCT FROM (SELECT array_agg(DISTINCT observation_id ORDER BY observation_id) FROM unnest(ids) AS members(observation_id))
   OR cardinality(ids)<>(SELECT count(*) FROM unnest(ids) AS members(observation_id)
    JOIN app.degradation_observations o ON o.id=members.observation_id AND o.classification='HEALTHY'
    JOIN app.forward_observation_publications p ON p.observation_id=o.id
    JOIN app.evaluations e ON e.id=o.evaluation_id AND e.run_id=p.run_id
    JOIN app.releases r ON r.id=a.release_id AND r.candidate_id=e.subject_candidate_id
    JOIN app.forward_evaluation_inputs f ON f.input_set_id=e.input_set_id AND f.policy_id=a.automation_policy_id
    JOIN app.handoff_offers h ON h.id=f.handoff_id AND h.release_id=o.release_id
      AND h.downstream_id=a.downstream_id AND h.environment='PAPER'
    WHERE o.policy_id=a.automation_policy_id) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='automatic Live requires exact original native Paper observations';
  END IF;
 ELSIF ids IS NOT NULL THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='promotion evidence belongs only to automatic Live';
 END IF;
 RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER live_promotion AFTER INSERT ON app.approvals
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION app.guard_live_promotion_evidence();
CREATE CONSTRAINT TRIGGER live_promotion AFTER INSERT ON app.live_promotion_evidence
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION app.guard_live_promotion_evidence();
