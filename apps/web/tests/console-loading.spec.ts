import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { Schema } from '../src/api';
import { validateResponse } from '../src/generated/responses.cjs';

// Synthetic presentation/transport fixtures only. Native API acceptance stays
// in the independently provisioned Rust/PostgreSQL browser harness.
async function session(page: Page, failProjects = false) {
  let reads = 0;
  await page.route('**/api/**', async route => {
    const path = new URL(route.request().url()).pathname;
    const reply = (json: unknown) => route.fulfill({ json });
    if (path === '/api/v2/auth/status') return reply({ schema_version: 1, setup_required: false });
    if (path === '/api/v2/auth/session') return reply({ schema_version: 1,
      authenticated_at: new Date().toISOString(), expires_at: new Date(Date.now() + 43_200_000).toISOString() });
    if (path === '/api/v2/projects') {
      reads++;
      if (failProjects && reads === 1) return route.fulfill({ status: 429,
        headers: { 'Content-Type': 'application/problem+json', 'Retry-After': '2' },
        body: JSON.stringify({ type: 'urn:quazonai:problem:rate-limited', title: 'RATE_LIMITED',
          status: 429, code: 'RATE_LIMITED', detail: '请求过于频繁。',
          request_id: '01990000-0000-7000-8000-000000000001', retryable: true,
          field_errors: [], safe_next_actions: [] }),
      });
      return reply({ schema_version: 1, items: [], next_cursor: null });
    }
    if (path === '/api/v2/runs') return reply({ schema_version: 1, items: [], next_cursor: null });
    return route.abort('blockedbyclient');
  });
  return () => reads;
}

test('an unopened section is deferred; a failed chunk leaves navigation usable', async ({ page }) => {
  await session(page);
  let requested = 0;
  await page.route('**/src/portfolio.tsx', route => { requested++; return route.abort('failed'); });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
  expect(requested).toBe(0);
  await page.getByRole('menuitem', { name: '组合', exact: true }).click();
  await expect(page.getByRole('heading', { name: '页面暂时无法显示' })).toBeVisible();
  expect(requested).toBe(1);
  await expect(page.getByRole('menu', { name: '主导航' })).toBeVisible();
  await page.getByRole('menuitem', { name: '研究', exact: true }).click();
  await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: '页面暂时无法显示' })).toHaveCount(0);
});

test('late section loading cannot replace a newer navigation choice', async ({ page }) => {
  await session(page);
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  let requested = false;
  await page.route('**/src/settings.tsx', async route => {
    requested = true; await held; await route.continue();
  });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  await expect.poll(() => requested).toBe(true);
  await expect(page.getByRole('status', { name: '正在载入页面' })).toBeVisible();
  await page.getByRole('menuitem', { name: '研究', exact: true }).click();
  await expect(page.getByRole('heading', { name: '研究', exact: true })).toBeVisible();
  const loaded = page.waitForResponse(response => new URL(response.url()).pathname === '/src/settings.tsx');
  release();
  await (await loaded).finished();
  await expect(page.getByRole('heading', { name: '设置', exact: true })).toHaveCount(0);
  await expect(page.getByRole('menuitem', { name: '研究', exact: true })).toHaveClass(/ant-menu-item-selected/);
});

test('an idle editor has no error-clock interval and keeps its fields on theme changes', async ({ page }) => {
  await session(page);
  await page.addInitScript(() => {
    const active = new Set<number>();
    Object.defineProperty(window, '__testOneSecondIntervals', { value: active });
    const schedule = window.setInterval.bind(window);
    const cancel = window.clearInterval.bind(window);
    window.setInterval = ((...args: Parameters<typeof window.setInterval>) => {
      const id = schedule(...args); if (args[1] === 1000) active.add(id); return id;
    }) as typeof window.setInterval;
    window.clearInterval = id => { if (typeof id === 'number') active.delete(id); cancel(id); };
  });
  await page.goto('/');
  await page.getByRole('button', { name: '新建研究', exact: true }).click();
  const editor = page.getByRole('dialog', { name: '新建研究项目', exact: true });
  const name = editor.getByLabel('研究名称');
  await name.fill('保留未提交的研究');
  expect(await page.evaluate(() => (window as unknown as {
    __testOneSecondIntervals: Set<number>;
  }).__testOneSecondIntervals.size)).toBe(0);
  // Changing the system theme exercises the same non-remounting preference
  // update without clicking through the editor's native modal mask.
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect(name).toHaveValue('保留未提交的研究');
  await page.getByRole('button', { name: '关闭', exact: true }).click();
  await page.getByRole('button', { name: '继续编辑', exact: true }).click();
  await expect(name).toHaveValue('保留未提交的研究');
});

