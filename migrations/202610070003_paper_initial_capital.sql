-- A model initial condition, not an observed account or a balance ledger.
-- Keep its original object identity across Build retries and mandate changes.
CREATE TABLE app.paper_initial_capital_sources (
 weights_artifact_id app.identity PRIMARY KEY REFERENCES app.artifacts,
 project_id app.identity NOT NULL REFERENCES app.projects,
 mandate_id app.identity NOT NULL REFERENCES app.portfolio_mandates,
 downstream_id app.identity NOT NULL REFERENCES app.downstream_integrations,
 trader_id app.nonempty NOT NULL,
 account_id app.nonempty NOT NULL,
 execution_assumptions_id app.identity NOT NULL REFERENCES app.execution_assumptions,
 base_currency app.nonempty NOT NULL,
 starting_capital numeric NOT NULL CHECK(starting_capital>0),
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(downstream_id,trader_id,account_id),
 CHECK(trader_id=btrim(trader_id) AND trader_id !~ '[[:cntrl:]]'),
 CHECK(account_id=btrim(account_id) AND account_id !~ '[[:cntrl:]]')
);
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.paper_initial_capital_sources
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();

-- Legacy weights and claims have no account subdivision. They cannot prove
-- this account is empty, even after rejection, missing ACK or a new mandate.
CREATE FUNCTION app.paper_account_uninitialized(downstream uuid, trader text, account text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT NOT EXISTS(SELECT 1 FROM app.forward_weight_snapshots WHERE downstream_id=downstream)
  AND NOT EXISTS(SELECT 1 FROM app.handoff_offers WHERE downstream_id=downstream AND claimed_at IS NOT NULL)
  AND NOT EXISTS(SELECT 1 FROM app.native_account_sources WHERE downstream_id=downstream
    AND native_trader_id=trader AND native_account_id=account)
$$;
CREATE FUNCTION app.paper_initial_capital_unused(artifact uuid)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM app.paper_initial_capital_sources s
  WHERE s.weights_artifact_id=artifact
   AND app.paper_account_uninitialized(s.downstream_id,s.trader_id,s.account_id))
$$;
CREATE FUNCTION app.guard_paper_initial_capital_source() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM id FROM app.downstream_integrations WHERE id=NEW.downstream_id
  AND enabled AND environments IN ('PAPER','BOTH') FOR UPDATE;
 IF NOT FOUND OR NOT app.paper_account_uninitialized(NEW.downstream_id,NEW.trader_id,NEW.account_id) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PAPER_ACCOUNT_ALREADY_HAS_STATE';
 END IF;
 IF NOT EXISTS(SELECT 1 FROM app.artifacts a JOIN app.portfolio_mandates m ON m.id=NEW.mandate_id
  JOIN app.execution_assumptions e ON e.id=NEW.execution_assumptions_id
  WHERE a.id=NEW.weights_artifact_id AND a.project_id=NEW.project_id AND m.project_id=NEW.project_id
   AND m.execution_assumptions_id=e.id AND m.base_currency=NEW.base_currency AND e.base_currency=NEW.base_currency
   AND m.capital_assumption=NEW.starting_capital AND e.starting_capital=NEW.starting_capital
   AND a.kind='REPORT' AND a.schema_name='qz.portfolio_current_weights' AND a.schema_version='1'
   AND a.origin='SYNTHETIC' AND a.access_class='EVALUATOR_ONLY' AND a.byte_count>0
   AND a.storage_backend='LOCAL' AND a.storage_object_ref=a.id::text AND a.storage_version='1'
   AND a.media_type='application/json') THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PAPER_INITIAL_CAPITAL_SOURCE_BINDING';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER paper_initial_capital_source BEFORE INSERT ON app.paper_initial_capital_sources
 FOR EACH ROW EXECUTE FUNCTION app.guard_paper_initial_capital_source();

