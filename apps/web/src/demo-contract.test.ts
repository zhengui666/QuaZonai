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
    expect(dataset.origin).toBe('FIXTURE');
    previousEnd = dataset.event_end;
  }
});
