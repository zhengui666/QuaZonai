import { describe, expect, it } from 'vitest';
import { ApiFailure } from './api';
import type { Schema } from './api';
import { createFrozenRequest, datasetIssue, requiresReload, runtimeValidationIssue, unknownOutcome, utcMicros,
  validationArtifacts, validationInputIssue, validationRequest } from './data-input-options';
import type { DatasetSelection } from './data-input-options';

const source: Schema['DataSourceView'] = { id: 'source', runtime_id: 'runtime', revision: '9007199254740993',
  name: 'Registered source', enabled: true, provider_kind: 'NAUTILUS_CATALOG', native_catalog_ref: 'catalog', created_at: '', updated_at: '' };
const dataset: Schema['DatasetView'] = { id: 'dataset', source_id: 'source', partition: 'DISCOVERY',
  native_metadata_artifact_id: 'global-metadata', native_snapshot_ref: 'original-snapshot', storage_version: '9007199254740993',
  origin: 'FIXTURE', pit_status: 'UNVERIFIED', license_state: 'ACTIVE', checked_at: '2026-01-01T00:00:00Z',
  source_enabled: true, runtime_enabled: true, available_through: '2026-01-01T00:00:00.000001Z',
  data_kind: 'BAR', data_use_grant_id: 'grant', event_start: '2025-01-01T00:00:00Z', event_end: '2025-12-31T00:00:00Z',
  quality_artifact_id: 'global-quality', revision_policy: 'UNKNOWN', row_count: '9007199254740993', schema_version: '1',
  timezone: 'UTC', universe_version_id: 'universe', created_at: '2026-01-01T00:00:00Z' };
const selection: DatasetSelection = { dataset, source };
const runtime: Schema['RuntimeView'] = { id: 'runtime', revision: '9007199254740993', protocol_version: 1,
  configuration: { name: 'Runtime', endpoint: 'https://runtime.invalid', allowed_capabilities: ['DATA_VALIDATE'],
    enabled: true, development_http: false, tls_policy: 'SYSTEM_CA' }, ca_configured: false, credential_configured: true,
  created_at: '', updated_at: '' };
const input: Schema['InputSetView'] = { header: { id: 'input', project_id: 'project', purpose: 'DISCOVERY', revision: '1',
  decision_cutoff: dataset.available_through, frozen_at: '2026-01-02T00:00:00Z', created_at: '2026-01-02T00:00:00Z' },
  items: [{ id: 'item', ordinal: 0, item: { kind: 'DATASET', dataset_revision_id: dataset.id, role: 'DISCOVERY' }, origin: 'FIXTURE', pit_status: 'UNVERIFIED' }] };
const limits = { cpu_seconds: '9007199254740993', wall_seconds: 60, memory_mib: 512, output_bytes: '67108864' };

describe('exact frozen input selection', () => {
  it('retains microseconds and rejects rounded, normalized, invalid or imprecise time inputs', () => {
    expect(utcMicros('2026-01-01T00:00:00.000001Z')! - utcMicros('2026-01-01T00:00:00Z')!).toBe(1n);
    expect(utcMicros('2026-01-01T00:00:00.1Z')! - utcMicros('2026-01-01T00:00:00Z')!).toBe(100000n);
    for (const value of ['2026-02-30T00:00:00Z', '2026-01-01', '2026-01-01T00:00:00.0000001Z',
      '2026-01-01T00:00:00+00:00', ' 2026-01-01T00:00:00Z', '2026-01-01T24:00:00Z']) expect(utcMicros(value)).toBeUndefined();
    expect(() => createFrozenRequest('project', 'DISCOVERY', '2026-01-01T00:00:00Z', 'runtime', [selection])).toThrow('available_through');
  });
  it('freezes only explicit IDs, dataset roles and exact cutoff without promoting FIXTURE/UNVERIFIED', () => {
    expect(datasetIssue(selection, 'DISCOVERY', 'runtime')).toBeUndefined();
    const request = createFrozenRequest('project', 'DISCOVERY', dataset.available_through, 'runtime', [selection]);
    expect(request).toEqual({ schema_version: 1, project_id: 'project', purpose: 'DISCOVERY', decision_cutoff: dataset.available_through,
      items: [{ kind: 'DATASET', dataset_revision_id: 'dataset', role: 'DISCOVERY' }] });
    expect(dataset.origin).toBe('FIXTURE'); expect(dataset.pit_status).toBe('UNVERIFIED');
  });
  it('reserves one native task input and rejects duplicates, changed purpose and cross-Runtime sources', () => {
    const selections = Array.from({ length: 255 }, (_, index) => ({ source, dataset: { ...dataset, id: `dataset-${index}` } }));
    expect(createFrozenRequest('project', 'DISCOVERY', dataset.available_through, 'runtime', selections).items).toHaveLength(255);
    expect(() => createFrozenRequest('project', 'DISCOVERY', dataset.available_through, 'runtime', [...selections, selection])).toThrow('1–255');
    expect(() => createFrozenRequest('project', 'DISCOVERY', dataset.available_through, 'runtime', [selection, selection])).toThrow('重复');
    expect(() => createFrozenRequest('project', 'VALIDATION', dataset.available_through, 'runtime', [selection])).toThrow('分区');
    expect(() => createFrozenRequest('project', 'DISCOVERY', dataset.available_through, 'other-runtime', [selection])).toThrow('Runtime');
    expect(() => createFrozenRequest('project', 'DISCOVERY', dataset.available_through, 'runtime', [])).toThrow('1–255');
  });
  it('blocks missing registration evidence, stale licensing and disabled consumers', () => {
    for (const change of [{ native_metadata_artifact_id: null }, { license_state: 'EXPIRED' as const }, { license_state: 'REVOKED' as const },
      { source_enabled: false }, { runtime_enabled: false }]) expect(datasetIssue({ source, dataset: { ...dataset, ...change } }, 'DISCOVERY', 'runtime')).toBeTruthy();
    expect(datasetIssue({ dataset, source: { ...source, id: 'wrong-source' } }, 'DISCOVERY', 'runtime')).toBeTruthy();
  });
});

