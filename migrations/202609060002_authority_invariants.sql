-- A8.1/A9 and handoff/feedback review corrections. These are native relational
-- invariants, not substitutes for live HTTP authentication or release policy.
-- SQLx applies this additive migration atomically; applied files stay unchanged.
ALTER TABLE app.machine_principals ADD CONSTRAINT principal_kind_bindings CHECK (
  (kind='MISSION' AND project_id IS NOT NULL AND run_id IS NOT NULL AND downstream_id IS NULL)
  OR (kind='DOWNSTREAM' AND downstream_id IS NOT NULL AND run_id IS NULL)
  OR (kind IN ('CLI','AUTOMATION') AND downstream_id IS NULL AND run_id IS NULL)
);

CREATE FUNCTION app.valid_machine_scopes_v1(codes text[]) RETURNS boolean
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
  SELECT coalesce(
    array_ndims(codes)=1 AND array_lower(codes,1)=1
    AND cardinality(codes) BETWEEN 1 AND 10
    AND cardinality(codes)=(SELECT count(DISTINCT code) FROM unnest(codes) AS u(code))
    AND NOT EXISTS (
      SELECT 1 FROM unnest(codes) AS u(code)
      WHERE code IS NULL OR code NOT IN (
        'RESEARCH_READ','EXPERIMENT_SUBMIT','ARTIFACT_SUBMIT','EVIDENCE_READ',
        'RUN_READ','RUN_CANCEL','DOWNSTREAM_CLAIM','DOWNSTREAM_ACK','FORWARD_SUBMIT','DOCTOR_READ'
      )
    ), false
  )
$$;
ALTER TABLE app.machine_credentials ADD CONSTRAINT credential_closed_scopes
CHECK (app.valid_machine_scopes_v1(scope_codes));

