import { expect, test } from 'vitest';
import { demoPackage, demoResponse, id, packageBytes, records } from '../demo/records';
import { validateProblem, validateResponse } from './generated/responses.cjs';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';
import document from '../../../contracts/generated/api-v2.openapi.json';

test('synthetic preview preserves native response contracts and denies every write', () => {
  for (const [path, { contract, value }] of records) {
    expect(validateResponse(contract, 'get', 200, value, 'application/json'), contract).toBe(true);
    expect(demoResponse('GET', path)).toEqual({ status: 200, value });
    for (const method of ['POST', 'PATCH', 'PUT', 'DELETE']) {
      const denied = demoResponse(method, path);
      expect(denied.status).toBe(403);
      expect(validateProblem(denied.value)).toBe(true);
    }
  }
  expect(demoResponse('POST', '/api/v2/handoffs/arbitrary/claim').status).toBe(403);
  expect(demoResponse('GET', '/api/v2/unknown').status).toBe(404);
  expect(demoResponse('GET', '/api/v2/data/revisions', 'SEALED')).toMatchObject({ status: 200, value: {
    items: [{ partition: 'SEALED', origin: 'FIXTURE', row_count: '200', license_state: 'EXPIRED', source_enabled: false, runtime_enabled: false }],
  } });
  expect(demoResponse('GET', '/api/v2/portfolio-candidates/01990000-0000-7000-8000-000000000203')).toMatchObject({
    status: 200, value: { header: { origin: 'FIXTURE', execution_status: 'FAILED', target_artifact_id: null, cash_weight: null }, members: [], targets: [] },
  });
});

test('expired DEMO package uses the native package contract and original attachment bytes', () => {
  const ajv = new Ajv2020({ strict: false, inlineRefs: false });
  addFormats(ajv); ajv.addSchema(document, 'native');
  expect(ajv.validate({ $ref: 'native#/components/schemas/TargetPackageV1' }, demoPackage)).toBe(true);
  const metadata = records.get(`/api/v2/artifacts/${id(511)}`)!.value as { byte_count: string; kind: string; origin: string };
  expect(metadata).toMatchObject({ kind: 'PACKAGE', origin: 'FIXTURE', byte_count: String(Buffer.byteLength(packageBytes)) });
  expect(demoResponse('GET', `/api/v2/artifacts/${id(511)}/content`)).toEqual({ status: 200, value: packageBytes, binary: true });
  expect(JSON.parse(packageBytes)).toEqual(demoPackage);
  expect(demoPackage.environment_origin).toBe('DEMO');
  for (const path of [`/api/v2/releases/${id(510)}/approvals`, '/api/v2/handoffs', `/api/v2/handoffs/${id(510)}/claim`]) {
    expect(demoResponse('POST', path).status).toBe(403);
  }
});


