import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import { readFileSync, readdirSync } from 'node:fs';

// Deliberately synthetic transport fixtures; these are not native service or
// scientific-evaluation evidence. The actual Vite artifacts execute unchanged.
const document = JSON.parse(readFileSync(new URL('../../../contracts/generated/api-v2.openapi.json', import.meta.url), 'utf8'));
const manifest = JSON.parse(readFileSync(new URL('../src/generated/response-contract/manifest.json', import.meta.url), 'utf8')) as {
  files: { path: string; provenance: { schemas?: string[] } }[];
};
function moduleFor(path: string, method: string, status: number) {
  const schema = document.paths[path][method].responses[status].content['application/json'].schema;
  const pointer = schema.$ref && Object.keys(schema).length === 1 ? schema.$ref
    : `#/paths/${path.replaceAll('~', '~0').replaceAll('/', '~1')}/${method}/responses/${status}/content/application~1json/schema`;
  const file = manifest.files.find(file => file.provenance.schemas?.includes(pointer));
  if (!file) throw new Error(`Missing original schema provenance: ${pointer}`);
  return file.path.split('/').at(-1)!.replace(/\.cjs$/, '');
}
const listModule = moduleFor('/api/v2/projects', 'get', 200);
const receiptModule = moduleFor('/api/v2/projects', 'post', 201);
const id = '01990000-0000-7000-8000-000000000001';
const project = { id, name: '保留已提交的研究', description: '', state: 'DRAFT', revision: '1', created_by: 'OPERATOR',
  root_lineage_id: id, created_at: '2026-09-30T00:00:00Z', updated_at: '2026-09-30T00:00:00Z' };
async function session(page: Page, evaluation = false) {
  let artifactWrites = 0;
  let reads = 0; const writes: { key: string | undefined; body: string | null }[] = [];
  await page.route('**/api/**', async route => {
    const request = route.request(); const path = new URL(request.url()).pathname;
    if (path === '/api/v2/auth/status') return route.fulfill({ json: { schema_version: 1, setup_required: false } });
    if (path === '/api/v2/auth/session') return route.fulfill({ json: { schema_version: 1,
      authenticated_at: new Date().toISOString(), expires_at: new Date(Date.now() + 43_200_000).toISOString() } });
    if (path === '/api/v2/projects' && request.method() === 'GET') {
      reads++; return route.fulfill({ json: { schema_version: 1, items: evaluation || writes.length ? [project] : [], next_cursor: null } });
    }
    if (path === '/api/v2/projects' && request.method() === 'POST') {
      writes.push({ key: request.headers()['idempotency-key'], body: request.postData() });
      return route.fulfill({ status: 201, json: { schema_version: 1, replayed: writes.length > 1, resource: project } });
    }
    if (evaluation && path === `/api/v2/projects/${id}`) return route.fulfill({ json: project });
    if (evaluation && ['/api/v2/briefs', '/api/v2/artifacts'].includes(path)) {
      if (request.method() !== 'GET') { artifactWrites++; return route.abort('blockedbyclient'); }
      return route.fulfill({ json: { schema_version: 1, items: [], next_cursor: null } });
    }
    if (path === '/api/v2/runs') return route.fulfill({ json: { schema_version: 1, items: [], next_cursor: null } });
    return route.abort('blockedbyclient');
  });
  return { reads: () => reads, writes, artifactWrites: () => artifactWrites };
}

test('normal browser loading uses selective CJS interop without reaching the eager facade', async ({ page }) => {
  await session(page); const requests: string[] = []; page.on('request', request => requests.push(request.url()));
  await page.goto('/'); await expect(page.getByText('暂无研究项目', { exact: true })).toBeVisible();
  expect(requests.some(url => url.includes(listModule))).toBe(true);
  expect(requests.some(url => /\/generated\/responses\.cjs|response-contract\.js(?:\?|$)/.test(url))).toBe(false);
});

