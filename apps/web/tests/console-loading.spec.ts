import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';

// Synthetic presentation/transport fixtures only. Native API acceptance stays
// in the independently provisioned Rust/PostgreSQL browser harness.
async function session(page: Page, failProjects = false) {
  let reads = 0;
  await page.route('**/api/**', async route => {
    const path = new URL(route.request().url()).pathname;
    const reply = (json: unknown) => route.fulfill({ json });
    if (path === '/api/v2/auth/status') return reply({ schema_version: 1, setup_required: false });
    if (path === '/api/v2/auth/session') return reply({ schema_version: 1,
      authenticated_at: new Date().toISOString(), expires_at: new Date(Date.now() + 43_200_000).toISOString() });
    if (path === '/api/v2/projects') {
      reads++;
      if (failProjects && reads === 1) return route.fulfill({ status: 429,
        headers: { 'Content-Type': 'application/problem+json', 'Retry-After': '2' },
        body: JSON.stringify({ type: 'urn:quazonai:problem:rate-limited', title: 'RATE_LIMITED',
          status: 429, code: 'RATE_LIMITED', detail: '请求过于频繁。',
          request_id: '01990000-0000-7000-8000-000000000001', retryable: true,
          field_errors: [], safe_next_actions: [] }),
      });
      return reply({ schema_version: 1, items: [], next_cursor: null });
    }
    if (path === '/api/v2/runs') return reply({ schema_version: 1, items: [], next_cursor: null });
    return route.abort('blockedbyclient');
  });
  return () => reads;
}

test('an unopened section is deferred; a failed chunk leaves navigation usable', async ({ page }) => {
  await session(page);
  let requested = 0;
  await page.route('**/src/portfolio.tsx', route => { requested++; return route.abort('failed'); });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
  expect(requested).toBe(0);
  await page.getByRole('menuitem', { name: '组合', exact: true }).click();
  await expect(page.getByRole('heading', { name: '页面暂时无法显示' })).toBeVisible();
  expect(requested).toBe(1);
  await expect(page.getByRole('menu', { name: '主导航' })).toBeVisible();
  await page.getByRole('menuitem', { name: '研究', exact: true }).click();
  await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: '页面暂时无法显示' })).toHaveCount(0);
});

test('late section loading cannot replace a newer navigation choice', async ({ page }) => {
  await session(page);
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  let requested = false;
  await page.route('**/src/settings.tsx', async route => {
    requested = true; await held; await route.continue();
  });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  await expect.poll(() => requested).toBe(true);
  await expect(page.getByRole('status', { name: '正在载入页面' })).toBeVisible();
  await page.getByRole('menuitem', { name: '研究', exact: true }).click();
  await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
  const loaded = page.waitForResponse(response => new URL(response.url()).pathname === '/src/settings.tsx');
  release();
  await (await loaded).finished();
  await expect(page.getByRole('heading', { name: '设置', exact: true })).toHaveCount(0);
  await expect(page.getByRole('menuitem', { name: '研究', exact: true })).toHaveClass(/ant-menu-item-selected/);
});

test('an idle editor has no error-clock interval and keeps its fields on theme changes', async ({ page }) => {
  await session(page);
  await page.addInitScript(() => {
    const active = new Set<number>();
    Object.defineProperty(window, '__testOneSecondIntervals', { value: active });
    const schedule = window.setInterval.bind(window);
    const cancel = window.clearInterval.bind(window);
    window.setInterval = ((...args: Parameters<typeof window.setInterval>) => {
      const id = schedule(...args); if (args[1] === 1000) active.add(id); return id;
    }) as typeof window.setInterval;
    window.clearInterval = id => { if (typeof id === 'number') active.delete(id); cancel(id); };
  });
  await page.goto('/');
  await page.getByRole('button', { name: '新建研究', exact: true }).click();
  const name = page.getByRole('textbox', { name: '研究名称', exact: true });
  await name.fill('保留未提交的研究');
  expect(await page.evaluate(() => (window as unknown as {
    __testOneSecondIntervals: Set<number>;
  }).__testOneSecondIntervals.size)).toBe(0);
  // Changing the system theme exercises the same non-remounting preference
  // update without clicking through the editor's native modal mask.
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect(name).toHaveValue('保留未提交的研究');
  await page.getByRole('button', { name: '关闭', exact: true }).click();
  await page.getByRole('button', { name: '继续编辑', exact: true }).click();
  await expect(name).toHaveValue('保留未提交的研究');
});

test('retry countdown expires without replaying the request automatically', async ({ page }) => {
  const reads = await session(page, true);
  await page.clock.install();
  await page.goto('/');
  const retry = page.getByRole('button', { name: '重新载入', exact: true });
  await expect(retry).toBeDisabled();
  await expect(page.getByText('2 秒后可重试', { exact: true })).toBeVisible();
  await page.clock.fastForward(3000);
  await expect(retry).toBeEnabled();
  await expect(page.getByText(/秒后可重试/)).toHaveCount(0);
  expect(reads()).toBe(1);
  await retry.click();
  await expect(page.getByText('暂无研究项目', { exact: true })).toBeVisible();
  expect(reads()).toBe(2);
});
