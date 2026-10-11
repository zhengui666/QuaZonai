import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

// Demo transport/presentation data only. This suite makes no model or scientific
// backend calls; real Rust -> official Host acceptance belongs to the native suite.
const model = { provider: 'test-provider', modelId: 'plugin-general-test', name: 'Demo model (test only)', reasoning: true };
const input = { goal: 'Demo research — test-only owner-defined plugin goal', model: { provider: model.provider, modelId: model.modelId },
  data: { owner_defined_plugin: { ref: 'test-only-owner-input', decimal: '9007199254740993' } }, policy: { max_trials: 3 } };
const metric = { counts: { observations: '9007199254740993', missing: '1' }, pearson_ic: { status: 'INSUFFICIENT', value: null } };
const summary = { request: { owner_ref: 'test-only' }, native_versions: { method: 'official-test-version' }, methods: { pearson_ic: 'native' }, consumed_fuel: '71',
  partitions: [{ partition: 'VALIDATION', start_ns: '1234567890123456789', end_ns: '1234567890123456790', metric,
    assets: [{ instrument_id: 'test-only-owner-instrument', metric }] }] };
const view = { conversation: { id: 0 }, entries: [{ id: 1, conversationId: 0, kind: 'demo', data: { message: 'Demo native entry' } }],
  docs: { 'pi.live': { run: { taskId: 3, inputs: [1] }, generation: { attempt: 1 } }, 'pi.usage': { models: { 'test/model': { input: 0, output: 14, totalTokens: 14, reasoning: null, cost: { total: null } } }, tools: {} } },
  research: { input, trials: [
    { taskId: 5, proposal: { hypothesis: 'Demo hypothesis — test only', plugin_payload: 'x'.repeat(600) }, status: 'COMPUTED', result: summary, error: null },
    { taskId: 6, proposal: { hypothesis: 'Demo failed hypothesis' }, status: 'FAILED', result: null, error: 'native_test_failure' },
  ], final: { taskId: 7, trialId: 5, comparedTrials: [5, 6], reason: 'Demo native selection reason', status: 'COMPUTED', result: { ...summary, partitions: [{ ...summary.partitions[0], partition: 'HELDOUT' }] }, error: null } } };

async function setup(page: Page, options: { emptyModels?: boolean; lostAck?: boolean; rejection?: boolean; emptyList?: boolean; malformedModels?: boolean } = {}) {
  const writes: { path: string; bytes: string | null; body: unknown }[] = []; const cursors: (string | null)[] = [];
  let rejectRetry = false; let malformedModels = !!options.malformedModels; let failView = false;
  await page.route('**/api/**', async route => {
    const request = route.request(); const url = new URL(request.url()); const path = url.pathname;
    const reply = (json: unknown, status = 200) => route.fulfill({ status, json });
    if (path === '/api/v2/auth/status') return reply({ schema_version: 1, setup_required: false });
    if (path === '/api/v2/auth/session') return reply({ schema_version: 1, authenticated_at: '2026-10-01T00:00:00Z', expires_at: '2099-01-01T00:00:00Z' });
    if (path === '/api/v2/auth/cli/devices') return reply([]);
    if (path === '/api/v2/projects') return reply({ schema_version: 1, items: [], next_cursor: null });
    if (path === '/api/v2/research/models') return reply(malformedModels ? [{ provider: 'invalid' }] : options.emptyModels ? [] : [model]);
    if (path === '/api/v2/research/tasks') return reply({ scheduling: 'running', tasks: [{ record: { id: 3, conversationId: 0, kind: 'pi.generate', state: { status: 'running' } }, state: { kind: 'waiting', on: [5] } }], submissions: [] });
    if (path === '/api/v2/research/usage') return reply(view.docs['pi.usage']);
    if (path === '/api/v2/research' && request.method() === 'GET') {
      cursors.push(url.searchParams.get('cursor'));
      return reply(options.emptyList ? { items: [] } : url.searchParams.has('cursor') ? { items: [{ id: 10 }] } : { items: [{ id: 0 }], cursor: { order: 'descending', after: 0 } });
    }
    if (path === '/api/v2/research/0' || path === '/api/v2/research/10') {
      if (failView) { failView = false; return route.abort('failed'); }
      return reply({ ...view, conversation: { id: Number(path.split('/').at(-1)) } });
    }
    if (request.method() === 'POST') {
      writes.push({ path, bytes: request.postData(), body: request.postDataJSON() });
      if (path === '/api/v2/research') {
        if (options.lostAck && writes.filter(write => write.path === path).length === 1) return route.abort('failed');
        if (options.rejection || rejectRetry) { rejectRetry = false; return reply({ error: 'request_id_conflict' }, 409); }
        return reply({ conversationId: 0, submissionId: 1 }, 202);
      }
      if (path === '/api/v2/research/0/stop') return reply(view, 202);
    }
    return route.abort('blockedbyclient');
  });
  await page.goto('/');
  await expect(page.getByRole('heading', { level: 1, name: '研究', exact: true })).toBeVisible();
  return { writes, cursors, rejectNextRetry: () => { rejectRetry = true; }, breakModels: () => { malformedModels = true; }, failNextView: () => { failView = true; } };
}
async function fill(page: Page) {
  await page.getByLabel('研究目标', { exact: true }).fill(input.goal);
  const choice = page.getByRole('combobox', { name: '可用模型', exact: true });
  await expect(choice).toBeEnabled(); await choice.click(); await page.getByRole('option', { name: /Demo model/ }).click();
  await page.getByLabel('数据输入 · JSON 对象', { exact: true }).fill(JSON.stringify(input.data));
  await page.getByLabel('研究政策 · JSON 对象', { exact: true }).fill(JSON.stringify(input.policy));
}