test('a delayed response validator cannot replace a newer navigation choice', async ({ page }) => {
  await session(page); let release!: () => void; const held = new Promise<void>(resolve => { release = resolve; }); let pending = 0;
  await page.route(url => url.pathname.includes(listModule), async route => { pending++; await held; await route.continue(); });
  await page.goto('/'); await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
  await expect.poll(() => pending).toBeGreaterThan(0);
  await page.getByRole('menuitem', { name: '运行', exact: true }).click();
  await expect(page.getByRole('heading', { name: '运行', exact: true })).toBeVisible();
  release(); await page.waitForLoadState('networkidle');
  await expect(page.getByRole('heading', { name: '研究', exact: true })).toHaveCount(0);
  await expect(page.getByRole('menuitem', { name: '运行', exact: true })).toHaveClass(/ant-menu-item-selected/);
});

test('a failed validator preserves the write intent and edits without automatic replay', async ({ page }) => {
  const state = await session(page); let fail = true;
  await page.route(url => url.pathname.includes(receiptModule), route => fail ? route.abort('failed') : route.continue());
  await page.goto('/'); await expect(page.getByText('暂无研究项目', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '新建研究', exact: true }).click();
  await page.getByLabel('研究名称').fill(project.name);
  await page.getByRole('button', { name: '保存项目', exact: true }).click();
  await expect(page.getByText(/响应校验组件加载失败/)).toBeVisible();
  expect(state.writes).toHaveLength(1); await expect(page.getByLabel('研究名称')).toHaveValue(project.name);
  await page.getByRole('button', { name: '保存项目', exact: true }).click();
  await expect.poll(() => state.writes.length).toBe(2); expect(state.writes[1]).toEqual(state.writes[0]);
  // An evaluated-module failure may remain cached in this document. Recovery
  // reads the committed result after refresh; it never silently retries a write.
  fail = false; page.once('dialog', dialog => dialog.accept()); await page.reload();
  await expect(page.getByRole('button', { name: project.name, exact: true })).toBeVisible();
  expect(state.writes).toHaveLength(2);
});


const reportModule = moduleFor('/api/v2/artifacts/{id}/agent-evaluation', 'get', 200);
const reportFixture = readFileSync(new URL('../../../tests/fixtures/agent-evaluation/unrun-v1.json', import.meta.url));
async function openReportUpload(page: Page) {
  await page.goto('/');
  await page.getByRole('button', { name: project.name, exact: true }).click();
  await page.getByRole('tab', { name: 'Agent 评估', exact: true }).click();
  await page.getByRole('button', { name: '上传报告', exact: true }).click();
  return page.getByRole('dialog', { name: '上传 Agent 评估报告' });
}
test('a failed local report validator never accepts or uploads the selected report', async ({ page }) => {
  const state = await session(page, true);
  await page.route(url => url.pathname.includes(reportModule), route => route.abort('failed'));
  const editor = await openReportUpload(page);
  await editor.locator('input[type=file]').setInputFiles({ name: 'unrun-v1.json', mimeType: 'application/json', buffer: reportFixture });
  await expect(editor.getByText(/本地报告校验组件加载失败；报告尚未上传/)).toBeVisible();
  await expect(editor.getByText(/无法读取有效 JSON/)).toHaveCount(0);
  await expect(editor.getByText(/unrun-v1.json/)).toHaveCount(0);
  await expect(editor.getByRole('button', { name: '上传报告', exact: true })).toBeDisabled();
  expect(state.artifactWrites()).toBe(0);
});

