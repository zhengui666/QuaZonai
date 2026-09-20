// Tests the actual components and ECharts on the existing isolated Vite server.
import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { Schema } from '../src/api';
import { id, reply } from './fixtures';

function response(count = 4): Schema['EquityCurveV1'] {
  const start = 1767225600000000000n;
  return { schema_version: 1, project_id: id(1), candidate_id: id(81), evaluation_id: id(91), run_id: id(94), origin: 'FIXTURE',
    curve: { status: 'READY', source_artifact_id: id(96), series: {
      native_version: '0.63.0', base_currency: 'USD', starting_capital: '1000',
      period_start_ns: start.toString(), period_end_ns: (start + BigInt(count) * 1_000_000_000n).toString(),
      source_point_count: String(count), window_point_count: String(count), resolution: 'NATIVE', sampled: false,
      points: Array.from({ length: count }, (_, index) => ({
        timestamp_ns: (start + BigInt(index) * 1_000_000_000n + 123_456_789n).toString(),
        value: `${1000 + index}.123456789012345678`, reason_code: null,
      })),
    } } };
}
async function start(page: Page, count = 4) {
  const reads: URL[] = [];
  await page.route('**/api/v2/evaluations/*/equity-curve*', async route => {
    const url = new URL(route.request().url()); reads.push(url);
    const value = response(count);
    if (value.curve.status !== 'READY') throw new Error('fixture');
    const series = value.curve.series;
    const start = url.searchParams.get('start_ns'); const end = url.searchParams.get('end_ns');
    series.points = series.points.filter(point => (!start || BigInt(point.timestamp_ns) >= BigInt(start)) && (!end || BigInt(point.timestamp_ns) <= BigInt(end)));
    series.window_point_count = String(series.points.length);
    await reply(route, value);
  });
  await page.goto('http://127.0.0.1:4179/tests/equity-harness.html');
  await expect.poll(() => page.evaluate(() => window.equityHarness.inspect().points)).toBe(count);
  return reads;
}

test('theme and container changes keep native zoom, while complete-range explicitly resets it', async ({ page }, info) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await start(page);
  const initial = await page.evaluate(() => window.equityHarness.inspect());
  await page.evaluate(() => window.equityHarness.zoom(40, 90));
  await expect.poll(() => page.evaluate(() => window.equityHarness.inspect().start)).toBe(40);
  await page.getByRole('button', { name: '切换测试主题', exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.equityHarness.inspect().background)).not.toBe(initial.background);
  expect((await page.evaluate(() => window.equityHarness.inspect())).id).toBe(initial.id);
  expect((await page.evaluate(() => window.equityHarness.inspect())).start).toBe(40);
  await page.screenshot({ path: info.outputPath('equity-dark-fixture.png'), fullPage: true });
  await page.getByRole('button', { name: '切换测试宽度', exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.equityHarness.inspect().width)).toBe(320);
  await page.getByRole('button', { name: '切换测试可见性', exact: true }).click();
  await expect(page.getByTestId('equity-chart')).toBeHidden();
  await page.getByRole('button', { name: '切换测试可见性', exact: true }).click();
  await expect(page.getByTestId('equity-chart')).toBeVisible();
  await expect.poll(() => page.evaluate(() => window.equityHarness.inspect().width)).toBe(320);
  await page.getByRole('button', { name: '完整区间', exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.equityHarness.inspect().start)).toBe(0);
  await expect.poll(() => page.evaluate(() => window.equityHarness.inspect().end)).toBe(100);
  for (let attempt = 0; attempt < 3; attempt++) {
    await page.getByRole('button', { name: '切换测试挂载', exact: true }).click();
    await expect(page.locator('.echarts-for-react')).toHaveCount(0);
    await page.getByRole('button', { name: '切换测试挂载', exact: true }).click();
    await expect.poll(() => page.evaluate(() => window.equityHarness.inspect().points)).toBe(4);
    await expect(page.getByTestId('equity-chart').locator('canvas')).toHaveCount(1);
  }
  expect(errors).toEqual([]);
});

test('native local-time controls send an inclusive millisecond range without revaluing it', async ({ page }) => {
  const reads = await start(page);
  const before = reads.length;
  await page.getByLabel('开始（本地时间）', { exact: true }).fill('2026-01-01T00:00:01.123');
  await page.getByRole('button', { name: '查看区间', exact: true }).click();
  await expect(page.getByText('请选择开始和结束时间。', { exact: true })).toBeVisible();
  expect(reads.length).toBe(before);
  await page.getByLabel('结束（本地时间）', { exact: true }).fill('2026-01-01T00:00:01.123');
  await page.getByRole('button', { name: '查看区间', exact: true }).press('Enter');
  await expect.poll(() => page.evaluate(() => window.equityHarness.inspect().points)).toBe(1);
  expect(reads.at(-1)!.searchParams.get('start_ns')).toBe('1767225601123000000');
  expect(reads.at(-1)!.searchParams.get('end_ns')).toBe('1767225601123999999');
  await page.getByText('明细（1）', { exact: true }).click();
  await expect(page.getByRole('cell', { name: '1001.123456789012345678', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '完整区间', exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.equityHarness.inspect().points)).toBe(4);
  await expect(page.getByLabel('开始（本地时间）', { exact: true })).toHaveValue('');
  await expect(page.getByLabel('结束（本地时间）', { exact: true })).toHaveValue('');
});

for (const count of [1000, 10000]) test(`${count} bounded observations render through the real library`, async ({ page }, info) => {
  const begin = Date.now();
  await start(page, count);
  const firstRenderMs = Date.now() - begin;
  await page.evaluate(() => window.equityHarness.zoom(50, 75));
  await expect.poll(() => page.evaluate(() => window.equityHarness.inspect().start)).toBe(50);
  const snapshot = await page.evaluate(() => window.equityHarness.inspect());
  expect(snapshot.points).toBe(count);
  await expect(page.getByTestId('equity-chart').locator('canvas')).toHaveCount(1);
  await info.attach('equity-render-measurement', { contentType: 'application/json', body: JSON.stringify({
    fixture: true, observations: count, viewport: info.project.name, navigationToConfiguredChartMs: firstRenderMs,
    renderer: 'ECharts Canvas', includesDevelopmentModuleLoad: true,
  }, null, 2) });
});