ALTER TABLE app.portfolio_build_tasks ADD COLUMN paper_initial_weights_artifact_id app.identity
 REFERENCES app.paper_initial_capital_sources(weights_artifact_id);
ALTER TABLE app.portfolio_build_tasks DROP CONSTRAINT portfolio_build_source_shape;
ALTER TABLE app.portfolio_build_tasks ADD CONSTRAINT portfolio_build_source_shape CHECK (
 (source_kind='FORECAST' AND purpose IS NULL AND (
  ((snapshot_id IS NOT NULL)::integer+(last_target_candidate_id IS NOT NULL)::integer=1)
  OR (snapshot_id IS NULL AND last_target_candidate_id IS NULL AND paper_initial_weights_artifact_id IS NOT NULL
   AND (request->'current_weights_source'->>'kind'='PAPER_INITIAL_CAPITAL') IS TRUE)))
 OR (source_kind='STRATEGY_ALPHA' AND purpose IS NOT NULL
  AND snapshot_id IS NULL AND last_target_candidate_id IS NULL AND paper_initial_weights_artifact_id IS NULL
  AND (request->>'source_kind'='STRATEGY_ALPHA') IS TRUE
  AND (request->'purpose'->>'purpose'=purpose) IS TRUE));

ALTER TABLE app.portfolio_candidates ADD COLUMN paper_initial_weights_artifact_id app.identity
 REFERENCES app.paper_initial_capital_sources(weights_artifact_id);
ALTER TABLE app.portfolio_candidates DROP CONSTRAINT portfolio_candidates_current_weights_source_check;
ALTER TABLE app.portfolio_candidates ADD CONSTRAINT portfolio_candidates_current_weights_source_check
 CHECK(current_weights_source IN ('FORWARD_SNAPSHOT','LAST_TARGET','PAPER_INITIAL_CAPITAL','NONE'));
CREATE FUNCTION app.guard_paper_build_root() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.paper_initial_weights_artifact_id IS NOT NULL AND NOT EXISTS (
  SELECT 1 FROM app.paper_initial_capital_sources s JOIN app.portfolio_mandates m ON m.id=NEW.mandate_id
  JOIN app.runs r ON r.id=NEW.run_id
  WHERE s.weights_artifact_id=NEW.paper_initial_weights_artifact_id AND s.project_id=r.project_id
   AND m.project_id=s.project_id AND m.base_currency=s.base_currency
   AND m.capital_assumption=s.starting_capital AND m.execution_assumptions_id=s.execution_assumptions_id
   AND NEW.source_kind='FORECAST' AND NEW.request->>'environment'='PAPER'
   AND (
    (NEW.snapshot_id IS NULL AND NEW.last_target_candidate_id IS NULL
     AND NEW.request->'current_weights_source'->>'kind'='PAPER_INITIAL_CAPITAL'
     AND NEW.request->'current_weights_source'->>'downstream_id'=s.downstream_id::text
     AND NEW.request->'current_weights_source'->>'trader_id'=s.trader_id
     AND NEW.request->'current_weights_source'->>'account_id'=s.account_id)
    OR EXISTS(SELECT 1 FROM app.forward_weight_snapshots w WHERE w.id=NEW.snapshot_id
     AND w.project_id=s.project_id AND w.downstream_id=s.downstream_id AND w.environment='PAPER'
     AND w.content->'paper_initialization'->>'artifact_id'=s.weights_artifact_id::text)
    OR EXISTS(SELECT 1 FROM app.portfolio_candidates c WHERE c.id=NEW.last_target_candidate_id
     AND c.project_id=s.project_id AND c.paper_initial_weights_artifact_id=s.weights_artifact_id)
   )
 ) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PAPER_BUILD_ROOT_BINDING';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER paper_build_root BEFORE INSERT ON app.portfolio_build_tasks
 FOR EACH ROW EXECUTE FUNCTION app.guard_paper_build_root();
