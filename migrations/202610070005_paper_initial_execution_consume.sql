-- One irrevocable model-initialization consumption on the original receipt
-- mechanism. No new account ledger, renewable lease or recovery/reset command.
ALTER TABLE app.handoff_offers ADD COLUMN paper_claim_credential_id app.identity
 REFERENCES app.machine_credentials;

CREATE FUNCTION app.paper_claim_credential_valid(credential uuid, project uuid, downstream uuid, at_time timestamptz)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM app.machine_credentials c
  JOIN app.machine_principals p ON p.id=c.principal_id
  JOIN app.projects project_row ON project_row.id=p.project_id
  WHERE c.id=credential AND p.kind='DOWNSTREAM' AND p.project_id=project
   AND p.downstream_id=downstream AND p.enabled AND project_row.state='ACTIVE'
   AND c.principal_epoch=p.credential_epoch AND NOT c.lease_bound
   AND c.scope_codes @> ARRAY['DOWNSTREAM_CLAIM']::text[]
   AND c.issued_at<=at_time AND c.expires_at>at_time
   AND NOT EXISTS(SELECT 1 FROM app.machine_credential_revocations r
    WHERE r.credential_id=c.id AND r.effective_at<=at_time))
$$;
CREATE FUNCTION app.guard_paper_claim_credential() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='INSERT' THEN
  IF NEW.paper_claim_credential_id IS NOT NULL THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PAPER_CLAIM_CREDENTIAL_TRANSITION';
  END IF;
 ELSIF NEW.paper_claim_credential_id IS DISTINCT FROM OLD.paper_claim_credential_id THEN
  IF OLD.paper_claim_credential_id IS NOT NULL OR OLD.state<>'OFFERED' OR NEW.state<>'CLAIMED'
   OR NOT EXISTS(SELECT 1 FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id
    JOIN app.paper_initial_capital_sources s ON s.weights_artifact_id=r.paper_initial_weights_artifact_id
    WHERE r.id=NEW.release_id AND app.paper_release_root_valid(r.id)
     AND c.current_weights_source='PAPER_INITIAL_CAPITAL'
     AND c.current_weights_artifact_id=s.weights_artifact_id
     AND s.downstream_id=NEW.downstream_id AND NEW.environment='PAPER'
     AND app.paper_claim_credential_valid(NEW.paper_claim_credential_id,s.project_id,s.downstream_id,clock_timestamp())) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PAPER_CLAIM_CREDENTIAL_BINDING';
  END IF;
 END IF;
 -- Historical or old-writer NULL remains unknown, readable and never consumable.
 -- Other kinds of claim are not required to carry this new initialization field.
 RETURN NEW;
END $$;
CREATE TRIGGER paper_claim_credential BEFORE INSERT OR UPDATE ON app.handoff_offers
 FOR EACH ROW EXECUTE FUNCTION app.guard_paper_claim_credential();

CREATE UNIQUE INDEX paper_initial_execution_once ON app.command_receipts(resource_id)
 WHERE operation='PAPER_INITIAL_EXECUTION_CONSUME';

-- Recheck original finite authorizations without rerunning research or
-- treating the claimed initial condition as unused again.
CREATE FUNCTION app.paper_initial_execution_sources_current(original jsonb, at_time timestamptz)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT original IS NOT NULL
  AND (original->'package'->>'valid_from')::timestamptz<=at_time
  AND (original->'package'->>'valid_until')::timestamptz>at_time
  AND (original->'package'->'current_weights'->>'valid_until_ns')::numeric>extract(epoch FROM at_time)*1000000000
  AND jsonb_array_length(original->'package'->'qualification_refs')>0
  AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements_text(original->'package'->'qualification_refs') ref
   LEFT JOIN app.qualifications q ON q.id=ref.value::uuid
   WHERE q.id IS NULL OR q.granted_at>at_time OR q.valid_until<=at_time
    OR EXISTS(SELECT 1 FROM app.qualification_revocations v WHERE v.qualification_id=q.id AND v.effective_at<=at_time))
  AND jsonb_array_length(original->'package'->'input_revision_refs')>0
  AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements_text(original->'package'->'input_revision_refs') ref
   LEFT JOIN app.dataset_revisions d ON d.id=ref.value::uuid
   LEFT JOIN app.data_sources source ON source.id=d.source_id
   LEFT JOIN app.runtime_integrations runtime ON runtime.id=source.runtime_id
   LEFT JOIN app.data_use_grants g ON g.id=d.data_use_grant_id
   WHERE d.id IS NULL OR NOT source.enabled OR NOT runtime.enabled OR g.id IS NULL
    OR g.valid_from>at_time OR g.valid_until<=at_time
    OR g.allowed_uses NOT IN ('RESEARCH_AND_PAPER','RESEARCH_PAPER_LIVE')
    OR EXISTS(SELECT 1 FROM app.data_use_revocations v WHERE v.grant_id=g.id AND v.effective_at<=at_time))
