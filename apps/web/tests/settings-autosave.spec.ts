import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { Schema } from '../src/api';

async function setup(page: Page) {
  const now = new Date().toISOString();
  const runtime: Schema['RuntimeView'] = {
    id: '01990000-0000-7000-8000-000000000011', revision: '1', protocol_version: 1, created_at: now, updated_at: now,
    credential_configured: true, ca_configured: false,
    configuration: { name: 'Runtime A', endpoint: 'https://runtime.example', tls_policy: 'SYSTEM_CA',
      development_http: false, enabled: true, allowed_capabilities: ['DATA_VALIDATE'] },
  };
  const downstream: Schema['DownstreamView'] = {
    id: '01990000-0000-7000-8000-000000000012', revision: '1', created_at: now, updated_at: now,
    credential_configured: true,
    configuration: { name: 'Downstream A', endpoint: 'https://downstream.example', environments: 'PAPER',
      accepted_package_versions: ['1'], development_http: false, enabled: true },
  };
  const source: Schema['DataSourceView'] = {
    id: '01990000-0000-7000-8000-000000000013', revision: '1', created_at: now, updated_at: now,
    name: 'Source A', runtime_id: runtime.id, native_catalog_ref: 'catalog/source-a',
    provider_kind: 'NAUTILUS_CATALOG', enabled: true,
  };
  const writes: { kind: string; key: string | undefined; body: unknown }[] = [];
  const secretId = '01990000-0000-7000-8000-000000000014';
  let failRuntime = false;
  let rejectRuntime = false;
  let conflictRuntime = false;
  let holdRuntime: Promise<void> | undefined;
  await page.route('**/api/**', async route => {
    const request = route.request(); const path = new URL(request.url()).pathname;
    const reply = (json: unknown, status = 200) => route.fulfill({ status, body: JSON.stringify(json), contentType: 'application/json' });
    if (path === '/api/v2/auth/status') return reply({ schema_version: 1, setup_required: false });
    if (path === '/api/v2/auth/session') return reply({ schema_version: 1, authenticated_at: now, expires_at: new Date(Date.now() + 43_200_000).toISOString() });
    if (path === '/api/v2/auth/cli/devices') return reply([]);
    if (path === '/api/v2/projects') return reply({ schema_version: 1, items: [], next_cursor: null });
    if (path === '/api/v2/settings/codex') return reply({ schema_version: 1, items: [], next_cursor: null });
    if (path === '/api/v2/settings/credentials') {
      const body: Schema['IntegrationSecretCreate'] = request.postDataJSON();
      return reply({ schema_version: 1, replayed: false,
        resource: { id: secretId, purpose: body.intent.purpose, label: body.intent.label, created_at: now } }, 201);
    }
    if (path === '/api/v2/integrations/runtimes') {
      if (request.method() !== 'GET') { writes.push({ kind: 'runtime-create', key: request.headers()['idempotency-key'], body: request.postDataJSON() }); return route.abort('blockedbyclient'); }
      return reply({ schema_version: 1, items: [runtime], next_cursor: null });
    }
    if (path === `/api/v2/integrations/runtimes/${runtime.id}/readiness`) return reply({ schema_version: 1, runtime_id: runtime.id,
      integration_revision: runtime.revision, state: 'NOT_CHECKED', latest_observation: null, available_job_kinds: [] });
    if (path === `/api/v2/integrations/runtimes/${runtime.id}`) {
      if (request.method() === 'GET') return reply(runtime);
      const body: Schema['RuntimeUpdate'] = request.postDataJSON();
      writes.push({ kind: 'runtime', key: request.headers()['idempotency-key'], body });
      if (holdRuntime) { await holdRuntime; holdRuntime = undefined; }
      if (failRuntime) { failRuntime = false; return route.abort('failed'); }
      if (rejectRuntime) {
        rejectRuntime = false;
        return route.fulfill({ status: 422, contentType: 'application/problem+json', body: JSON.stringify({
          type: 'about:blank', title: 'Invalid integration origin', status: 422, code: 'INVALID_INTEGRATION_ORIGIN',
          detail: 'Origin must not contain a path', request_id: secretId, retryable: false, safe_next_actions: [], field_errors: [],
        }) });
      }
      if (conflictRuntime) {
        conflictRuntime = false;
        runtime.configuration.name = 'Runtime external'; runtime.revision = (BigInt(runtime.revision) + 1n).toString();
        return route.fulfill({ status: 409, contentType: 'application/problem+json', body: JSON.stringify({
          type: 'about:blank', title: 'Revision conflict', status: 409, code: 'REVISION_CONFLICT',
          detail: 'Configuration changed elsewhere', request_id: secretId, retryable: false,
          current_revision: runtime.revision, safe_next_actions: ['RELOAD'], field_errors: [],
        }) });
      }
      expect(body.expected_revision).toBe(runtime.revision);
      runtime.configuration = body.configuration; runtime.revision = (BigInt(runtime.revision) + 1n).toString();
      if (body.ca_certificate_ref) runtime.ca_configured = true;
      return reply({ schema_version: 1, replayed: false, resource: runtime });
    }
    if (path === '/api/v2/integrations/downstreams') return reply({ schema_version: 1, items: [downstream], next_cursor: null });
    if (path === `/api/v2/integrations/downstreams/${downstream.id}`) {
      const body: Schema['DownstreamUpdate'] = request.postDataJSON();
      writes.push({ kind: 'downstream', key: request.headers()['idempotency-key'], body });
      expect(body.expected_revision).toBe(downstream.revision);
      downstream.configuration = body.configuration; downstream.revision = (BigInt(downstream.revision) + 1n).toString();
      return reply({ schema_version: 1, replayed: false, resource: downstream });
    }
    if (path === '/api/v2/data/sources') return reply({ schema_version: 1, items: [source], next_cursor: null });
    if (path === `/api/v2/data/sources/${source.id}`) {
      if (request.method() === 'GET') return reply(source);
      const body: Schema['DataSourceUpdate'] = request.postDataJSON();
      writes.push({ kind: 'source', key: request.headers()['idempotency-key'], body });
      expect(body.expected_revision).toBe(source.revision);
      source.name = body.name; source.enabled = body.enabled; source.revision = (BigInt(source.revision) + 1n).toString();
      return reply({ schema_version: 1, replayed: false, resource: source });
    }
    if (path === `/api/v2/data/sources/${source.id}/grants`) return reply({ schema_version: 1, items: [], next_cursor: null });
    return route.abort('blockedbyclient');
  });
  await page.goto('/');
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  return { runtime, downstream, source, secretId, writes, failNextRuntime: () => { failRuntime = true; },
    rejectNextRuntime: () => { rejectRuntime = true; }, conflictNextRuntime: () => { conflictRuntime = true; },
    holdNextRuntime: () => {
      let release!: () => void;
      holdRuntime = new Promise<void>(resolve => { release = resolve; });
      return release;
    } };
}

