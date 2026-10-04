import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { Schema } from '../src/api';

// Synthetic browser interaction fixtures only; no actual license, Runtime,
// database mutation or native source admission is asserted by these tests.
async function setup(page: Page) {
  const now = new Date().toISOString();
  const source: Schema['DataSourceView'] = {
    id: '01990000-0000-7000-8000-000000000011', runtime_id: '01990000-0000-7000-8000-000000000012',
    name: '交互测试目录', native_catalog_ref: 'synthetic/catalog', provider_kind: 'NAUTILUS_CATALOG',
    revision: '9007199254740993', enabled: true, created_at: now, updated_at: now,
  };
  const grant: Schema['DataGrantView'] = {
    id: '01990000-0000-7000-8000-000000000013', source_id: source.id, version: '1',
    license_reference: 'fixture-terms', evidence_artifact_id: '01990000-0000-7000-8000-000000000014',
    allowed_uses: 'RESEARCH', valid_from: now, valid_until: null, created_at: now,
    license_state: 'ACTIVE', checked_at: now,
  };
  const runtime: Schema['RuntimeView'] = {
    id: source.runtime_id, protocol_version: 1, revision: '1', created_at: now, updated_at: now,
    credential_configured: true, ca_configured: false, last_capability_snapshot_artifact_id: null,
    configuration: { name: '交互测试 Runtime', endpoint: 'http://127.0.0.1:9999', development_http: true,
      tls_policy: 'SYSTEM_CA', allowed_capabilities: ['DATA_VALIDATE'], enabled: true },
  };
  const state = { writes: [] as { method: string; path: string; key: string | undefined; body: unknown }[],
    hold: undefined as Promise<void> | undefined };
  await page.route('**/api/**', async route => {
    const request = route.request(); const path = new URL(request.url()).pathname;
    const reply = (json: unknown) => route.fulfill({ json });
    const pageOf = (items: unknown[]) => reply({ schema_version: 1, items, next_cursor: null });
    if (!['GET', 'HEAD'].includes(request.method())) {
      state.writes.push({ method: request.method(), path, key: request.headers()['idempotency-key'], body: request.postDataJSON() });
      if (state.hold) await state.hold;
      // A missing response leaves the actual mutation outcome unknown.
      return route.abort('failed');
    }
    if (path === '/api/v2/auth/status') return reply({ schema_version: 1, setup_required: false });
    if (path === '/api/v2/auth/session') return reply({ schema_version: 1, authenticated_at: now,
      expires_at: new Date(Date.now() + 43_200_000).toISOString() });
    if (path === '/api/v2/data/sources') return pageOf([source]);
    if (path === `/api/v2/data/sources/${source.id}`) return reply(source);
    if (path === `/api/v2/data/sources/${source.id}/grants`) return pageOf([grant]);
    if (path === `/api/v2/integrations/runtimes/${runtime.id}`) return reply(runtime);
    if (path === '/api/v2/integrations/runtimes') return pageOf([runtime]);
    if (['/api/v2/projects', '/api/v2/settings/codex', '/api/v2/data/universes'].includes(path)) return pageOf([]);
    return route.abort('blockedbyclient');
  });
  await page.goto('/');
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  await page.getByRole('tab', { name: '数据', exact: true }).click();
  await page.getByRole('button', { name: '查看许可与版本登记', exact: true }).click();
  await expect(page.getByRole('button', { name: '登记原生数据版本', exact: true })).toBeEnabled();
  return state;
}

test('every data editor retains dirty input on Cancel and Escape until abandonment is confirmed', async ({ page }) => {
  const state = await setup(page);
  for (const [button, title, field, value] of [
    ['登记数据源', '登记数据源', '数据源名称', '未保存的来源'],
    ['登记许可授权', '授权数据用途：交互测试目录', '许可出处或合同编号', '未保存的许可'],
    ['撤销授权', '撤销这份数据授权？', '撤销说明', '未保存的原因'],
    ['登记原生数据版本', '读取并登记原生数据：交互测试目录', '原生存储版本', 'pending-version'],
  ]) {
    await page.getByRole('button', { name: button!, exact: true }).click();
    const editor = page.getByRole('dialog', { name: title!, exact: true });
    const input = editor.getByLabel(field!, { exact: true });
    await input.fill(value!);
    await editor.getByRole('button', { name: '返回', exact: true }).click();
    const confirm = page.getByRole('dialog', { name: '放弃未保存的更改？', exact: true });
    await expect(confirm).toBeVisible();
    await confirm.getByRole('button', { name: '继续编辑', exact: true }).click();
    await expect(confirm).toHaveCount(0);
    await expect(input).toHaveValue(value!);
    await input.press('Escape');
    await expect(confirm).toBeVisible();
    await confirm.getByRole('button', { name: '确认离开', exact: true }).click();
    await expect(editor).toHaveCount(0);
  }
  expect(state.writes).toHaveLength(0);
});

test('a pristine unsubmitted data editor closes without a warning', async ({ page }) => {
  await setup(page);
  await page.getByRole('button', { name: '登记许可授权', exact: true }).click();
  const editor = page.getByRole('dialog', { name: '授权数据用途：交互测试目录', exact: true });
  await editor.getByRole('button', { name: '返回', exact: true }).click();
  await expect(editor).toHaveCount(0);
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('lost-response autosave survives closing and reopens with the original retry', async ({ page }) => {
  const state = await setup(page);
  let release!: () => void;
  state.hold = new Promise<void>(resolve => { release = resolve; });
  await page.getByRole('button', { name: '修改数据源', exact: true }).click();
  let editor = page.getByRole('dialog', { name: '修改数据源显示与启用状态', exact: true });
  const name = editor.getByLabel('数据源名称', { exact: true });
  try {
    await name.fill('交互测试目录更新');
    await expect.poll(() => state.writes.length).toBe(1);
  } finally { release(); state.hold = undefined; }
  await expect(editor.getByText('连接中断，提交结果未知；请重试当前操作', { exact: true })).toBeVisible();
  await editor.getByRole('button', { name: '关闭', exact: true }).click();
  await expect(editor).toHaveCount(0);

  await page.getByRole('button', { name: '修改数据源', exact: true }).click();
  editor = page.getByRole('dialog', { name: '修改数据源显示与启用状态', exact: true });
  await expect(editor.getByLabel('数据源名称', { exact: true })).toHaveValue('交互测试目录更新');
  await editor.getByRole('button', { name: '重试', exact: true }).click();
  await expect.poll(() => state.writes.length).toBe(2);
  expect(state.writes[0]!.key).toBeTruthy();
  expect(state.writes[1]).toEqual(state.writes[0]);
});
