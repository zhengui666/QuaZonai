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

test('Approval reads all decisions and retries the exact original intent', async ({ page }) => {
  await fixture(page);
  const future = { ...release, valid_until: '2099-01-01T00:00:00Z' };
  const down: Schema['DownstreamView'] = { id: id(85), configuration: { name: 'Approval target', endpoint: 'https://downstream.invalid', accepted_package_versions: ['1'], environments: 'BOTH', enabled: true, development_http: false }, credential_configured: true, revision: '7', created_at: release.created_at, updated_at: release.created_at };
  const writes: { body: unknown; key: string | undefined }[] = []; const cursors: (string | null)[] = [];
  await page.route('**/api/v2/**', route => {
    const request = route.request(); const url = new URL(request.url());
    if (url.pathname === `/api/v2/projects/${project.id}/releases`) return reply(route, { schema_version: 1, items: [future], next_cursor: null });
    if (url.pathname === `/api/v2/releases/${release.id}`) return reply(route, future);
    if (url.pathname === '/api/v2/integrations/downstreams') return reply(route, { schema_version: 1, items: [down], next_cursor: null });
    if (url.pathname === `/api/v2/integrations/downstreams/${down.id}`) return reply(route, down);
    if (url.pathname === `/api/v2/releases/${release.id}/decisions`) {
      const cursor = url.searchParams.get('cursor'); cursors.push(cursor);
      const decision: Schema['ReleaseDecisionViewV1'] = { id: id(402), project_id: project.id, release_id: release.id, candidate_id: release.candidate_id, downstream_id: down.id, environment: 'PAPER', ordinal: 2, decision: 'REOPEN', supersedes_decision_id: id(401), reason_code: 'RECONSIDER', reason: 'Original operator decision', decided_at: release.created_at, created_at: release.created_at, decided_by: 'OPERATOR' };
      return reply(route, { schema_version: 1, items: cursor ? [decision] : [], next_cursor: cursor ? null : id(403) });
    }
    if (url.pathname === `/api/v2/releases/${release.id}/approvals` && request.method() === 'POST') {
      const body = request.postDataJSON() as Schema['ReleaseApproveV1']; writes.push({ body, key: request.headers()['idempotency-key'] });
      if (writes.length === 1) return route.abort('failed');
      const resource: Schema['ApprovalViewV1'] = { id: id(410), project_id: project.id, release_id: release.id, candidate_id: release.candidate_id, downstream_id: body.downstream_id, environment: body.environment, authority_kind: 'OPERATOR', automation_policy_id: null, evidence_set_id: id(411), granted_at: release.created_at, created_at: release.created_at, valid_until: body.valid_until, downstream_revision: body.expected_downstream_revision, decision_ordinal: 2, readiness_observation_id: id(412) };
      return reply(route, { schema_version: 1, resource, replayed: true }, 201);
    }
    return route.fallback();
  });
  await page.goto('/'); await navigate(page, '交付');
  await page.getByRole('combobox', { name: '选择交付所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: project.name }).click();
  await page.getByRole('button', { name: 'Release 00000102', exact: true }).click();
  await page.getByRole('button', { name: '审批此目标包', exact: true }).click();
  const modal = page.getByRole('dialog', { name: '审批原目标包', exact: true });
  await modal.getByRole('combobox', { name: '选择审批下游', exact: true }).click();
  await page.getByText(`Approval target · ${down.id}`, { exact: true }).click();
  await modal.getByRole('combobox', { name: '审批环境', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: 'Paper' }).click();
  await modal.getByLabel('审批截止时间（本地时间）', { exact: true }).fill('2098-01-01T00:00');
  await expect(modal.getByRole('button', { name: '确认审批', exact: true })).toBeEnabled();
  await modal.getByRole('button', { name: '确认审批', exact: true }).click();
  await modal.getByRole('button', { name: '重试同一审批', exact: true }).click();
  await expect(modal.getByText('原审批已保存，尚未发送 Offer。', { exact: true })).toBeVisible();
  expect(cursors).toContain(id(403)); expect(writes).toHaveLength(2); expect(writes[0]).toEqual(writes[1]);
  expect(writes[0]!.body).toEqual({ schema_version: 1, downstream_id: down.id, environment: 'PAPER', expected_downstream_revision: '7', expected_latest_decision_id: id(402), valid_until: '2098-01-01T00:00:00.000Z' });
});

for (const duplicate of [false, true]) test(`Offer preserves original approval and exact predecessor: duplicate=${duplicate}`, async ({ page }) => {
  await fixture(page); const writes: { body: unknown; key: string | undefined }[] = [];
  const future = { ...release, valid_until: '2099-01-01T00:00:00Z' };
  const approval: Schema['ApprovalViewV1'] = { id: id(510), project_id: project.id, candidate_id: release.candidate_id, release_id: release.id, downstream_id: id(85), environment: 'PAPER', authority_kind: 'OPERATOR', automation_policy_id: null, evidence_set_id: id(511), granted_at: release.created_at, created_at: release.created_at, valid_until: future.valid_until, downstream_revision: '7', decision_ordinal: 0, readiness_observation_id: id(512) };
  const previous: Schema['HandoffViewV1'] = { id: id(501), project_id: project.id, candidate_id: duplicate ? release.candidate_id : id(90), mandate_id: release.mandate_id, release_id: duplicate ? release.id : id(91), approval_id: id(92), downstream_id: approval.downstream_id, environment: 'PAPER', delivery_sequence: '9007199254740994', revision: '1', state: 'CLAIMED', supersedes_handoff_id: null, offered_at: release.created_at, expires_at: release.valid_until, claimed_at: release.created_at, external_claim_id: 'previous-claim', acknowledged_at: null };
  await page.route('**/api/v2/**', route => {
    const request = route.request(); const url = new URL(request.url());
    if (url.pathname === `/api/v2/projects/${project.id}/releases`) return reply(route, { schema_version: 1, items: [future], next_cursor: null });
    if (url.pathname === `/api/v2/releases/${release.id}`) return reply(route, future);
    if (url.pathname === `/api/v2/releases/${release.id}/approvals`) return reply(route, { schema_version: 1, items: [approval], next_cursor: null });
    if (url.pathname === `/api/v2/approvals/${approval.id}`) return reply(route, approval);
    if (url.pathname === `/api/v2/projects/${project.id}/handoffs`) {
      const cursor = url.searchParams.get('cursor');
      return reply(route, { schema_version: 1, items: cursor ? [previous] : [{ ...previous, id: id(502), delivery_sequence: '9007199254740993', candidate_id: id(90), release_id: id(91) }], next_cursor: cursor ? null : id(502) });
    }
    if (url.pathname === '/api/v2/handoffs' && request.method() === 'POST') {
      const body = request.postDataJSON() as Schema['HandoffOfferV1']; writes.push({ body, key: request.headers()['idempotency-key'] });
      if (writes.length === 1) return route.abort('failed');
      return reply(route, { schema_version: 1, replayed: true, resource: { ...previous, id: id(520), candidate_id: release.candidate_id, release_id: body.release_id, approval_id: body.approval_id, state: 'OFFERED', supersedes_handoff_id: body.supersedes_handoff_id, expires_at: body.expires_at, claimed_at: null, external_claim_id: null } }, 201);
    }
    return route.fallback();
  });
  await page.goto('/'); await navigate(page, '交付');
  await page.getByRole('combobox', { name: '选择交付所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: project.name }).click();
  await page.getByRole('button', { name: 'Release 00000102', exact: true }).click();
  await page.getByText('原审批历史', { exact: true }).click();
  await page.getByRole('button', { name: '登记 Offer', exact: true }).click();
  const modal = page.getByRole('dialog', { name: '登记原目标 Offer', exact: true });
  await modal.getByLabel('Offer 截止时间（本地时间）', { exact: true }).fill('2098-01-01T00:00');
  if (duplicate) {
    await expect(modal.getByText('原版本已登记 Offer 或原候选已领取，不能重复交付。', { exact: true })).toBeVisible();
    await expect(modal.getByRole('button', { name: '确认登记 Offer', exact: true })).toBeDisabled(); expect(writes).toEqual([]);
  } else {
    await modal.getByRole('button', { name: '确认登记 Offer', exact: true }).click();
    await modal.getByRole('button', { name: '重试同一 Offer', exact: true }).click();
    await expect(modal.getByText('原 Offer 已登记。', { exact: true })).toBeVisible();
    expect(writes).toHaveLength(2); expect(writes[0]).toEqual(writes[1]);
    expect(writes[0]!.body).toEqual({ schema_version: 1, release_id: release.id, approval_id: approval.id, supersedes_handoff_id: previous.id, expires_at: '2098-01-01T00:00:00.000Z' });
  }
  expect((await new AxeBuilder({ page }).include('.ant-modal').withTags(['wcag2a', 'wcag2aa']).analyze()).violations).toEqual([]);
});

test('Approval revocation keeps latest history and replays immediate intent', async ({ page }) => {
  await fixture(page); const writes: { body: unknown; key: string | undefined }[] = [];
  const approval: Schema['ApprovalViewV1'] = { id: id(610), project_id: project.id, candidate_id: release.candidate_id, release_id: release.id, downstream_id: id(85), environment: 'PAPER', authority_kind: 'OPERATOR', automation_policy_id: null, evidence_set_id: id(611), granted_at: release.created_at, created_at: release.created_at, valid_until: release.valid_until, downstream_revision: '7', decision_ordinal: 0, readiness_observation_id: id(612) };
  const old: Schema['ApprovalRevocationViewV1'] = { id: id(620), approval_id: approval.id, created_at: release.created_at, effective_at: '2099-01-01T00:00:00Z', reason_code: null, reason: 'Original scheduled revocation' };
  await page.route('**/api/v2/**', route => {
    const request = route.request(); const url = new URL(request.url());
    if (url.pathname === `/api/v2/projects/${project.id}/releases`) return reply(route, { schema_version: 1, items: [release], next_cursor: null });
    if (url.pathname === `/api/v2/releases/${release.id}`) return reply(route, release);
    if (url.pathname === `/api/v2/releases/${release.id}/approvals`) return reply(route, { schema_version: 1, items: [approval], next_cursor: null });
    if (url.pathname === `/api/v2/approvals/${approval.id}/revocations`) return reply(route, { schema_version: 1, items: [old], next_cursor: null });
    if (url.pathname === `/api/v2/approvals/${approval.id}/revoke`) {
      const body = request.postDataJSON() as Schema['ApprovalRevokeV1']; writes.push({ body, key: request.headers()['idempotency-key'] });
      if (writes.length === 1) return route.abort('failed');
      return reply(route, { schema_version: 1, replayed: true, resource: { ...old, id: id(621), effective_at: release.created_at, reason_code: body.reason_code, reason: body.reason } }, 201);
    }
    return route.fallback();
  });
  await page.goto('/'); await navigate(page, '交付');
  await page.getByRole('combobox', { name: '选择交付所属项目', exact: true }).click();
  await page.locator('.ant-select-dropdown:visible .ant-select-item-option-content').filter({ hasText: project.name }).click();
  await page.getByRole('button', { name: 'Release 00000102', exact: true }).click();
  await page.getByText('原审批历史', { exact: true }).click(); await page.getByRole('button', { name: '撤销审批', exact: true }).click();
  const modal = page.getByRole('dialog', { name: '撤销原审批', exact: true });
  await expect(modal.getByText(old.reason, { exact: true })).toBeVisible();
  await modal.getByLabel('原因代码', { exact: true }).fill('OPERATOR_REVOKED');
  await modal.getByLabel('撤销原因', { exact: true }).fill('Stop future delivery, retain original claims');
  await modal.getByRole('button', { name: '确认追加撤销', exact: true }).click();
  await modal.getByRole('button', { name: '重试同一撤销', exact: true }).click();
  await expect(modal.getByText('原撤销已追加。', { exact: true })).toBeVisible();
  expect(writes).toHaveLength(2); expect(writes[0]).toEqual(writes[1]);
  expect(writes[0]!.body).toEqual({ schema_version: 1, expected_latest_revocation_id: old.id, effective_at: null, reason_code: 'OPERATOR_REVOKED', reason: 'Stop future delivery, retain original claims' });
  expect((await new AxeBuilder({ page }).include('.ant-modal').withTags(['wcag2a', 'wcag2aa']).analyze()).violations).toEqual([]);
});
