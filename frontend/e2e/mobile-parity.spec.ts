import { expect, test } from '@playwright/test';

const widths = [320, 375, 390, 430];
const routes = ['/', '/ideas', '/research', '/alpha', '/alphas', '/portfolio', '/approval', '/approvals', '/handoff', '/handoffs', '/admin'];

test('shared client keeps every primary route usable at phone widths', async ({ page }) => {
  for (const width of widths) {
    await page.setViewportSize({ width, height: 844 });
    for (const route of routes) {
      await page.goto(route, { waitUntil: 'domcontentloaded' });
      await expect(page.locator('.qz-app')).toBeVisible();
      await expect(page.locator('body')).not.toContainText('Page not found');
      const overflow = await page.evaluate(() => ({
        documentWidth: document.documentElement.scrollWidth,
        viewportWidth: document.documentElement.clientWidth,
      }));
      expect(overflow.documentWidth, `${route} overflows at ${width}px`).toBeLessThanOrEqual(overflow.viewportWidth);
    }
  }
});

test('mobile navigation preserves access to less frequent capabilities', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  const more = page.getByRole('button', { name: 'More' });
  if (await more.isVisible()) {
    await more.click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toContainText('Administration');
    await expect(dialog).toContainText('Alpha Library');
    await expect(dialog).toContainText('Handoff Center');
    const box = await dialog.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.x).toBeGreaterThanOrEqual(0);
    expect(box!.y).toBeGreaterThanOrEqual(0);
    expect(box!.x + box!.width).toBeLessThanOrEqual(390);
    expect(box!.y + box!.height).toBeLessThanOrEqual(844);
  }
});

test('administration capability tabs remain reachable on phone widths', async ({ page }) => {
  for (const width of widths) {
    await page.setViewportSize({ width, height: 844 });
    await page.goto('/admin');
    const tabs = page.getByRole('tab');
    await expect(tabs).toHaveCount(6);
    for (let index = 0; index < await tabs.count(); index += 1) {
      const box = await tabs.nth(index).boundingBox();
      expect(box, `tab ${index} has no layout at ${width}px`).not.toBeNull();
      expect(box!.x).toBeGreaterThanOrEqual(0);
      expect(box!.x + box!.width).toBeLessThanOrEqual(width);
    }
  }
});

test('mobile lists use the shared card projection and preserve empty states', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/research');
  await expect(page.locator('.qz-mobile-card-list, .qz-empty')).toBeVisible();
  if (await page.locator('.qz-mobile-card-list').count()) {
    await expect(page.locator('.qz-mobile-card-list')).toBeVisible();
  } else {
    await expect(page.locator('.qz-empty')).toBeVisible();
  }
  await expect(page.locator('.qz-table')).toHaveCount(0);
});