test('a cancelled delayed report validation cannot populate a later upload', async ({ page }) => {
  const state = await session(page, true);
  let release!: () => void; const held = new Promise<void>(resolve => { release = resolve; }); let pending = 0;
  await page.route(url => url.pathname.includes(reportModule), async route => { pending++; await held; await route.continue(); });
  const editor = await openReportUpload(page);
  await editor.locator('input[type=file]').setInputFiles({ name: 'cancelled-report.json', mimeType: 'application/json', buffer: reportFixture });
  await expect.poll(() => pending).toBeGreaterThan(0);
  await expect(editor.getByRole('button', { name: '上传报告', exact: true })).toBeDisabled();
  await editor.getByRole('button', { name: '取消', exact: true }).click();
  const discard = page.getByRole('dialog', { name: '放弃未上传的报告？', exact: true });
  await expect(discard).toBeVisible();
  await discard.getByRole('button', { name: '关闭', exact: true }).click();
  await expect(discard).toHaveCount(0);
  await expect(editor).toHaveCount(0);
  await page.getByRole('button', { name: '上传报告', exact: true }).click();
  release(); await page.waitForLoadState('networkidle');
  await expect(editor.getByText(/cancelled-report.json/)).toHaveCount(0);
  await expect(editor.getByRole('button', { name: '上传报告', exact: true })).toBeDisabled();
  await editor.locator('input[type=file]').setInputFiles({ name: 'current-report.json', mimeType: 'application/json', buffer: reportFixture });
  await expect(editor.getByText(/current-report.json/)).toBeVisible();
  await expect(editor.getByRole('button', { name: '上传报告', exact: true })).toBeEnabled();
  expect(state.artifactWrites()).toBe(0);
});