CREATE FUNCTION app.guard_paper_candidate_root() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.paper_initial_weights_artifact_id IS DISTINCT FROM
  (SELECT paper_initial_weights_artifact_id FROM app.portfolio_build_tasks WHERE run_id=NEW.run_id)
  OR (NEW.current_weights_source='PAPER_INITIAL_CAPITAL' AND NEW.paper_initial_weights_artifact_id IS NULL) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PAPER_CANDIDATE_ROOT_BINDING';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER paper_candidate_root BEFORE INSERT ON app.portfolio_candidates
 FOR EACH ROW EXECUTE FUNCTION app.guard_paper_candidate_root();

ALTER TABLE app.releases ADD COLUMN paper_initial_weights_artifact_id app.identity
 REFERENCES app.paper_initial_capital_sources(weights_artifact_id);
ALTER TABLE app.releases DROP CONSTRAINT releases_environment_check;
ALTER TABLE app.releases ADD CONSTRAINT releases_environment_check
 CHECK(environment IN ('DEMO','REAL','SYNTHETIC'));
ALTER TABLE app.releases ADD CONSTRAINT paper_release_scope CHECK (
 (paper_initial_weights_artifact_id IS NULL AND environment<>'SYNTHETIC')
 OR (paper_initial_weights_artifact_id IS NOT NULL AND source_kind='FORECAST_EVALUATION'
  AND package_schema_version='2' AND environment='SYNTHETIC' AND execution_environment='PAPER'));
CREATE FUNCTION app.paper_release_root_valid(release uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id
  JOIN app.portfolio_build_tasks b ON b.run_id=c.run_id
  JOIN app.paper_initial_capital_sources s ON s.weights_artifact_id=r.paper_initial_weights_artifact_id
  WHERE r.id=release AND r.source_kind='FORECAST_EVALUATION' AND r.package_schema_version='2'
   AND r.environment='SYNTHETIC' AND r.execution_environment='PAPER'
   AND c.source_kind='FORECAST' AND c.paper_initial_weights_artifact_id=s.weights_artifact_id
   AND c.project_id=s.project_id AND b.paper_initial_weights_artifact_id=s.weights_artifact_id
   AND b.request->>'environment'='PAPER')
$$;
CREATE FUNCTION app.guard_paper_release_root() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.paper_initial_weights_artifact_id IS DISTINCT FROM
  (SELECT paper_initial_weights_artifact_id FROM app.portfolio_candidates WHERE id=NEW.candidate_id) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PAPER_RELEASE_ROOT_BINDING';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER paper_release_root BEFORE INSERT ON app.releases
 FOR EACH ROW EXECUTE FUNCTION app.guard_paper_release_root();
CREATE OR REPLACE FUNCTION app.release_package_valid(candidate uuid, package uuid, version text, environment text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM app.portfolio_candidates c JOIN app.artifacts a ON a.id=package
  WHERE c.id=candidate AND a.project_id=c.project_id AND a.kind='PACKAGE' AND a.byte_count>0
   AND a.media_type='application/json' AND a.schema_name='qz.target_package' AND a.schema_version=version
   AND (
    (c.source_kind='FORECAST' AND version IN ('1','2') AND c.paper_initial_weights_artifact_id IS NULL
     AND (environment='DEMO' OR (environment='REAL' AND a.origin='REAL' AND a.access_class='DELIVERY')))
    OR (c.source_kind='FORECAST' AND version='2' AND c.paper_initial_weights_artifact_id IS NOT NULL
     AND environment='SYNTHETIC' AND a.origin='SYNTHETIC' AND a.access_class='DELIVERY')
    OR (c.source_kind='STRATEGY_ALPHA' AND c.purpose='CURRENT_DECISION' AND version='2'
     AND a.access_class='DELIVERY' AND ((environment='DEMO' AND a.origin IN ('SYNTHETIC','FIXTURE','LEGACY_UNKNOWN'))
      OR (environment='REAL' AND a.origin='REAL')))))
