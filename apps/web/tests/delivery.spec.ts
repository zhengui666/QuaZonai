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

for (const wrong of [false, true]) test(`Handoff original Claim history and project binding: wrong=${wrong}`, async ({ page }) => {
  const state = await fixture(page); const cursors: (string | null)[] = [];
  const handoff: Schema['HandoffViewV1'] = {
    id: id(202), project_id: project.id, candidate_id: id(80), mandate_id: id(81), release_id: release.id,
    approval_id: id(84), downstream_id: id(85), environment: 'PAPER', delivery_sequence: '9007199254740993', revision: '1',
    state: 'CLAIMED', supersedes_handoff_id: id(201), offered_at: release.created_at, expires_at: release.valid_until,
    claimed_at: '2026-09-13T00:02:00Z', external_claim_id: 'original-downstream-claim', acknowledged_at: null,
  };
  await page.route('**/api/v2/**', route => {
    const url = new URL(route.request().url());
    if (url.pathname === `/api/v2/projects/${project.id}/releases`) return reply(route, { schema_version: 1, items: [], next_cursor: null });
    if (url.pathname === `/api/v2/projects/${project.id}/handoffs`) {
      const cursor = url.searchParams.get('cursor'); cursors.push(cursor);
      return reply(route, { schema_version: 1, items: [{ ...handoff, id: cursor ? id(201) : handoff.id }], next_cursor: cursor ? null : handoff.id });
    }
    if (url.pathname === `/api/v2/handoffs/${handoff.id}`) return reply(route, { ...handoff, project_id: wrong ? id(999) : project.id });
    return route.fallback();
  });
  await page.goto('/'); await navigate(page, '交付');
  await page.getByRole('combobox', { name: '选择交付所属项目' }).click();
  await page.getByText(`${project.name} · ${project.id}`, { exact: true }).click();
  await page.getByRole('tab', { name: '交付记录', exact: true }).click();
  await expect(page.getByText('9007199254740993', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '下一页', exact: true }).click();
  await page.getByRole('button', { name: '上一页', exact: true }).click();
  await page.getByRole('button', { name: `Handoff ${handoff.id.slice(-8)}`, exact: true }).click();
  const detail = page.getByRole('dialog', { name: '原交付记录' });
  if (wrong) {
    await expect(detail.getByText(handoff.external_claim_id!, { exact: true })).toHaveCount(0);
    await expect(detail.getByRole('button', { name: '重新载入', exact: true })).toBeVisible();
  } else {
    await expect(detail.getByText(handoff.external_claim_id!, { exact: true })).toBeVisible();
    await expect(detail.getByText('尚未确认', { exact: true })).toBeVisible();
    expect((await new AxeBuilder({ page }).include('.ant-drawer').withTags(['wcag2a', 'wcag2aa']).analyze()).violations).toEqual([]);
  }
  expect(cursors).toContain(handoff.id); expect(state.commands).toEqual([]);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
});

for (const wrong of [false, true]) test(`Release original approvals retain nullable history: wrong=${wrong}`, async ({ page }) => {
  const state = await fixture(page);
  const approval: Schema['ApprovalViewV1'] = {
    id: id(302), project_id: project.id, candidate_id: release.candidate_id, release_id: release.id,
    downstream_id: id(85), environment: 'PAPER', authority_kind: 'OPERATOR', automation_policy_id: null,
    evidence_set_id: id(303), granted_at: release.created_at, created_at: release.created_at, valid_until: release.valid_until,
    downstream_revision: null, decision_ordinal: null, readiness_observation_id: null,
  };
  const cursors: (string | null)[] = [];
  await page.route('**/api/v2/**', route => {
    const url = new URL(route.request().url());
    if (url.pathname === `/api/v2/projects/${project.id}/releases`) return reply(route, { schema_version: 1, items: [release], next_cursor: null });
    if (url.pathname === `/api/v2/releases/${release.id}`) return reply(route, release);
    if (url.pathname === `/api/v2/releases/${release.id}/approvals`) {
      const cursor = url.searchParams.get('cursor'); cursors.push(cursor);
      return reply(route, { schema_version: 1, items: [{ ...approval, candidate_id: wrong ? id(999) : release.candidate_id, id: cursor ? id(301) : approval.id }], next_cursor: cursor ? null : approval.id });
    }
    return route.fallback();
  });
  await page.goto('/'); await navigate(page, '交付');
  await page.getByRole('combobox', { name: '选择交付所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: project.name }).click();
  await page.getByRole('button', { name: 'Release 00000102', exact: true }).click();
  const detail = page.getByRole('dialog', { name: '原始目标包版本', exact: true });
  await detail.getByText('原审批历史', { exact: true }).click();
  if (wrong) {
    await expect(detail.getByText(approval.id, { exact: true })).toHaveCount(0);
    await expect(detail.getByRole('button', { name: '重新载入', exact: true })).toBeVisible();
  } else {
    await expect(detail.getByText(approval.id, { exact: true })).toBeVisible();
    await detail.getByRole('button', { name: '下一页', exact: true }).click();
    await expect(detail.getByText(id(301), { exact: true })).toBeVisible();
    await detail.getByRole('button', { name: '上一页', exact: true }).click();
    await detail.locator('.ant-table-row-expand-icon').click();
    await expect(detail.getByText(approval.evidence_set_id, { exact: true })).toBeVisible();
    await expect(detail.getByText('历史未记录', { exact: true })).toHaveCount(3);
    expect(cursors).toContain(approval.id);
    expect((await new AxeBuilder({ page }).include('.ant-drawer').withTags(['wcag2a', 'wcag2aa']).analyze()).violations).toEqual([]);
  }
  expect(state.commands).toEqual([]);
});