test('retry countdown expires without replaying the request automatically', async ({ page }) => {
  const reads = await session(page, true);
  await page.clock.install();
  await page.goto('/');
  const retry = page.getByRole('button', { name: '重新载入', exact: true });
  await expect(retry).toBeDisabled();
  await expect(page.getByText('2 秒后可重试', { exact: true })).toBeVisible();
  await page.clock.fastForward(3000);
  await expect(retry).toBeEnabled();
  await expect(page.getByText(/秒后可重试/)).toHaveCount(0);
  expect(reads()).toBe(1);
  await retry.click();
  await expect(page.getByText('你的下一个研究，从这里开始', { exact: true })).toBeVisible();
  expect(reads()).toBe(2);
});

test('research workbench offers an actionable empty state and guarded shortcuts', async ({ page }) => {
  await session(page);
  await page.goto('/');
  await expect(page.getByRole('heading', { name: '研究项目', exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: '你的下一个研究，从这里开始' })).toBeVisible();
  await expect(page.getByLabel('搜索本页研究项目')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '下一页', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: '创建第一个研究' }).click();
  await expect(page.getByRole('dialog', { name: '新建研究项目' })).toBeVisible();
  await page.getByRole('button', { name: '取消', exact: true }).click();
  await page.getByRole('button', { name: '运行记录 追踪执行、状态与回执' }).click();
  await expect(page.getByRole('heading', { name: '运行', exact: true })).toBeVisible();
});

test('project cards search only the loaded page and clear the filter when paging', async ({ page }) => {
  await session(page);
  const project = (index: number, name: string): Schema['ProjectView'] => ({
    id: `01990000-0000-7000-8000-00000000000${index}`, name, description: 'Research evidence', state: 'DRAFT',
    created_by: 'OPERATOR', revision: '1', root_lineage_id: `01990000-0000-7000-8000-00000000000${index}`,
    created_at: '2026-10-01T00:00:00Z', updated_at: '2026-10-01T00:00:00Z', current_brief_id: null,
  });
  await page.route('**/api/v2/projects?*', route => {
    const next = new URL(route.request().url()).searchParams.has('cursor');
    return route.fulfill({ json: { schema_version: 1,
      items: next ? [project(3, '第三个研究')] : [project(1, '动量研究'), project(2, '均值回归')], next_cursor: next ? null : '01990000-0000-7000-8000-000000000002' } });
  });
  await page.goto('/');
  const search = page.getByLabel('搜索本页研究项目');
  await expect(page.getByRole('article')).toHaveCount(2);
  await search.fill('动量');
  await expect(page.getByRole('article')).toHaveCount(1);
  await expect(page.getByRole('button', { name: '动量研究', exact: true })).toBeVisible();
  await search.fill('不存在');
  await expect(page.getByRole('heading', { name: '本页没有匹配的项目' })).toBeVisible();
  await page.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(search).toHaveValue('');
  await expect(page.getByRole('button', { name: '第三个研究', exact: true })).toBeVisible();
});