CREATE FUNCTION app.guard_machine_issuance() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p app.machine_principals; mission app.runs; project_state text;
BEGIN
  IF NOT app.valid_machine_scopes_v1(NEW.scope_codes) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='invalid machine scope set';
  END IF;
  -- Immutable binding lookup determines the project -> run -> principal lock
  -- order. Authority is reread under the last lock, never granted by this read.
  SELECT * INTO STRICT p FROM app.machine_principals WHERE id=NEW.principal_id;
  IF p.project_id IS NOT NULL THEN
    SELECT state INTO STRICT project_state FROM app.projects WHERE id=p.project_id FOR SHARE;
  END IF;
  IF p.kind='MISSION' THEN
    SELECT * INTO STRICT mission FROM app.runs
      WHERE id=p.run_id AND project_id=p.project_id FOR SHARE;
  END IF;
  SELECT * INTO STRICT p FROM app.machine_principals WHERE id=NEW.principal_id FOR SHARE;
  IF NOT p.enabled OR NEW.principal_epoch<>p.credential_epoch THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='inactive or stale principal issuance';
  END IF;
  IF p.project_id IS NULL AND NEW.scope_codes IS DISTINCT FROM ARRAY['DOCTOR_READ']::text[] THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='non-doctor credential requires a project';
  END IF;
  IF NEW.issued_by='MISSION_SERVICE' AND p.kind<>'MISSION' THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='mission service cannot issue another identity kind';
  END IF;
  IF p.kind='DOWNSTREAM' THEN
    IF NOT NEW.scope_codes <@ ARRAY['DOWNSTREAM_CLAIM','DOWNSTREAM_ACK','FORWARD_SUBMIT','DOCTOR_READ']::text[] THEN
      RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='downstream scope exceeds delivery boundary';
    END IF;
  ELSIF NEW.scope_codes && ARRAY['DOWNSTREAM_CLAIM','DOWNSTREAM_ACK','FORWARD_SUBMIT']::text[] THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='non-downstream principal cannot act as a downstream';
  END IF;
  IF p.kind='MISSION' AND (
      project_state<>'ACTIVE' OR mission.state NOT IN ('DISPATCHING','RUNNING','RECONCILING')
      OR NEW.expires_at>mission.deadline_at OR NEW.expires_at<=clock_timestamp()
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='mission credential outlives active work';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER issuance_authority BEFORE INSERT ON app.machine_credentials
FOR EACH ROW EXECUTE FUNCTION app.guard_machine_issuance();
-- Existing immutable issuances must satisfy timeless bindings too. Do not
-- rewrite historical credentials or compare their epoch with today's epoch.
DO $$ BEGIN
  IF EXISTS (
    SELECT 1 FROM app.machine_credentials c JOIN app.machine_principals p ON p.id=c.principal_id
    LEFT JOIN app.runs r ON r.id=p.run_id
    WHERE (p.project_id IS NULL AND c.scope_codes IS DISTINCT FROM ARRAY['DOCTOR_READ']::text[])
       OR (c.issued_by='MISSION_SERVICE' AND p.kind<>'MISSION')
       OR (p.kind='DOWNSTREAM' AND NOT c.scope_codes <@ ARRAY['DOWNSTREAM_CLAIM','DOWNSTREAM_ACK','FORWARD_SUBMIT','DOCTOR_READ']::text[])
       OR (p.kind<>'DOWNSTREAM' AND c.scope_codes && ARRAY['DOWNSTREAM_CLAIM','DOWNSTREAM_ACK','FORWARD_SUBMIT']::text[])
       OR (p.kind='MISSION' AND c.expires_at>r.deadline_at)
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='existing credential binding requires explicit operator repair';
  END IF;
END $$;

ALTER TABLE app.handoff_offers ADD CONSTRAINT handoff_claim_shape CHECK (
  (state NOT IN ('CLAIMED','ACKNOWLEDGED') OR (external_claim_id IS NOT NULL AND claimed_at IS NOT NULL))
  AND (state NOT IN ('OFFERED','REVOKED','EXPIRED') OR (external_claim_id IS NULL AND claimed_at IS NULL))
  AND (external_claim_id IS NULL OR length(btrim(external_claim_id))>0)
  AND (claimed_at IS NULL OR (claimed_at>=offered_at AND claimed_at<expires_at))
  AND (state<>'ACKNOWLEDGED' OR acknowledged_at IS NOT NULL)
  AND (acknowledged_at IS NULL OR (
    state IN ('ACKNOWLEDGED','REJECTED') AND claimed_at IS NOT NULL AND acknowledged_at>=claimed_at
  ))
);
CREATE FUNCTION app.guard_handoff_transition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP='INSERT' THEN
    IF NEW.state<>'OFFERED' THEN
      RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='handoff starts offered, not already claimed';
    END IF;
    RETURN NEW;
  END IF;
  IF NEW.state=OLD.state THEN
    IF ROW(NEW.external_claim_id,NEW.claimed_at,NEW.acknowledged_at)
       IS DISTINCT FROM ROW(OLD.external_claim_id,OLD.claimed_at,OLD.acknowledged_at) THEN
      RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='handoff evidence changes only with a transition';
    END IF;
    RETURN NEW;
  END IF;
  IF NOT ((OLD.state='OFFERED' AND NEW.state IN ('CLAIMED','REJECTED','REVOKED','EXPIRED'))
       OR (OLD.state='CLAIMED' AND NEW.state IN ('ACKNOWLEDGED','REJECTED'))) THEN
    RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='invalid handoff transition; claimed execution cannot be revoked';
  END IF;
  IF OLD.claimed_at IS NOT NULL AND
     ROW(NEW.external_claim_id,NEW.claimed_at) IS DISTINCT FROM ROW(OLD.external_claim_id,OLD.claimed_at) THEN
    RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='claimed external identity is immutable';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER handoff_transition BEFORE INSERT OR UPDATE ON app.handoff_offers
FOR EACH ROW EXECUTE FUNCTION app.guard_handoff_transition();

ALTER TABLE app.forward_messages ADD CONSTRAINT feedback_revision_shape CHECK (
  (message_revision=1 AND supersedes_message_id IS NULL AND coverage_status IN ('COMPLETE','PARTIAL'))
  OR (message_revision>1 AND supersedes_message_id IS NOT NULL AND coverage_status='CORRECTION')
);
-- Together with the existing logical tuple UNIQUE/FK and immutable parent, the
-- immediately previous revision excludes jumps, forks, cycles and cross-stream
-- corrections without a first-party sequencer or deduplication hash.
CREATE FUNCTION app.guard_feedback_revision() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE previous integer;
BEGIN
  IF NEW.supersedes_message_id IS NOT NULL THEN
    SELECT message_revision INTO previous FROM app.forward_messages WHERE id=NEW.supersedes_message_id;
    IF previous IS NULL THEN
      RAISE EXCEPTION USING ERRCODE='23503', MESSAGE='correction requires its published predecessor';
    END IF;
    IF NEW.message_revision-1<>previous THEN
      RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='correction must increment the immediate predecessor';
    END IF;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER feedback_revision BEFORE INSERT ON app.forward_messages
FOR EACH ROW EXECUTE FUNCTION app.guard_feedback_revision();
DO $$ BEGIN
  IF EXISTS (
    SELECT 1 FROM app.forward_messages child JOIN app.forward_messages parent
      ON parent.id=child.supersedes_message_id
    WHERE child.message_revision-1<>parent.message_revision
  ) THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='existing feedback revision chain requires explicit repair';
  END IF;
END $$;

-- Existing UNIQUE indexes already cover input membership, event cursors,
-- attempts, candidate members/targets and logical feedback tuple prefixes.
CREATE INDEX projects_by_state ON app.projects(state,updated_at);
CREATE INDEX experiments_by_family ON app.experiments(family_id,ordinal);
CREATE INDEX artifacts_by_producer_run ON app.artifacts(producer_run_id);
CREATE INDEX evaluations_by_alpha_conclusion ON app.evaluations(subject_alpha_version_id,concluded_at DESC);
CREATE INDEX evaluations_by_candidate ON app.evaluations(subject_candidate_id);
CREATE INDEX releases_by_candidate ON app.releases(candidate_id);
CREATE INDEX handoffs_by_delivery_state ON app.handoff_offers(downstream_id,environment,state,delivery_sequence);
