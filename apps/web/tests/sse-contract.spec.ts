// SYNTHETIC HTTP faults exercise the real RunEvents browser client, not DB evidence.
import { test, expect } from '@playwright/test';
import { fixture, navigate, problem } from './fixtures';

for (const status of [418, 500, 401]) {
  test(`SSE rejects undeclared/malformed failure ${status} without an authentication event`, async ({ page }) => {
    const state = await fixture(page);
    let requests = 0;
    await page.addInitScript(() => {
      window.addEventListener('quazonai-auth-changed', () => {
        document.documentElement.dataset.authEvents = String(Number(document.documentElement.dataset.authEvents ?? '0') + 1);
      });
    });
    await page.route('**/api/v2/runs/*/events', async route => {
      requests++;
      const body = { ...problem('AUTH_REQUIRED', status, 'UNTRUSTED_SSE_PROBLEM'),
        ...(status === 401 ? { request_id: 'invalid' } : {}) };
      await route.fulfill({ status, contentType: 'application/problem+json', json: body });
    });
    await page.goto('/'); await navigate(page, '运行');
    await page.getByRole('button', { name: 'IMPORT · 00000003', exact: true }).click();
    await expect(page.getByText(`服务返回了无法识别的响应（HTTP ${status}）。未将它当成空列表或成功结果。`, { exact: true })).toBeVisible();
    await expect(page.getByText('UNTRUSTED_SSE_PROBLEM', { exact: true })).toHaveCount(0);
    expect(await page.evaluate(() => document.documentElement.dataset.authEvents ?? '0')).toBe('0');
    expect(requests).toBe(1);
    expect(state.commands).toHaveLength(0);
  });
}

for (const [status, media] of [[201, 'text/event-stream'], [200, 'text/event-stream-invalid']] as const) {
  test(`SSE refuses undeclared success ${status} ${media}`, async ({ page }) => {
    const state = await fixture(page);
    await page.route('**/api/v2/runs/*/events', route => route.fulfill({ status, contentType: media, body: ': not a declared stream\n\n' }));
    await page.goto('/'); await navigate(page, '运行');
    await page.getByRole('button', { name: 'IMPORT · 00000003', exact: true }).click();
    await expect(page.getByText('运行事件接口没有返回合同声明的事件流。', { exact: true })).toBeVisible();
    expect(state.commands).toHaveLength(0);
  });
}

test('SSE emits authentication expiry only after validating the declared Problem', async ({ page }) => {
  await fixture(page);
  await page.addInitScript(() => {
    window.addEventListener('quazonai-auth-changed', () => {
      document.documentElement.dataset.authEvents = String(Number(document.documentElement.dataset.authEvents ?? '0') + 1);
    });
  });
  await page.route('**/api/v2/runs/*/events', route => route.fulfill({
    status: 401, contentType: 'application/problem+json', json: problem('AUTH_REQUIRED', 401),
  }));
  await page.goto('/'); await navigate(page, '运行');
  await page.getByRole('button', { name: 'IMPORT · 00000003', exact: true }).click();
  await expect.poll(() => page.evaluate(() => document.documentElement.dataset.authEvents ?? '0')).toBe('1');
});
