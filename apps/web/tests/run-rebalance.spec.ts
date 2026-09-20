// Controlled display fixtures; native provenance is tested against PostgreSQL separately.
import { expect, test } from '@playwright/test';
import type { Schema } from '../src/api';
import { fixture, id, navigate, reply } from './fixtures';

for (const present of [false, true]) test(`Run shows original rebalance facts or explicit absence: ${present}`, async ({ page }) => {
  const state = await fixture(page);
  state.run.kind = 'PORTFOLIO_BUILD';
  const origin: Schema['RunRebalanceViewV1'] = { schema_version: 1, rebalance: present ? {
    build_run_id: state.run.id, policy_id: id(61), downstream_id: id(62), source_candidate_id: id(63),
    decision_cutoff: '2026-09-15T00:00:00Z', created_at: '2026-09-15T00:00:01Z', study_run_id: id(64), release_id: id(65),
    request: { schema_version: 1, cycle_id: id(66), mandate_id: id(67), input_set_id: id(68), runtime_id: id(69), expected_runtime_revision: '9007199254740993',
      current_weights_source: { kind: 'FORWARD_SNAPSHOT', snapshot_id: id(70) }, environment: 'PAPER',
      members: [{ qualification_id: id(71), ensemble_weight: '0.4' }, { qualification_id: id(72), ensemble_weight: '0.6' }],
      limits: { schema_version: 1, experiments: 0, cpu_seconds: '60', wall_seconds: 60, memory_mib: 1024, output_bytes: '1048576' } },
  } : null };
  await page.route('**/api/v2/runs/*/rebalance', route => reply(route, origin));
  await page.goto('/'); await navigate(page, '运行');
  await page.getByRole('button', { name: 'PORTFOLIO_BUILD · 00000003', exact: true }).click();
  await expect(page.getByText('自动再平衡来源', { exact: true })).toBeVisible();
  if (present) {
    await expect(page.getByText(id(61), { exact: true })).toBeVisible();
    await expect(page.getByText(id(65), { exact: true })).toBeVisible();
    await expect(page.getByText(`${id(69)} / 9007199254740993`, { exact: true })).toBeVisible();
    await page.getByText('原 Build 请求（成员、权重来源与限额）', { exact: true }).click();
    await expect(page.locator('pre').filter({ hasText: 'FORWARD_SNAPSHOT' })).toContainText(id(71));
  } else {
    await expect(page.getByText('无自动再平衡关联')).toBeVisible();
  }
});