$$;
CREATE OR REPLACE FUNCTION app.guard_real_delivery() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM app.releases r WHERE r.id=NEW.release_id
  AND ((r.source_kind='FORECAST_EVALUATION' AND r.environment='REAL')
   OR (r.source_kind='NATIVE_TARGET_DECISION' AND r.execution_environment='PAPER' AND NEW.environment='PAPER')
   OR (app.paper_release_root_valid(r.id) AND NEW.environment='PAPER' AND EXISTS(
    SELECT 1 FROM app.paper_initial_capital_sources s WHERE s.weights_artifact_id=r.paper_initial_weights_artifact_id
     AND s.downstream_id=NEW.downstream_id)))) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='RELEASE_ENVIRONMENT_NOT_DELIVERABLE';
 END IF;
 RETURN NEW;
END $$;
CREATE FUNCTION app.guard_paper_initial_claim() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE initial app.paper_initial_capital_sources%ROWTYPE; source_kind text; accepted timestamptz;
BEGIN
 IF OLD.state='OFFERED' AND NEW.state='CLAIMED' AND EXISTS(
  SELECT 1 FROM app.releases WHERE id=NEW.release_id AND paper_initial_weights_artifact_id IS NOT NULL) THEN
  -- Match the Store lock order and serialize direct writers against source
  -- creation and the destination FK of new weights/account observations.
  PERFORM id FROM app.downstream_integrations WHERE id=NEW.downstream_id FOR UPDATE;
  SELECT s.* INTO initial
   FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id
   JOIN app.paper_initial_capital_sources s ON s.weights_artifact_id=r.paper_initial_weights_artifact_id
   WHERE r.id=NEW.release_id FOR UPDATE OF s;
  IF FOUND THEN
   accepted:=clock_timestamp();
   IF accepted<NEW.offered_at OR accepted>=NEW.expires_at THEN
    RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='RELEASE_EXPIRED';
   END IF;
   NEW.claimed_at:=accepted;
   SELECT c.current_weights_source INTO source_kind FROM app.releases r
    JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE r.id=NEW.release_id;
   IF NEW.environment<>'PAPER' OR NEW.downstream_id<>initial.downstream_id
    OR NOT app.paper_release_root_valid(NEW.release_id) OR clock_timestamp()<initial.created_at THEN
    RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PAPER_CLAIM_ROOT_BINDING';
   END IF;
   IF source_kind='PAPER_INITIAL_CAPITAL' AND NOT app.paper_initial_capital_unused(initial.weights_artifact_id) THEN
    RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PAPER_ACCOUNT_ALREADY_HAS_STATE';
   END IF;
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER paper_initial_claim BEFORE UPDATE ON app.handoff_offers
 FOR EACH ROW EXECUTE FUNCTION app.guard_paper_initial_claim();

-- Original Paper returns keep simulated provenance through evaluation. This
-- function recognizes only the immutable root and exact claimed destination.
CREATE FUNCTION app.forward_delivery_origin(handoff uuid) RETURNS text LANGUAGE sql STABLE AS $$
 SELECT CASE WHEN r.environment='REAL' THEN 'REAL'
  WHEN h.environment='PAPER' AND app.paper_release_root_valid(r.id) AND EXISTS(
   SELECT 1 FROM app.paper_initial_capital_sources s WHERE s.weights_artifact_id=r.paper_initial_weights_artifact_id
    AND s.downstream_id=h.downstream_id) THEN 'SYNTHETIC' ELSE NULL END
 FROM app.handoff_offers h JOIN app.releases r ON r.id=h.release_id WHERE h.id=handoff
$$;
CREATE OR REPLACE FUNCTION app.forward_report_valid(handoff uuid, report uuid)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM app.handoff_offers h JOIN app.releases r ON r.id=h.release_id
  JOIN app.portfolio_candidates c ON c.id=r.candidate_id JOIN app.artifacts a ON a.id=report
  WHERE h.id=handoff AND a.project_id=c.project_id AND a.kind='REPORT' AND a.media_type='application/json'
   AND a.schema_name='qz.forward_report' AND a.schema_version='1'
   AND a.origin=app.forward_delivery_origin(h.id) AND a.access_class='EVALUATOR_ONLY' AND a.byte_count>0)