test('frozen synthetic context resolves exact project, partition and Runtime references', () => {
  const frozen = records.get(`/api/v2/briefs/${id(10)}/execution-context`)!.value as import('./api').Schema['FrozenBriefV1'];
  expect(frozen.brief).toEqual(records.get(`/api/v2/briefs/${id(10)}`)!.value);
  const context = frozen.execution_context;
  const runtime = records.get(`/api/v2/integrations/runtimes/${context.runtime_id}`)!.value as import('./api').Schema['RuntimeView'];
  expect(BigInt(context.runtime_revision)).toBeLessThan(BigInt(runtime.revision));
  expect(runtime.configuration.enabled).toBe(false);
  const readiness = records.get(`/api/v2/integrations/runtimes/${runtime.id}/readiness`)!.value as import('./api').Schema['RuntimeReadinessV1'];
  expect(readiness.state).toBe('DISABLED');
  expect(readiness.available_job_kinds).toEqual([]);
  const probe = readiness.latest_observation!;
  expect(probe.integration_revision).toBe(context.runtime_revision);
  expect(probe.observed_at <= frozen.brief.frozen_at!).toBe(true);
  expect(probe.valid_until > frozen.brief.frozen_at!).toBe(true);
  expect(Date.parse(probe.valid_until) - Date.parse(probe.observed_at)).toBe(60_000);
  expect(probe.outcome.status).toBe('AVAILABLE');
  if (probe.outcome.status !== 'AVAILABLE') throw new Error('missing historical probe');
  expect(probe.outcome.capabilities.job_kinds).toEqual(expect.arrayContaining(['DATA_VALIDATE', 'ALPHA_EVALUATE']));
  const assumptions = records.get(`/api/v2/execution-assumptions/${frozen.brief.content.execution_assumptions_id}`)!.value as import('./api').Schema['ExecutionAssumptionsViewV1'];
  expect(assumptions.capability_snapshot_artifact_id).toBe(probe.snapshot_artifact_id);
  expect(assumptions.engine_image_ref).toBe(probe.outcome.capabilities.image_refs.find(image => image.job_kind === 'ALPHA_EVALUATE')!.image_ref);
  const capabilities = probe.outcome.capabilities;
  expect(capabilities.job_kinds).toEqual(expect.arrayContaining(['PORTFOLIO_SIMULATE', 'PORTFOLIO_BUILD']));
  expect(capabilities.engine_versions).toMatchObject({ nautilus: '0.63.0', 'simulation-models': '1', 'candidate-simulation': '2', 'portfolio-models': '4', 'portfolio-weights': '1', 'portfolio-cost-source': '1', clarabel: '0.11.1', ndarray: '0.17.1' });
  expect(capabilities.image_refs.find(image => image.job_kind === 'PORTFOLIO_SIMULATE')!.image_ref).toBe(assumptions.engine_image_ref);
  for (const fee of assumptions.settings.fee_rates) expect(capabilities.venues.some(venue => fee.instrument_id.endsWith(`.${venue.venue}`))).toBe(true);
  expect(capabilities.artifact_schemas.map(item => item.name)).toEqual(expect.arrayContaining(['qz.wasm_model', 'qz.model_compilation', 'qz.native_forecast', 'qz.native_portfolio']));
  const attempts = new Set<string>();
  for (const { contract, value } of records.values()) {
    if (contract === '/api/v2/alphas/{id}/versions/{version}') {
      const version = value as import('./api').Schema['AlphaVersionView'];
      expect(version.runtime_image_ref).toBe(assumptions.engine_image_ref);
      expect(version.created_at >= frozen.brief.frozen_at! && version.created_at < probe.valid_until).toBe(true);
    }
    if (contract === '/api/v2/runs/{id}') {
      const run = value as import('./api').Schema['RunSnapshotV1'];
      expect(run.started_at! >= probe.observed_at && run.finished_at! < probe.valid_until).toBe(true);
      expect(run.deadline_at > run.queued_at! && run.deadline_at > run.finished_at! && run.deadline_at < probe.valid_until).toBe(true);
      expect(run.current_attempt_no).toBe(1);
      expect(run.active_attempt_id).toBeTruthy();
      expect(attempts.has(run.active_attempt_id!)).toBe(false);
      attempts.add(run.active_attempt_id!);
      if (['ALPHA_EVALUATE', 'PORTFOLIO_BUILD', 'PORTFOLIO_SIMULATE'].includes(run.kind)) {
        expect(capabilities.image_refs.find(image => image.job_kind === run.kind)?.image_ref).toBe(assumptions.engine_image_ref);
      }
    }
  }
  for (const purpose of ['DISCOVERY', 'VALIDATION', 'SEALED'] as const) {
    const field = `${purpose.toLowerCase()}_input_set_id` as 'discovery_input_set_id' | 'validation_input_set_id' | 'sealed_input_set_id';
    const input = records.get(`/api/v2/input-sets/${context[field]}`)!.value as import('./api').Schema['InputSetView'];
    expect(input.header).toMatchObject({ project_id: frozen.brief.project_id, purpose });
    expect(input.items[0]).toMatchObject({ origin: 'FIXTURE', pit_status: 'UNVERIFIED', item: { kind: 'DATASET', role: purpose,
      dataset_revision_id: frozen.brief.bindings.find(binding => binding.role === purpose)!.dataset_revision_id } });
  }
  const policy = records.get(`/api/v2/evaluation-policies/${frozen.brief.content.evaluation_policy_id}`)!.value as import('./api').Schema['EvaluationPolicyView'];
  expect(policy.selection_rule.comparison_input_set_id).toBe(context.validation_input_set_id);
  expect(policy.require_real_data).toBe(false);
  expect(policy.metric_requirements.every(item => item.scope === 'asset:0/fold:0')).toBe(true);
  expect(policy.sealed_metric_requirements!.every(item => item.scope === 'asset:0')).toBe(true);
  const universe = records.get(`/api/v2/data/universes/${frozen.brief.content.universe_version_id}`)!.value as import('./api').Schema['UniverseView'];
  expect(policy.split_policy.label_horizon_observations).toBe(frozen.brief.content.horizon_value);
  expect(BigInt(policy.split_policy.purge_observations)).toBeGreaterThanOrEqual(BigInt(frozen.brief.content.horizon_value!));
  expect(policy.split_policy.sealed_revision_id).toBe(frozen.brief.bindings.find(binding => binding.role === 'SEALED')!.dataset_revision_id);
  let previousEnd = '';
  for (const role of ['DISCOVERY', 'VALIDATION', 'SEALED']) {
    const binding = frozen.brief.bindings.find(binding => binding.role === role)!;
    const dataset = records.get(`/api/v2/data/revisions/${binding.dataset_revision_id}`)!.value as import('./api').Schema['DatasetView'];
    expect(BigInt(dataset.row_count)).toBeGreaterThanOrEqual(BigInt(policy.minimum_observations));
    expect(dataset.event_start >= previousEnd).toBe(true);
    expect(dataset.event_end > dataset.event_start).toBe(true);
    expect(dataset.available_through <= frozen.brief.frozen_at!).toBe(true);
    expect(universe.selection_asof <= dataset.available_through).toBe(true);
    expect(dataset.origin).toBe('FIXTURE');
    previousEnd = dataset.event_end;
  }
});
