import { expect, test } from '@playwright/test';

// Fault injection is confined to the existing Vite test server. The production
// bundle has no crash switch, fixture route or alternate application entry.
const origin = 'http://127.0.0.1:4179';
const appModule = /^http:\/\/127\.0\.0\.1:4179\/src\/App\.tsx(?:\?.*)?$/;

test('render failure offers confirmed recovery without exposing or replaying work', async ({ page }) => {
  let failing = true;
  let moduleRequests = 0;
  const mutations: string[] = [];
  page.on('request', (request) => {
    if (request.url().startsWith(`${origin}/api/`) && !['GET', 'HEAD'].includes(request.method())) {
      mutations.push(request.method());
    }
  });
  await page.route(appModule, async (route) => {
    moduleRequests += 1;
    await route.fulfill({
      contentType: 'application/javascript',
      body: failing
        ? 'export default function App() { throw new Error("private-render-fixture-detail"); }'
        : 'export default function App() { return "页面已恢复"; }',
    });
  });

  await page.goto(origin);
  const heading = page.getByRole('heading', { level: 1, name: '页面暂时无法显示' });
  await expect(heading).toBeVisible();
  await expect(page.locator('body')).not.toContainText('private-render-fixture-detail');
  await expect(page.getByText('界面发生异常，未提交的内容可能未保存。', { exact: false })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);

  const requestsBeforeCancel = moduleRequests;
  const reload = page.getByRole('button', { name: '重新加载页面', exact: true });
  await reload.focus();
  await expect(reload).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.getByText('未保存的内容将无法恢复。已经发送的操作不会被撤销。')).toBeVisible();
  await page.getByRole('button', { name: '留在此页', exact: true }).click();
  await expect(heading).toBeVisible();
  expect(moduleRequests).toBe(requestsBeforeCancel);

  failing = false;
  await reload.click();
  await page.getByRole('button', { name: '确认重新加载', exact: true }).click();
  await expect(page.getByText('页面已恢复', { exact: true })).toBeVisible();
  expect(moduleRequests).toBeGreaterThan(requestsBeforeCancel);
  expect(mutations).toEqual([]);
});

test('healthy children render without a recovery prompt', async ({ page }) => {
  await page.route(appModule, (route) => route.fulfill({
    contentType: 'application/javascript',
    body: 'export default function App() { return "正常研究工作台"; }',
  }));
  await page.goto(origin);
  await expect(page.getByText('正常研究工作台', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '重新加载页面', exact: true })).toHaveCount(0);
});
