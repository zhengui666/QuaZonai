import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import { readFileSync, readdirSync } from 'node:fs';
import type { Schema } from '../src/api';

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
function isModuleRequest(url: URL | string, module: string) {
  const pathname = typeof url === 'string' ? new URL(url).pathname : url.pathname;
  return new RegExp(`(?:^|[/_])${module}(?=[.-])`).test(pathname);
}
const listModule = moduleFor('/api/v2/projects', 'get', 200);
const receiptModule = moduleFor('/api/v2/data/sources', 'post', 201);
const reportModule = moduleFor('/api/v2/artifacts/{id}/agent-evaluation', 'get', 200);
const id = '01990000-0000-7000-8000-000000000001';
const firstReportId = '01990000-0000-7000-8000-000000000002';
const secondReportId = '01990000-0000-7000-8000-000000000003';
const now = '2026-09-30T00:00:00Z';
const project: Schema['ProjectView'] = { id, name: '保留服务器原研究', description: 'Synthetic transport fixture', state: 'ACTIVE', revision: '1', created_by: 'OPERATOR',
  root_lineage_id: id, created_at: now, updated_at: now, current_brief_id: null };
const reportFixture = readFileSync(new URL('../../../tests/fixtures/agent-evaluation/unrun-v1.json', import.meta.url), 'utf8');
const report: Schema['AgentEvaluationReportV1'] = JSON.parse(reportFixture);
const laterReport: Schema['AgentEvaluationReportV1'] = { ...report, runner: { ...report.runner, name: 'later-report-fixture' } };
const artifact = (reportId: string): Schema['ArtifactView'] => ({
  id: reportId, project_id: id, kind: 'REPORT', media_type: 'application/json', schema_name: 'qz.agent_evaluation',
  schema_version: '1', access_class: 'RESEARCH', origin: 'SYNTHETIC', created_by: 'OPERATOR', created_at: now,
  byte_count: String(Buffer.byteLength(reportFixture)), producer_run_id: null, producer_attempt_id: null,
});
async function session(page: Page) {
  let reads = 0;
  const writes: string[] = [];
  const reportReads: string[] = [];
  await page.route('**/api/**', async route => {
    const request = route.request(); const path = new URL(request.url()).pathname;
    const reply = (json: unknown) => route.fulfill({ json });
    const listing = (items: unknown[]) => reply({ schema_version: 1, items, next_cursor: null });
    if (!['GET', 'HEAD'].includes(request.method())) { writes.push(`${request.method()} ${path}`); return route.abort('blockedbyclient'); }
    if (path === '/api/v2/auth/status') return reply({ schema_version: 1, setup_required: false });
    if (path === '/api/v2/auth/session') return reply({ schema_version: 1,
      authenticated_at: new Date().toISOString(), expires_at: new Date(Date.now() + 43_200_000).toISOString() });
    if (path === '/api/v2/projects') { reads++; return listing([project]); }
    if (path === `/api/v2/projects/${id}`) return reply(project);
    if ([`/api/v2/projects/${id}/briefs`, `/api/v2/projects/${id}/cycles`, '/api/v2/runs', '/api/v2/settings/codex'].includes(path)) return listing([]);
    if (path === '/api/v2/artifacts') return listing([artifact(firstReportId), artifact(secondReportId)]);
    if (path === `/api/v2/artifacts/${firstReportId}/agent-evaluation`) { reportReads.push(path); return reply(report); }
    if (path === `/api/v2/artifacts/${secondReportId}/agent-evaluation`) { reportReads.push(path); return reply(laterReport); }
    return route.abort('blockedbyclient');
  });
  return { reads: () => reads, writes, reportReads };
}

async function openReports(page: Page) {
  await page.goto('/');
  await page.getByRole('button', { name: project.name, exact: true }).click();
  await page.getByRole('tab', { name: 'Agent 评估', exact: true }).click();
  await expect(page.getByRole('button', { name: firstReportId, exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '上传报告', exact: true })).toHaveCount(0);
  await expect(page.locator('input[type=file]')).toHaveCount(0);
}