$$;

CREATE FUNCTION app.guard_paper_weights_root() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 -- Take the destination lock before deciding whether initialization exists;
 -- otherwise a direct writer could check absence, wait on the FK, then append
 -- unrooted weights immediately after a concurrent source receipt commits.
 PERFORM id FROM app.downstream_integrations WHERE id=NEW.downstream_id FOR SHARE;
 IF (NEW.content->'paper_initialization' IS NULL OR NEW.content->'paper_initialization'='null'::jsonb)
  AND EXISTS(SELECT 1 FROM app.paper_initial_capital_sources WHERE downstream_id=NEW.downstream_id) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PAPER_WEIGHTS_ROOT_REQUIRED';
 END IF;
 IF NEW.content->'paper_initialization' IS NOT NULL AND NEW.content->'paper_initialization'<>'null'::jsonb
  AND NOT EXISTS(SELECT 1 FROM app.paper_initial_capital_sources s
   JOIN app.releases r ON r.paper_initial_weights_artifact_id=s.weights_artifact_id
   JOIN app.handoff_offers h ON h.release_id=r.id
   JOIN app.handoff_transfers t ON t.handoff_id=h.id AND t.external_claim_id=h.external_claim_id AND t.claimed_at=h.claimed_at
   JOIN app.artifacts a ON a.id=NEW.report_artifact_id
   WHERE s.weights_artifact_id::text=NEW.content->'paper_initialization'->>'artifact_id'
    AND s.project_id=NEW.project_id AND s.downstream_id=NEW.downstream_id AND NEW.environment='PAPER'
    AND s.downstream_id::text=NEW.content->'paper_initialization'->>'downstream_id'
    AND s.trader_id=NEW.content->'paper_initialization'->>'trader_id'
    AND s.account_id=NEW.content->'paper_initialization'->>'account_id'
    AND s.base_currency=NEW.content->>'base_currency' AND a.origin='SYNTHETIC'
    AND h.downstream_id=s.downstream_id AND h.environment='PAPER' AND h.claimed_at IS NOT NULL
    AND app.paper_release_root_valid(r.id) AND t.provenance='RECORDED_TRANSITION'
    AND extract(epoch FROM h.claimed_at)*1000000000 <= (NEW.content->>'asof_ns')::numeric) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PAPER_WEIGHTS_ROOT_BINDING';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER paper_weights_root BEFORE INSERT ON app.forward_weight_snapshots
 FOR EACH ROW EXECUTE FUNCTION app.guard_paper_weights_root();