test('empty availability gives concrete Host configuration guidance without collecting model keys', async ({ page }) => {
  const state = await setup(page, { emptyModels: true, emptyList: true });
  await expect(page.getByText('Host 尚无可用模型', { exact: true })).toBeVisible();
  await expect(page.getByText(/QZ_RESEARCH_CONFIG.*providerModules/)).toBeVisible();
  await expect(page.getByRole('button', { name: '提交研究', exact: true })).toBeDisabled();
  await expect(page.getByLabel('研究目标', { exact: true })).toHaveValue('');
  await expect(page.getByLabel('数据输入 · JSON 对象', { exact: true })).toHaveValue('');
  await expect(page.getByRole('button', { name: '登录 ChatGPT', exact: true })).toHaveCount(0);
  await page.getByRole('menuitem', { name: '设置', exact: true }).click();
  await expect(page.getByRole('heading', { name: '鉴权管理', exact: true })).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Codex', exact: true })).toHaveCount(0);
  expect(state.writes).toEqual([]);
});

test('exact native request intent survives timeout, study inspection and a later rejection until its 202 receipt', async ({ page }) => {
  const state = await setup(page, { lostAck: true }); await fill(page);
  await page.getByRole('button', { name: '提交研究', exact: true }).click();
  await expect(page.getByRole('button', { name: '重试同一研究请求' })).toBeVisible();
  expect(state.writes).toHaveLength(1);
  const captured = state.writes[0]!.body as { requestId: string; input: typeof input };
  expect(captured.input).toEqual(input); expect(captured.requestId).toBeTruthy();
  await expect(page.getByLabel('研究目标', { exact: true })).toBeDisabled();
  await page.getByRole('button', { name: /研究 #0 原生会话/ }).click();
  await expect(page.getByRole('heading', { name: '研究详情', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '新建研究', exact: true }).click();
  await expect(page.getByLabel('研究目标', { exact: true })).toHaveValue(input.goal);
  state.rejectNextRetry(); await page.getByRole('button', { name: '重试同一研究请求' }).click();
  await expect(page.getByText('原生研究错误：request_id_conflict', { exact: true })).toBeVisible();
  await expect(page.getByLabel('研究目标', { exact: true })).toBeDisabled();
  await page.getByRole('button', { name: '重试同一研究请求' }).click();
  await expect(page.getByText('研究提交已被接纳', { exact: true }).first()).toBeVisible();
  expect(state.writes).toHaveLength(3); expect(state.writes.map(write => write.bytes)).toEqual(Array(3).fill(state.writes[0]!.bytes));
  expect(await page.evaluate(() => JSON.stringify([localStorage, sessionStorage]))).not.toContain(captured.requestId);
  await page.getByRole('button', { name: '新建研究', exact: true }).click();
  await expect(page.getByLabel('研究目标', { exact: true })).toHaveValue('');
  await fill(page); await page.getByRole('button', { name: '提交研究', exact: true }).click();
  await expect(page.getByRole('heading', { name: '研究详情', exact: true })).toBeVisible();
  expect((state.writes[3]!.body as { requestId: string }).requestId).not.toBe(captured.requestId);
});

test('native cursors, metrics, failures, final selection and 202 stop requests remain truthful', async ({ page }) => {
  const state = await setup(page);
  await page.getByRole('button', { name: '下一页', exact: true }).click();
  await expect(page.getByRole('button', { name: /研究 #10/ })).toBeVisible();
  expect(JSON.parse(state.cursors.at(-1)!)).toEqual({ order: 'descending', after: 0 });
  await page.getByRole('button', { name: '上一页', exact: true }).click();
  await page.getByRole('button', { name: /研究 #0 原生会话/ }).click();
  await expect(page.getByRole('button', { name: /研究 #0 原生会话/ })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('region', { name: 'VALIDATION 指标', exact: true }).getByText('9007199254740993', { exact: true }).first()).toBeVisible();
  await expect(page.getByRole('region', { name: 'VALIDATION 指标', exact: true }).getByText('未报告（null）', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('native_test_failure', { exact: true })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Task #5', exact: true })).toHaveAttribute('href', '#trial-5');
  await page.getByText('Host 任务与用量 · 所有研究', { exact: true }).click();
  await expect(page.getByRole('region', { name: 'Host 用量', exact: true }).getByText('cost.total（原生）', { exact: true })).toBeVisible();
  await expect(page.getByText('scheduling：running', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '请求停止', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: '请求停止', exact: true }).click();
  await expect(page.getByText('已请求停止，等待原生状态确认', { exact: true })).toBeVisible();
  expect(state.writes.map(write => write.path)).toEqual(['/api/v2/research/0/stop']);
  await expect(page.getByText('已取消', { exact: true })).toHaveCount(0);
  state.failNextView(); await page.getByRole('button', { name: '刷新研究', exact: true }).click();
  await expect(page.getByText('数据未更新', { exact: true })).toBeVisible();
  await expect(page.getByText('native_test_failure', { exact: true })).toBeVisible();
});

test('invalid JSON and a definitive first rejection never issue an accidental duplicate', async ({ page, context }) => {
  const state = await setup(page, { rejection: true }); await fill(page);
  await page.getByLabel('数据输入 · JSON 对象', { exact: true }).fill('[]');
  await page.getByRole('button', { name: '提交研究', exact: true }).click();
  await expect(page.getByText(/数据输入必须是 JSON 对象/)).toBeVisible(); expect(state.writes).toHaveLength(0);
  await page.getByLabel('数据输入 · JSON 对象', { exact: true }).fill('{}');
  await context.setOffline(true); await expect(page.getByRole('button', { name: '提交研究', exact: true })).toBeDisabled();
  await context.setOffline(false); await page.getByRole('button', { name: '提交研究', exact: true }).click();
  await expect(page.getByText('原生研究错误：request_id_conflict', { exact: true })).toBeVisible();
  await expect(page.getByLabel('研究目标', { exact: true })).toBeEnabled(); expect(state.writes).toHaveLength(1);
});

for (const colorScheme of ['light', 'dark'] as const) test(`${colorScheme}: responsive form/native JSON, keyboard focus and reduced motion`, async ({ page }, info) => {
  test.setTimeout(120_000); await page.emulateMedia({ colorScheme, reducedMotion: 'reduce' }); const state = await setup(page);
  for (const width of [1440, 768, 390]) {
    await page.setViewportSize({ width, height: 1000 });
    await expect(page.getByLabel('研究目标', { exact: true })).toBeVisible();
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width + 1);
    await page.getByLabel('研究目标', { exact: true }).focus(); await expect(page.getByLabel('研究目标', { exact: true })).toBeFocused();
    await page.keyboard.press('Tab'); await expect(page.getByRole('combobox', { name: '可用模型', exact: true })).toBeFocused();
    const study = page.getByRole('button', { name: /研究 #0 原生会话/ });
    await study.focus(); await expect(study).toHaveCSS('outline-style', 'solid'); await expect(study.locator('..')).toHaveCSS('animation-name', 'none');
    await page.screenshot({ path: info.outputPath(`demo-research-form-${colorScheme}-${width}.png`), fullPage: true, animations: 'disabled' });
    await study.press('Enter'); await expect(page.getByRole('heading', { name: '研究详情', exact: true })).toBeVisible();
    await page.getByText('试验 #5 · 完整提案', { exact: true }).click();
    await page.getByText('完整原生 ConversationView · entries / docs / research', { exact: true }).click();
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width + 1);
    await page.screenshot({ path: info.outputPath(`demo-research-results-${colorScheme}-${width}.png`), fullPage: true, animations: 'disabled' });
    expect((await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa']).analyze()).violations).toEqual([]);
    await page.getByRole('button', { name: '新建研究', exact: true }).click();
  }
  expect(state.writes).toEqual([]);
});

test('unusable native model responses keep a responsive, editable form and never auto-submit', async ({ page }, info) => {
  await page.emulateMedia({ reducedMotion: 'reduce' }); const state = await setup(page, { malformedModels: true });
  await expect(page.getByRole('combobox', { name: '可用模型', exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: '提交研究', exact: true })).toBeDisabled();
  await expect(page.getByRole('alert').first()).toBeVisible();
  for (const width of [1440, 768, 390]) {
    await page.setViewportSize({ width, height: 1000 });
    const goal = page.getByLabel('研究目标', { exact: true }); await expect(goal).toHaveValue('');
    await goal.focus(); await expect(goal).toBeFocused(); await expect(goal).toHaveCSS('outline-style', 'solid');
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width + 1);
    await page.screenshot({ path: info.outputPath(`research-model-error-${width}.png`), fullPage: true, animations: 'disabled' });
  }
  expect(state.writes).toEqual([]);
});
