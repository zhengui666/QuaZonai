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

test('401 returns a stable six-digit authenticator login, not a reload loop', async ({ page }) => {
  const state = await fixture(page, { authenticated: false }); await page.goto('/');
  await expect(page.getByRole('heading', { name: '使用验证器登录' })).toBeVisible();
  await expect(page.getByLabel('动态验证码')).toBeVisible();
  await page.getByLabel('动态验证码').fill('123456');
  await page.getByRole('button', { name: '登录', exact: true }).click();
  await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
  expect(state.commands.filter(item => item.path === '/api/v2/auth/login')).toHaveLength(1);
  const storage = await page.evaluate(() => ({ local: Object.keys(localStorage), session: Object.keys(sessionStorage) }));
  expect(storage).toEqual({ local: [], session: [] });
});

test('a lost create response retries the same body and idempotency key', async ({ page }) => {
  const state = await fixture(page, { loseFirstCreate: true }); await page.goto('/');
  await page.getByRole('button', { name: '新建研究', exact: true }).click();
  await page.getByLabel('研究名称').fill('重试不重复创建');
  await page.getByLabel('研究说明', { exact: true }).fill('合成界面测试。');
  await page.getByRole('button', { name: '保存项目', exact: true }).click();
  await expect(page.getByText('连接中断，尚不能确定操作是否已提交。', { exact: false })).toBeVisible();
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
  await expect(page.getByText('服务器当前版本：9007199254740994。请先重载；不会覆盖新版本。')).toBeVisible();
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
  await expect(page.getByText('响应字段或合同版本不兼容。未将它当成空列表或成功操作。')).toBeVisible();
  await expect(page.getByText('尚无研究项目。', { exact: false })).toHaveCount(0);
});

test('recent verification preserves the cancel intent and requires an explicit resubmit', async ({ page }) => {
  const state = await fixture(page, { requireVerify: true }); await page.goto('/');
  await navigate(page, '运行');
  await page.getByRole('button', { name: 'IMPORT · 00000003', exact: true }).click();
  await page.getByRole('button', { name: '请求取消运行', exact: true }).click();
  await page.getByRole('button', { name: '确认请求取消', exact: true }).click();
  const verify = page.getByRole('dialog', { name: '重新验证敏感操作' });
  await expect(verify).toBeVisible();
  await verify.getByLabel('动态验证码').fill('123456');
  await verify.getByRole('button', { name: '确认验证', exact: true }).click();
  await expect(verify).not.toBeVisible();
  const cancels = () => state.commands.filter(item => item.path.endsWith('/cancel'));
  expect(cancels()).toHaveLength(1);
  await page.getByRole('button', { name: '确认请求取消', exact: true }).click();
  await expect(page.getByText('已请求取消', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('已取消', { exact: true })).toHaveCount(0);
  expect(cancels()).toHaveLength(2);
  expect(cancels()[0]?.key).toBe(cancels()[1]?.key);
  expect(cancels()[0]?.body).toEqual(cancels()[1]?.body);
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

test('logout removes private project content and never claims to cancel runs', async ({ page }) => {
  const state = await fixture(page); await page.goto('/');
  await page.getByRole('button', { name: '退出登录', exact: true }).click();
  await page.getByRole('button', { name: '确认退出', exact: true }).click();
  await expect(page.getByRole('heading', { name: '使用验证器登录' })).toBeVisible();
  await expect(page.getByRole('button', { name: project.name, exact: true })).toHaveCount(0);
  expect(state.commands.filter(item => item.path.endsWith('/cancel'))).toHaveLength(0);
});
