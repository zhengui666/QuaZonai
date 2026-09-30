import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import type { Schema } from '../src/api';
import { fixture, rememberPrivateValue } from './native-auth-support';

const config = fixture();
const root = dirname(config.redactionsFile);
const checkpoint: { receipt: { resource: Schema['ProjectView'] } } = JSON.parse(readFileSync(resolve(root, 'restart-project.json'), 'utf8'));
const peer: { credential: string; ca_pem: string; endpoint: string;
  metadata: { registered_ref: string; storage_version: string; available_through: string; origin: string; pit_status: string };
} = JSON.parse(readFileSync(resolve(root, 'native-data-peer.json'), 'utf8'));
// This is the original browser session, retained by the real API restart phase.
test.use({ storageState: resolve(root, 'restart-browser.json') });

test('registered fixture data freezes once and admits one original validation Run through the real API', async ({ page, context }) => {
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
  // All initial domain objects use native authenticated HTTP. The TLS peer is
  // deliberately synthetic transport evidence, never a production OCI Runtime.
  const credential = await post<Schema['CredentialView']>('/api/v2/settings/credentials', {
    intent: { schema_version: 1, purpose: 'RUNTIME', label: 'Browser admission fixture' }, value: peer.credential,
  }, 201);
  const ca = await post<Schema['CredentialView']>('/api/v2/settings/credentials', {
    intent: { schema_version: 1, purpose: 'TLS_CA', label: 'Browser admission test CA' }, value: peer.ca_pem,
  }, 201);
  const runtimeName = 'Native browser admission Runtime';
  const runtime = await post<Schema['RuntimeView']>('/api/v2/integrations/runtimes', {
    schema_version: 1, configuration: { name: runtimeName, endpoint: peer.endpoint, tls_policy: 'PINNED_CA',
      allowed_capabilities: ['DATA_VALIDATE'], enabled: true, development_http: false },
    credential_ref: credential.resource.id, ca_certificate_ref: ca.resource.id,
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

  async function openInputs() {
    await page.goto('/');
    await page.getByRole('menuitem', { name: '设置', exact: true }).click();
    await page.getByRole('tab', { name: '数据', exact: true }).click();
    await page.getByRole('tab', { name: '冻结输入', exact: true }).click();
    await page.getByRole('combobox', { name: '冻结输入所属研究项目', exact: true }).click();
    await page.getByTitle(`${project.name} · ${project.id}`, { exact: true }).click();
  }
  await openInputs();
  const creation: { key: string | undefined; body: Schema['InputSetCreate'] }[] = [];
  let frozen: Schema['CommandResult_InputSetView'] | undefined;
  await page.route('**/api/v2/input-sets', async route => {
    if (route.request().method() !== 'POST') return route.continue();
    creation.push({ key: route.request().headers()['idempotency-key'], body: route.request().postDataJSON() });
    const response = await route.fetch();
    expect(response.status()).toBe(201);
    const receipt = await response.json() as Schema['CommandResult_InputSetView'];
    if (creation.length === 1) { frozen = receipt; return route.abort('failed'); }
    expect(receipt).toEqual({ ...frozen, replayed: true });
    return route.fulfill({ response });
  });
  await page.getByRole('button', { name: '新建冻结输入', exact: true }).click();
  const editor = page.getByRole('dialog', { name: '创建并冻结项目输入', exact: true });
  await editor.getByRole('combobox', { name: '选择冻结输入的 Runtime', exact: true }).click();
  await page.getByTitle(`${runtimeName} · ${runtime.resource.id}`, { exact: true }).click();
  const row = editor.getByRole('row').filter({ hasText: dataset.resource.id });
  await expect(row.getByRole('checkbox')).toBeEnabled();
  await row.getByRole('checkbox').check();
  await editor.getByLabel('决策截止（精确 UTC）', { exact: true }).fill(peer.metadata.available_through);
  await expect(editor.getByText(/未构成合格真实 PIT 证据/)).toBeVisible();
  await editor.getByRole('button', { name: '确认创建并冻结', exact: true }).click();
  await expect(editor.getByText(/提交结果未知：原请求与幂等键已保留/)).toBeVisible();
  await expect(editor.getByLabel('决策截止（精确 UTC）', { exact: true })).toBeDisabled();
  await editor.getByRole('button', { name: '原样重试创建请求', exact: true }).click();
  await expect(editor).toHaveCount(0);
  expect(creation).toHaveLength(2);
  expect(creation[0]!.key).toBeTruthy();
  expect(creation[1]).toEqual(creation[0]);
  expect(frozen?.resource.header.frozen_at).toBeTruthy();
  const inputId = frozen!.resource.header.id;
  await expect(page.getByText(/未构成合格真实 PIT 证据/)).toBeVisible();

  const validation: { key: string | undefined; body: Schema['DataValidateRequest'] }[] = [];
  let admitted: Schema['CommandResult_RunSnapshotV1'] | undefined;
  await page.route('**/api/v2/data/validate', async route => {
    validation.push({ key: route.request().headers()['idempotency-key'], body: route.request().postDataJSON() });
    const response = await route.fetch();
    expect(response.status()).toBe(202);
    const receipt = await response.json() as Schema['CommandResult_RunSnapshotV1'];
    if (validation.length === 1) { admitted = receipt; return route.abort('failed'); }
    expect(receipt).toEqual({ ...admitted, replayed: true });
    return route.fulfill({ response });
  });
  await page.getByRole('button', { name: '请求数据质量验证', exact: true }).click();
  const validationEditor = page.getByRole('dialog', { name: '单独请求 DATA_VALIDATE', exact: true });
  await validationEditor.getByRole('combobox', { name: '确认实际数据 Runtime', exact: true }).click();
  await page.getByTitle(runtime.resource.id, { exact: true }).last().click();
  await validationEditor.getByLabel('CPU 总秒数（精确整数）', { exact: true }).fill('10');
  await validationEditor.getByLabel('输出上限（精确字节数，最多 64 MiB）', { exact: true }).fill('65536');
  await expect(validationEditor.getByRole('button', { name: '确认排队数据验证', exact: true })).toBeEnabled();
  await validationEditor.getByRole('button', { name: '确认排队数据验证', exact: true }).click();
  await expect(validationEditor.getByText(/提交结果未知：原请求和幂等键已锁定/)).toBeVisible();
  await validationEditor.getByRole('button', { name: '原样重试验证请求', exact: true }).click();
  await expect(validationEditor).toHaveCount(0);
  expect(validation).toHaveLength(2);
  expect(validation[0]!.key).toBeTruthy();
  expect(validation[1]).toEqual(validation[0]);
  expect(admitted!.resource.kind).toBe('DATA_VALIDATE');
  expect(admitted!.resource.state).toBe('QUEUED');
  expect(admitted!.resource.input_set_id).toBe(inputId);
  await page.getByRole('button', { name: '打开所选运行详情与取消', exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText(admitted!.resource.id);
  // A fresh page recovers the same immutable input from the service, not a
  // browser-persisted draft. No completed validation or qualification is claimed.
  await openInputs();
  await page.getByRole('button', { name: inputId, exact: true }).click();
  await expect(page.getByText(/未构成合格真实 PIT 证据/)).toBeVisible();
  const original = await page.request.get(`/api/v2/input-sets/${inputId}`);
  expect(original.status()).toBe(200);
  expect(await original.json()).toEqual(frozen!.resource);
  const registered = await page.request.get(`/api/v2/data/revisions/${dataset.resource.id}`);
  expect(registered.status()).toBe(200);
  const unchanged: Schema['DatasetView'] = await registered.json();
  expect(unchanged.origin).toBe('FIXTURE');
  expect(unchanged.pit_status).toBe('UNVERIFIED');
});
