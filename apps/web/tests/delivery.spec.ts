// Controlled presentation fixtures; native Release authority is tested in Rust/PG.
import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import type { Schema } from '../src/api';
import { fixture, id, navigate, project, reply } from './fixtures';

const release: Schema['ReleaseViewV1'] = {
  id: id(102), project_id: project.id, candidate_id: id(80), mandate_id: id(81), evaluation_id: id(82),
  package_artifact_id: id(83), package_schema_version: '1', market_capability_version: 'controlled-market/1',
  environment: 'REAL', asof: '2026-09-13T00:00:00Z', valid_from: '2026-09-13T00:01:00Z', valid_until: '2026-09-13T01:00:00Z', created_at: '2026-09-13T00:01:00Z',
};
for (const wrong of [false, true]) test(`Release project pagination and exact historical detail: wrong=${wrong}`, async ({ page }) => {
  const state = await fixture(page); const cursors: (string | null)[] = [];
  await page.route('**/api/v2/**', route => {
    const url = new URL(route.request().url());
    if (url.pathname === `/api/v2/projects/${project.id}/releases`) {
      const cursor = url.searchParams.get('cursor'); cursors.push(cursor);
      return reply(route, { schema_version: 1, items: [{ ...release, id: cursor ? id(101) : release.id }], next_cursor: cursor ? null : release.id });
    }
    if (url.pathname === `/api/v2/releases/${release.id}`) return reply(route, { ...release, project_id: wrong ? id(999) : project.id });
    return route.fallback();
  });
  await page.goto('/'); await navigate(page, '交付');
  await page.getByRole('combobox', { name: '选择交付所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: project.name }).click();
  await page.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Release 00000101', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '上一页', exact: true }).click();
  await page.getByRole('button', { name: 'Release 00000102', exact: true }).click();
  const detail = page.getByRole('dialog', { name: '原始目标包版本', exact: true });
  if (wrong) {
    await expect(detail.getByText(release.package_artifact_id, { exact: true })).toHaveCount(0);
    await expect(detail.getByRole('button', { name: '重新载入', exact: true })).toBeVisible();
  } else {
    await expect(detail.getByText(release.package_artifact_id, { exact: true })).toBeVisible();
    await expect(detail.getByText('历史有效期不是当前审批资格', { exact: true })).toBeVisible();
    expect((await new AxeBuilder({ page }).include('.ant-drawer').withTags(['wcag2a', 'wcag2aa']).analyze()).violations).toEqual([]);
  }
  expect(cursors).toContain(release.id);
  expect(state.commands).toEqual([]);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
});
