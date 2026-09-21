import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { fixture } from './fixtures';

for (const mode of ['light', 'dark'] as const) {
  test(`system ${mode} initializes the native theme with accessible controls`, async ({ page }, info) => {
    await page.emulateMedia({ colorScheme: mode });
    await fixture(page); await page.goto('/');
    await expect(page.locator('html')).toHaveAttribute('data-theme', mode);
    await expect(page.getByRole('button', { name: mode === 'dark' ? '切换为浅色主题' : '切换为深色主题' })).toBeVisible();
    expect((await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa']).analyze()).violations).toEqual([]);
    await page.screenshot({ path: info.outputPath(`console-${mode}.png`), fullPage: true });
  });
}

test('switch preserves an unsaved editor and persists across reload', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'light' }); await fixture(page); await page.goto('/');
  await page.getByRole('button', { name: '新建研究', exact: true }).click();
  await page.getByLabel('研究名称').fill('保留主题切换中的草稿');
  // The header is outside the modal. Use the existing theme button's DOM event
  // to exercise the handler without dismissing an intentionally modal editor.
  await page.getByRole('button', { name: '切换为深色主题' }).dispatchEvent('click');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await expect(page.getByLabel('研究名称')).toHaveValue('保留主题切换中的草稿');
  expect(await page.evaluate(() => localStorage.getItem('quazonai.theme'))).toBe('dark');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});

test('system changes apply until the user makes an explicit choice', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'light' }); await fixture(page); await page.goto('/');
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.getByRole('button', { name: '切换为浅色主题' }).click();
  await page.emulateMedia({ colorScheme: 'light' }); await page.emulateMedia({ colorScheme: 'dark' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
});

test('explicit preferences synchronize across same-origin tabs', async ({ page, context }) => {
  await page.emulateMedia({ colorScheme: 'light' }); await fixture(page); await page.goto('/');
  const other = await context.newPage(); await fixture(other); await other.goto('/');
  await page.getByRole('button', { name: '切换为深色主题' }).click();
  await expect(other.locator('html')).toHaveAttribute('data-theme', 'dark');
  await other.close();
});

test('blocked browser storage still permits session-only switching', async ({ page }) => {
  await page.addInitScript(() => {
    for (const key of ['getItem', 'setItem'] as const) {
      Storage.prototype[key] = () => { throw new DOMException('Blocked', 'SecurityError'); };
    }
  });
  await page.emulateMedia({ colorScheme: 'light' }); await fixture(page); await page.goto('/');
  await page.getByRole('button', { name: '切换为深色主题' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});
