// Presentation fixtures only; native window aggregation is verified in Rust/PG.
import { expect, test } from '@playwright/test';
import type { Schema } from '../src/api';
import { fixture, id, navigate, project, reply } from './fixtures';

test('Forward preserves correction evidence and rejects another stream window', async ({ page }) => {
  await fixture(page);
  const at = '2026-09-15T00:00:00Z';
  const message: Schema['ForwardMessageViewV1'] = {
    id: id(600), project_id: project.id, downstream_id: id(601), release_id: id(602), handoff_id: id(603),
    external_message_id: 'controlled-correction', stream_id: 'paper daily / A', sequence: '2', message_revision: 2,
    supersedes_message_id: id(599), coverage_status: 'CORRECTION', observation_count: '9007199254740993',
    report_artifact_id: id(604), issued_at: at, received_at: at, window_start: at, window_end: '2026-09-16T00:00:00Z',
  };
  let wrongStream = true;
  await page.route('**/api/v2/**', route => {
    const url = new URL(route.request().url());
    if (url.pathname === `/api/v2/projects/${project.id}/forward`) return reply(route, { schema_version: 1, items: [message], next_cursor: null });
    if (url.pathname === `/api/v2/handoffs/${message.handoff_id}/forward-window`) {
      expect(url.searchParams.get('stream_id')).toBe(message.stream_id);
      const window: Schema['ForwardWindowViewV1'] = {
        handoff_id: message.handoff_id, stream_id: wrongStream ? 'wrong stream' : message.stream_id,
        latest_message_ids: [message.id], complete_observations: '0', is_contiguous: false,
        reason_codes: ['SEQUENCE_GAP', 'PARTIAL'], returns_frequency: null, window_start: null, window_end: null,
      };
      return reply(route, window);
    }
    return route.fallback();
  });
  await page.goto('/'); await navigate(page, '交付');
  await page.getByRole('combobox', { name: '选择交付所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: project.name }).click();
  await page.getByRole('tab', { name: 'Forward 证据', exact: true }).click();
  await expect(page.getByRole('cell', { name: '9007199254740993', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Forward 00000600', exact: true }).click();
  const detail = page.getByRole('dialog', { name: 'Forward 原消息与当前窗口', exact: true });
  await expect(detail.getByText(message.supersedes_message_id!, { exact: true })).toBeVisible();
  await expect(detail.getByText('请求未完成，请重试并检查服务状态。', { exact: true })).toBeVisible();
  await expect(detail.getByText('SEQUENCE_GAP · PARTIAL', { exact: true })).toBeHidden();
  wrongStream = false;
  await detail.getByRole('button', { name: '刷新连续窗口', exact: true }).click();
  await expect(detail.getByText('SEQUENCE_GAP · PARTIAL', { exact: true })).toBeVisible();
  await expect(detail.getByText('不连续或证据不足', { exact: true })).toBeVisible();
  await expect(detail.getByText('未提供', { exact: true })).toBeVisible();
  wrongStream = true;
  await detail.getByRole('button', { name: '刷新连续窗口', exact: true }).click();
  await expect(detail.getByText('以下是上次成功读取的数据，当前无法确认其最新状态。', { exact: true })).toBeVisible();
  await expect(detail.getByText('SEQUENCE_GAP · PARTIAL', { exact: true })).toBeVisible();
  await expect(detail.getByText('请求未完成，请重试并检查服务状态。', { exact: true })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(detail).toBeHidden();
});