test('existing Runtime, Downstream and data source edits save without a Save action or navigation prompt', async ({ page }) => {
  const { runtime, downstream, source, secretId, writes, failNextRuntime, holdNextRuntime } = await setup(page);
  await page.getByRole('tab', { name: '集成', exact: true }).click();
  await page.getByRole('button', { name: '登记 Runtime', exact: true }).click();
  const createDialog = page.getByRole('dialog', { name: '登记 Runtime' });
  await page.waitForTimeout(600); // A new record must remain explicit beyond the autosave debounce.
  expect(writes).toHaveLength(0);
  await expect(createDialog.getByText('请先登记本次必需的凭据和证书。')).toHaveCount(0);
  await createDialog.getByRole('button', { name: '返回' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const runtimeDialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  failNextRuntime();
  await runtimeDialog.getByRole('textbox', { name: '名称' }).fill('Runtime B');
  await expect(runtimeDialog.getByRole('button', { name: '重试' })).toBeVisible();
  await runtimeDialog.getByRole('button', { name: '重试' }).click();
  await expect.poll(() => runtime.configuration.name).toBe('Runtime B');
  expect(writes.filter(write => write.kind === 'runtime').map(write => write.key)).toEqual([writes[0]?.key, writes[0]?.key]);
  const release = holdNextRuntime();
  await runtimeDialog.getByRole('textbox', { name: 'Runtime HTTPS origin' }).fill('https://runtime-b.example');
  await expect.poll(() => writes.filter(write => write.kind === 'runtime').length).toBe(3);
  await runtimeDialog.getByRole('switch', { name: '允许新任务' }).click();
  release();
  await expect.poll(() => runtime.configuration.enabled).toBe(false);
  expect(writes.filter(write => write.kind === 'runtime').map(write => (write.body as Schema['RuntimeUpdate']).expected_revision)).toEqual(['1', '1', '2', '3']);
  const releaseCredential = holdNextRuntime();
  await runtimeDialog.getByRole('textbox', { name: '新的 RUNTIME 凭据' }).fill('a'.repeat(32));
  await runtimeDialog.getByRole('button', { name: '登记凭据' }).click();
  await expect.poll(() => writes.filter(write => write.kind === 'runtime').length).toBe(5);
  await expect(runtimeDialog.getByRole('button', { name: '放弃本次绑定' })).toBeDisabled();
  releaseCredential();
  await expect.poll(() => runtime.revision).toBe('5');
  expect((writes.filter(write => write.kind === 'runtime').at(-1)?.body as Schema['RuntimeUpdate']).credential_ref).toBe(secretId);
  await runtimeDialog.getByRole('textbox', { name: '名称' }).fill('Runtime C');
  await expect.poll(() => runtime.configuration.name).toBe('Runtime C');
  expect((writes.filter(write => write.kind === 'runtime').at(-1)?.body as Schema['RuntimeUpdate']).credential_ref).toBeNull();
  const releaseClose = holdNextRuntime();
  await runtimeDialog.getByRole('textbox', { name: '名称' }).fill('Runtime D');
  await expect.poll(() => writes.filter(write => write.kind === 'runtime').length).toBe(7);
  await runtimeDialog.getByRole('switch', { name: '允许新任务' }).click();
  await runtimeDialog.getByRole('button', { name: '关闭' }).click();
  await expect(runtimeDialog).toHaveCount(0);
  releaseClose();
  await expect.poll(() => runtime.configuration.enabled).toBe(true);
  expect(runtime.configuration.name).toBe('Runtime D');

  await page.getByRole('tab', { name: '目标交付下游' }).click();
  await page.getByRole('button', { name: '修改下游' }).click();
  const downstreamDialog = page.getByRole('dialog', { name: '修改目标交付下游' });
  await downstreamDialog.getByRole('switch', { name: '允许未来目标交付' }).click();
  await expect.poll(() => downstream.configuration.enabled).toBe(false);
  await downstreamDialog.getByRole('button', { name: '关闭' }).click();
  await expect(downstreamDialog).toHaveCount(0);

  await page.getByRole('tab', { name: '数据', exact: true }).click();
  await page.getByRole('button', { name: '查看许可与版本登记' }).click();
  await page.getByRole('button', { name: '修改数据源' }).click();
  const sourceDialog = page.getByRole('dialog', { name: '修改数据源显示与启用状态' });
  await sourceDialog.getByRole('textbox', { name: '数据源名称' }).fill('Source B');
  await sourceDialog.getByRole('button', { name: '关闭' }).click();
  await expect(sourceDialog).toHaveCount(0);
  await expect.poll(() => source.name).toBe('Source B');
  expect(writes.map(write => write.kind)).toEqual(['runtime', 'runtime', 'runtime', 'runtime', 'runtime', 'runtime', 'runtime', 'runtime', 'downstream', 'source']);
  await page.getByRole('tab', { name: '鉴权管理', exact: true }).click();
  await page.getByRole('textbox', { name: '当前密码' }).fill('test-only-password');
  await page.getByRole('tab', { name: 'Codex', exact: true }).click();
  await page.getByRole('menuitem', { name: '研究', exact: true }).click();
  await expect(page.getByText('请先保存或取消更改')).toHaveCount(0);
});

test('closing and reopening an editor keeps writes ordered and uncertain retries identical', async ({ page }) => {
  const { runtime, writes, holdNextRuntime, failNextRuntime } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  let dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  const release = holdNextRuntime();
  await dialog.getByRole('textbox', { name: '名称' }).fill('Runtime B');
  await expect.poll(() => writes.length).toBe(1);
  await dialog.getByRole('button', { name: '关闭' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  await expect(dialog.getByRole('textbox', { name: '名称' })).toHaveValue('Runtime B');
  await dialog.getByRole('textbox', { name: '名称' }).fill('Runtime C');
  await page.waitForTimeout(600);
  expect(writes).toHaveLength(1);
  release();
  await expect.poll(() => runtime.configuration.name).toBe('Runtime C');
  expect(writes.map(write => (write.body as Schema['RuntimeUpdate']).expected_revision)).toEqual(['1', '2']);
  failNextRuntime();
  await dialog.getByRole('textbox', { name: '名称' }).fill('Runtime D');
  await dialog.getByRole('button', { name: '关闭' }).click();
  await expect.poll(() => writes.length).toBe(3);
  await page.getByRole('button', { name: '修改配置' }).click();
  dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  await expect(dialog.getByRole('button', { name: '重试' })).toBeVisible();
  await dialog.getByRole('button', { name: '重试' }).click();
  await expect.poll(() => runtime.configuration.name).toBe('Runtime D');
  expect(writes[3]?.key).toBe(writes[2]?.key);
  expect(writes[3]?.body).toEqual(writes[2]?.body);
});

test('a corrected server-rejected setting saves with a new request', async ({ page }) => {
  const { runtime, writes, rejectNextRuntime } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  rejectNextRuntime();
  await dialog.getByRole('textbox', { name: 'Runtime HTTPS origin' }).fill('https://runtime.example/path');
  await expect(dialog.getByText('Origin must not contain a path')).toBeVisible();
  await dialog.getByRole('textbox', { name: 'Runtime HTTPS origin' }).fill('https://runtime-b.example');
  await expect.poll(() => runtime.configuration.endpoint).toBe('https://runtime-b.example');
  expect(writes).toHaveLength(2);
  expect(writes[1]?.key).not.toBe(writes[0]?.key);
});

test('a revision conflict retains the edit on the canonical revision until retry', async ({ page }) => {
  const { runtime, writes, conflictNextRuntime } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  conflictNextRuntime();
  await dialog.getByRole('textbox', { name: '名称' }).fill('Runtime local');
  await expect(dialog.getByText('配置在其他地方已更改，已保留本次编辑，请检查后重试')).toBeVisible();
  await expect(dialog.getByRole('textbox', { name: '名称' })).toHaveValue('Runtime local');
  await dialog.getByRole('button', { name: '重试' }).click();
  await expect.poll(() => runtime.configuration.name).toBe('Runtime local');
  expect(writes.map(write => (write.body as Schema['RuntimeUpdate']).expected_revision)).toEqual(['1', '2']);
});

test('a credential reference survives a conflict after its editor closes', async ({ page }) => {
  const { runtime, secretId, writes, conflictNextRuntime, holdNextRuntime } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  let dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  conflictNextRuntime();
  const release = holdNextRuntime();
  await dialog.getByRole('textbox', { name: '新的 RUNTIME 凭据' }).fill('c'.repeat(32));
  await dialog.getByRole('button', { name: '登记凭据' }).click();
  await expect.poll(() => writes.length).toBe(1);
  await dialog.getByRole('button', { name: '关闭' }).click();
  release();
  await expect.poll(() => runtime.revision).toBe('2');
  await page.getByRole('button', { name: '修改配置' }).click();
  dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  await expect(dialog.getByText(secretId)).toBeVisible();
  await expect(dialog.getByRole('button', { name: '重试' })).toBeVisible();
  await dialog.getByRole('button', { name: '重试' }).click();
  await expect.poll(() => runtime.revision).toBe('3');
  expect((writes[1]?.body as Schema['RuntimeUpdate']).credential_ref).toBe(secretId);
  expect((writes[1]?.body as Schema['RuntimeUpdate']).expected_revision).toBe('2');
});

test('an offline follow-up write pauses until the editor reconnects', async ({ page }) => {
  const { runtime, writes, holdNextRuntime } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  const release = holdNextRuntime();
  await dialog.getByRole('textbox', { name: '名称' }).fill('Runtime B');
  await expect.poll(() => writes.length).toBe(1);
  await dialog.getByRole('switch', { name: '允许新任务' }).click();
  await dialog.getByRole('button', { name: '关闭' }).click();
  await page.evaluate(() => Object.defineProperty(navigator, 'onLine', { configurable: true, get: () => false }));
  release();
  await expect.poll(() => runtime.revision).toBe('2');
  await page.waitForTimeout(200);
  expect(writes).toHaveLength(1);
  await page.evaluate(() => { Object.defineProperty(navigator, 'onLine', { configurable: true, get: () => true }); window.dispatchEvent(new Event('online')); });
  await page.getByRole('button', { name: '修改配置' }).click();
  await expect.poll(() => runtime.configuration.enabled).toBe(false);
  expect(writes).toHaveLength(2);
});

test('a newly bound Runtime CA remains configured for later autosaves', async ({ page }) => {
  const { runtime, writes } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  await dialog.getByRole('combobox', { name: 'TLS 信任方式' }).click();
  await page.getByText('指定 CA 证书', { exact: true }).last().click();
  await dialog.getByRole('textbox', { name: '新的 CA PEM 证书' }).fill('TEST CA');
  await dialog.getByRole('button', { name: '登记证书' }).click();
  await expect.poll(() => runtime.ca_configured).toBe(true);
  await dialog.getByRole('textbox', { name: '名称' }).fill('Runtime with CA');
  await expect.poll(() => runtime.configuration.name).toBe('Runtime with CA');
  expect(writes.filter(write => write.kind === 'runtime')).toHaveLength(2);
});

test('closing with an incomplete setting does not send it', async ({ page }) => {
  const { writes } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  await dialog.getByRole('textbox', { name: '名称' }).fill('');
  await dialog.getByRole('button', { name: '关闭' }).click();
  await expect(dialog).toHaveCount(0);
  await page.waitForTimeout(600);
  expect(writes).toHaveLength(0);
  await page.getByRole('button', { name: '修改配置' }).click();
  await expect(dialog.getByRole('textbox', { name: '名称' })).toHaveValue('Runtime A');
});

test('a pending import survives navigation and retries with the same identity', async ({ page }) => {
  await setup(page);
  const exportRef = '01990000-0000-7000-8000-000000000021';
  const report: Schema['HistoricalImportReportV1'] = { schema_version: 1, id: '01990000-0000-7000-8000-000000000022',
    export_ref: exportRef, source_installation_id: '01990000-0000-7000-8000-000000000023', dry_run: true,
    projected_rows: '0', new_rows: '0', existing_rows: '0', checked_relationships: '0',
    unverified_relationships: [], manual_review_required: false };
  const attempts: { key: string | undefined; body: unknown }[] = [];
  let release!: () => void;
  const hold = new Promise<void>(resolve => { release = resolve; });
  await page.route('**/api/v2/migrations/**', async route => {
    const request = route.request();
    if (new URL(request.url()).pathname === '/api/v2/migrations/reports')
      return route.fulfill({ contentType: 'application/json', body: JSON.stringify({ schema_version: 1, items: [], next_cursor: null }) });
    if (new URL(request.url()).pathname === '/api/v2/migrations/import') {
      attempts.push({ key: request.headers()['idempotency-key'], body: request.postDataJSON() });
      if (attempts.length === 1) { await hold; return route.abort('failed'); }
      return route.fulfill({ status: 202, contentType: 'application/json', body: JSON.stringify({ schema_version: 1, replayed: true, resource: report }) });
    }
    return route.abort('blockedbyclient');
  });
  await page.getByRole('tab', { name: '迁移' }).click();
  await page.getByRole('button', { name: '导入历史投影' }).click();
  const dialog = page.getByRole('dialog', { name: '导入历史投影' });
  await dialog.getByRole('textbox', { name: '已登记的导出编号' }).fill(exportRef);
  await dialog.getByRole('button', { name: '提交导入请求' }).click();
  await expect.poll(() => attempts.length).toBe(1);
  await dialog.getByRole('button', { name: '返回' }).click();
  await page.getByRole('tab', { name: '鉴权管理' }).click();
  release();
  await page.getByRole('menuitem', { name: '研究', exact: true }).click();
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  await page.getByRole('tab', { name: '迁移' }).click();
  const resumed = page.getByRole('dialog', { name: '导入历史投影' });
  await expect(resumed.getByRole('button', { name: '重试同一导入请求' })).toBeVisible();
  await resumed.getByRole('button', { name: '重试同一导入请求' }).click();
  await expect(resumed.getByText(report.id)).toBeVisible();
  expect(attempts[1]).toEqual(attempts[0]);
});

test('a completed import receipt remains visible when the report list fails', async ({ page }) => {
  await setup(page);
  const exportRef = '01990000-0000-7000-8000-000000000051';
  const report: Schema['HistoricalImportReportV1'] = { schema_version: 1, id: '01990000-0000-7000-8000-000000000052',
    export_ref: exportRef, source_installation_id: '01990000-0000-7000-8000-000000000053', dry_run: true,
    projected_rows: '0', new_rows: '0', existing_rows: '0', checked_relationships: '0',
    unverified_relationships: [], manual_review_required: false };
  let release!: () => void;
  const hold = new Promise<void>(resolve => { release = resolve; });
  let attempts = 0;
  await page.route('**/api/v2/migrations/**', async route => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/api/v2/migrations/reports') return route.abort('failed');
    if (path === '/api/v2/migrations/import') {
      attempts++; await hold;
      return route.fulfill({ status: 202, contentType: 'application/json', body: JSON.stringify({
        schema_version: 1, replayed: false, resource: report,
      }) });
    }
    return route.abort('blockedbyclient');
  });
  await page.getByRole('tab', { name: '迁移' }).click();
  await page.getByRole('button', { name: '导入历史投影' }).click();
  const dialog = page.getByRole('dialog', { name: '导入历史投影' });
  await dialog.getByRole('textbox', { name: '已登记的导出编号' }).fill(exportRef);
  await dialog.getByRole('button', { name: '提交导入请求' }).click();
  await expect.poll(() => attempts).toBe(1);
  await dialog.getByRole('button', { name: '返回' }).click();
  await page.getByRole('tab', { name: '鉴权管理' }).click();
  release();
  await page.getByRole('tab', { name: '迁移' }).click();
  await expect(page.getByText('导入回执已保存')).toBeVisible();
  await expect(page.getByText(report.id)).toBeVisible();
  await page.getByRole('button', { name: '关闭回执' }).click();
  await expect(page.getByText(report.id)).toHaveCount(0);
});

test('a rejected import remains visible with its draft after navigation', async ({ page }) => {
  await setup(page);
  const exportRef = '01990000-0000-7000-8000-000000000031';
  let release!: () => void;
  const hold = new Promise<void>(resolve => { release = resolve; });
  await page.route('**/api/v2/migrations/**', async route => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/api/v2/migrations/reports')
      return route.fulfill({ contentType: 'application/json', body: JSON.stringify({ schema_version: 1, items: [], next_cursor: null }) });
    if (path === '/api/v2/migrations/import') {
      await hold;
      return route.fulfill({ status: 422, contentType: 'application/problem+json', body: JSON.stringify({
        type: 'about:blank', title: 'Invalid export', status: 422, code: 'INVALID_EXPORT', detail: 'Export cannot be imported',
        request_id: '01990000-0000-7000-8000-000000000032', retryable: false, safe_next_actions: [], field_errors: [],
      }) });
    }
    return route.abort('blockedbyclient');
  });
  await page.getByRole('tab', { name: '迁移' }).click();
  await page.getByRole('button', { name: '导入历史投影' }).click();
  const dialog = page.getByRole('dialog', { name: '导入历史投影' });
  await dialog.getByRole('textbox', { name: '已登记的导出编号' }).fill(exportRef);
  const response = page.waitForResponse(res => new URL(res.url()).pathname === '/api/v2/migrations/import' && res.status() === 422);
  await dialog.getByRole('button', { name: '提交导入请求' }).click();
  await dialog.getByRole('button', { name: '返回' }).click();
  await page.getByRole('menuitem', { name: '研究', exact: true }).click();
  release(); await response;
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  await page.getByRole('tab', { name: '迁移' }).click();
  const resumed = page.getByRole('dialog', { name: '导入历史投影' });
  await expect(resumed.getByText('Export cannot be imported')).toBeVisible();
  await expect(resumed.getByRole('textbox', { name: '已登记的导出编号' })).toHaveValue(exportRef);
  await expect(resumed.getByRole('textbox', { name: '已登记的导出编号' })).toBeEnabled();
});

test('a pending password change retains its result across settings navigation', async ({ page }) => {
  await setup(page);
  let release!: () => void;
  const hold = new Promise<void>(resolve => { release = resolve; });
  let attempts = 0;
  await page.route('**/api/v2/auth/password', async route => {
    attempts++;
    await hold;
    return route.fulfill({ status: 422, contentType: 'application/problem+json', body: JSON.stringify({
      type: 'about:blank', title: 'Incorrect password', status: 422, code: 'INCORRECT_PASSWORD',
      detail: 'Current password is incorrect', request_id: '01990000-0000-7000-8000-000000000041',
      retryable: false, safe_next_actions: [], field_errors: [],
    }) });
  });
  await page.getByRole('tab', { name: '鉴权管理' }).click();
  await page.getByRole('textbox', { name: '当前密码' }).fill('incorrect-current');
  await page.getByRole('textbox', { name: /^\* 新密码$/ }).fill('new-password-123');
  await page.getByRole('textbox', { name: '确认新密码' }).fill('new-password-123');
  await page.getByRole('button', { name: '修改密码并重新登录' }).click();
  await expect.poll(() => attempts).toBe(1);
  await page.getByRole('tab', { name: 'Codex' }).click();
  await expect.poll(() => page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(true);
  release();
  await page.getByRole('tab', { name: '鉴权管理' }).click();
  await expect(page.getByText('Current password is incorrect')).toBeVisible();
  await expect(page.getByRole('button', { name: '修改密码并重新登录' })).toBeEnabled();
  await expect.poll(() => page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(false);
  expect(attempts).toBe(1);
});