$$;

CREATE FUNCTION app.paper_initial_execution_claim_valid(root uuid, handoff uuid, credential uuid, at_time timestamptz)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM app.paper_initial_capital_sources s
  JOIN app.releases r ON r.paper_initial_weights_artifact_id=s.weights_artifact_id
  JOIN app.portfolio_candidates c ON c.id=r.candidate_id
  JOIN app.handoff_offers h ON h.release_id=r.id
  JOIN app.handoff_transfers t ON t.handoff_id=h.id
  JOIN app.approvals a ON a.id=h.approval_id
  JOIN app.evaluations e ON e.id=r.evaluation_id
  JOIN app.downstream_integrations d ON d.id=h.downstream_id
  JOIN app.command_receipts claimed ON claimed.operation='HANDOFF_CLAIM'
   AND claimed.principal_scope='DOWNSTREAM:'||h.downstream_id::text
   AND claimed.resource_id=h.id AND claimed.idempotency_key=h.external_claim_id
  JOIN LATERAL (SELECT * FROM app.downstream_probe_observations p WHERE p.downstream_id=d.id
   ORDER BY p.started_at DESC,p.id DESC LIMIT 1) probe ON true
  WHERE s.weights_artifact_id=root AND h.id=handoff
   AND s.downstream_id=h.downstream_id AND h.environment='PAPER' AND h.state='CLAIMED'
   AND app.paper_release_root_valid(r.id) AND c.current_weights_source='PAPER_INITIAL_CAPITAL'
   AND c.current_weights_artifact_id=s.weights_artifact_id
   AND t.provenance='RECORDED_TRANSITION' AND t.external_claim_id=h.external_claim_id
   AND h.paper_claim_credential_id=credential
   AND app.paper_claim_credential_valid(credential,s.project_id,s.downstream_id,at_time)
   AND h.claimed_at>=s.created_at AND h.claimed_at<=at_time AND h.expires_at>at_time
   AND r.valid_from<=at_time AND r.valid_until>at_time
   AND a.granted_at<=at_time AND a.valid_until>at_time
   AND e.execution_status='SUCCEEDED' AND e.evidence_status='VALID' AND e.decision='PASS'
   AND e.valid_until>at_time AND app.approval_evidence_valid(r.id,a.evidence_set_id)
   AND d.enabled AND d.environments IN ('PAPER','BOTH') AND d.revision=a.downstream_revision
   AND probe.integration_revision=d.revision AND probe.valid_until>at_time AND probe.observed_at<=at_time
   AND probe.outcome->'result'->>'status'='AVAILABLE'
   AND probe.outcome->'result'->'capabilities'->>'accepting_targets'='true'
   AND probe.outcome->'result'->'capabilities'->'accepted_package_versions' @> '["2"]'::jsonb
   AND probe.outcome->'result'->'capabilities'->'environments' @> '["PAPER"]'::jsonb
   AND probe.outcome->'result'->'capabilities'->'market_capability_versions'
    @> (claimed.response_nonsecret_body->'resource'->'package'->'compatible_market_capabilities')
   AND app.paper_initial_execution_sources_current(claimed.response_nonsecret_body->'resource',at_time)
   AND '2'=ANY(d.accepted_package_versions) AND a.readiness_observation_id IS NOT NULL
   AND a.decision_ordinal=coalesce((SELECT decision.ordinal FROM app.release_decisions decision
    WHERE decision.candidate_id=c.id AND decision.downstream_id=d.id AND decision.environment='PAPER'
    ORDER BY decision.ordinal DESC LIMIT 1),0)
   AND coalesce((SELECT decision.decision='REOPEN' FROM app.release_decisions decision
    WHERE decision.candidate_id=c.id AND decision.downstream_id=d.id AND decision.environment='PAPER'
    ORDER BY decision.ordinal DESC LIMIT 1),true)
   AND NOT EXISTS(SELECT 1 FROM app.approval_revocations revoked
    WHERE revoked.approval_id=a.id AND revoked.effective_at<=at_time)
   AND (a.authority_kind='OPERATOR' OR (a.authority_kind='FROZEN_POLICY' AND EXISTS(
    SELECT 1 FROM app.automation_policies policy JOIN app.projects p ON p.id=policy.project_id
    WHERE policy.id=a.automation_policy_id AND p.id=s.project_id
     AND p.current_automation_policy_id=policy.id AND policy.mandate_id=c.mandate_id
     AND policy.downstream_id=s.downstream_id AND policy.enabled_for_new_rebalances
     AND policy.mode<>'MANUAL' AND policy.authorized_at<=at_time AND policy.valid_until>at_time
     AND NOT EXISTS(SELECT 1 FROM app.policy_revocations revoked
      WHERE revoked.automation_policy_id=policy.id AND revoked.effective_at<=at_time))))
   AND NOT EXISTS(SELECT 1 FROM app.forward_weight_snapshots w WHERE w.downstream_id=s.downstream_id)
   AND NOT EXISTS(SELECT 1 FROM app.native_account_sources n WHERE n.downstream_id=s.downstream_id
    AND n.native_trader_id=s.trader_id AND n.native_account_id=s.account_id)
   AND NOT EXISTS(SELECT 1 FROM app.handoff_offers other WHERE other.downstream_id=s.downstream_id
    AND other.id<>h.id AND other.claimed_at IS NOT NULL))
