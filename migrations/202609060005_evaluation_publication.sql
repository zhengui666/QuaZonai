-- Additive invariants. Existing migration checksums and immutable evidence stay
-- unchanged. A publication seals membership; it never grants a PASS decision.
CREATE TABLE app.evaluation_publications (
  evaluation_id app.identity PRIMARY KEY REFERENCES app.evaluations
);
INSERT INTO app.evaluation_publications(evaluation_id) SELECT id FROM app.evaluations;
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.evaluation_publications
FOR EACH ROW EXECUTE FUNCTION app.reject_change();

CREATE FUNCTION app.publish_evaluation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  INSERT INTO app.evaluation_publications(evaluation_id) VALUES(NEW.id) ON CONFLICT DO NOTHING;
  RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER evaluation_publication AFTER INSERT ON app.evaluations
DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION app.publish_evaluation();

CREATE FUNCTION app.guard_evaluation_metric() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  PERFORM id FROM app.evaluations WHERE id=NEW.evaluation_id FOR UPDATE;
  IF NOT FOUND THEN
    RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='evaluation must exist before its metrics';
  END IF;
  IF EXISTS(SELECT 1 FROM app.evaluation_publications WHERE evaluation_id=NEW.evaluation_id) THEN
    RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='published evaluation metric membership is immutable';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER evaluation_metric BEFORE INSERT ON app.metric_values
FOR EACH ROW EXECUTE FUNCTION app.guard_evaluation_metric();

-- References consume an already complete aggregate, including within the
-- creation transaction. The column argument comes only from these DDL bindings.
CREATE FUNCTION app.seal_referenced_evaluation() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE evaluation app.identity;
BEGIN
  evaluation := (to_jsonb(NEW)->>TG_ARGV[0])::uuid;
  IF evaluation IS NOT NULL THEN
    PERFORM id FROM app.evaluations WHERE id=evaluation FOR UPDATE;
    IF NOT FOUND THEN
      RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='evaluation must exist before consumption';
    END IF;
    INSERT INTO app.evaluation_publications(evaluation_id) VALUES(evaluation) ON CONFLICT DO NOTHING;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER seal_evaluation BEFORE INSERT ON app.qualifications
FOR EACH ROW EXECUTE FUNCTION app.seal_referenced_evaluation('qualifying_evaluation_id');
CREATE TRIGGER seal_evaluation BEFORE INSERT ON app.releases
FOR EACH ROW EXECUTE FUNCTION app.seal_referenced_evaluation('evaluation_id');
CREATE TRIGGER seal_evaluation BEFORE INSERT ON app.calibrations
FOR EACH ROW EXECUTE FUNCTION app.seal_referenced_evaluation('validation_evaluation_id');
CREATE TRIGGER seal_evaluation BEFORE INSERT ON app.qualification_revocations
FOR EACH ROW EXECUTE FUNCTION app.seal_referenced_evaluation('evidence_evaluation_id');
CREATE TRIGGER seal_evaluation BEFORE INSERT ON app.evidence_exposures
FOR EACH ROW EXECUTE FUNCTION app.seal_referenced_evaluation('evaluation_id');
CREATE TRIGGER seal_evaluation BEFORE INSERT ON app.forward_evidence_windows
FOR EACH ROW EXECUTE FUNCTION app.seal_referenced_evaluation('evaluation_id');
CREATE TRIGGER seal_evaluation BEFORE INSERT ON app.degradation_observations
FOR EACH ROW EXECUTE FUNCTION app.seal_referenced_evaluation('evaluation_id');
-- Candidate -> allocation Evaluation is a deferred circular FK. Its header
-- may name the evaluation before that evaluation is created in this transaction;
-- both aggregates are still sealed before commit. Do not add a BEFORE guard
-- that makes this documented atomic creation order impossible.

-- All joined identities are immutable (InputSet is frozen). Timing and current
-- authorization remain separate checks in Wake admission, not this history FK.
CREATE FUNCTION app.degradation_binding_valid(project uuid, release uuid, evaluation uuid, policy uuid)
RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT EXISTS (
    SELECT 1 FROM app.automation_policies p
    JOIN app.releases r ON r.id=release AND r.mandate_id=p.mandate_id
    JOIN app.portfolio_candidates c ON c.id=r.candidate_id AND c.project_id=p.project_id
    JOIN app.evaluations e ON e.id=evaluation AND e.subject_candidate_id=c.id AND e.project_id=c.project_id
    JOIN app.input_sets i ON i.id=e.input_set_id AND i.project_id=e.project_id
    JOIN app.forward_evidence_windows w ON w.release_id=r.id AND w.evaluation_id=e.id AND w.input_set_id=i.id
    WHERE p.id=policy AND p.project_id=project AND e.evaluation_kind='FORWARD'
      AND i.purpose='FORWARD' AND i.frozen_at IS NOT NULL
  )
$$;
DO $$ BEGIN
  IF EXISTS(SELECT 1 FROM app.degradation_observations o
            WHERE NOT app.degradation_binding_valid(o.project_id,o.release_id,o.evaluation_id,o.policy_id)) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='existing degradation evidence has incompatible immutable bindings; do not relabel history';
  END IF;
END $$;
CREATE FUNCTION app.guard_degradation_binding() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT app.degradation_binding_valid(NEW.project_id,NEW.release_id,NEW.evaluation_id,NEW.policy_id) THEN
    RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='degradation requires this project policy, release candidate and exact forward evidence';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER degradation_binding BEFORE INSERT ON app.degradation_observations
FOR EACH ROW EXECUTE FUNCTION app.guard_degradation_binding();

CREATE FUNCTION app.guard_browser_epoch() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.session_epoch < OLD.session_epoch THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='global browser revocation epoch cannot decrease';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER browser_epoch BEFORE UPDATE ON app.operator_auth_state
FOR EACH ROW EXECUTE FUNCTION app.guard_browser_epoch();