// Presentation contracts only; real Rust acceptance remains in native-console.
async function researchWorkspace(page: Page, options: { archived?: boolean; current?: boolean; foreignBrief?: boolean; draft?: boolean } = {}) {
  const id = (suffix: number) => `01990000-0000-7000-8000-${String(suffix).padStart(12, '0')}`;
  const now = '2026-10-01T00:00:00Z';
  const budget: Schema['BudgetV1'] = { schema_version: 1, max_experiments: 10, max_parallel_runs: 1, max_turns_per_mission: 10,
    max_repair_turns: 2, max_wall_seconds: 3600, max_cpu_seconds: '3600', max_memory_mib: 4096,
    max_output_bytes: '104857600', max_cycles_per_day: 1, min_cycle_interval_seconds: 3600, cost_enforcement: 'UNAVAILABLE' };
  const project: Schema['ProjectView'] = { id: id(1), root_lineage_id: id(1), name: '项目工作流验收', description: 'Synthetic interaction fixture',
    state: options.archived ? 'ARCHIVED' : 'ACTIVE', revision: '1', created_by: 'OPERATOR', created_at: now, updated_at: now,
    current_brief_id: options.current ? id(2) : null };
  const brief: Schema['BriefView'] = { id: id(2), project_id: options.foreignBrief ? id(9) : project.id, version: 2, revision: '1', state: options.draft ? 'DRAFT' : 'FROZEN',
    created_at: now, updated_at: now, bindings: [{ dataset_revision_id: id(8), role: 'DISCOVERY', access_policy: 'METADATA_ONLY' }], content: { hypothesis: '验证真实引用而非自动选择首个版本', economic_rationale: '隔离的交互测试说明',
      universe_version_id: id(3), evaluation_policy_id: id(4), execution_assumptions_id: id(5), target_kind: 'SCORE',
      horizon_kind: 'FIXED_BARS', horizon_value: '1', base_currency: 'USD', budget,
      stop_rule: { schema_version: 1, stop_on_qualified_count: 2, stop_on_budget: true, stop_on_invalid_data: true } } };
  const cycle: Schema['CycleViewV1'] = { schema_version: 1, id: id(6), project_id: project.id, brief_id: brief.id, ordinal: 1, revision: '1',
    trigger: 'OPERATOR', state: 'WAITING_INPUT', budget, reserved_experiments: 1, used_experiments: 2, reserved_cpu_seconds: '0',
    created_at: now, next_action: '核对输入边界', initial_run_id: id(7), available_actions: [] };
  expect(validateResponse('/api/v2/projects/{id}', 'GET', 200, project, 'application/json')).toBe(true);
  expect(validateResponse('/api/v2/briefs/{id}', 'GET', 200, brief, 'application/json')).toBe(true);
  expect(validateResponse('/api/v2/projects/{id}/cycles', 'GET', 200, { schema_version: 1, items: [cycle], next_cursor: null }, 'application/json')).toBe(true);
  const requestedInputs: string[] = [];
  await page.route('**/api/**', route => {
    const url = new URL(route.request().url()); const path = url.pathname;
    const reply = (json: unknown) => route.fulfill({ json });
    const listing = (items: unknown[]) => reply({ schema_version: 1, items, next_cursor: null });
    if (path === '/api/v2/auth/status') return reply({ schema_version: 1, setup_required: false });
    if (path === '/api/v2/auth/session') return reply({ schema_version: 1, authenticated_at: now, expires_at: '2099-01-01T00:00:00Z' });
    if (path === '/api/v2/projects') return listing([project]);
    if (path === `/api/v2/projects/${project.id}`) return reply(project);
    if (path === `/api/v2/projects/${project.id}/briefs`) return listing([brief]);
    if (path === `/api/v2/briefs/${brief.id}`) {
      if (route.request().method() === 'PATCH') {
        const body = route.request().postDataJSON() as Schema['BriefUpdate'];
        brief.content = body.content; brief.bindings = body.bindings; brief.revision = '2';
        return reply({ schema_version: 1, resource: brief, replayed: false });
      }
      return reply(brief);
    }
    if (path === `/api/v2/projects/${project.id}/cycles`) return listing([cycle]);
    if (path === '/api/v2/input-sets') { requestedInputs.push(url.searchParams.get('project_id') ?? ''); return listing([]); }
    if (['/api/v2/data/revisions', '/api/v2/integrations/runtimes'].includes(path)) return listing([]);
    return route.abort('blockedbyclient');
  });
  await page.goto('/');
  await page.getByRole('button', { name: project.name, exact: true }).click();
  await expect(page.getByRole('heading', { name: project.name, exact: true })).toBeVisible();
  return { project, brief, cycle, requestedInputs };
}

test('project overview never substitutes an available Brief for the actual current reference', async ({ page }) => {
  const state = await researchWorkspace(page);
  await expect(page.getByRole('heading', { name: '尚未选择当前 Brief' })).toBeVisible();
  await expect(page.getByText(state.brief.content.hypothesis, { exact: true })).toHaveCount(0);
  await page.getByRole('tab', { name: '研究 Brief', exact: true }).click();
  await expect(page.getByRole('heading', { name: state.brief.content.hypothesis, exact: true })).toBeVisible();
  await expect(page.getByText('当前版本', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '启动新 Cycle', exact: true })).toBeEnabled();
});

