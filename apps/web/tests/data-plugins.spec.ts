import { expect, test, type Page } from '@playwright/test';
import type { DataSourceUpdate, DataSourceView } from '../../../packages/contracts/src/data-sources.ts';

async function plugins(page: Page) {
  let source: DataSourceView = { name: 'test.data', version: '1.0', enabled: false, state: 'disabled', fields: [{ key: 'path', label: '配置路径', type: 'text', required: true }, { key: 'token', label: '访问令牌', type: 'password' }, { key: 'limit', label: '条数', type: 'number' }, { key: 'history', label: '加载历史', type: 'boolean' }], config: { path: '', token: '', limit: 10, history: false }, datasetIds: [], lastLoadedAt: null, error: null };
  const writes: { method: string; body: DataSourceUpdate | null }[] = []; let failSave = false; let failLoad = false;
  await page.route('**/api/v3/**', route => {
    const request = route.request(); const path = new URL(request.url()).pathname.replace('/api/v3', '');
    if (path === '/session') return route.fulfill({ json: { authenticated: true, setupRequired: false } });
    if (path === '/data-sources') return route.fulfill({ json: [source] });
    if (path === '/extensions') return route.fulfill({ json: [{ ...source, kind: 'data' }] });
    if (path === '/datasets') return route.fulfill({ json: [] });
    if (path.startsWith('/data-sources/test.data')) {
      writes.push({ method: request.method(), body: request.postData() ? request.postDataJSON() : null });
      if (request.method() === 'PUT') {
        if (failSave) return route.fulfill({ status: 400, json: { message: '配置路径不可用' } });
        source = { ...source, ...request.postDataJSON(), state: request.postDataJSON().enabled ? 'loaded' : 'disabled' };
      } else {
        if (failLoad) return route.fulfill({ status: 422, json: { message: '数据加载失败' } });
        source = { ...source, state: 'loaded', datasetIds: ['registered-dataset'], lastLoadedAt: '2026-10-26T00:00:00Z' };
      }
      return route.fulfill({ json: source });
    }
    return route.fulfill({ status: 404, json: { error: 'not_found' } });
  });
  return { writes, changeConfig: (config: DataSourceView['config']) => { source = { ...source, config }; }, failSave: (value: boolean) => { failSave = value; }, failLoad: (value: boolean) => { failLoad = value; } };
}

test('data plugins save dynamic fields before loading and link from extensions', async ({ page }, info) => {
  const state = await plugins(page); await page.goto('/#/settings?section=extensions');
  await page.getByRole('link', { name: '管理', exact: true }).click();
  await expect(page.getByRole('heading', { name: '数据插件', exact: true })).toBeVisible();
  const plugin = page.getByRole('region', { name: 'test.data', exact: true });
  await expect(plugin.getByRole('button', { name: '加载数据', exact: true })).toBeDisabled();
  await plugin.getByRole('switch', { name: '启用 test.data', exact: true }).check();
  await plugin.getByRole('button', { name: '保存配置', exact: true }).click();
  await expect(plugin.getByText('请填写配置路径', { exact: true })).toBeVisible(); expect(state.writes).toHaveLength(0);
  await plugin.getByLabel('配置路径', { exact: true }).fill('/tmp/data.json');
  await plugin.getByLabel('访问令牌', { exact: true }).fill('test-plugin-token');
  await expect(plugin.getByLabel('访问令牌', { exact: true })).toHaveAttribute('type', 'password');
  await plugin.getByLabel('条数', { exact: true }).fill('25');
  await plugin.getByRole('switch', { name: '加载历史', exact: true }).check();
  await plugin.getByRole('button', { name: '保存配置', exact: true }).click();
  await expect(plugin.getByText('已保存配置，可加载数据', { exact: true })).toBeVisible();
  expect(state.writes).toEqual([{ method: 'PUT', body: { enabled: true, config: { path: '/tmp/data.json', token: 'test-plugin-token', limit: 25, history: true } } }]);
  expect(await page.evaluate(() => JSON.stringify({ ...sessionStorage, ...localStorage }))).not.toContain('test-plugin-token');
  await plugin.getByRole('button', { name: '加载数据', exact: true }).click();
  await expect(plugin.getByText('已加载数据', { exact: true })).toBeVisible(); await expect(plugin).toContainText('1 个数据集');
  await plugin.getByRole('switch', { name: '启用 test.data', exact: true }).uncheck();
  await plugin.getByRole('button', { name: '保存配置', exact: true }).click();
  await expect(plugin.getByRole('button', { name: '加载数据', exact: true })).toBeDisabled();
  await expect(plugin.getByText('已保存配置', { exact: true })).toBeVisible();
  await expect(plugin.getByRole('switch', { name: '启用 test.data', exact: true })).toBeEnabled();
  for (const width of [1440, 390]) {
    await page.setViewportSize({ width, height: 1000 });
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
    await page.screenshot({ path: info.outputPath(`data-plugins-${width}.png`), fullPage: true });
  }
});

test('data plugin failures preserve editable configuration and permit loading again', async ({ page }) => {
  const state = await plugins(page); await page.goto('/#/settings?section=data-plugins');
  const plugin = page.getByRole('region', { name: 'test.data', exact: true });
  await plugin.getByRole('switch', { name: '启用 test.data', exact: true }).check();
  await plugin.getByLabel('配置路径', { exact: true }).fill('/tmp/retry.json');
  state.failSave(true); await plugin.getByRole('button', { name: '保存配置', exact: true }).click();
  await expect(plugin.getByRole('alert')).toContainText('配置路径不可用'); await expect(plugin.getByLabel('配置路径', { exact: true })).toHaveValue('/tmp/retry.json');
  await expect(plugin.getByRole('button', { name: '加载数据', exact: true })).toBeDisabled();
  state.failSave(false); await plugin.getByRole('button', { name: '保存配置', exact: true }).click();
  await expect(plugin.getByText('已保存配置，可加载数据', { exact: true })).toBeVisible();
  state.failLoad(true); await plugin.getByRole('button', { name: '加载数据', exact: true }).click();
  await expect(plugin.getByRole('alert')).toContainText('数据加载失败'); await expect(plugin.getByLabel('配置路径', { exact: true })).toHaveValue('/tmp/retry.json');
  state.failLoad(false); await plugin.getByRole('button', { name: '加载数据', exact: true }).click();
  await expect(plugin.getByText('已加载数据', { exact: true })).toBeVisible(); expect(state.writes.map(item => item.method)).toEqual(['PUT', 'PUT', 'POST', 'POST']);
});


test('data plugin refresh updates pristine fields and preserves unsaved fields', async ({ page }) => {
  const state = await plugins(page); await page.goto('/#/settings?section=data-plugins');
  const plugin = page.getByRole('region', { name: 'test.data', exact: true });
  await expect(plugin.getByLabel('配置路径', { exact: true })).toHaveValue('');
  state.changeConfig({ path: '/tmp/external.json', token: '', limit: 10, history: false });
  await page.getByRole('button', { name: '刷新目录', exact: true }).click();
  await expect(plugin.getByLabel('配置路径', { exact: true })).toHaveValue('/tmp/external.json');
  await plugin.getByLabel('配置路径', { exact: true }).fill('/tmp/draft.json');
  state.changeConfig({ path: '/tmp/newer.json', token: '', limit: 10, history: false });
  await page.getByRole('button', { name: '刷新目录', exact: true }).click();
  await expect(plugin.getByLabel('配置路径', { exact: true })).toHaveValue('/tmp/draft.json');
  await expect(plugin.getByRole('button', { name: '保存配置', exact: true })).toBeEnabled();
});