-- Preserve the complete inherited-limits and native-task protection, varying
-- only the proven economic origin for this explicit Paper root.
CREATE OR REPLACE FUNCTION app.guard_run_admission_identity() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE r app.runs; expected_limits jsonb;
BEGIN
 SELECT * INTO STRICT r FROM app.runs WHERE id=NEW.run_id FOR UPDATE;
 IF NEW.project_id IS DISTINCT FROM r.project_id OR NEW.cycle_id IS DISTINCT FROM r.cycle_id THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='run admission must bind the exact project and optional cycle';
 END IF;
 IF NEW.cycle_id IS NULL THEN
  IF r.kind='FORWARD_EVALUATE' THEN
   SELECT app.forward_execution_limits(a.limits) INTO expected_limits
    FROM app.forward_evaluation_inputs f
    JOIN app.input_sets i ON i.id=f.input_set_id AND i.project_id=f.project_id
    JOIN app.handoff_offers h ON h.id=f.handoff_id
    JOIN app.releases released ON released.id=h.release_id
    JOIN app.portfolio_candidates candidate ON candidate.id=released.candidate_id AND candidate.project_id=f.project_id
    JOIN app.run_admissions a ON a.run_id=candidate.run_id AND a.project_id=candidate.project_id
    JOIN app.handoff_transfers transfer ON transfer.handoff_id=h.id AND transfer.downstream_id=h.downstream_id
       AND transfer.external_claim_id=h.external_claim_id AND transfer.claimed_at=h.claimed_at
    WHERE f.input_set_id=r.input_set_id AND f.project_id=r.project_id AND f.runtime_id=NEW.runtime_id
      AND a.runtime_id=NEW.runtime_id AND f.request->>'operation'='EVALUATE_FORWARD'
      AND i.purpose='FORWARD' AND i.frozen_at IS NOT NULL
      AND h.state IN ('CLAIMED','ACKNOWLEDGED') AND app.forward_delivery_origin(h.id) IS NOT NULL
      AND transfer.provenance='RECORDED_TRANSITION';
   IF expected_limits IS NULL OR NEW.limits IS DISTINCT FROM expected_limits
      OR NEW.normalized_request->>'max_parallel_runs' IS DISTINCT FROM '2' THEN
    RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='forward admission requires its exact inherited protected input';
   END IF;
  ELSIF r.kind NOT IN ('IMPORT','EXPORT','DATA_VALIDATE') OR NEW.limits->>'experiments' IS DISTINCT FROM '0' THEN
   RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='run admission must bind the exact project and optional cycle';
  END IF;
 END IF;
 RETURN NEW;
END $$;

CREATE OR REPLACE FUNCTION app.guard_native_task_binding() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE valid boolean;
BEGIN
 PERFORM id FROM app.runs WHERE id=NEW.run_id FOR UPDATE;
 SELECT r.state='QUEUED' AND r.active_attempt_id IS NULL
   AND (r.kind IN ('DATA_VALIDATE','ALPHA_EVALUATE','PORTFOLIO_BUILD','PORTFOLIO_SIMULATE')
     OR (r.kind='FORWARD_EVALUATE' AND NEW.cpu=1 AND NEW.access_class='EVALUATOR_ONLY' AND p.origin=NEW.origin
       AND NEW.output_schemas='[{"name":"qz.forward_evaluation","version":"1"}]'::jsonb
       AND EXISTS(SELECT 1 FROM app.forward_evaluation_inputs f WHERE f.input_set_id=r.input_set_id
          AND f.project_id=r.project_id AND f.runtime_id=a.runtime_id AND f.parameters_artifact_id=p.id
          AND NEW.origin=app.forward_delivery_origin(f.handoff_id))))
   AND p.project_id=r.project_id AND p.kind='PARAMETERS'
   AND p.media_type='application/json' AND p.schema_name='qz.native_task' AND p.schema_version='1'
   AND p.storage_backend='LOCAL' AND p.storage_object_ref=p.id::text AND p.storage_version='1'
   AND p.byte_count BETWEEN 1 AND 8388608 AND p.access_class=NEW.access_class
   AND p.created_by IN ('OPERATOR','RUNTIME') AND p.producer_run_id IS NULL AND p.producer_attempt_id IS NULL
   AND c.schema_name='qz.runtime_probe' AND c.schema_version='1' AND c.kind='REPORT'
   AND EXISTS(SELECT 1 FROM app.runtime_probe_observations o WHERE o.snapshot_artifact_id=c.id
       AND o.runtime_id=a.runtime_id AND o.integration_revision=a.runtime_revision
       AND o.valid_until>clock_timestamp() AND o.outcome->'result'->>'status'='AVAILABLE')
 INTO valid FROM app.runs r JOIN app.run_admissions a ON a.run_id=r.id
 JOIN app.artifacts p ON p.id=NEW.parameters_artifact_id
 JOIN app.artifacts c ON c.id=NEW.capability_snapshot_artifact_id
 WHERE r.id=NEW.run_id;
 IF valid IS DISTINCT FROM true THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='native task definition must bind a new authorized Run';
 END IF;
 RETURN NEW;
END $$;
