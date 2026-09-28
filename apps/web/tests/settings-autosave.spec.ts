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
  const caId = '01990000-0000-7000-8000-000000000015';
  let failRuntime = false;
  let rejectRuntime = false;
  let conflictRuntime = false;
  let matchConflictRuntime = false;
  let holdRuntime: Promise<void> | undefined;
  let holdSource: Promise<void> | undefined;
  let holdSecret: Promise<void> | undefined;
  let failSecret = false;
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
      if (holdSecret) { await holdSecret; holdSecret = undefined; }
      if (failSecret) { failSecret = false; return route.abort('failed'); }
      return reply({ schema_version: 1, replayed: false,
        resource: { id: body.intent.purpose === 'TLS_CA' ? caId : secretId,
          purpose: body.intent.purpose, label: body.intent.label, created_at: now } }, 201);
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
        runtime.configuration.name = matchConflictRuntime ? body.configuration.name : 'Runtime external';
        matchConflictRuntime = false; runtime.revision = (BigInt(runtime.revision) + 1n).toString();
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
      if (holdSource) { await holdSource; holdSource = undefined; }
      expect(body.expected_revision).toBe(source.revision);
      source.name = body.name; source.enabled = body.enabled; source.revision = (BigInt(source.revision) + 1n).toString();
      return reply({ schema_version: 1, replayed: false, resource: source });
    }
    if (path === `/api/v2/data/sources/${source.id}/grants`) return reply({ schema_version: 1, items: [], next_cursor: null });
    return route.abort('blockedbyclient');
  });
  await page.goto('/');
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  return { runtime, downstream, source, secretId, caId, writes, failNextRuntime: () => { failRuntime = true; },
    failNextSecret: () => { failSecret = true; },
    rejectNextRuntime: () => { rejectRuntime = true; }, conflictNextRuntime: () => { conflictRuntime = true; },
    matchNextRuntime: () => { conflictRuntime = true; matchConflictRuntime = true; },
    holdNextRuntime: () => {
      let release!: () => void;
      holdRuntime = new Promise<void>(resolve => { release = resolve; });
      return release;
    }, holdNextSource: () => {
      let release!: () => void;
      holdSource = new Promise<void>(resolve => { release = resolve; });
      return release;
    }, holdNextSecret: () => {
      let release!: () => void;
      holdSecret = new Promise<void>(resolve => { release = resolve; });
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
  await expect.poll(() => page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(false);
  await runtimeDialog.getByRole('textbox', { name: '名称' }).fill('Runtime C');
  await expect.poll(() => runtime.configuration.name).toBe('Runtime C');
  expect((writes.filter(write => write.kind === 'runtime').at(-1)?.body as Schema['RuntimeUpdate']).credential_ref).toBeNull();
  const releaseClose = holdNextRuntime();
  await runtimeDialog.getByRole('textbox', { name: '名称' }).fill('Runtime D');
  await expect.poll(() => writes.filter(write => write.kind === 'runtime').length).toBe(7);
  await runtimeDialog.getByRole('switch', { name: '允许新任务' }).click();
  await runtimeDialog.getByRole('button', { name: '关闭' }).click();
  await expect(runtimeDialog).toHaveCount(0);
  await expect(page.getByRole('button', { name: '执行原生探测' })).toBeDisabled();
  releaseClose();
  await expect.poll(() => runtime.configuration.enabled).toBe(true);
  expect(runtime.configuration.name).toBe('Runtime D');
  await expect(page.getByRole('button', { name: '执行原生探测' })).toBeEnabled();

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

test('uncertain Runtime creation keeps its command and credential across Settings navigation', async ({ page }) => {
  const { runtime, secretId } = await setup(page);
  const requests: { key: string | undefined; body: Schema['RuntimeCreate'] }[] = [];
  await page.route(/\/api\/v2\/integrations\/runtimes(?:\?|$)/, async route => {
    const request = route.request();
    if (request.method() === 'GET') return route.fallback();
    const body: Schema['RuntimeCreate'] = request.postDataJSON();
    requests.push({ key: request.headers()['idempotency-key'], body });
    if (requests.length === 1) return route.abort('failed');
    return route.fulfill({ status: 201, contentType: 'application/json', body: JSON.stringify({
      schema_version: 1, replayed: true, resource: { ...runtime, configuration: body.configuration },
    }) });
  });
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '登记 Runtime', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '登记 Runtime' });
  await dialog.getByRole('textbox', { name: '名称' }).fill('Runtime B');
  await dialog.getByRole('textbox', { name: 'Runtime HTTPS origin' }).fill('https://runtime-b.example');
  await dialog.getByRole('textbox', { name: '新的 RUNTIME 凭据' }).fill('a'.repeat(32));
  await dialog.getByRole('button', { name: '登记凭据' }).click();
  await expect(dialog.getByText(secretId)).toBeVisible();
  await dialog.getByRole('button', { name: '保存配置' }).click();
  await expect.poll(() => requests.length).toBe(1);
  await expect(dialog.getByRole('button', { name: '重试当前操作' })).toBeVisible();
  await dialog.getByRole('button', { name: '返回' }).click();
  await page.getByRole('tab', { name: '数据', exact: true }).click();
  await expect(page.getByText('Runtime 登记结果待确认')).toBeVisible();
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '登记 Runtime', exact: true }).click();
  const reopened = page.getByRole('dialog', { name: '登记 Runtime' });
  await expect(reopened.getByRole('button', { name: '重试当前操作' })).toBeVisible();
  await reopened.getByRole('button', { name: '重试当前操作' }).click();
  await expect.poll(() => requests.length).toBe(2);
  expect(requests[1]).toEqual(requests[0]);
  await expect(reopened).toHaveCount(0);
  await expect(page.getByText('Runtime 登记回执已确认')).toBeVisible();
  await expect(page.getByText(runtime.id)).toBeVisible();
  await page.getByRole('button', { name: '关闭回执' }).click();
  await expect.poll(() => page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(false);
});