$$;

CREATE FUNCTION app.guard_paper_initial_execution_receipt() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE root app.paper_initial_capital_sources%ROWTYPE; handoff app.handoff_offers%ROWTYPE;
 request jsonb; response jsonb; original jsonb; credential uuid; accepted timestamptz;
BEGIN
 IF NEW.operation<>'PAPER_INITIAL_EXECUTION_CONSUME' THEN RETURN NEW; END IF;
 request:=NEW.normalized_nonsecret_request->'request';
 response:=NEW.response_nonsecret_body->'resource';
 SELECT * INTO root FROM app.paper_initial_capital_sources WHERE weights_artifact_id=NEW.resource_id;
 IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PAPER_INITIAL_EXECUTION_ROOT'; END IF;
 PERFORM id FROM app.projects WHERE id=root.project_id FOR UPDATE;
 credential:=(NEW.normalized_nonsecret_request->>'consuming_credential_id')::uuid;
 PERFORM p.id FROM app.machine_principals p JOIN app.machine_credentials c ON c.principal_id=p.id
  WHERE c.id=credential FOR UPDATE OF p;
 PERFORM id FROM app.machine_credentials WHERE id=credential FOR UPDATE;
 PERFORM c.id FROM app.portfolio_candidates c JOIN app.releases r ON r.candidate_id=c.id
  JOIN app.handoff_offers h ON h.release_id=r.id
  WHERE h.id=(NEW.normalized_nonsecret_request->>'handoff_id')::uuid FOR UPDATE OF c;
 PERFORM id FROM app.downstream_integrations WHERE id=root.downstream_id FOR UPDATE;
 PERFORM a.id FROM app.approvals a JOIN app.handoff_offers h ON h.approval_id=a.id
  WHERE h.id=(NEW.normalized_nonsecret_request->>'handoff_id')::uuid FOR UPDATE OF a;
 PERFORM p.id FROM app.automation_policies p JOIN app.approvals a ON a.automation_policy_id=p.id
  JOIN app.handoff_offers h ON h.approval_id=a.id
  WHERE h.id=(NEW.normalized_nonsecret_request->>'handoff_id')::uuid FOR UPDATE OF p;
 SELECT * INTO handoff FROM app.handoff_offers
  WHERE id=(NEW.normalized_nonsecret_request->>'handoff_id')::uuid FOR UPDATE;
 PERFORM weights_artifact_id FROM app.paper_initial_capital_sources WHERE weights_artifact_id=NEW.resource_id FOR UPDATE;
 SELECT response_nonsecret_body->'resource' INTO original FROM app.command_receipts
  WHERE operation='HANDOFF_CLAIM' AND principal_scope='DOWNSTREAM:'||root.downstream_id::text
   AND resource_id=handoff.id AND idempotency_key=handoff.external_claim_id
   AND response_nonsecret_body->>'replayed'='false';
 -- The original finite authorization rows serialize their revocation FKs.
 PERFORM q.id FROM app.qualifications q
  WHERE q.id IN (SELECT value::uuid FROM jsonb_array_elements_text(original->'package'->'qualification_refs'))
  ORDER BY q.id FOR UPDATE;
 PERFORM g.id FROM app.data_use_grants g JOIN app.dataset_revisions d ON d.data_use_grant_id=g.id
  WHERE d.id IN (SELECT value::uuid FROM jsonb_array_elements_text(original->'package'->'input_revision_refs'))
  ORDER BY g.id FOR UPDATE OF g;
 accepted:=clock_timestamp();
 IF original IS NULL OR NOT app.paper_initial_execution_claim_valid(NEW.resource_id,handoff.id,credential,accepted)
  OR (NEW.principal_scope='DOWNSTREAM:'||root.downstream_id::text) IS NOT TRUE
  OR (NEW.normalized_nonsecret_request->>'schema_version'='1') IS NOT TRUE
  OR (request->>'schema_version'='1') IS NOT TRUE
  OR (request->>'release_id'=handoff.release_id::text) IS NOT TRUE
  OR (request->>'external_claim_id'=handoff.external_claim_id) IS NOT TRUE
  OR (request->'paper_initialization'=jsonb_build_object('artifact_id',root.weights_artifact_id,
    'downstream_id',root.downstream_id,'trader_id',root.trader_id,'account_id',root.account_id)) IS NOT TRUE
  OR ((request->>'owner_instance_id')::uuid<>'00000000-0000-0000-0000-000000000000'::uuid) IS NOT TRUE
  OR (NEW.response_status=200 AND NEW.response_nonsecret_body->>'replayed'='false') IS NOT TRUE
  OR (NEW.response_nonsecret_body->>'schema_version'='1' AND response->>'schema_version'='1') IS NOT TRUE
  OR (response->>'state'='CONSUMED') IS NOT TRUE
  OR (response->'paper_initialization'=request->'paper_initialization') IS NOT TRUE
  OR (response->>'owner_instance_id'=request->>'owner_instance_id') IS NOT TRUE
  OR (response->>'consuming_credential_id'=credential::text) IS NOT TRUE
  OR (response->>'package_artifact_id'=(SELECT package_artifact_id::text FROM app.releases WHERE id=handoff.release_id)) IS NOT TRUE
  OR (response->'claim'=original) IS NOT TRUE
  OR ((response->>'consumed_at')::timestamptz>=handoff.claimed_at
   AND (response->>'consumed_at')::timestamptz<=accepted) IS NOT TRUE
  OR (original->'handoff'->>'id'=handoff.id::text AND original->'handoff'->>'state'='CLAIMED'
   AND original->'handoff'->>'external_claim_id'=handoff.external_claim_id
   AND original->'handoff'->>'release_id'=handoff.release_id::text) IS NOT TRUE
  OR (original->'package'->>'package_schema_version'='2'
   AND original->'package'->>'source_kind'='FORECAST_EVALUATION'
   AND original->'package'->'current_weights'->'paper_initialization'=request->'paper_initialization'
   AND original->'package'->>'release_id'=handoff.release_id::text
   AND original->'package'->>'project_id'=root.project_id::text
   AND original->'package'->>'environment_origin'='SYNTHETIC'
   AND original->'package'->'source'->>'build_environment'='PAPER'
   AND original->'package'->'source'->>'current_weights_artifact_id'=root.weights_artifact_id::text
   AND (original->'package'->>'capital_assumption')::numeric=root.starting_capital
   AND original->'package'->>'base_currency'=root.base_currency
   AND original->'package'->>'cost_assumption_ref'=root.execution_assumptions_id::text
   AND original->'package'->'current_weights'->'source'->>'kind'='PAPER_INITIAL_CAPITAL') IS NOT TRUE
 THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PAPER_INITIAL_EXECUTION_BINDING';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER paper_initial_execution_receipt BEFORE INSERT ON app.command_receipts
 FOR EACH ROW EXECUTE FUNCTION app.guard_paper_initial_execution_receipt();
