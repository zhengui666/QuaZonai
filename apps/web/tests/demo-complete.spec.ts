// Complete credential-free SYNTHETIC interaction. It proves Demo behavior only;
// no account, native model, Runtime, qualification or production delivery executes.
import { readFile } from 'node:fs/promises';
import { expect, test } from '@playwright/test';
import type { Schema } from '../src/api';
import { navigate } from './fixtures';

test.use({ baseURL: 'http://127.0.0.1:4179' });

async function choose(page: import('@playwright/test').Page, label: string, text: string) {
  const field = page.getByRole('combobox', { name: label, exact: true });
  await field.click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: text }).click();
  await expect(field).toHaveAttribute('aria-expanded', 'false');
}

test('credential-free Demo freezes context, starts a synthetic Cycle and opens separate historical research and package evidence', async ({ page }) => {
  test.setTimeout(90_000);
  const failed: string[] = [];
  const starts: string[] = [];
  const hypothesis = 'SYNTHETIC · 完整交互 Demo，不构成真实研究结论';
  page.on('pageerror', error => failed.push(error.message));
  page.on('response', response => {
    if (new URL(response.url()).pathname.startsWith('/api/') && response.status() >= 400) failed.push(`${response.status()} ${response.url()}`);
  });
  page.on('request', request => {
    if (request.method() === 'POST' && /^\/api\/v2\/projects\/[^/]+\/cycles$/.test(new URL(request.url()).pathname)) starts.push(request.url());
  });

  await page.goto('/');
  await expect(page.getByRole('note', { name: '合成预览说明' })).toContainText('SYNTHETIC / FIXTURE');
  await page.getByRole('button', { name: 'SYNTHETIC · 双 Alpha 研究示例', exact: true }).click();
  await page.getByRole('button', { name: '查看冻结版本', exact: true }).click();
  await page.getByRole('button', { name: '以此创建新版本', exact: true }).click();
  const brief = page.getByRole('dialog', { name: 'Brief · 版本 1 的新草稿', exact: true });
  await brief.getByLabel('可检验的假设', { exact: true }).fill(hypothesis);
  await brief.getByRole('button', { name: '保存 Brief 草稿', exact: true }).click();
  await expect(brief).toBeHidden();

  const row = page.getByRole('row').filter({ hasText: hypothesis });
  await expect(row).toHaveCount(1);
  await row.getByRole('button', { name: '冻结执行上下文', exact: true }).click();
  await choose(page, '选择执行 Runtime', 'SYNTHETIC · Demo Runtime');
  for (const purpose of ['DISCOVERY', 'VALIDATION', 'SEALED']) {
    const field = page.getByRole('combobox', { name: `选择 ${purpose} 输入集`, exact: true });
    await field.click();
    await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').first().click();
  }
  const freezing = page.waitForResponse(response => response.request().method() === 'POST'
    && /^\/api\/v2\/briefs\/[^/]+\/freeze$/.test(new URL(response.url()).pathname));
  await page.getByRole('button', { name: '确认冻结 Brief', exact: true }).click();
  const freezeResponse = await freezing;
  expect(freezeResponse.status()).toBe(200);
  const frozen: { resource: Schema['FrozenBriefV1'] } = await freezeResponse.json();
  const freezeRequest: Schema['BriefFreezeV1'] = freezeResponse.request().postDataJSON();
  expect(frozen.resource.brief.state).toBe('FROZEN');
  expect(frozen.resource.brief.revision).toBe(String(BigInt(freezeRequest.expected_revision) + 1n));
  expect(frozen.resource.brief.content.hypothesis).toBe(hypothesis);
  expect(new URL(freezeResponse.url()).pathname).toBe(`/api/v2/briefs/${frozen.resource.brief.id}/freeze`);
  const key = freezeResponse.request().headers()['idempotency-key'];
  expect(key).toBeTruthy();
  const replay = await page.request.post(freezeResponse.url(), { headers: { 'Idempotency-Key': key! }, data: freezeRequest });
  expect(replay.status()).toBe(200);
  expect(await replay.json()).toEqual({ schema_version: 1, resource: frozen.resource, replayed: true });
  const frozenRead = await page.request.get(`/api/v2/briefs/${frozen.resource.brief.id}`);
  expect(frozenRead.status()).toBe(200);
  expect(await frozenRead.json()).toEqual(frozen.resource.brief);
  await expect(page.getByText('Brief 已冻结，尚未启动研究。', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '返回查看记录', exact: true }).click();

  const project = page.getByText('SYNTHETIC · 双 Alpha 研究示例', { exact: true }).first();
  await expect(project).toBeVisible();
  const activate = page.getByRole('button', { name: '修改项目状态', exact: true });
  await activate.click();
  const editor = page.getByRole('dialog', { name: '编辑研究项目', exact: true });
  await editor.getByLabel('项目状态', { exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: '启用' }).click();
  await editor.getByRole('button', { name: '保存项目', exact: true }).click();
  await expect(editor).toBeHidden();

  // Both frozen versions remain visible; select the newly authored one, not row order.
  await expect(row).toHaveCount(1);
  await row.getByRole('button', { name: '启动新 Cycle', exact: true }).click();
  const startup = page.getByRole('dialog', { name: '确认启动研究 Cycle', exact: true });
  await expect(startup).toContainText(`Brief ${frozen.resource.brief.id}`);
  await choose(page, '选择研究者 Codex 配置', 'SYNTHETIC · Demo Researcher');
  await choose(page, '选择独立 Reviewer Codex 配置', 'SYNTHETIC · Demo Reviewer');
  const starting = page.waitForResponse(response => response.request().method() === 'POST'
    && new URL(response.url()).pathname === `/api/v2/projects/${frozen.resource.brief.project_id}/cycles`);
  await startup.getByRole('button', { name: '确认启动 Cycle', exact: true }).click();
  const startResponse = await starting;
  expect(startResponse.status()).toBe(202);
  const request: Schema['CycleStartV1'] = startResponse.request().postDataJSON();
  const started: { resource: Schema['CycleStartedV1'] } = await startResponse.json();
  expect(request.brief_id).toBe(frozen.resource.brief.id);
  expect(started.resource.cycle.brief_id).toBe(frozen.resource.brief.id);
  expect(started.resource.cycle.project_id).toBe(frozen.resource.brief.project_id);
  expect(started.resource.run.cycle_id).toBe(started.resource.cycle.id);
  await expect(page.getByText('Cycle 与准备运行已由服务器登记。', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '返回查看记录', exact: true }).click();
  await page.getByRole('tab', { name: '研究周期', exact: true }).click();
  const cycleRow = page.getByRole('row').filter({ hasText: started.resource.cycle.id });
  await expect(cycleRow).toHaveCount(1);
  await expect(cycleRow).toContainText('NO_SUPPORTED_CANDIDATE');
  await expect(cycleRow).toContainText('本周期未执行实验、没有合格候选');
  await expect(cycleRow.getByRole('button', { name: '查看试验选择', exact: true })).toHaveCount(0);
  await cycleRow.getByRole('button', { name: '查看准备运行', exact: true }).click();
  const runDetails = page.getByRole('dialog', { name: '运行详情', exact: true });
  await expect(runDetails).toContainText(started.resource.run.id);
  await expect(runDetails).toContainText(started.resource.cycle.id);
  await expect(runDetails).toContainText('SYNTHETIC_PRESENTATION_ONLY');
  await expect(runDetails.getByRole('button', { name: '请求取消运行', exact: true })).toBeDisabled();
  await page.keyboard.press('Escape');
  await expect(runDetails).toBeHidden();

  // Historical selection remains available; the new Cycle has no fabricated snapshot.
  const historicalCycleId = '01990000-0000-7000-8000-000000000300';
  const historicalRow = page.getByRole('row').filter({ hasText: historicalCycleId });
  await expect(historicalRow).toHaveCount(1);
  const selecting = page.waitForResponse(response => response.request().method() === 'GET'
    && new URL(response.url()).pathname === `/api/v2/cycles/${historicalCycleId}/selection`);
  await historicalRow.getByRole('button', { name: '查看试验选择', exact: true }).click();
  const selectionResponse = await selecting;
  expect(selectionResponse.status()).toBe(200);
  const selection: Schema['CycleSelectionV1'] = await selectionResponse.json();
  expect(selection.cycle_id).toBe(historicalCycleId);
  expect(selection.cycle_id).not.toBe(started.resource.cycle.id);
  expect(selection.selected_count).toBe('0');
  const selectionDetails = page.getByRole('dialog', { name: '冻结试验选择', exact: true });
  await expect(selectionDetails.getByRole('row').filter({ hasText: 'INVALID_EVIDENCE' })).toHaveCount(2);
  await page.keyboard.press('Escape');
  await expect(selectionDetails).toBeHidden();

  await navigate(page, 'Alpha');
  await page.getByRole('combobox', { name: '选择 Alpha 所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: 'SYNTHETIC · 双 Alpha' }).click();
  await expect(page.getByRole('button', { name: 'SYNTHETIC · 历史展示 1', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'SYNTHETIC · 历史展示 2', exact: true })).toBeVisible();

  await navigate(page, '组合');
  await page.getByRole('combobox', { name: '选择组合所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: 'SYNTHETIC · 双 Alpha' }).click();
  await page.getByRole('tab', { name: '候选快照', exact: true }).click();
  await expect(page.getByRole('button', { name: '01990000-0000-7000-8000-000000000500', exact: true })).toBeVisible();

  await navigate(page, '交付');
  await page.getByRole('combobox', { name: '选择交付所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: 'SYNTHETIC · 双 Alpha' }).click();
  const releaseId = '01990000-0000-7000-8000-000000000510';
  const reading = page.waitForResponse(response => response.request().method() === 'GET'
    && new URL(response.url()).pathname === `/api/v2/releases/${releaseId}`);
  await page.getByRole('button', { name: 'Release 00000510', exact: true }).click();
  const releaseResponse = await reading;
  expect(releaseResponse.status()).toBe(200);
  const release: Schema['ReleaseViewV1'] = await releaseResponse.json();
  expect(release.environment).toBe('DEMO');
  const details = page.getByRole('dialog', { name: '原始目标包版本', exact: true });
  await expect(details.getByText('DEMO 目标包不能用于 Paper 或 Live 审批及交付。', { exact: true })).toBeVisible();
  await expect(details.getByRole('button', { name: '审批此目标包', exact: true })).toBeDisabled();

  // Inspect the original historical package; it is not evidence produced by the new Cycle.
  const downloading = page.waitForEvent('download');
  await details.getByRole('button', { name: '下载原始目标包', exact: true }).click();
  const downloaded = await downloading;
  expect(await downloaded.failure()).toBeNull();
  expect(downloaded.suggestedFilename()).toBe(`${release.package_artifact_id}.bin`);
  const file = await downloaded.path();
  expect(file).not.toBeNull();
  const contents: unknown = JSON.parse(await readFile(file!, 'utf8'));
  expect(contents).toMatchObject({
    release_id: releaseId, project_id: frozen.resource.brief.project_id,
    limitations: expect.arrayContaining([
      expect.stringContaining('SYNTHETIC / FIXTURE'),
      expect.stringContaining('不能审批、登记 Offer 或领取'),
    ]),
  });
  expect(starts).toHaveLength(1);
  expect(failed).toEqual([]);
});