const artifactReceiptModule = moduleFor('/api/v2/artifacts', 'post', 201);
for (const fault of ['lost-ack', 'receipt-validator'] as const) {
  test(`an uncertain report ${fault} locks original content and key until explicit abandonment`, async ({ page }) => {
    await session(page, true);
    const writes: { key: string | undefined; body: string | null }[] = [];
    await page.route('**/api/v2/artifacts', async route => {
      if (route.request().method() !== 'POST') return route.fallback();
      writes.push({ key: route.request().headers()['idempotency-key'], body: route.request().postData() });
      if (fault === 'lost-ack') return route.abort('failed');
      return route.fulfill({ status: 201, json: { schema_version: 1, replayed: writes.length > 1, resource: {
        id: '01990000-0000-7000-8000-000000000002', project_id: id, kind: 'REPORT', media_type: 'application/json',
        schema_name: 'qz.operator_report', schema_version: '1', access_class: 'RESEARCH', origin: 'SYNTHETIC',
        created_by: 'OPERATOR', created_at: '2026-09-30T00:00:00Z', byte_count: String(reportFixture.length),
        producer_run_id: null, producer_attempt_id: null,
      } } });
    });
    if (fault === 'receipt-validator') await page.route(url => url.pathname.includes(artifactReceiptModule), route => route.abort('failed'));
    const editor = await openReportUpload(page);
    await editor.locator('input[type=file]').setInputFiles({ name: 'original-report.json', mimeType: 'application/json', buffer: reportFixture });
    await expect(editor.getByRole('button', { name: '上传报告', exact: true })).toBeEnabled();
    await editor.getByRole('button', { name: '上传报告', exact: true }).click();
    await expect(editor.getByText(fault === 'lost-ack' ? /连接中断，提交结果未知/ : /响应校验组件加载失败/)).toBeVisible();
    await expect(editor.getByText(/原报告内容与幂等键已锁定/)).toBeVisible();
    await expect(editor.getByRole('button', { name: '选择 JSON 报告', exact: true })).toBeDisabled();
    await expect(editor.locator('input[type=file]')).toBeDisabled();
    expect(writes).toHaveLength(1);
    const retryButton = editor.locator('button').filter({ hasText: '原样重试上传请求' });
    await expect(retryButton).toHaveCount(1);
    await expect(retryButton).toHaveAccessibleName('原样重试上传请求');
    await editor.getByRole('button', { name: '原样重试上传请求', exact: true }).click();
    await expect.poll(() => writes.length).toBe(2);
    await expect(editor.getByRole('button', { name: '取消', exact: true })).toBeEnabled();
    expect(writes[0]!.key).toBeTruthy(); expect(writes[1]).toEqual(writes[0]);
    await expect(editor.getByText(/original-report.json/)).toBeVisible();
    await editor.getByRole('button', { name: '取消', exact: true }).click();
    const discard = page.getByRole('dialog', { name: '关闭报告上传？', exact: true });
    await expect(discard).toBeVisible();
    await expect(discard.getByText(/关闭不会撤回已保存的报告/)).toBeVisible();
    await discard.getByRole('button', { name: '继续编辑', exact: true }).click();
    await expect(discard).toHaveCount(0);
    await expect(editor.locator('input[type=file]')).toBeDisabled();
    await editor.getByRole('button', { name: '取消', exact: true }).click();
    await expect(discard).toBeVisible();
    await discard.getByRole('button', { name: '关闭', exact: true }).click();
    await expect(discard).toHaveCount(0);
    await expect(editor).toHaveCount(0);
    await page.getByRole('button', { name: '上传报告', exact: true }).click();
    const changed = { ...JSON.parse(reportFixture.toString()), recorded_at: '2026-09-30T01:00:00Z' };
    await editor.locator('input[type=file]').setInputFiles({ name: 'replacement-report.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(changed)) });
    await expect(editor.getByText(/replacement-report.json/)).toBeVisible();
    expect(writes).toHaveLength(2);
    await editor.getByRole('button', { name: '上传报告', exact: true }).click();
    await expect.poll(() => writes.length).toBe(3);
    expect(writes[2]!.key).not.toBe(writes[0]!.key);
    expect(writes[2]!.body).not.toBe(writes[0]!.body);
  });
}

test.describe('production PWA validator precache', () => {
  test.use({ serviceWorkers: 'allow' });
  test('a failed validator download cannot activate a partially precached application', async ({ page, context }, info) => {
    test.skip(info.project.name !== 'production-chunks', 'PWA is intentionally disabled in Vite dev');
    await session(page);
    const deferredModule = moduleFor('/api/v2/artifacts/{id}', 'get', 200);
    let blocked = 0;
    await context.route(url => url.pathname.includes(deferredModule), route => { blocked++; return route.abort('failed'); });
    await page.goto('/'); await expect(page.getByText('暂无研究项目', { exact: true })).toBeVisible();
    await expect.poll(() => blocked).toBeGreaterThan(0);
    await expect.poll(() => page.evaluate(async () => {
      const registration = await navigator.serviceWorker.getRegistration();
      return !registration || (!registration.active && !registration.installing && !registration.waiting);
    })).toBe(true);
    expect(await page.evaluate(() => navigator.serviceWorker.controller !== null)).toBe(false);
    await expect(page.getByRole('menu', { name: '主导航' })).toBeVisible();
  });
  test('a delayed static validator delays installation, then every lazy validator is cached', async ({ page, context }, info) => {
    test.skip(info.project.name !== 'production-chunks', 'PWA is intentionally disabled in Vite dev');
    await session(page);
    const deferredModule = moduleFor('/api/v2/artifacts/{id}', 'get', 200);
    let release!: () => void; const held = new Promise<void>(resolve => { release = resolve; }); let pending = 0;
    await context.route(url => url.pathname.includes(deferredModule), async route => { pending++; await held; await route.continue(); });
    await page.goto('/'); await expect(page.getByText('暂无研究项目', { exact: true })).toBeVisible();
    await expect.poll(() => pending).toBeGreaterThan(0);
    expect(await page.evaluate(() => navigator.serviceWorker.controller !== null)).toBe(false);
    release(); await page.evaluate(async () => { await navigator.serviceWorker.ready; });
    await page.reload(); await expect.poll(() => page.evaluate(() => navigator.serviceWorker.controller !== null)).toBe(true);
    const cached = await page.evaluate(async () => {
      const paths = [];
      for (const name of await caches.keys()) for (const request of await (await caches.open(name)).keys()) paths.push(new URL(request.url).pathname);
      return paths;
    });
    const validators = readdirSync(new URL('../dist/assets/', import.meta.url)).filter(file => /^(schema|shared)-.*\.js$/.test(file));
    expect(validators.length).toBeGreaterThan(100);
    for (const file of validators) expect(cached).toContain('/assets/' + file);
    expect(cached.some(path => path.startsWith('/api/') || path.startsWith('/health/'))).toBe(false);
  });
});