test('validator request matching distinguishes sequence prefixes in dev and production', () => {
  for (const pathname of ['/src/generated/response-contract/modules/schema-1.cjs',
    '/node_modules/.vite/deps/@quazonai_web_response-contract_modules_schema-1.js',
    '/assets/schema-1-build.js']) {
    const url = new URL(pathname, 'http://127.0.0.1');
    expect(isModuleRequest(url, 'schema-1')).toBe(true);
    expect(isModuleRequest(url.href, 'schema-1')).toBe(true);
    expect(isModuleRequest(new URL(pathname.replace('schema-1', 'schema-10'), url), 'schema-1')).toBe(false);
  }
});

test('normal browser loading uses selective CJS interop without reaching the eager facade', async ({ page }) => {
  const state = await session(page); const requests: string[] = []; page.on('request', request => requests.push(request.url()));
  await page.goto('/'); await expect(page.getByRole('button', { name: project.name, exact: true })).toBeVisible();
  expect(requests.some(url => isModuleRequest(url, listModule))).toBe(true);
  expect(requests.some(url => /\/generated\/responses\.cjs|response-contract\.js(?:\?|$)/.test(url))).toBe(false);
  expect(state.writes).toEqual([]);
});

test('a delayed response validator cannot replace a newer navigation choice', async ({ page }) => {
  const state = await session(page); let release!: () => void; const held = new Promise<void>(resolve => { release = resolve; }); let pending = 0;
  await page.route(url => isModuleRequest(url, listModule), async route => { pending++; await held; await route.continue(); });
  try {
    await page.goto('/'); await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
    await expect.poll(() => pending).toBeGreaterThan(0);
    await page.getByRole('menuitem', { name: '运行', exact: true }).click();
    await expect(page.getByRole('heading', { name: '运行', exact: true })).toBeVisible();
  } finally { release(); }
  await page.waitForLoadState('networkidle');
  await expect(page.getByRole('heading', { name: '研究', exact: true })).toHaveCount(0);
  await expect(page.getByRole('menuitem', { name: '运行', exact: true })).toHaveClass(/ant-menu-item-selected/);
  expect(state.writes).toEqual([]);
});

test('a failed Settings receipt validator retains the original command without automatic replay', async ({ page }) => {
  const state = await session(page);
  const runtime: Schema['RuntimeView'] = { id: '01990000-0000-7000-8000-000000000004', revision: '1', protocol_version: 1,
    created_at: now, updated_at: now, credential_configured: true, ca_configured: false,
    configuration: { name: 'Runtime fixture', endpoint: 'https://runtime.example', tls_policy: 'SYSTEM_CA',
      development_http: false, enabled: true, allowed_capabilities: ['DATA_VALIDATE'] } };
  const source: Schema['DataSourceView'] = { id: '01990000-0000-7000-8000-000000000005', revision: '1',
    created_at: now, updated_at: now, name: '保留已提交的数据源', runtime_id: runtime.id,
    native_catalog_ref: 'catalog/validator-fixture', provider_kind: 'NAUTILUS_CATALOG', enabled: true };
  const writes: { key: string | undefined; body: Schema['DataSourceCreate'] }[] = [];
  await page.route('**/api/v2/integrations/runtimes**', route => {
    if (route.request().method() !== 'GET') return route.fallback();
    const path = new URL(route.request().url()).pathname;
    if (path === '/api/v2/integrations/runtimes') return route.fulfill({ json: { schema_version: 1, items: [runtime], next_cursor: null } });
    if (path === `/api/v2/integrations/runtimes/${runtime.id}`) return route.fulfill({ json: runtime });
    return route.fallback();
  });
  await page.route(/\/api\/v2\/data\/sources(?:\?|$)/, route => {
    const request = route.request();
    if (request.method() === 'GET') return route.fulfill({ json: { schema_version: 1, items: writes.length ? [source] : [], next_cursor: null } });
    if (request.method() !== 'POST') return route.fallback();
    writes.push({ key: request.headers()['idempotency-key'], body: request.postDataJSON() });
    return route.fulfill({ status: 201, json: { schema_version: 1, replayed: writes.length > 1, resource: source } });
  });
  let fail = true;
  await page.route(url => isModuleRequest(url, receiptModule), route => fail ? route.abort('failed') : route.continue());
  await page.goto('/');
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  await page.getByRole('tab', { name: '数据', exact: true }).click();
  await page.getByRole('button', { name: '登记数据源', exact: true }).click();
  const editor = page.getByRole('dialog', { name: '登记数据源', exact: true });
  await editor.getByRole('textbox', { name: /^\*?\s*数据源名称$/ }).fill(source.name);
  await editor.getByRole('combobox', { name: '选择已登记的 Runtime', exact: true }).click();
  await page.getByText(runtime.configuration.name, { exact: true }).last().click();
  await editor.getByRole('textbox', { name: /^\*?\s*Runtime 原生目录登记键$/ }).fill(source.native_catalog_ref);
  await editor.getByRole('button', { name: '登记', exact: true }).click();
  await expect(editor.getByText(/响应校验组件加载失败/)).toBeVisible();
  await expect(editor.getByRole('textbox', { name: /^\*?\s*数据源名称$/ })).toHaveValue(source.name);
  await expect(editor.getByRole('textbox', { name: /^\*?\s*数据源名称$/ })).toBeDisabled();
  const retry = editor.getByRole('button', { name: '重试当前操作', exact: true });
  await expect(retry).toHaveAccessibleName('重试当前操作');
  await expect(retry).toHaveAttribute('aria-busy', 'false');
  expect(writes).toHaveLength(1);
  expect(writes[0]?.key).toBeTruthy();
  expect(writes[0]?.body).toEqual({ schema_version: 1, name: source.name, runtime_id: runtime.id,
    native_catalog_ref: source.native_catalog_ref, provider_kind: 'NAUTILUS_CATALOG', enabled: true });
  await retry.click();
  await expect.poll(() => writes.length).toBe(2);
  await expect(editor.getByText(/响应校验组件加载失败/)).toBeVisible();
  expect(writes[1]).toEqual(writes[0]);
  // Evaluated-module failures can remain cached in this document. Refresh
  // recovers by reading the committed result, never by replaying the command.
  fail = false; page.once('dialog', dialog => dialog.accept()); await page.reload();
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  await page.getByRole('tab', { name: '数据', exact: true }).click();
  await expect(page.getByText(source.name, { exact: true })).toBeVisible();
  expect(writes).toHaveLength(2);
  expect(state.writes).toEqual([]);
});

