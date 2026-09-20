import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

// Fault injection is confined to the existing Vite test server. The production
// bundle has no crash switch, fixture route or alternate application entry.
const origin = 'http://127.0.0.1:4179';
const appModule = /^http:\/\/127\.0\.0\.1:4179\/src\/App\.tsx(?:\?.*)?$/;
const projectModule = new RegExp('^http://127[.]0[.]0[.]1:4179/src/projects[.]tsx(?:[?].*)?$');

for (const mode of ['light', 'dark'] as const) for (const target of ['application root', 'nested project view'] as const) {
  test(`${mode} ${target}: confirmed recovery without exposing or replaying work`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: mode });
    let failing = true;
    let moduleRequests = 0;
    let nativeModuleRequests = 0;
    const mutations: string[] = [];
    const diagnostics: string[] = [];
    page.on('console', (message) => { diagnostics.push(message.text()); });
    page.on('pageerror', (error) => { diagnostics.push(error.message); });
    page.on('request', (request) => {
      if (request.url().startsWith(`${origin}/api/`) && !['GET', 'HEAD'].includes(request.method())) {
        mutations.push(request.method());
      }
    });
    await page.route(target === 'application root' ? appModule : projectModule, async (route) => {
      const native = new URL(route.request().url());
      if (native.searchParams.has('boundary-original')) {
        nativeModuleRequests += 1;
        await route.continue();
        return;
      }
      moduleRequests += 1;
      const body = failing
        ? 'throw new Error("private-render-fixture-detail");'
        : 'return "页面已恢复";';
      native.searchParams.set('boundary-original', 'true');
      // Retain the module's real exports. Only the rendered Projects component
      // changes, so this remains a nested render failure rather than an import error.
      await route.fulfill({
        contentType: 'application/javascript',
        body: target === 'application root'
          ? `export default function App() { ${body} }`
          : `export * from ${JSON.stringify(native.pathname + native.search)}; export function Projects() { ${body} }`,
      });
    });

    await page.goto(origin);
    const heading = page.getByRole('heading', { level: 1, name: '页面暂时无法显示' });
    await expect(heading).toBeVisible();
    expect(moduleRequests).toBeGreaterThan(0);
    if (target === 'nested project view') expect(nativeModuleRequests).toBeGreaterThan(0);
    await expect(page.locator('html')).toHaveAttribute('data-theme', mode);
    expect((await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa']).analyze()).violations).toEqual([]);
    await expect(page.locator('body')).not.toContainText('private-render-fixture-detail');
    await expect(page.getByText('界面发生异常，未提交的内容可能未保存。', { exact: false })).toHaveCount(0);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);

    const requestsBeforeCancel = moduleRequests;
    const reload = page.getByRole('button', { name: '重新加载页面', exact: true });
    await reload.focus();
    await expect(reload).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(page.getByText('未保存内容将丢失；已提交操作不会撤销。')).toBeVisible();
    await page.getByRole('button', { name: '留在此页', exact: true }).click();
    await expect(heading).toBeVisible();
    expect(moduleRequests).toBe(requestsBeforeCancel);

    failing = false;
    await reload.click();
    await page.getByRole('button', { name: '确认重新加载', exact: true }).click();
    // Nested content shares main with the skip link. Test the rendered content,
    // not whether a parent's entire text is exactly one React text node.
    const recovered = target === 'application root' ? page.locator('body') : page.locator('#main-content');
    await expect(recovered).toContainText('页面已恢复');
    await expect(heading).toHaveCount(0);
    expect(moduleRequests).toBeGreaterThan(requestsBeforeCancel);
    expect(mutations).toEqual([]);
    expect(diagnostics.join('\n')).not.toContain('private-render-fixture-detail');
  });
}

test('healthy children render without a recovery prompt', async ({ page }) => {
  await page.route(appModule, (route) => route.fulfill({
    contentType: 'application/javascript',
    body: 'export default function App() { return "正常研究工作台"; }',
  }));
  await page.goto(origin);
  await expect(page.getByText('正常研究工作台', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '重新加载页面', exact: true })).toHaveCount(0);
});
