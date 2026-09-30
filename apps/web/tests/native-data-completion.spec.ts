import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import { randomUUID, createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import type { Schema } from '../src/api';
import { fixture, rememberPrivateValue } from './native-auth-support';

const config = fixture();
const root = dirname(config.redactionsFile);
const read = (name: string) => JSON.parse(readFileSync(resolve(root, name), 'utf8'));
const save = (name: string, value: unknown) => writeFileSync(resolve(root, name), JSON.stringify(value), { mode: 0o600 });
const hash = (bytes: Buffer) => createHash('sha256').update(bytes).digest('hex');
type Quality = { schema_version: number; native_version: string; checked_at: string; datasets: {
  dataset_revision_id: string; row_count: string; first_event_ns: string; last_event_ns: string;
  available_through_ns: string; instrument_ids: string[]; selection: Record<string, unknown>;
  last_bar_notionals: { instrument_id: string; event_ns: string; available_ns: string; close_price: string;
    traded_volume: string; currency: string; notional_value: string }[];
}[] };
type Manifest = { run_id: string; attempt_no: number; external_job_id: string; input_set_id: string; state: string;
  started_at: string | null; error: { code: string } | null;
  artifacts: { schema: { name: string; version: string }; storage_ref: string; byte_count: string }[] };
const peer: { mode: string; endpoint: string; credential: string; image: string;
  metadata: { quality: Quality; registered_ref: string; storage_version: string } } = read('native-data-peer.json');
const admitted: { project: Schema['ProjectView']; runtime: Schema['RuntimeView']; dataset: Schema['DatasetView'];
  frozen: Schema['CommandResult_InputSetView']; admitted: Schema['CommandResult_RunSnapshotV1'];
  creation: { key: string; body: Schema['InputSetCreate'] }; validation: { key: string; body: Schema['DataValidateRequest'] };
} = read('native-data-admission.json');

test.use({ storageState: resolve(root, 'restart-browser.json') });

async function openInput(page: Page, runId?: string) {
  await page.goto('/');
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  await page.getByRole('tab', { name: '数据', exact: true }).click();
  await page.getByRole('tab', { name: '冻结输入', exact: true }).click();
  await page.getByRole('combobox', { name: '冻结输入所属研究项目', exact: true }).click();
  await page.getByTitle(`${admitted.project.name} · ${admitted.project.id}`, { exact: true }).click();
  await page.getByRole('button', { name: admitted.frozen.resource.header.id, exact: true }).click();
  await expect(page.getByText(/未构成合格真实 PIT 证据/)).toBeVisible();
  if (runId) {
    const row = page.getByRole('row').filter({ hasText: runId });
    await row.getByRole('button', { name: '查看产物', exact: true }).click();
    await page.getByRole('button', { name: '刷新所选运行产物', exact: true }).click();
  }
}
async function artifacts(page: Page, run: Schema['RunSnapshotV1']) {
  const values: Schema['ArtifactView'][] = [];
  let cursor: string | undefined;
  do {
    const response = await page.request.get('/api/v2/artifacts', { params: { project_id: admitted.project.id, limit: 25, ...(cursor ? { cursor } : {}) } });
    expect(response.status()).toBe(200);
    const body: { items: Schema['ArtifactView'][]; next_cursor?: string | null } = await response.json();
    values.push(...body.items.filter(item => item.producer_run_id === run.id));
    cursor = body.next_cursor ?? undefined;
  } while (cursor);
  for (const item of values) {
    expect(item.project_id).toBe(admitted.project.id);
    expect(item.producer_attempt_id).toBe(run.active_attempt_id);
    expect(item.origin).toBe('FIXTURE'); expect(item.created_by).toBe('RUNTIME');
  }
  return values;
}
async function terminalRun(page: Page, id: string, state: 'SUCCEEDED' | 'FAILED') {
  let run: Schema['RunSnapshotV1'] | undefined;
  await expect.poll(async () => {
    const response = await page.request.get(`/api/v2/runs/${id}`); expect(response.status()).toBe(200);
    run = await response.json();
    expect(run!.state).not.toBe(state === 'SUCCEEDED' ? 'FAILED' : 'SUCCEEDED');
    expect(run!.state).not.toBe('CANCELLED'); return run!.state;
  }, { timeout: 180_000, intervals: [250, 500, 1000] }).toBe(state);
  expect(run!.active_attempt_id).toBeTruthy(); expect(run!.current_attempt_no).toBe(1);
  expect(run!.input_set_id).toBe(admitted.frozen.resource.header.id);
  return run!;
}
async function runtimeBytes(page: Page, path: string) {
  const response = await page.request.get(`${peer.endpoint}/runtime/v1/${path}`, {
    headers: { Authorization: `Bearer ${peer.credential}` },
  });
  expect(response.status()).toBe(200); return response.body();
}
async function replay(page: Page) {
  for (const [path, intent, original, status] of [
    ['/api/v2/input-sets', admitted.creation, admitted.frozen, 201],
    ['/api/v2/data/validate', admitted.validation, admitted.admitted, 202],
  ] as const) {
    const response = await page.request.post(path, { headers: { Origin: config.baseUrl, 'Idempotency-Key': intent.key }, data: intent.body });
    expect(response.status()).toBe(status); expect(await response.json()).toEqual({ ...original, replayed: true });
  }
  expect(admitted.admitted.resource.state).toBe('QUEUED');
  const dataset = await page.request.get(`/api/v2/data/revisions/${admitted.dataset.id}`);
  expect(dataset.status()).toBe(200);
  expect(await dataset.json()).toMatchObject({ id: admitted.dataset.id, origin: 'FIXTURE', pit_status: 'UNVERIFIED', row_count: '3' });
  const input = await page.request.get(`/api/v2/input-sets/${admitted.frozen.resource.header.id}`);
  expect(input.status()).toBe(200); expect(await input.json()).toEqual(admitted.frozen.resource);
}

test('real Worker publishes original OCI bytes and preserves identities across process restart', async ({ page, context }) => {
  expect(peer.mode).toBe('native-execution');
  rememberPrivateValue(config, peer.credential);
  for (const cookie of await context.cookies()) rememberPrivateValue(config, cookie.value);
  if (config.phase === 'data-corrupt-admission') {
    // Fresh probe; a second explicit request is separate from replay of the
    // successful command. The harness already corrupted the actual native file.
    const probe = await page.request.post(`/api/v2/integrations/runtimes/${admitted.runtime.id}/probe`, {
      headers: { Origin: config.baseUrl, 'Idempotency-Key': randomUUID() },
      data: { schema_version: 1, expected_revision: admitted.runtime.revision },
    });
    expect(probe.status()).toBe(200);
    const response = await page.request.post('/api/v2/data/validate', {
      headers: { Origin: config.baseUrl, 'Idempotency-Key': randomUUID() }, data: admitted.validation.body,
    });
    expect(response.status()).toBe(202);
    const negative: Schema['CommandResult_RunSnapshotV1'] = await response.json();
    expect(negative.resource.id).not.toBe(admitted.admitted.resource.id);
    expect(negative.resource.state).toBe('QUEUED'); save('native-data-negative.json', negative);
    return;
  }
  if (config.phase === 'data-corrupt-complete') {
    const negative: Schema['CommandResult_RunSnapshotV1'] = read('native-data-negative.json');
    const run = await terminalRun(page, negative.resource.id, 'FAILED');
    const outputs = await artifacts(page, run);
    expect(outputs.map(item => item.schema_name)).toEqual(['qz.job_result']);
    const raw = await runtimeBytes(page, `jobs/${encodeURIComponent(`${run.id}/1`)}/result`);
    const manifest: Manifest = JSON.parse(raw.toString());
    expect(manifest.state).toBe('FAILED'); expect(manifest.started_at).toBeTruthy();
    expect(manifest.artifacts).toEqual([]); expect(manifest.error?.code).toBe('NATIVE_JOB_FAILED');
    const content = await page.request.get(`/api/v2/artifacts/${outputs[0]!.id}/content`);
    expect(content.status()).toBe(200); expect(await content.body()).toEqual(raw);
    await openInput(page, run.id);
    await expect(page.getByText('当前运行尚未成功；这里的产物不作为已通过的数据质量结论。', { exact: true })).toBeVisible();
    await expect(page.getByRole('row').filter({ hasText: 'qz.data_quality' })).toHaveCount(0);
    await replay(page); return;
  }
  expect(['data-complete', 'data-restored']).toContain(config.phase);
  const run = await terminalRun(page, admitted.admitted.resource.id, 'SUCCEEDED');
  const outputs = await artifacts(page, run);
  expect(outputs.map(item => item.schema_name).sort()).toEqual(['qz.data_quality', 'qz.job_result']);
  const manifestBytes = await runtimeBytes(page, `jobs/${encodeURIComponent(`${run.id}/1`)}/result`);
  const manifest: Manifest = JSON.parse(manifestBytes.toString());
  expect(manifest).toMatchObject({ run_id: run.id, attempt_no: 1, external_job_id: `${run.id}/1`,
    input_set_id: admitted.frozen.resource.header.id, state: 'SUCCEEDED', error: null });
  expect(manifest.started_at).toBeTruthy(); expect(manifest.artifacts).toHaveLength(1);
  const descriptor = manifest.artifacts[0]!;
  expect(descriptor.schema).toEqual({ name: 'qz.data_quality', version: '1' });
  const qualityBytes = await runtimeBytes(page, `jobs/${encodeURIComponent(`${run.id}/1`)}/artifacts/${descriptor.storage_ref}`);
  expect(String(qualityBytes.length)).toBe(descriptor.byte_count);
  const quality: Quality = JSON.parse(qualityBytes.toString());
  expect(quality.schema_version).toBe(1); expect(quality.native_version).toBe('nautilus-persistence/0.63.0');
  expect(quality.datasets).toHaveLength(1);
  const measured = quality.datasets[0]!;
  const prepared = peer.metadata.quality.datasets[0]!;
  expect(prepared.dataset_revision_id).not.toBe(admitted.dataset.id);
  expect(measured).toEqual({ ...prepared, dataset_revision_id: admitted.dataset.id });
  expect(quality.checked_at).not.toBe(peer.metadata.quality.checked_at);
  expect(measured).toMatchObject({ row_count: '3', first_event_ns: '60000000000', last_event_ns: '180000000000',
    available_through_ns: '1704153601000000000', instrument_ids: ['BTC-USD.COINBASE'] });
  expect(measured.last_bar_notionals).toHaveLength(1);
  expect(measured.last_bar_notionals[0]).toMatchObject({ instrument_id: 'BTC-USD.COINBASE', event_ns: '180000000000',
    available_ns: '1704153601000000000', close_price: '42000.99', traded_volume: '0.10000001', currency: 'USD', notional_value: '4200.1' });
  await openInput(page, run.id);
  const downloads: { id: string; schema: string; sha256: string }[] = [];
  for (const item of outputs) {
    const expected = item.schema_name === 'qz.job_result' ? manifestBytes : qualityBytes;
    const row = page.getByRole('row').filter({ hasText: item.id });
    await expect(row).toContainText(run.active_attempt_id!); await expect(row).toContainText('FIXTURE');
    const downloaded = page.waitForEvent('download');
    await row.getByRole('button', { name: '下载原始产物', exact: true }).click();
    const download = await downloaded; expect(await download.failure()).toBeNull();
    expect(download.suggestedFilename()).toBe(`${item.id}.bin`);
    const stream = await download.createReadStream(); expect(stream).toBeTruthy();
    const chunks: Buffer[] = []; for await (const chunk of stream!) chunks.push(Buffer.from(chunk));
    const actual = Buffer.concat(chunks); expect(actual).toEqual(expected);
    expect(String(actual.length)).toBe(item.byte_count);
    downloads.push({ id: item.id, schema: item.schema_name, sha256: hash(actual) });
  }
  const evidence = { run_id: run.id, attempt_id: run.active_attempt_id, downloads: downloads.sort((a, b) => a.id.localeCompare(b.id)), quality };
  if (config.phase === 'data-complete') save('native-data-quality.json', evidence);
  else expect(evidence).toEqual(read('native-data-quality.json'));
  await replay(page);

  // Hold an actual content response, navigate within the SPA so unmount must
  // abort the transfer, then release the same bytes. No synthetic payloads.
  const item = outputs[0]!;
  const path = `/api/v2/artifacts/${item.id}/content`;
  let release!: () => void;
  const held = new Promise<void>(done => { release = done; });
  let reached!: () => void;
  const requested = new Promise<void>(done => { reached = done; });
  let handled!: () => void; const finished = new Promise<void>(done => { handled = done; });
  let downloadCount = 0; page.on('download', () => { downloadCount += 1; });
  await page.route(`**${path}`, async route => {
    const response = await route.fetch(); reached(); await held;
    await route.fulfill({ response }).catch(() => { /* Navigation aborted this request. */ }); handled();
  });
  await page.getByRole('row').filter({ hasText: item.id }).getByRole('button', { name: '下载原始产物', exact: true }).click();
  await requested;
  const cancelled = page.waitForEvent('requestfailed', { predicate: request => request.url().endsWith(path) });
  try {
    await page.getByRole('tab', { name: '数据登记', exact: true }).click();
    await cancelled;
  } finally { release(); }
  await finished;
  await page.unroute(`**${path}`);
  expect(downloadCount).toBe(0);
});
