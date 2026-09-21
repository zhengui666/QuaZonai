import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { fixture, id, navigate, project, reply } from './fixtures';

test('real browser renders the synthetic contract without page overflow or accessibility violations', async ({ page }, info) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await fixture(page); await page.goto('/');
  await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: project.name, exact: true })).toBeVisible();
  const dimensions = await page.evaluate(() => ({ viewport: innerWidth, content: document.documentElement.scrollWidth }));
  expect(dimensions.content).toBeLessThanOrEqual(dimensions.viewport + 1);
  const audit = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa']).analyze();
  expect(audit.violations).toEqual([]);
  expect(errors).toEqual([]);
  await page.screenshot({ path: info.outputPath('synthetic-console.png'), fullPage: true });
});

test('local workbench has no enrollment, login or device-management flow', async ({ page }) => {
  const state = await fixture(page); const paths: string[] = [];
  page.on('request', request => paths.push(new URL(request.url()).pathname));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
  await expect(page.getByLabel('动态验证码')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '登录', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '退出登录', exact: true })).toHaveCount(0);
  expect(paths.filter(path => path.startsWith('/api/v2/bootstrap') || path.startsWith('/api/v2/auth/'))).toEqual([]);
  expect(state.commands).toHaveLength(0);
});

test('a lost create response retries the same body and idempotency key', async ({ page }) => {
  const state = await fixture(page, { loseFirstCreate: true }); await page.goto('/');
  await page.getByRole('button', { name: '新建研究', exact: true }).click();
  await page.getByLabel('研究名称').fill('重试不重复创建');
  await page.getByLabel('研究说明', { exact: true }).fill('合成界面测试。');
  await page.getByRole('button', { name: '保存项目', exact: true }).click();
  await expect(page.getByText('连接中断，提交结果未知；请重试当前操作', { exact: false })).toBeVisible();
  await page.getByRole('button', { name: '保存项目', exact: true }).click();
  await expect(page.getByRole('button', { name: '重试不重复创建', exact: true })).toBeVisible();
  const creates = state.commands.filter(item => item.path === '/api/v2/projects' && item.method === 'POST');
  expect(creates).toHaveLength(2);
  expect(creates[0]?.key).toBeTruthy();
  expect(creates[1]?.key).toBe(creates[0]?.key);
  expect(creates[1]?.body).toEqual(creates[0]?.body);
  expect(state.projects.filter(item => item.name === '重试不重复创建')).toHaveLength(1);
});

test('409 retains the local form, shows the exact revision, and blocks overwrite', async ({ page }) => {
  const state = await fixture(page, { conflict: true }); await page.goto('/');
  await page.getByRole('button', { name: '编辑', exact: true }).click();
  await page.getByLabel('研究名称').fill('尚未保存的本地修改');
  await page.getByRole('button', { name: '保存项目', exact: true }).click();
  await expect(page.getByText('当前版本：9007199254740994，请重新载入')).toBeVisible();
  await expect(page.getByLabel('研究名称')).toHaveValue('尚未保存的本地修改');
  await expect(page.getByRole('button', { name: '保存项目', exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: '保存项目', exact: true })).toHaveAttribute('aria-busy', 'false');
  expect(state.commands.filter(item => item.method === 'PATCH')).toHaveLength(1);
});

test('offline submission is refused and reconnection never queues a write', async ({ page, context }) => {
  const state = await fixture(page); await page.goto('/');
  await page.getByRole('button', { name: '新建研究', exact: true }).click();
  await page.getByLabel('研究名称').fill('离线草稿');
  await context.setOffline(true);
  await expect(page.getByRole('button', { name: '保存项目', exact: true })).toBeDisabled();
  expect(state.commands).toHaveLength(0);
  await context.setOffline(false);
  await expect(page.getByRole('button', { name: '保存项目', exact: true })).toBeEnabled();
  expect(state.commands).toHaveLength(0);
});

test('invalid success data is a contract error, never an empty project list', async ({ page }) => {
  await fixture(page);
  await page.route(url => url.pathname === '/api/v2/projects', route => reply(route, {}));
  await page.goto('/');
  await expect(page.getByText('响应数据不兼容')).toBeVisible();
  await expect(page.getByText('暂无研究项目', { exact: true })).toHaveCount(0);
});

test('local cancellation retains confirmation and uses no verification challenge', async ({ page }) => {
  const state = await fixture(page); await page.goto('/');
  await navigate(page, '运行');
  await page.getByRole('button', { name: 'IMPORT · 00000003', exact: true }).click();
  await page.getByRole('button', { name: '请求取消运行', exact: true }).click();
  await page.getByRole('button', { name: '确认请求取消', exact: true }).click();
  await expect(page.getByText('已请求取消', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('已取消', { exact: true })).toHaveCount(0);
  await expect(page.getByLabel('动态验证码')).toHaveCount(0);
  expect(state.commands.filter(item => item.path.endsWith('/cancel'))).toHaveLength(1);
  expect(state.commands.filter(item => item.path.startsWith('/api/v2/auth/'))).toHaveLength(0);
});

test('SSE reconnect carries one exact advanced cursor, including unknown compatible events', async ({ page }) => {
  const state = await fixture(page); const headers: (string | null)[] = [];
  await page.route('**/api/v2/runs/*/events', async route => {
    headers.push(await route.request().headerValue('last-event-id'));
    if (headers.length > 1) return route.fulfill({ contentType: 'text/event-stream', body: 'event: reset-required\ndata: {}\n\n' });
    state.run.last_event_seq = '2';
    const event = { schema_version: 1, run_id: id(3), seq: '2', attempt_id: null, event_type: 'run.future_note', occurred_at: '2026-09-08T00:00:01Z', payload: { schema_version: 1, note: 'synthetic future event' } };
    return route.fulfill({ contentType: 'text/event-stream', body: `id: ${id(3)}:2\nevent: run.future_note\ndata: ${JSON.stringify(event)}\n\n` });
  });
  await page.goto('/'); await navigate(page, '运行');
  await page.getByRole('button', { name: 'IMPORT · 00000003', exact: true }).click();
  await expect(page.getByText('run.future_note · #2')).toBeVisible();
  await expect.poll(() => headers.length).toBeGreaterThanOrEqual(2);
  expect(headers[0]).toBe(`${id(3)}:1`);
  expect(headers[1]).toBe(`${id(3)}:2`);
  expect(headers[1]).not.toContain(',');
  expect(state.run.state).toBe('QUEUED');
});

for (const terminalState of ['SUCCEEDED', 'FAILED', 'CANCELLED'] as const) {
  test(`terminal run cannot request cancellation (${terminalState})`, async ({ page }) => {
    const state = await fixture(page); state.run.state = terminalState;
    await page.goto('/'); await navigate(page, '运行');
    await page.getByRole('button', { name: 'IMPORT · 00000003', exact: true }).click();
    await expect(page.getByRole('button', { name: '请求取消运行', exact: true })).toBeDisabled();
    expect(state.commands.filter(item => item.path.endsWith('/cancel'))).toHaveLength(0);
  });
}
