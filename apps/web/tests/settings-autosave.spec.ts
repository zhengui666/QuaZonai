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
  let holdRuntime: Promise<void> | undefined;
  await page.route('**/api/**', async route => {
    const request = route.request(); const path = new URL(request.url()).pathname;
    const reply = (json: unknown, status = 200) => route.fulfill({ status, body: JSON.stringify(json), contentType: 'application/json' });
    if (path === '/api/v2/auth/status') return reply({ schema_version: 1, setup_required: false });
    if (path === '/api/v2/auth/session') return reply({ schema_version: 1, authenticated_at: now, expires_at: new Date(Date.now() + 43_200_000).toISOString() });
    if (path === '/api/v2/auth/cli/devices') return reply([]);
    if (path === '/api/v2/projects') return reply({ schema_version: 1, items: [], next_cursor: null });
    if (path === '/api/v2/settings/codex') return reply({ schema_version: 1, items: [], next_cursor: null });
    if (path === '/api/v2/settings/credentials') return reply({ schema_version: 1, replayed: false,
      resource: { id: secretId, purpose: 'RUNTIME', label: 'RUNTIME service credential', created_at: now } }, 201);
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
      if (failRuntime) { failRuntime = false; return route.abort('failed'); }
      if (holdRuntime) { await holdRuntime; holdRuntime = undefined; }
      expect(body.expected_revision).toBe(runtime.revision);
      runtime.configuration = body.configuration; runtime.revision = (BigInt(runtime.revision) + 1n).toString();
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
  await runtimeDialog.getByRole('textbox', { name: '新的 RUNTIME 凭据' }).fill('a'.repeat(32));
  await runtimeDialog.getByRole('button', { name: '登记凭据' }).click();
  await expect.poll(() => writes.filter(write => write.kind === 'runtime').length).toBe(5);
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