test('a failed report validator cannot display unvalidated evidence and reload recovers the original report', async ({ page }) => {
  const state = await session(page); let fail = true;
  await page.route(url => isModuleRequest(url, reportModule), route => fail ? route.abort('failed') : route.continue());
  await openReports(page);
  await page.getByRole('button', { name: firstReportId, exact: true }).click();
  const detail = page.getByRole('dialog', { name: 'Agent 评估详情', exact: true });
  await expect(detail.getByText(/响应校验组件加载失败/)).toBeVisible();
  await expect(detail.getByText(`${report.runner.name} / ${report.runner.version}`, { exact: true })).toHaveCount(0);
  expect(state.reportReads).toEqual([`/api/v2/artifacts/${firstReportId}/agent-evaluation`]);
  const retry = detail.getByRole('button', { name: '重新载入', exact: true });
  await expect(retry).toHaveAccessibleName('重新载入');
  await retry.click();
  await expect.poll(() => state.reportReads.length).toBe(2);
  await expect(detail.getByText(/响应校验组件加载失败/)).toBeVisible();
  fail = false;
  await openReports(page);
  await page.getByRole('button', { name: firstReportId, exact: true }).click();
  await expect(detail.getByText(`${report.runner.name} / ${report.runner.version}`, { exact: true })).toBeVisible();
  await expect(detail.getByText('PROTOCOL_ONLY：协议测试，不是实际模型评估', { exact: true })).toBeVisible();
  expect(state.reportReads).toEqual(Array(3).fill(`/api/v2/artifacts/${firstReportId}/agent-evaluation`));
  expect(state.writes).toEqual([]);
});