describe('standalone DATA_VALIDATE request', () => {
  it('uses exact revision/counter strings and does not mutate original bindings or input', () => {
    const request = validationRequest(input, 'project', runtime, [selection], limits);
    expect(request.expected_runtime_revision).toBe('9007199254740993');
    expect(request.limits.cpu_seconds).toBe('9007199254740993'); expect(request.limits.experiments).toBe(0);
    expect(request.input_set_id).toBe('input'); expect(input.items[0]?.origin).toBe('FIXTURE');
  });
  it('never admits SEALED, mixed/artifact input, wrong project, missing binding, or cross-Runtime data', () => {
    for (const purpose of ['SEALED', 'FORWARD', 'PORTFOLIO'] as const) expect(validationInputIssue({ ...input, header: { ...input.header, purpose } }, 'project')).toBeTruthy();
    expect(validationInputIssue(input, 'other-project')).toBeTruthy();
    expect(validationInputIssue({ ...input, items: [{ id: 'artifact', ordinal: 0, item: { kind: 'ARTIFACT', artifact_id: 'a', role: 'REPORT' }, origin: 'FIXTURE' }] }, 'project')).toBeTruthy();
    expect(() => validationRequest(input, 'project', runtime, [], limits)).toThrow('绑定');
    expect(() => validationRequest(input, 'project', { ...runtime, id: 'wrong' }, [selection], limits)).toThrow('Runtime');
  });
  it('checks native general bounds without unsafe number coercion', () => {
    for (const change of [{ cpu_seconds: '0' }, { cpu_seconds: '9223372036854775808' }, { cpu_seconds: '1e3' },
      { output_bytes: '67108865' }, { output_bytes: '0' }, { wall_seconds: 86401 }, { wall_seconds: 1.5 }, { memory_mib: 1048577 }]) {
      expect(() => validationRequest(input, 'project', runtime, [selection], { ...limits, ...change })).toThrow('限额');
    }
  });
  it('requires current matching Runtime observation and does not choose a different revision', () => {
    const readiness: Schema['RuntimeReadinessV1'] = { schema_version: 1, runtime_id: runtime.id,
      integration_revision: runtime.revision, state: 'AVAILABLE', available_job_kinds: ['DATA_VALIDATE'],
      latest_observation: { id: 'probe', runtime_id: runtime.id, integration_revision: runtime.revision,
        observed_at: '2026-01-01T00:00:00Z', valid_until: '2026-01-01T01:00:00Z', snapshot_artifact_id: 'probe-artifact',
        outcome: { status: 'UNAVAILABLE', reason: 'UNAVAILABLE' } } };
    expect(runtimeValidationIssue(runtime, readiness, Date.parse('2026-01-01T00:30:00Z'))).toContain('尚未具备');
    expect(runtimeValidationIssue(runtime, { ...readiness, integration_revision: '9007199254740992' }, 0)).toContain('版本');
    const available = { ...readiness, latest_observation: { ...readiness.latest_observation!, outcome: { status: 'AVAILABLE', capabilities: {} } } } as Schema['RuntimeReadinessV1'];
    expect(runtimeValidationIssue(runtime, available, Date.parse('2026-01-01T00:30:00Z'))).toBeUndefined();
    expect(runtimeValidationIssue(runtime, available, Date.parse('2026-01-01T01:00:00Z'))).toContain('过期');
  });
});

it('distinguishes unknown acknowledgement from confirmed stale-revision rejection', () => {
  for (const failure of [new Error('network'), new ApiFailure('NETWORK_UNKNOWN', 'unknown'), new ApiFailure('HTTP_CONTRACT_ERROR', 'bad ack', 200), new ApiFailure('UNAVAILABLE', 'server', 503)]) expect(unknownOutcome(failure)).toBe(true);
  const conflict = new ApiFailure('REVISION_CONFLICT', 'reload', 409);
  expect(unknownOutcome(conflict)).toBe(false); expect(requiresReload(conflict)).toBe(true);
});

it('keeps producer run and attempt identity, excludes global/other-run/Sealed artifacts', () => {
  const artifact: Schema['ArtifactView'] = { id: 'a', project_id: 'project', producer_run_id: 'run', producer_attempt_id: 'attempt-1',
    kind: 'DATA_QUALITY', created_by: 'RUNTIME', access_class: 'RESEARCH', byte_count: '123', origin: 'FIXTURE',
    schema_name: 'qz.data_quality', schema_version: '1', media_type: 'application/json', created_at: '' };
  const second = { ...artifact, id: 'second', producer_attempt_id: 'attempt-2' };
  const items = [artifact, second, { ...artifact, project_id: 'other' }, { ...artifact, producer_run_id: 'old-run' },
    { ...artifact, producer_attempt_id: null }, { ...artifact, created_by: 'OPERATOR' as const }, { ...artifact, access_class: 'EVALUATOR_ONLY' as const }];
  expect(validationArtifacts(items, 'project', 'run', 'attempt-1')).toEqual([artifact]);
  expect(validationArtifacts(items, 'project', 'run', 'attempt-2')).toEqual([second]);
  expect(validationArtifacts(items, 'project', 'run', null)).toEqual([]);
});
