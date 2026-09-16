-- Complete the reviewed relational ownership/publication invariants. Native
-- PostgreSQL constraints and row locks own consistency; these are not scientific
-- qualification, provider authentication, or full downstream authorization.
LOCK TABLE app.experiments, app.alpha_versions, app.portfolio_candidates,
  app.candidate_alphas, app.candidate_targets, app.evaluations, app.approvals,
  app.forward_evidence_windows, app.run_attempts, app.runs, app.run_events,
  app.codex_sessions IN SHARE ROW EXCLUSIVE MODE;

ALTER TABLE app.projects ADD CONSTRAINT project_lineage_identity UNIQUE(id,root_lineage_id);
ALTER TABLE app.experiment_families ADD CONSTRAINT family_project_lineage
  FOREIGN KEY(project_id,root_lineage_id) REFERENCES app.projects(id,root_lineage_id);
ALTER TABLE app.experiments ADD CONSTRAINT no_self_parent CHECK(parent_experiment_id IS DISTINCT FROM id);
CREATE FUNCTION app.guard_experiment_ancestry() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF EXISTS (
    WITH RECURSIVE ancestors(id,parent_id) AS (
      SELECT id,parent_experiment_id FROM app.experiments WHERE id=NEW.id
      UNION ALL
      SELECT e.id,e.parent_experiment_id FROM app.experiments e JOIN ancestors a ON e.id=a.parent_id
    ) CYCLE id SET cyclic USING path
    SELECT 1 FROM ancestors WHERE cyclic
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='cyclic experiment ancestry';
  END IF;
  RETURN NULL;
END $$;
-- Parent links are immutable, and their immediate FK requires visible parents.
-- AFTER ROW checks see all rows of one multi-row INSERT, including a new cycle.
CREATE CONSTRAINT TRIGGER experiment_ancestry AFTER INSERT ON app.experiments
FOR EACH ROW EXECUTE FUNCTION app.guard_experiment_ancestry();

CREATE FUNCTION app.guard_alpha_lineage() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE lineage app.identity; code app.identity;
BEGIN
  -- Serialize publication with changes to the experiment result. Merely holding
  -- a FK key-share lock would not stop non-key result changes.
  SELECT code_artifact_id INTO code FROM app.experiments WHERE id=NEW.experiment_id AND project_id=NEW.project_id FOR UPDATE;
  SELECT f.root_lineage_id INTO lineage FROM app.experiments e
    JOIN app.experiment_families f ON f.id=e.family_id WHERE e.id=NEW.experiment_id;
  IF lineage IS NULL OR lineage<>NEW.root_lineage_id OR code IS NULL OR code<>NEW.code_artifact_id THEN
    RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='alpha must retain its experiment root lineage and code';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER alpha_lineage BEFORE INSERT ON app.alpha_versions
FOR EACH ROW EXECUTE FUNCTION app.guard_alpha_lineage();
CREATE FUNCTION app.guard_consumed_experiment() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF ROW(NEW.code_artifact_id,NEW.parameter_artifact_id,NEW.run_id,NEW.outcome,NEW.outcome_reason,NEW.conclusion_artifact_id)
      IS DISTINCT FROM ROW(OLD.code_artifact_id,OLD.parameter_artifact_id,OLD.run_id,OLD.outcome,OLD.outcome_reason,OLD.conclusion_artifact_id)
     AND (EXISTS(SELECT 1 FROM app.alpha_versions WHERE experiment_id=OLD.id)
       OR EXISTS(SELECT 1 FROM app.evaluations WHERE run_id=OLD.run_id)) THEN
    RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='consumed experiment results are immutable';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER consumed_experiment BEFORE UPDATE ON app.experiments
FOR EACH ROW EXECUTE FUNCTION app.guard_consumed_experiment();

-- An internal publication marker implements the already immutable candidate
-- aggregate, not another public candidate state or workflow. All child records
-- must be inserted in the candidate's creation transaction. A consumer seals it
-- earlier in that transaction; otherwise a deferred native trigger seals it at
-- commit. No transaction ID, process memory, hash or clock is used as authority.
CREATE TABLE app.candidate_publications (
  candidate_id app.identity PRIMARY KEY REFERENCES app.portfolio_candidates
);
INSERT INTO app.candidate_publications(candidate_id) SELECT id FROM app.portfolio_candidates;
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.candidate_publications
FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE FUNCTION app.publish_candidate() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  INSERT INTO app.candidate_publications(candidate_id) VALUES(NEW.id) ON CONFLICT DO NOTHING;
  RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER candidate_publication AFTER INSERT ON app.portfolio_candidates
DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION app.publish_candidate();
CREATE FUNCTION app.guard_candidate_member() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE owner app.identity;
BEGIN
  SELECT project_id INTO owner FROM app.portfolio_candidates WHERE id=NEW.candidate_id FOR UPDATE;
  IF owner IS NULL THEN
    RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='candidate must exist before its members';
  END IF;
  IF EXISTS(SELECT 1 FROM app.candidate_publications WHERE candidate_id=NEW.candidate_id) THEN
    RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='published candidate membership is immutable';
  END IF;
  IF TG_TABLE_NAME='candidate_alphas' THEN
    IF NOT EXISTS(SELECT 1 FROM app.alpha_versions WHERE id=NEW.alpha_version_id AND project_id=owner) THEN
      RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='candidate alpha must belong to the same project';
    END IF;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER candidate_member BEFORE INSERT ON app.candidate_alphas
FOR EACH ROW EXECUTE FUNCTION app.guard_candidate_member();
CREATE TRIGGER candidate_member BEFORE INSERT ON app.candidate_targets
FOR EACH ROW EXECUTE FUNCTION app.guard_candidate_member();
CREATE FUNCTION app.seal_evaluation_inputs() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.subject_candidate_id IS NOT NULL THEN
    PERFORM 1 FROM app.portfolio_candidates WHERE id=NEW.subject_candidate_id FOR UPDATE;
    INSERT INTO app.candidate_publications(candidate_id) VALUES(NEW.subject_candidate_id) ON CONFLICT DO NOTHING;
  END IF;
  -- Direct experiment/run consumers share the result-publication lock too.
  PERFORM id FROM app.experiments WHERE run_id=NEW.run_id ORDER BY id FOR UPDATE;
  RETURN NEW;
END $$;
CREATE TRIGGER seal_evaluation_inputs BEFORE INSERT ON app.evaluations
FOR EACH ROW EXECUTE FUNCTION app.seal_evaluation_inputs();

CREATE FUNCTION app.guard_policy_approval_binding() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.authority_kind='FROZEN_POLICY' AND NOT EXISTS (
    SELECT 1 FROM app.automation_policies p JOIN app.releases r ON r.id=NEW.release_id
    JOIN app.portfolio_candidates c ON c.id=r.candidate_id
    WHERE p.id=NEW.automation_policy_id AND p.project_id=c.project_id
      AND p.mandate_id=r.mandate_id AND p.downstream_id=NEW.downstream_id
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='policy must authorize this exact release mandate and downstream';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER policy_approval_binding BEFORE INSERT ON app.approvals
FOR EACH ROW EXECUTE FUNCTION app.guard_policy_approval_binding();
CREATE FUNCTION app.guard_forward_window_binding() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM app.releases r JOIN app.evaluations e ON e.subject_candidate_id=r.candidate_id
    JOIN app.input_sets i ON i.id=e.input_set_id AND i.project_id=e.project_id
    WHERE r.id=NEW.release_id AND e.id=NEW.evaluation_id AND e.input_set_id=NEW.input_set_id
      AND e.evaluation_kind='FORWARD' AND i.purpose='FORWARD' AND i.frozen_at IS NOT NULL
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='forward window requires the release candidate and exact frozen forward inputs';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER forward_window_binding BEFORE INSERT ON app.forward_evidence_windows
FOR EACH ROW EXECUTE FUNCTION app.guard_forward_window_binding();

ALTER TABLE app.artifacts ADD CONSTRAINT artifact_producer_identity UNIQUE(id,producer_run_id,producer_attempt_id);
ALTER TABLE app.run_attempts ADD CONSTRAINT manifest_exact_producer
  FOREIGN KEY(result_manifest_artifact_id,run_id,id)
  REFERENCES app.artifacts(id,producer_run_id,producer_attempt_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE app.run_attempts ADD CONSTRAINT accepted_manifest_present
  CHECK(accepted_at IS NULL OR result_manifest_artifact_id IS NOT NULL);
CREATE FUNCTION app.guard_accepted_attempt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.accepted_at IS NOT NULL AND
      ROW(NEW.result_manifest_artifact_id,NEW.accepted_at,NEW.runtime_state,NEW.error_class,NEW.error_code)
      IS DISTINCT FROM ROW(OLD.result_manifest_artifact_id,OLD.accepted_at,OLD.runtime_state,OLD.error_class,OLD.error_code) THEN
    RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='accepted attempt evidence is immutable';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER accepted_attempt BEFORE UPDATE ON app.run_attempts
FOR EACH ROW EXECUTE FUNCTION app.guard_accepted_attempt();
CREATE FUNCTION app.guard_session_profile_revision() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE actual app.revision;
BEGIN
  SELECT revision INTO actual FROM app.codex_profiles WHERE id=NEW.profile_id FOR SHARE;
  IF actual IS NULL OR actual<>NEW.profile_revision THEN
    RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='session must freeze an existing current profile revision';
  END IF;
  -- requested_settings is already protected by the immutable-binding trigger;
  -- future profile edits cannot rewrite this saved native request or revision.
  RETURN NEW;
END $$;
CREATE TRIGGER session_profile_revision BEFORE INSERT ON app.codex_sessions
FOR EACH ROW EXECUTE FUNCTION app.guard_session_profile_revision();

CREATE FUNCTION app.advance_run_event_cursor() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE previous bigint;
BEGIN
  -- AFTER INSERT also supports several ordered events in one INSERT statement.
  -- The lock is retained until commit, coupling stream and projection visibility.
  SELECT last_event_seq INTO previous FROM app.runs WHERE id=NEW.run_id FOR UPDATE;
  IF previous IS NULL OR previous=9223372036854775807 OR NEW.seq<>previous+1 THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='event sequence must follow the locked run cursor';
  END IF;
  UPDATE app.runs SET last_event_seq=NEW.seq WHERE id=NEW.run_id;
  RETURN NULL;
END $$;
CREATE FUNCTION app.guard_run_event_cursor() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP='INSERT' THEN
    IF NEW.last_event_seq<>0 THEN
      RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='new run cannot claim preexisting events';
    END IF;
  ELSIF NEW.last_event_seq<>OLD.last_event_seq THEN
    IF OLD.last_event_seq=9223372036854775807 OR NEW.last_event_seq<>OLD.last_event_seq+1
       OR NOT EXISTS(SELECT 1 FROM app.run_events WHERE run_id=NEW.id AND seq=NEW.last_event_seq) THEN
      RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='run cursor requires the exact next durable event';
    END IF;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER event_cursor AFTER INSERT ON app.run_events FOR EACH ROW EXECUTE FUNCTION app.advance_run_event_cursor();
CREATE TRIGGER event_cursor BEFORE INSERT OR UPDATE ON app.runs FOR EACH ROW EXECUTE FUNCTION app.guard_run_event_cursor();

-- Existing immutable facts are verified rather than silently repaired or
-- relabelled. Historical session revisions can be older than today's profile;
-- no synthetic settings history is invented for them.
DO $$ BEGIN
  IF EXISTS(SELECT 1 FROM app.alpha_versions a JOIN app.experiments e ON e.id=a.experiment_id
      JOIN app.experiment_families f ON f.id=e.family_id WHERE a.root_lineage_id<>f.root_lineage_id OR a.code_artifact_id IS DISTINCT FROM e.code_artifact_id)
    OR EXISTS(SELECT 1 FROM app.candidate_alphas m JOIN app.portfolio_candidates c ON c.id=m.candidate_id
      JOIN app.alpha_versions a ON a.id=m.alpha_version_id WHERE a.project_id<>c.project_id)
    OR EXISTS(SELECT 1 FROM app.approvals a JOIN app.automation_policies p ON p.id=a.automation_policy_id
      JOIN app.releases r ON r.id=a.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id
      WHERE p.project_id<>c.project_id OR p.mandate_id<>r.mandate_id OR p.downstream_id<>a.downstream_id)
    OR EXISTS(SELECT 1 FROM app.forward_evidence_windows w JOIN app.releases r ON r.id=w.release_id
      JOIN app.evaluations e ON e.id=w.evaluation_id JOIN app.input_sets i ON i.id=w.input_set_id
      WHERE e.subject_candidate_id IS DISTINCT FROM r.candidate_id OR e.input_set_id<>w.input_set_id
        OR e.evaluation_kind<>'FORWARD' OR i.purpose<>'FORWARD' OR i.frozen_at IS NULL)
    OR EXISTS(SELECT 1 FROM app.codex_sessions s JOIN app.codex_profiles p ON p.id=s.profile_id
      WHERE s.profile_revision>p.revision)
    OR EXISTS(SELECT 1 FROM app.runs r LEFT JOIN LATERAL (
      SELECT count(*) n,coalesce(max(seq),0) last FROM app.run_events WHERE run_id=r.id
    ) e ON true WHERE r.last_event_seq<>e.last OR e.last<>e.n)
  THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='existing evidence binding requires explicit operator repair';
  END IF;
  IF EXISTS (
    WITH RECURSIVE ancestry(id,parent_id) AS (
      SELECT id,parent_experiment_id FROM app.experiments
      UNION ALL
      SELECT e.id,e.parent_experiment_id FROM app.experiments e JOIN ancestry a ON e.id=a.parent_id
    ) CYCLE id SET cyclic USING path SELECT 1 FROM ancestry WHERE cyclic
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='existing experiment ancestry is cyclic';
  END IF;
END $$;