test('a dismissed delayed report validation cannot populate a later report drawer', async ({ page }) => {
  const state = await session(page);
  let release!: () => void; const held = new Promise<void>(resolve => { release = resolve; }); let pending = 0;
  await page.route(url => isModuleRequest(url, reportModule), async route => { pending++; await held; await route.continue(); });
  await openReports(page);
  const detail = page.getByRole('dialog', { name: 'Agent 评估详情', exact: true });
  try {
    await page.getByRole('button', { name: firstReportId, exact: true }).click();
    await expect.poll(() => pending).toBeGreaterThan(0);
    await expect(detail.getByRole('status', { name: '正在载入', exact: true })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(detail).toHaveCount(0);
    await expect(page.getByRole('dialog', { name: /放弃|关闭报告上传/ })).toHaveCount(0);
    await page.getByRole('button', { name: secondReportId, exact: true }).click();
    await expect.poll(() => state.reportReads.length).toBe(2);
  } finally { release(); }
  await expect(detail.getByText(`${laterReport.runner.name} / ${laterReport.runner.version}`, { exact: true })).toBeVisible();
  await expect(detail.getByText(`${report.runner.name} / ${report.runner.version}`, { exact: true })).toHaveCount(0);
  await expect(detail.getByRole('status', { name: '正在载入', exact: true })).toHaveCount(0);
  expect(state.reportReads).toEqual([`/api/v2/artifacts/${firstReportId}/agent-evaluation`, `/api/v2/artifacts/${secondReportId}/agent-evaluation`]);
  expect(state.writes).toEqual([]);
});

for (const fault of ['network', 'invalid-json', 'invalid-contract'] as const) {
  test(`a report ${fault} failure retries only the original read and never invents evidence`, async ({ page }) => {
    const state = await session(page);
    const attempts: { method: string; path: string; key: string | undefined; body: string | null }[] = [];
    await page.route(`**/api/v2/artifacts/${firstReportId}/agent-evaluation`, route => {
      const request = route.request();
      attempts.push({ method: request.method(), path: new URL(request.url()).pathname,
        key: request.headers()['idempotency-key'], body: request.postData() });
      if (attempts.length === 1) {
        if (fault === 'network') return route.abort('failed');
        if (fault === 'invalid-json') return route.fulfill({ status: 200, contentType: 'application/json', body: '{' });
        return route.fulfill({ json: { ...report, cases: 'invalid-report-cases' } });
      }
      return route.fulfill({ json: report });
    });
    await openReports(page);
    await page.getByRole('button', { name: firstReportId, exact: true }).click();
    const detail = page.getByRole('dialog', { name: 'Agent 评估详情', exact: true });
    await expect(detail.getByText(fault === 'network' ? '连接中断，未能读取数据；请重试'
      : fault === 'invalid-json' ? 'JSON 响应无效' : '响应数据不兼容', { exact: true })).toBeVisible();
    await expect(detail.getByText(`${report.runner.name} / ${report.runner.version}`, { exact: true })).toHaveCount(0);
    expect(attempts).toHaveLength(1);
    const retry = detail.getByRole('button', { name: '重新载入', exact: true });
    await expect(retry).toHaveAccessibleName('重新载入');
    await retry.click();
    await expect(detail.getByText(`${report.runner.name} / ${report.runner.version}`, { exact: true })).toBeVisible();
    await expect(detail.getByText('PROTOCOL_ONLY：协议测试，不是实际模型评估', { exact: true })).toBeVisible();
    expect(attempts).toHaveLength(2);
    expect(attempts[0]).toEqual({ method: 'GET', path: `/api/v2/artifacts/${firstReportId}/agent-evaluation`, key: undefined, body: null });
    expect(attempts[1]).toEqual(attempts[0]);
    await page.keyboard.press('Escape');
    await expect(detail).toHaveCount(0);
    await expect(page.getByRole('dialog', { name: /放弃|关闭报告上传/ })).toHaveCount(0);
    expect(state.writes).toEqual([]);
  });
}

test.describe('production PWA validator precache', () => {
  test.use({ serviceWorkers: 'allow' });
  test('a failed validator download cannot activate a partially precached application', async ({ page, context }, info) => {
    test.skip(info.project.name !== 'production-chunks', 'PWA is intentionally disabled in Vite dev');
    await session(page);
    const deferredModule = moduleFor('/api/v2/artifacts/{id}', 'get', 200);
    let blocked = 0;
    await context.route(url => isModuleRequest(url, deferredModule), route => { blocked++; return route.abort('failed'); });
    await page.goto('/'); await expect(page.getByRole('button', { name: project.name, exact: true })).toBeVisible();
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
    await context.route(url => isModuleRequest(url, deferredModule), async route => { pending++; await held; await route.continue(); });
    await page.goto('/'); await expect(page.getByRole('button', { name: project.name, exact: true })).toBeVisible();
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
