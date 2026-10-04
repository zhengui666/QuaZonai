import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import type { Schema } from '../src/api';
import { fixture, rememberPrivateValue } from './native-auth-support';

const config = fixture();
const root = dirname(config.redactionsFile);
const checkpoint: { receipt: { resource: Schema['ProjectView'] } } = JSON.parse(readFileSync(resolve(root, 'restart-project.json'), 'utf8'));
const peer: { credential: string; ca_pem?: string; endpoint: string; mode?: 'native-execution';
  metadata: { registered_ref: string; storage_version: string; available_through: string; origin: string; pit_status: string };
} = JSON.parse(readFileSync(resolve(root, 'native-data-peer.json'), 'utf8'));
// This is the original browser session, retained by the real API restart phase.
test.use({ storageState: resolve(root, 'restart-browser.json') });

test('native API commands freeze and admit once while the data observation page stays read-only', async ({ page, context }) => {
  expect(config.phase).toBe('data-admission');
  expect(peer.metadata.origin).toBe('FIXTURE');
  expect(peer.metadata.pit_status).toBe('UNVERIFIED');
  rememberPrivateValue(config, peer.credential);
  for (const cookie of await context.cookies()) rememberPrivateValue(config, cookie.value);
  const project = checkpoint.receipt.resource;
  async function post<T>(path: string, body: unknown, expectedStatus: number) {
    const response = await page.request.post(path, { headers: { Origin: config.baseUrl, 'Idempotency-Key': randomUUID() }, data: body });
    expect(response.status(), path).toBe(expectedStatus);
    return await response.json() as { schema_version: 1; replayed: boolean; resource: T };
  }
  // All objects use native authenticated HTTP. Ordinary admission retains its
  // controlled TLS peer; explicit execution uses the actual loopback Runtime.
  const credential = await post<Schema['CredentialView']>('/api/v2/settings/credentials', {
    intent: { schema_version: 1, purpose: 'RUNTIME', label: 'Browser admission fixture' }, value: peer.credential,
  }, 201);
  const ca = peer.mode === 'native-execution' ? undefined : await post<Schema['CredentialView']>('/api/v2/settings/credentials', {
    intent: { schema_version: 1, purpose: 'TLS_CA', label: 'Browser admission test CA' }, value: peer.ca_pem,
  }, 201);
  const runtimeName = 'Native browser admission Runtime';
  const runtime = await post<Schema['RuntimeView']>('/api/v2/integrations/runtimes', {
    schema_version: 1, configuration: { name: runtimeName, endpoint: peer.endpoint, tls_policy: peer.mode === 'native-execution' ? 'SYSTEM_CA' : 'PINNED_CA',
      allowed_capabilities: ['DATA_VALIDATE'], enabled: true, development_http: peer.mode === 'native-execution' },
    credential_ref: credential.resource.id, ca_certificate_ref: ca?.resource.id ?? null,
  }, 201);
  const proof = await post<Schema['ArtifactView']>('/api/v2/artifacts', {
    schema_version: 1, project_id: project.id, kind: 'REPORT',
    content: JSON.stringify({ schema_version: 1, license: 'Controlled browser fixture only; not market-data permission' }),
  }, 201);
  const source = await post<Schema['DataSourceView']>('/api/v2/data/sources', {
    schema_version: 1, name: 'Native browser controlled catalog', runtime_id: runtime.resource.id,
    native_catalog_ref: peer.metadata.registered_ref, provider_kind: 'NAUTILUS_CATALOG', enabled: true,
  }, 201);
  const grant = await post<Schema['DataGrantView']>(`/api/v2/data/sources/${source.resource.id}/grants`, {
    schema_version: 1, source_id: source.resource.id, license_reference: 'Synthetic admission fixture terms',
    evidence_artifact_id: proof.resource.id, allowed_uses: 'RESEARCH', valid_from: '2000-01-01T00:00:00Z', valid_until: null,
  }, 201);
  const dataset = await post<Schema['DatasetView']>('/api/v2/data/revisions', {
    schema_version: 1, source_id: source.resource.id, grant_id: grant.resource.id,
    expected_source_revision: source.resource.revision, expected_runtime_revision: runtime.resource.revision,
    native_storage_version: peer.metadata.storage_version, existing_universe_version_id: null,
  }, 200);
  expect(dataset.resource.origin).toBe('FIXTURE');
  expect(dataset.resource.pit_status).toBe('UNVERIFIED');
  const probe = await post<Schema['RuntimeProbeViewV1']>(`/api/v2/integrations/runtimes/${runtime.resource.id}/probe`, {
    schema_version: 1, expected_revision: runtime.resource.revision,
  }, 200);
  expect(probe.resource.outcome.status).toBe('AVAILABLE');

  // Business commands remain real authenticated API operations, owned by the
  // external caller. Replay the same body/key and retain the original receipts
  // for the Worker/restart phase; the observation page never submits them.
  const creation = { key: randomUUID(), body: {
    schema_version: 1, project_id: project.id, purpose: 'DISCOVERY', decision_cutoff: peer.metadata.available_through,
    items: [{ kind: 'DATASET', dataset_revision_id: dataset.resource.id, role: 'DISCOVERY' }],
  } satisfies Schema['InputSetCreate'] };
  async function command<T>(path: string, intent: { key: string; body: unknown }, status: number) {
    const response = await page.request.post(path, { headers: { Origin: config.baseUrl, 'Idempotency-Key': intent.key }, data: intent.body });
    expect(response.status(), path).toBe(status);
    return await response.json() as T;
  }
  const frozen = await command<Schema['CommandResult_InputSetView']>('/api/v2/input-sets', creation, 201);
  expect(frozen.replayed).toBe(false);
  expect(await command('/api/v2/input-sets', creation, 201)).toEqual({ ...frozen, replayed: true });
  expect(frozen.resource.header).toMatchObject({ project_id: project.id, purpose: 'DISCOVERY', decision_cutoff: peer.metadata.available_through });
  expect(frozen.resource.header.frozen_at).toBeTruthy();
  expect(frozen.resource.items).toHaveLength(1);
  expect(frozen.resource.items[0]!.item).toEqual(creation.body.items[0]);
  const inputId = frozen.resource.header.id;
  const validation = { key: randomUUID(), body: {
    schema_version: 1, project_id: project.id, input_set_id: inputId,
    runtime_id: runtime.resource.id, expected_runtime_revision: runtime.resource.revision,
    limits: { schema_version: 1, experiments: 0, cpu_seconds: peer.mode === 'native-execution' ? '60' : '10',
      output_bytes: '65536', memory_mib: peer.mode === 'native-execution' ? 1024 : 512,
      wall_seconds: peer.mode === 'native-execution' ? 120 : 60 },
  } satisfies Schema['DataValidateRequest'] };
  const admitted = await command<Schema['CommandResult_RunSnapshotV1']>('/api/v2/data/validate', validation, 202);
  expect(admitted.replayed).toBe(false);
  expect(await command('/api/v2/data/validate', validation, 202)).toEqual({ ...admitted, replayed: true });
  expect(admitted.resource).toMatchObject({ kind: 'DATA_VALIDATE', state: 'QUEUED', input_set_id: inputId, project_id: project.id });
  if (peer.mode === 'native-execution') writeFileSync(resolve(root, 'native-data-admission.json'), JSON.stringify({
    project, runtime: runtime.resource, dataset: dataset.resource, frozen, admitted, creation, validation,
  }), { mode: 0o600 });

  const writes: string[] = [];
  page.on('request', request => {
    if (new URL(request.url()).pathname.startsWith('/api/') && !['GET', 'HEAD'].includes(request.method())) {
      writes.push(`${request.method()} ${new URL(request.url()).pathname}`);
    }
  });
  async function openInputs() {
    await page.goto('/');
    await page.getByRole('button', { name: project.name, exact: true }).click();
    await page.getByRole('tab', { name: '冻结输入', exact: true }).click();
    await page.getByRole('button', { name: inputId, exact: true }).click();
    await expect(page.getByText(/未构成合格真实 PIT 证据/)).toBeVisible();
    await expect(page.getByRole('button', { name: /新建冻结输入|请求数据质量验证/ })).toHaveCount(0);
    await expect(page.getByRole('dialog')).toHaveCount(0);
  }
  await openInputs();
  const runRow = page.getByRole('row').filter({ hasText: admitted.resource.id });
  await runRow.getByRole('button', { name: '查看产物', exact: true }).click();
  // Explicit refresh while pending is a read, never a second validation command.
  await page.getByRole('button', { name: '刷新所选运行产物', exact: true }).click();
  await expect(page.getByText('当前运行尚未成功；这里的产物不作为已通过的数据质量结论。', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '打开所选运行详情', exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText(admitted.resource.id);
  await expect(page.getByRole('button', { name: /请求取消|确认取消运行/ })).toHaveCount(0);
  // A fresh page recovers the same immutable input from the service, not a
  // browser-persisted draft. No completed validation or qualification is claimed.
  await openInputs();
  const original = await page.request.get(`/api/v2/input-sets/${inputId}`);
  expect(original.status()).toBe(200);
  expect(await original.json()).toEqual(frozen.resource);
  const registered = await page.request.get(`/api/v2/data/revisions/${dataset.resource.id}`);
  expect(registered.status()).toBe(200);
  const unchanged: Schema['DatasetView'] = await registered.json();
  expect(unchanged.origin).toBe('FIXTURE');
  expect(unchanged.pit_status).toBe('UNVERIFIED');
  expect(writes).toEqual([]);
});