test('an uncertain Runtime credential binding cannot be abandoned before reconciliation', async ({ page }) => {
  const { runtime, secretId, writes, failNextRuntime } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  failNextRuntime();
  await dialog.getByRole('textbox', { name: '新的 RUNTIME 凭据' }).fill('a'.repeat(32));
  await dialog.getByRole('button', { name: '登记凭据' }).click();
  await expect(dialog.getByRole('button', { name: '重试' })).toBeVisible();
  await expect(dialog.getByRole('button', { name: '放弃本次绑定' })).toBeDisabled();
  await dialog.getByRole('button', { name: '重试' }).click();
  await expect.poll(() => runtime.revision).toBe('2');
  expect(writes.map(write => write.key)).toEqual([writes[0]?.key, writes[0]?.key]);
  expect((writes[1]?.body as Schema['RuntimeUpdate']).credential_ref).toBe(secretId);
  await expect(dialog.getByRole('button', { name: '放弃本次绑定' })).toHaveCount(0);
});

test('uncertain Downstream creation keeps its command and credential across Settings navigation', async ({ page }) => {
  const { downstream, secretId } = await setup(page);
  const requests: { key: string | undefined; body: Schema['DownstreamCreate'] }[] = [];
  await page.route(/\/api\/v2\/integrations\/downstreams(?:\?|$)/, async route => {
    const request = route.request();
    if (request.method() === 'GET') return route.fallback();
    const body: Schema['DownstreamCreate'] = request.postDataJSON();
    requests.push({ key: request.headers()['idempotency-key'], body });
    if (requests.length === 1) return route.abort('failed');
    return route.fulfill({ status: 201, contentType: 'application/json', body: JSON.stringify({
      schema_version: 1, replayed: true, resource: { ...downstream, configuration: body.configuration },
    }) });
  });
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('tab', { name: '目标交付下游' }).click();
  await page.getByRole('button', { name: '登记目标交付下游' }).click();
  const dialog = page.getByRole('dialog', { name: '登记目标交付下游' });
  await dialog.getByRole('textbox', { name: '下游名称' }).fill('Downstream B');
  await dialog.getByRole('textbox', { name: '下游 HTTPS origin' }).fill('https://downstream-b.example');
  await dialog.getByRole('textbox', { name: '新的 DOWNSTREAM 凭据' }).fill('test-credential');
  await dialog.getByRole('button', { name: '登记凭据' }).click();
  await expect(dialog.getByText(secretId)).toBeVisible();
  await dialog.getByRole('button', { name: '保存下游配置' }).click();
  await expect.poll(() => requests.length).toBe(1);
  await expect(dialog.getByRole('button', { name: '重试当前操作' })).toBeVisible();
  await dialog.getByRole('button', { name: '返回' }).click();
  await page.getByRole('tab', { name: '数据', exact: true }).click();
  await expect(page.getByText('目标交付下游登记结果待确认')).toBeVisible();
  await page.getByRole('button', { name: '重试当前操作' }).click();
  await expect.poll(() => requests.length).toBe(2);
  expect(requests[1]).toEqual(requests[0]);
  await expect(page.getByText('目标交付下游登记回执已确认')).toBeVisible();
  await expect(page.getByText(downstream.id)).toBeVisible();
  await page.getByRole('button', { name: '关闭回执' }).click();
  await expect.poll(() => page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(false);
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

test('source dependent actions wait for a closed editor autosave to settle', async ({ page }) => {
  const { source, writes, holdNextSource } = await setup(page);
  await page.getByRole('tab', { name: '数据', exact: true }).click();
  await page.getByRole('button', { name: '查看许可与版本登记' }).click();
  await page.getByRole('button', { name: '修改数据源' }).click();
  const dialog = page.getByRole('dialog', { name: '修改数据源显示与启用状态' });
  const release = holdNextSource();
  await dialog.getByRole('textbox', { name: '数据源名称' }).fill('Source B');
  await expect.poll(() => writes.filter(write => write.kind === 'source').length).toBe(1);
  await dialog.getByRole('button', { name: '关闭' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('button', { name: '登记原生数据版本' })).toBeDisabled();
  await expect(page.getByRole('button', { name: '登记许可授权' })).toBeDisabled();
  release();
  await expect.poll(() => source.name).toBe('Source B');
  await expect(page.getByText('Source B').last()).toBeVisible();
  await expect(page.getByRole('button', { name: '登记原生数据版本' })).toBeEnabled();
  await expect(page.getByRole('button', { name: '登记许可授权' })).toBeEnabled();
});

test('data registration waits for the bound Runtime autosave across settings tabs', async ({ page }) => {
  const { runtime, writes, holdNextRuntime } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const release = holdNextRuntime();
  const dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  await dialog.getByRole('textbox', { name: '名称' }).fill('Runtime B');
  await expect.poll(() => writes.filter(write => write.kind === 'runtime').length).toBe(1);
  await dialog.getByRole('button', { name: '关闭' }).click();
  await expect(dialog).toHaveCount(0);
  await page.getByRole('tab', { name: '数据', exact: true }).click();
  await page.getByRole('button', { name: '查看许可与版本登记' }).click();
  await expect(page.getByRole('button', { name: '登记原生数据版本' })).toBeDisabled();
  release();
  await expect.poll(() => runtime.configuration.name).toBe('Runtime B');
  await expect(page.getByRole('button', { name: '登记原生数据版本' })).toBeEnabled();
  await expect(page.getByText('Runtime B')).toBeVisible();
});

test('new data source registration checks the selected Runtime after its autosave', async ({ page }) => {
  const { runtime, writes, holdNextRuntime } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const release = holdNextRuntime();
  const editor = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  await editor.getByRole('switch', { name: '允许新任务' }).click();
  await expect.poll(() => writes.filter(write => write.kind === 'runtime').length).toBe(1);
  await editor.getByRole('button', { name: '关闭' }).click();
  await page.getByRole('tab', { name: '数据', exact: true }).click();
  await page.getByRole('button', { name: '登记数据源' }).click();
  const dialog = page.getByRole('dialog', { name: '登记数据源' });
  await dialog.getByRole('textbox', { name: '数据源名称' }).fill('New source');
  await dialog.getByRole('combobox', { name: '选择已登记的 Runtime' }).click();
  await page.getByText('Runtime A', { exact: true }).last().click();
  await dialog.getByRole('textbox', { name: 'Runtime 原生目录登记键' }).fill('catalog/new-source');
  await expect(dialog.getByRole('button', { name: '登记', exact: true })).toBeDisabled();
  release();
  await expect.poll(() => runtime.configuration.enabled).toBe(false);
  await expect(dialog.getByText('所选 Runtime 已停用')).toBeVisible();
  await expect(dialog.getByRole('button', { name: '登记', exact: true })).toBeDisabled();
  expect(writes.filter(write => write.kind === 'source-create')).toHaveLength(0);
});

test('immutable portfolio editors wait for a Runtime autosave and read its new revision', async ({ page }) => {
  const { runtime, writes, holdNextRuntime } = await setup(page);
  const now = new Date().toISOString();
  const projectId = '01990000-0000-7000-8000-000000000071';
  const project: Schema['ProjectView'] = { id: projectId, root_lineage_id: projectId, name: 'Portfolio fixture',
    description: '', state: 'ACTIVE', revision: '1', created_by: 'OPERATOR', created_at: now, updated_at: now };
  await page.route(url => new URL(url).pathname === '/api/v2/projects', route => route.fulfill({ contentType: 'application/json', body: JSON.stringify({
    schema_version: 1, items: [project], next_cursor: null,
  }) }));
  await page.getByRole('tab', { name: '集成', exact: true }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const release = holdNextRuntime();
  await page.getByRole('dialog', { name: '修改 Runtime 配置' }).getByRole('textbox', { name: '名称' }).fill('Runtime B');
  await expect.poll(() => writes.filter(write => write.kind === 'runtime').length).toBe(1);
  await page.getByRole('dialog', { name: '修改 Runtime 配置' }).getByRole('button', { name: '关闭' }).click();
  await page.getByRole('menuitem', { name: '组合', exact: true }).click();
  await page.getByRole('combobox', { name: '选择组合所属项目' }).click();
  await page.getByText(`Portfolio fixture · ${projectId}`, { exact: true }).last().click();
  await page.getByRole('button', { name: '新建组合配置' }).click();
  const mandate = page.getByRole('dialog', { name: '新建不可变组合配置' });
  await mandate.getByRole('textbox', { name: 'Runtime 编号' }).fill(runtime.id);
  await expect(mandate.getByRole('button', { name: '保存不可变配置' })).toBeDisabled();
  await mandate.getByRole('button', { name: '取消' }).click();
  await page.getByRole('button', { name: '放弃修改' }).click();
  await page.getByRole('tab', { name: '执行假设' }).click();
  await page.getByRole('button', { name: '新建执行假设' }).click();
  const assumptions = page.getByRole('dialog', { name: '新建不可变执行假设' });
  await assumptions.getByRole('textbox', { name: 'Runtime 编号' }).fill(runtime.id);
  await expect(assumptions.getByRole('button', { name: '保存不可变执行假设' })).toBeDisabled();
  release();
  await expect(assumptions.getByText('Runtime 配置版本：2')).toBeVisible();
  await expect(assumptions.getByRole('button', { name: '保存不可变执行假设' })).toBeEnabled();
  await assumptions.getByRole('button', { name: '取消' }).click();
  await page.getByRole('button', { name: '放弃修改' }).click();
  await page.getByRole('tab', { name: '组合配置' }).click();
  await page.getByRole('button', { name: '新建组合配置' }).click();
  const again = page.getByRole('dialog', { name: '新建不可变组合配置' });
  await again.getByRole('textbox', { name: 'Runtime 编号' }).fill(runtime.id);
  await expect(again.getByText('Runtime 配置版本：2')).toBeVisible();
  await expect(again.getByRole('button', { name: '保存不可变配置' })).toBeEnabled();
});

test('an uncertain execution assumption retries the original Runtime revision', async ({ page }) => {
  const { runtime } = await setup(page);
  const now = new Date().toISOString();
  const projectId = '01990000-0000-7000-8000-000000000081';
  const project: Schema['ProjectView'] = { id: projectId, root_lineage_id: projectId, name: 'Portfolio fixture',
    description: '', state: 'ACTIVE', revision: '1', created_by: 'OPERATOR', created_at: now, updated_at: now };
  await page.route(url => new URL(url).pathname === '/api/v2/projects', route => route.fulfill({ contentType: 'application/json', body: JSON.stringify({
    schema_version: 1, items: [project], next_cursor: null,
  }) }));
  const requests: { key: string | undefined; body: Schema['ExecutionAssumptionsCreateV1'] }[] = [];
  await page.route('**/api/v2/execution-assumptions', route => {
    const request = route.request();
    requests.push({ key: request.headers()['idempotency-key'], body: request.postDataJSON() });
    return route.abort('failed');
  });
  await page.getByRole('menuitem', { name: '组合', exact: true }).click();
  await page.getByRole('combobox', { name: '选择组合所属项目' }).click();
  await page.getByText(`Portfolio fixture · ${projectId}`, { exact: true }).last().click();
  await page.getByRole('tab', { name: '执行假设' }).click();
  await page.getByRole('button', { name: '新建执行假设' }).click();
  const dialog = page.getByRole('dialog', { name: '新建不可变执行假设' });
  const id = '01990000-0000-7000-8000-000000000082';
  for (const [name, value] of [
    ['Runtime 编号', runtime.id], ['冻结输入编号', id], ['数据版本编号', id], ['结算规则引用', 'T+0'],
    ['基础币种', 'USD'], ['资本假设', '1000'], ['杠杆上限', '1'], ['敞口容差', '0.01'],
    ['限价成交概率（0 至 1）', '0.5'], ['滑点概率（0 至 1）', '0.1'], ['随机种子', '1'],
    ['基础延迟（纳秒）', '0'], ['插入附加延迟（纳秒）', '0'], ['更新附加延迟（纳秒）', '0'], ['取消附加延迟（纳秒）', '0'],
    ['资产 1 标识', 'BTCUSD'], ['资产 1 maker 费率', '0.001'], ['资产 1 taker 费率', '0.002'],
  ] as [string, string][]) await dialog.getByRole('textbox', { name }).fill(value);
  await dialog.getByRole('combobox', { name: '模拟账户模型' }).click();
  await page.getByText('现金', { exact: true }).last().click();
  await dialog.getByRole('spinbutton', { name: '快照间隔（毫秒）' }).fill('1000');
  await dialog.getByRole('button', { name: '保存不可变执行假设' }).click();
  await expect.poll(() => requests.length).toBe(1);
  expect(requests[0]?.body.expected_runtime_revision).toBe('1');
  runtime.revision = '2';
  await page.evaluate(async id => {
    const modulePath = '/src/settings-work.ts';
    const { setSettingsWork } = await import(modulePath);
    setSettingsWork(`autosave:runtime:${id}`, true); setSettingsWork(`autosave:runtime:${id}`, false);
  }, runtime.id);
  await expect(dialog.getByText('Runtime 配置版本：2')).toBeVisible();
  await dialog.getByRole('button', { name: '重试同一执行假设请求' }).click();
  await expect.poll(() => requests.length).toBe(2);
  expect(requests[1]).toEqual(requests[0]);
});

test('an uncertain portfolio mandate retries the original Runtime revision', async ({ page }) => {
  const { runtime } = await setup(page);
  const now = new Date().toISOString();
  const projectId = '01990000-0000-7000-8000-000000000091';
  const project: Schema['ProjectView'] = { id: projectId, root_lineage_id: projectId, name: 'Portfolio fixture',
    description: '', state: 'ACTIVE', revision: '1', created_by: 'OPERATOR', created_at: now, updated_at: now };
  await page.route(url => new URL(url).pathname === '/api/v2/projects', route => route.fulfill({ contentType: 'application/json', body: JSON.stringify({
    schema_version: 1, items: [project], next_cursor: null,
  }) }));
  const requests: { key: string | undefined; body: Schema['MandateCreateV1'] }[] = [];
  await page.route('**/api/v2/portfolio-mandates', route => {
    const request = route.request();
    requests.push({ key: request.headers()['idempotency-key'], body: request.postDataJSON() });
    return route.abort('failed');
  });
  await page.getByRole('menuitem', { name: '组合', exact: true }).click();
  await page.getByRole('combobox', { name: '选择组合所属项目' }).click();
  await page.getByText(`Portfolio fixture · ${projectId}`, { exact: true }).last().click();
  await page.getByRole('button', { name: '新建组合配置' }).click();
  const dialog = page.getByRole('dialog', { name: '新建不可变组合配置' });
  const id = '01990000-0000-7000-8000-000000000092';
  for (const [name, value] of [
    ['Runtime 编号', runtime.id], ['投资域版本编号', id], ['评估政策编号', id], ['执行假设编号', id],
    ['基础币种', 'USD'], ['资本假设', '1000'], ['费用依据产物编号', id],
  ] as [string, string][]) await dialog.getByRole('textbox', { name }).fill(value);
  await dialog.getByRole('button', { name: '保存不可变配置' }).click();
  await expect.poll(() => requests.length).toBe(1);
  expect(requests[0]?.body.expected_runtime_revision).toBe('1');
  runtime.revision = '2';
  await page.evaluate(async id => {
    const modulePath = '/src/settings-work.ts';
    const { setSettingsWork } = await import(modulePath);
    setSettingsWork(`autosave:runtime:${id}`, true); setSettingsWork(`autosave:runtime:${id}`, false);
  }, runtime.id);
  await expect(dialog.getByText('Runtime 配置版本：2')).toBeVisible();
  await dialog.getByRole('button', { name: '重试同一组合配置请求' }).click();
  await expect.poll(() => requests.length).toBe(2);
  expect(requests[1]).toEqual(requests[0]);
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

test('reverting a rejected edit clears its stale error without another write', async ({ page }) => {
  const { writes, rejectNextRuntime } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  rejectNextRuntime();
  await dialog.getByRole('textbox', { name: 'Runtime HTTPS origin' }).fill('https://runtime.example/path');
  await expect(dialog.getByText('Origin must not contain a path')).toBeVisible();
  await dialog.getByRole('textbox', { name: 'Runtime HTTPS origin' }).fill('https://runtime.example');
  await expect(dialog.getByText('Origin must not contain a path')).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: '重试' })).toHaveCount(0);
  expect(writes).toHaveLength(1);
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

test('an already applied edit clears its conflict instead of offering a no-op retry', async ({ page }) => {
  const { runtime, writes, matchNextRuntime } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  matchNextRuntime();
  await dialog.getByRole('textbox', { name: '名称' }).fill('Runtime B');
  await expect.poll(() => runtime.configuration.name).toBe('Runtime B');
  await expect(dialog.getByRole('button', { name: '重试' })).toHaveCount(0);
  expect(writes).toHaveLength(1);
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

test('an offline edit closed before debounce saves on reconnect without reopening', async ({ page }) => {
  const { runtime, writes } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  await dialog.getByRole('textbox', { name: '名称' }).fill('Runtime offline edit');
  await page.evaluate(() => { Object.defineProperty(navigator, 'onLine', { configurable: true, get: () => false }); window.dispatchEvent(new Event('offline')); });
  await dialog.getByRole('button', { name: '关闭' }).click();
  await expect(dialog).toHaveCount(0);
  await page.waitForTimeout(600);
  expect(writes).toHaveLength(0);
  await page.evaluate(() => { Object.defineProperty(navigator, 'onLine', { configurable: true, get: () => true }); window.dispatchEvent(new Event('online')); });
  await expect.poll(() => runtime.configuration.name).toBe('Runtime offline edit');
  expect(writes).toHaveLength(1);
});

test('a confirmed data source receipt survives a failed list refresh after retry', async ({ page }) => {
  const { source } = await setup(page);
  const requests: { key: string | undefined; body: Schema['DataSourceCreate'] }[] = [];
  let releaseRefresh!: () => void;
  const heldRefresh = new Promise<void>(resolve => { releaseRefresh = resolve; });
  let refreshes = 0;
  await page.route(/\/api\/v2\/data\/sources(?:\?|$)/, async route => {
    const request = route.request();
    if (request.method() === 'GET') {
      if (requests.length === 2) { refreshes++; await heldRefresh; return route.abort('failed'); }
      return route.fallback();
    }
    const body: Schema['DataSourceCreate'] = request.postDataJSON();
    requests.push({ key: request.headers()['idempotency-key'], body });
    if (requests.length === 1) return route.abort('failed');
    return route.fulfill({ status: 201, contentType: 'application/json', body: JSON.stringify({
      schema_version: 1, replayed: true, resource: { ...source, name: body.name, native_catalog_ref: body.native_catalog_ref },
    }) });
  });
  await page.getByRole('tab', { name: '数据', exact: true }).click();
  await page.getByRole('button', { name: '登记数据源' }).click();
  const dialog = page.getByRole('dialog', { name: '登记数据源' });
  await dialog.getByRole('textbox', { name: '数据源名称' }).fill('New source');
  await dialog.getByRole('combobox', { name: '选择已登记的 Runtime' }).click();
  await page.getByText('Runtime A', { exact: true }).last().click();
  await dialog.getByRole('textbox', { name: 'Runtime 原生目录登记键' }).fill('catalog/new-source');
  await dialog.getByRole('button', { name: '登记', exact: true }).click();
  await expect.poll(() => requests.length).toBe(1);
  await expect(dialog.getByRole('button', { name: '重试当前操作' })).toBeVisible();
  expect(await page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(true);
  await dialog.getByRole('button', { name: '返回' }).click();
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('tab', { name: '数据', exact: true }).click();
  await expect(page.getByText('数据源登记结果待确认')).toBeVisible();
  await page.getByRole('button', { name: '重试当前操作' }).click();
  await expect.poll(() => requests.length).toBe(2);
  expect(requests[1]).toEqual(requests[0]);
  await expect(page.getByText('数据源登记结果待确认')).toHaveCount(0);
  await expect.poll(() => refreshes).toBe(1);
  expect(await page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(true);
  releaseRefresh();
  await expect(page.getByText('数据源登记回执已确认')).toBeVisible();
  await expect(page.getByText(source.id)).toBeVisible();
  await page.getByRole('button', { name: '关闭回执' }).click();
  await expect.poll(() => page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(false);
});

test('a definite rejection after detached command retry stays visible until acknowledged', async ({ page }) => {
  const { source } = await setup(page);
  let attempts = 0;
  await page.route(/\/api\/v2\/data\/sources(?:\?|$)/, async route => {
    if (route.request().method() === 'GET') return route.fallback();
    attempts++;
    if (attempts === 1) return route.abort('failed');
    return route.fulfill({ status: 422, contentType: 'application/problem+json', body: JSON.stringify({
      type: 'about:blank', title: 'Invalid source', status: 422, code: 'INVALID_SOURCE', detail: 'Source rejected',
      request_id: source.id, retryable: false, safe_next_actions: [], field_errors: [],
    }) });
  });
  await page.getByRole('tab', { name: '数据', exact: true }).click();
  await page.getByRole('button', { name: '登记数据源' }).click();
  const dialog = page.getByRole('dialog', { name: '登记数据源' });
  await dialog.getByRole('textbox', { name: '数据源名称' }).fill('New source');
  await dialog.getByRole('combobox', { name: '选择已登记的 Runtime' }).click();
  await page.getByText('Runtime A', { exact: true }).last().click();
  await dialog.getByRole('textbox', { name: 'Runtime 原生目录登记键' }).fill('catalog/new-source');
  await dialog.getByRole('button', { name: '登记', exact: true }).click();
  await expect(dialog.getByRole('button', { name: '重试当前操作' })).toBeVisible();
  await dialog.getByRole('button', { name: '返回' }).click();
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('tab', { name: '数据', exact: true }).click();
  await page.getByRole('button', { name: '重试当前操作' }).click();
  await expect.poll(() => attempts).toBe(2);
  await expect(page.getByText('数据源登记未完成')).toBeVisible();
  await expect(page.getByText('Source rejected')).toBeVisible();
  expect(await page.evaluate(async () => {
    const path = '/src/settings-work.ts';
    return (await import(path)).settingsWorkActive();
  })).toBe(true);
  await page.getByRole('button', { name: '关闭错误' }).click();
  await expect(page.getByText('数据源登记未完成')).toHaveCount(0);
  await expect.poll(() => page.evaluate(async () => {
    const path = '/src/settings-work.ts';
    return (await import(path)).settingsWorkActive();
  })).toBe(false);
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

test('switching back to system trust abandons an unbound CA registration', async ({ page }) => {
  const { caId, writes } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  await dialog.getByRole('textbox', { name: '名称' }).fill('');
  await dialog.getByRole('combobox', { name: 'TLS 信任方式' }).click();
  await page.getByText('指定 CA 证书', { exact: true }).last().click();
  await dialog.getByRole('textbox', { name: '新的 CA PEM 证书' }).fill('TEST CA');
  await dialog.getByRole('button', { name: '登记证书' }).click();
  await expect(dialog.getByText(caId)).toBeVisible();
  await dialog.getByRole('combobox', { name: 'TLS 信任方式' }).click();
  await page.getByText('系统可信 CA', { exact: true }).last().click();
  await dialog.getByRole('textbox', { name: '名称' }).fill('Runtime A');
  await expect.poll(() => page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(false);
  expect(writes).toHaveLength(0);
  await dialog.getByRole('combobox', { name: 'TLS 信任方式' }).click();
  await page.getByText('指定 CA 证书', { exact: true }).last().click();
  await expect(dialog.getByText(caId)).toHaveCount(0);
  await expect(dialog.getByRole('textbox', { name: '新的 CA PEM 证书' })).toBeEnabled();
});

test('an unknown CA registration keeps its retry identity until reconciled', async ({ page }) => {
  const { caId, failNextSecret } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  await dialog.getByRole('combobox', { name: 'TLS 信任方式' }).click();
  await page.getByText('指定 CA 证书', { exact: true }).last().click();
  failNextSecret();
  await dialog.getByRole('textbox', { name: '新的 CA PEM 证书' }).fill('TEST CA');
  await dialog.getByRole('button', { name: '登记证书' }).click();
  await expect(dialog.getByRole('button', { name: '重试登记' })).toBeVisible();
  await expect(dialog.getByRole('combobox', { name: 'TLS 信任方式' })).toBeDisabled();
  await dialog.getByRole('button', { name: '关闭' }).click();
  await page.getByRole('tab', { name: '数据', exact: true }).click();
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const reopened = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  await expect(reopened.getByRole('button', { name: '重试登记' })).toBeVisible();
  await expect(reopened.getByRole('combobox', { name: 'TLS 信任方式' })).toBeDisabled();
  await reopened.getByRole('button', { name: '重试登记' }).click();
  await expect(reopened.getByText(caId)).toBeVisible();
  await expect(reopened.getByRole('combobox', { name: 'TLS 信任方式' })).toBeEnabled();
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

test('closing an invalid editor binds newly registered credential and CA to valid settings', async ({ page }) => {
  const { runtime, secretId, caId, writes } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '配置与原生探测' }).click();
  await page.getByRole('button', { name: '修改配置' }).click();
  const dialog = page.getByRole('dialog', { name: '修改 Runtime 配置' });
  await dialog.getByRole('textbox', { name: '名称' }).fill('');
  await dialog.getByRole('combobox', { name: 'TLS 信任方式' }).click();
  await page.getByText('指定 CA 证书', { exact: true }).last().click();
  await dialog.getByRole('textbox', { name: '新的 RUNTIME 凭据' }).fill('a'.repeat(32));
  await dialog.getByRole('button', { name: '登记凭据' }).click();
  await dialog.getByRole('textbox', { name: '新的 CA PEM 证书' }).fill('TEST CA');
  await dialog.getByRole('button', { name: '登记证书' }).click();
  await dialog.getByRole('button', { name: '关闭' }).click();
  await expect(dialog).toHaveCount(0);
  await expect.poll(() => runtime.revision).toBe('2');
  expect(writes).toHaveLength(1);
  expect(writes[0]?.body).toMatchObject({ credential_ref: secretId, ca_certificate_ref: caId,
    configuration: { name: 'Runtime A', tls_policy: 'PINNED_CA' } });
});

test('a one-time credential blocks PWA reload until it is bound or abandoned', async ({ page }) => {
  const { secretId, holdNextSecret } = await setup(page);
  await page.getByRole('tab', { name: '集成' }).click();
  await page.getByRole('button', { name: '登记 Runtime', exact: true }).click();
  const release = holdNextSecret();
  await page.getByRole('dialog', { name: '登记 Runtime' }).getByRole('textbox', { name: '新的 RUNTIME 凭据' }).fill('a'.repeat(32));
  await page.getByRole('button', { name: '登记凭据' }).click();
  await expect.poll(() => page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(true);
  release();
  const first = page.getByRole('dialog', { name: '登记 Runtime' });
  await expect(first.getByText(secretId)).toBeVisible();
  await first.getByRole('button', { name: '返回' }).click();
  await expect(first).toHaveCount(0);
  await page.getByRole('button', { name: '登记 Runtime', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '登记 Runtime' });
  await expect(dialog.getByText(secretId)).toBeVisible();
  await expect.poll(() => page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(true);
  await dialog.getByRole('button', { name: '放弃本次绑定' }).click();
  await expect.poll(() => page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(false);
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
  await expect.poll(() => page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(true);
  await page.getByRole('button', { name: '关闭回执' }).click();
  await expect(page.getByText(report.id)).toHaveCount(0);
  await expect.poll(() => page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    return (await import(modulePath)).settingsWorkActive();
  })).toBe(false);
});

test('error recovery asks before discarding a recoverable settings operation', async ({ page }) => {
  await setup(page);
  await page.evaluate(async () => {
    const reactPath = '/node_modules/.vite/deps/react.js';
    const domPath = '/node_modules/.vite/deps/react-dom_client.js';
    const boundaryPath = '/src/AppErrorBoundary.tsx';
    const themePath = '/src/theme.ts';
    const workPath = '/src/settings-work.ts';
    const React = (await import(reactPath)).default;
    const { createRoot } = (await import(domPath)).default;
    const { default: Boundary } = await import(boundaryPath);
    const { ColorThemeContext } = await import(themePath);
    (await import(workPath)).setSettingsWork('recovery-fixture', true);
    const host = document.createElement('div'); host.id = 'recovery-fixture'; document.body.append(host);
    const Crash = () => { throw new Error('synthetic render failure'); };
    createRoot(host, { onCaughtError: () => {} }).render(React.createElement(ColorThemeContext.Provider, { value: ['light', () => {}] },
      React.createElement(Boundary, null, React.createElement(Crash))));
  });
  await page.locator('#recovery-fixture').getByRole('button', { name: '重新加载页面' }).click();
  await expect(page.getByText('操作内容可能丢失')).toBeVisible();
  await expect(page.getByText('未保存内容将丢失')).toHaveCount(0);
  await page.getByRole('button', { name: '留在此页' }).click();
  await page.evaluate(async () => {
    const modulePath = '/src/settings-work.ts';
    (await import(modulePath)).setSettingsWork('recovery-fixture', false);
  });
  await expect(page.getByText('操作内容可能丢失')).toHaveCount(0);
});

test('error recovery keeps the reload warning for an active explicit operation', async ({ page }) => {
  await setup(page);
  await page.evaluate(async () => {
    const reactPath = '/node_modules/.vite/deps/react.js';
    const domPath = '/node_modules/.vite/deps/react-dom_client.js';
    const boundaryPath = '/src/AppErrorBoundary.tsx';
    const themePath = '/src/theme.ts';
    const uiPath = '/src/ui.tsx';
    const React = (await import(reactPath)).default;
    const { createRoot } = (await import(domPath)).default;
    const { default: Boundary } = await import(boundaryPath);
    const { ColorThemeContext } = await import(themePath);
    const { GuardProvider, useGuard } = await import(uiPath);
    const host = document.createElement('div'); host.id = 'guarded-recovery-fixture'; document.body.append(host);
    const Crash = () => {
      const [failed, setFailed] = React.useState(false);
      useGuard(true);
      React.useEffect(() => { setFailed(true); }, []);
      if (failed) throw new Error('synthetic render failure');
      return null;
    };
    createRoot(host, { onCaughtError: () => {} }).render(React.createElement(ColorThemeContext.Provider, { value: ['light', () => {}] },
      React.createElement(Boundary, null, React.createElement(GuardProvider, null, React.createElement(Crash)))));
  });
  await page.locator('#guarded-recovery-fixture').getByRole('button', { name: '重新加载页面' }).click();
  await expect(page.getByText('操作内容可能丢失')).toBeVisible();
  await expect(page.getByText('未保存内容将丢失')).toHaveCount(0);
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