test('project-scoped inputs reuse the chosen project and cannot be unmounted during an operation', async ({ page }) => {
  const state = await researchWorkspace(page);
  await page.getByRole('tab', { name: '冻结输入', exact: true }).click();
  await expect(page.getByLabel('冻结输入所属研究项目', { exact: true })).toHaveCount(0);
  await expect.poll(() => state.requestedInputs.length).toBeGreaterThan(0);
  expect(state.requestedInputs.every(id => id === state.project.id)).toBe(true);
  await page.getByRole('button', { name: '新建冻结输入', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '创建并冻结项目输入', exact: true });
  await expect(dialog).toBeVisible();
  await dialog.getByLabel('决策截止（精确 UTC）', { exact: true }).fill('2026-01-01T00:00:00Z');
  // Exercise the navigation guard directly; the normal dialog mask also blocks pointer input.
  await page.getByRole('tab', { name: '研究 Brief', exact: true }).dispatchEvent('click');
  await page.getByRole('button', { name: '返回研究列表', exact: true }).dispatchEvent('click');
  await expect(page.getByRole('tab', { name: '冻结输入', exact: true })).toHaveAttribute('aria-selected', 'true');
  await expect(dialog.getByLabel('决策截止（精确 UTC）', { exact: true })).toHaveValue('2026-01-01T00:00:00Z');
});

test('archived research retains readable frozen Briefs without allowing a new cycle', async ({ page }) => {
  const state = await researchWorkspace(page, { archived: true, current: true });
  await expect(page.getByRole('heading', { name: state.brief.content.hypothesis, exact: true })).toBeVisible();
  await page.getByRole('tab', { name: '研究 Brief', exact: true }).click();
  await expect(page.getByText('当前版本', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '新建 Brief 草稿', exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: '启动新 Cycle', exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: '查看冻结版本', exact: true })).toBeEnabled();
});

test('cycle cards show server next actions without inventing permissions from a run ID', async ({ page }) => {
  await researchWorkspace(page);
  await page.getByRole('tab', { name: '研究周期', exact: true }).click();
  await expect(page.getByText('核对输入边界', { exact: true })).toBeVisible();
  await expect(page.getByText('尚无周期结论', { exact: true })).toBeVisible();
  await expect(page.getByText('实验 已用 / 预约：2 / 1', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '查看准备运行', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '查看试验选择', exact: true })).toHaveCount(0);
});

test('current Brief from another project is rejected rather than displayed as evidence', async ({ page }) => {
  const state = await researchWorkspace(page, { current: true, foreignBrief: true });
  await expect(page.getByText('Brief 不属于当前研究项目', { exact: true })).toBeVisible();
  await expect(page.getByText(state.brief.content.hypothesis, { exact: true })).toHaveCount(0);
});


test('Brief authoring retains dirty content when project navigation is requested', async ({ page }) => {
  await researchWorkspace(page, { draft: true, current: true });
  await page.getByRole('tab', { name: '研究 Brief', exact: true }).click();
  await page.getByRole('button', { name: '查看 / 编辑', exact: true }).click();
  const editor = page.getByRole('dialog', { name: 'Brief · 版本 2', exact: true });
  await editor.getByLabel('可检验的假设', { exact: true }).fill('保留尚未提交的假设');
  for (const width of [390, 768, 1280]) {
    await page.setViewportSize({ width, height: 900 });
    await expect(editor.getByLabel('可检验的假设', { exact: true })).toHaveValue('保留尚未提交的假设');
  }
  await page.getByRole('tab', { name: '研究周期', exact: true }).dispatchEvent('click');
  await page.getByRole('button', { name: '返回研究列表', exact: true }).dispatchEvent('click');
  await expect(page.getByRole('tab', { name: '研究 Brief', exact: true })).toHaveAttribute('aria-selected', 'true');
  await expect(editor.getByLabel('可检验的假设', { exact: true })).toHaveValue('保留尚未提交的假设');
});


test('saving the current Brief invalidates the overview before returning to it', async ({ page }) => {
  const state = await researchWorkspace(page, { draft: true, current: true });
  // Warm the original overview cache before unmounting it; otherwise even an
  // unrelated query key could fetch fresh data on its first successful read.
  await expect(page.getByRole('heading', { name: state.brief.content.hypothesis, exact: true })).toBeVisible();
  await page.getByRole('tab', { name: '研究 Brief', exact: true }).click();
  await page.getByRole('button', { name: '查看 / 编辑', exact: true }).click();
  const editor = page.getByRole('dialog', { name: 'Brief · 版本 2', exact: true });
  await editor.getByLabel('可检验的假设', { exact: true }).fill('保存后应立即反映在工作概览中的假设');
  await editor.getByRole('button', { name: '保存 Brief 草稿', exact: true }).click();
  await expect(editor).toBeHidden();
  await page.getByRole('tab', { name: '工作概览', exact: true }).click();
  await expect(page.getByRole('heading', { name: '保存后应立即反映在工作概览中的假设', exact: true })).toBeVisible();
});
