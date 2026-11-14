import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { describe, expect, it } from 'vitest';
import type { ListPage, ResearchListItem } from '../../../packages/contracts/src/lists.ts';
import Research, { NativeResearchContinuation, nativeResearchInput, ResearchConversation, canContinueResearch, NativeStrategyResultView, ResearchMarkdown, safeMarkdownComponents, mergeResearchMessages, readableResearchText, ResearchRating, ResearchUncertainty, syncResearchWindow, acceptResearchHistoryPage } from './research';
import type { ResearchDetail } from '../../../packages/contracts/src/index.ts';
import { listQuery } from './list-values';

const visibleText = (html: string) => html.replace(/<[^>]*>/g, '');
function renderList(items: ResearchListItem[], query = '') {
  const params = new URLSearchParams(query);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData<ListPage<ResearchListItem>>(['/research', 'list', listQuery(params)], {
    items, total: items.length, page: 1, pageSize: 20, facets: {},
  });
  try {
    return renderToStaticMarkup(createElement(QueryClientProvider, { client }, createElement(Research, {
      route: { page: 'research', creating: false, params },
    })));
  } finally { client.clear(); }
}
const study: ResearchListItem = {
  id: 312987, title: '成交量研究', status: 'COMPUTED', trials: 3, model: 'private-provider/private-model',
  datasetId: 'internal-dataset-identity', datasetName: '冻结市场数据', scope: 'single-factor', modality: 'BAR',
  criterion: { status: 'not_met', metric: 'pearson_ic', value: -0.0123456789, target: 0.05, partition: 'HELDOUT' },
};

describe('research table presentation', () => {
  it('distinguishes an empty collection from an empty search result', () => {
    expect(visibleText(renderList([]))).toContain('尚无研究');
    const filtered = visibleText(renderList([], 'q=不存在的研究'));
    expect(filtered).toContain('没有匹配的研究');
    expect(filtered).not.toContain('尚无研究');
  });
  it('shows execution without retired scoring or technical identities', () => {
    const fullName = '研究名称'.repeat(25) + '\n完整问题只在详情显示';
    const html = renderList([{ ...study, title: fullName }]);
    const text = visibleText(html);
    expect(text).toContain('已计算');
    expect(text).not.toContain('未达到标准');
    expect(text).not.toContain('-0.0123456789');
    expect(text).not.toContain('独立评价指标');
    expect(text).toContain('冻结市场数据');
    expect(text).not.toContain(String(study.id));
    expect(html).not.toContain(study.model);
    expect(html).not.toContain(study.datasetId);
    expect(html).not.toContain(fullName);
    expect(html).not.toContain('完整问题只在详情显示');
    expect(html).toContain(`#/research/${study.id}`);
    expect(html).not.toContain('study-document');
  });

  it('does not reinterpret retired criterion data as native performance', () => {
    const html = renderList([{ ...study, criterion: { ...study.criterion!, metric: 'pearson_ic', value: 0 } }]);
    expect(html).not.toContain('pearson_ic');
    expect(html).not.toContain('验收结果');
    expect(html).not.toContain('评价指标');
  });
});


describe('same-conversation native continuation', () => {
  const detail = { id: 777, toolPolicy: 'NAUTILUS_ONLY', submission: { id: 42, status: 'done' }, final: null } as ResearchDetail;
  const render = (value: ResearchDetail, active = false) => renderToStaticMarkup(createElement(NativeResearchContinuation, { detail: value, active, changed: () => {} }));
  it('offers native follow-up after dialogue and native results, but never while active', () => {
    expect(render(detail)).toContain('后续研究要求');
    expect(render(detail)).toContain('继续研究');
    expect(render({ ...detail, final: { status: 'COMPUTED', trialId: null, reason: null, metrics: [], nativeStrategyResult: {} } })).toContain('继续研究');
    expect(render(detail, true)).toBe('');
  });
  it('does not turn historical scored finals into editable native research', () => {
    expect(render({ ...detail, toolPolicy: undefined })).toBe('');
    expect(render({ ...detail, final: { status: 'COMPUTED', trialId: 99, reason: null, metrics: [] } })).toBe('');
  });
});

 it('submits natural-language research through the native host without scored-model defaults', () => {
  expect(nativeResearchInput({ goal: '运行官方原生策略', datasetId: 'native-data', model: '{"provider":"p","modelId":"m"}', thinkingLevel: 'high', serviceTier: 'priority', publicLiteratureTopics: 'momentum', metric: 'pearson_ic', scope: 'single-factor', maxMembers: '1', scenarios: '1000' })).toEqual({ goal: '运行官方原生策略', datasetId: 'native-data', model: {provider:'p',modelId:'m'}, thinkingLevel:'high',serviceTier:'priority', publicLiteratureQueries:['momentum'] });
});

it('shows continuation for completed discussion stops and settled owner stops, not active or scored finals', () => {
  const detail = { id: 781, toolPolicy: 'NAUTILUS_ONLY', submission: { id: 43, status: 'done' }, final: { status: 'STOPPED', trialId: null, reason: 'Discussion complete', metrics: [] } } as unknown as ResearchDetail;
  expect(canContinueResearch(detail, false)).toBe(true);
  expect(renderToStaticMarkup(createElement(NativeResearchContinuation, { detail, active: false, changed: () => {} }))).toContain('继续研究');
  expect(canContinueResearch(detail, true)).toBe(false);
  expect(canContinueResearch({ ...detail, submission: { ...detail.submission!, status: 'unanswered', reason: 'aborted' }, final: { ...detail.final!, reason: 'owner_stop' } }, false)).toBe(true);
  expect(canContinueResearch({ ...detail, final: { ...detail.final!, trialId: 99 } }, false)).toBe(false);
  expect(canContinueResearch({ ...detail, submission: { ...detail.submission!, status: 'placed' } }, false)).toBe(false);
});
it('renders the original Agent question as safe text alongside the existing reply affordance', () => {
  const html = renderToStaticMarkup(createElement(ResearchConversation, { messages: [{ entryId: 99, text: '请选择周期：\n<script>alert(1)</script>' }] }));
  expect(html).toContain('研究对话'); expect(html).toContain('研究助手');
  const renderer = ResearchMarkdown({ content: '请选择周期：<script>alert(1)</script>' });
  expect(renderer.props.escapeRawHtml).toBe(true); expect(renderer.props.content).toContain('请选择周期：');
  expect(renderToStaticMarkup(createElement(ResearchConversation, { messages: [] }))).toContain('从一个研究问题开始');
});

it('shows readable official results, preserves nulls and never expands raw report JSON', () => {
  const value = { request: { instrument_id: 'TEST.SIM', strategy: { kind: 'EMA_CROSS' } }, nativeVersion: '0.69.0',
    canonicalResult: { orders: [{ quantity: '0.500000000000000001', internal_secret: 'not-visible' }] },
    equityPerformance: { status: 'UNAVAILABLE', reason: 'UNPRICED_PORTFOLIO_SNAPSHOTS', currency: null,
      statistics: [{ native_key: 'Sharpe Ratio (252 days)', annualization_days: 252, value: 999 }] } };
  const html = renderToStaticMarkup(createElement(NativeStrategyResultView, { value }));
  expect(visibleText(html)).toContain('0.69.0'); expect(visibleText(html)).toContain('TEST.SIM');
  expect(visibleText(html)).toContain('暂不可评价'); expect(visibleText(html)).toContain('部分账户快照缺少估值');
  expect(html).not.toContain('999'); expect(html).not.toContain('internal_secret'); expect(html).not.toContain('not-visible');
  expect(html).not.toContain('<pre'); expect(html).not.toContain('canonicalResult');
  expect(visibleText(html)).toContain('1 条已记录'); expect(visibleText(html)).toContain('明细尚未在此页面加载');
});

it('renders original assessment stars, missing grades, historical scope and real evidence links without recalculating', async () => {
  const { ResearchAssessments } = await import('./research');
  const html = renderToStaticMarkup(createElement(ResearchAssessments, { assessments: [
    { taskId: 41, mode: 'SYNTHESIS', status: 'completed', configuration: 'CURRENT', error: null,
      result: { judgment: { stars: null, rationale: '真实样本尚不足' }, dataJudgment: { stars: null }, ratingStatus: 'NOT_RATED_INSUFFICIENT_EVIDENCE', evidence: { logic: { result: { judgment: { stars: 0 } } }, ratingReadiness: { limitations: ['CPCV_NOT_RUN'] } } }, evidenceLinks: [{ taskId: 41, href: '/api/v3/research/7/evidence/41' }] },
    { taskId: 31, mode: 'LOGIC', status: 'completed', configuration: 'HISTORICAL', error: null, result: { judgment: { stars: 3, rationale: '原始逻辑结论' } }, evidenceLinks: [] },
  ] }));
  expect(visibleText(html)).toContain('未评级'); expect(visibleText(html)).toContain('0 / 5 星');
  expect(visibleText(html)).toContain('历史配置，不适用于当前输入'); expect(visibleText(html)).toContain('真实样本尚不足');
  expect(html).not.toContain('/api/v3/research/7/evidence/41'); expect(html).not.toContain('CPCV_NOT_RUN'); expect(html).not.toContain('ratingStatus'); expect(html).toContain('尚未完成组合净化交叉验证');
  expect(visibleText(html)).toContain('不代表收益承诺或交易许可');
});

it('keeps stopping available for live validation after a completed historical native result', async () => {
  const { hasActiveResearchExecution } = await import('./research');
  const detail = { final: { status: 'COMPUTED' }, tasks: [
    { id: 1, name: 'qz.native-strategy.backtest', status: 'terminal', outcome: 'completed', error: null },
    { id: 2, name: 'qz.native-strategy.backtest', status: 'sleeping', error: null },
  ], submission: { id: 3, status: 'placed' } } as ResearchDetail;
  expect(hasActiveResearchExecution(detail)).toBe(true);
  expect(hasActiveResearchExecution({ ...detail, submission: { id: 3, status: 'done' } })).toBe(true);
  expect(hasActiveResearchExecution({ ...detail, tasks: [detail.tasks[0]!], submission: { id: 3, status: 'done' } })).toBe(false);
  expect(hasActiveResearchExecution({ ...detail, tasks: [] })).toBe(true);
});

it('allows explicit continuation after owner stops validation following a historical native result', () => {
  const detail = { toolPolicy: 'NAUTILUS_ONLY', final: { status: 'COMPUTED', trialId: null, reason: 'explicit_native_strategy_backtest_completed', metrics: [], nativeStrategyResult: {} }, submission: { id: 3, status: 'unanswered', reason: 'aborted' } } as unknown as ResearchDetail;
  expect(canContinueResearch(detail, false)).toBe(true);
  expect(canContinueResearch(detail, true)).toBe(false);
  expect(canContinueResearch({ ...detail, submission: { id: 3, status: 'unanswered', reason: 'model_error' } }, false)).toBe(true);
  expect(canContinueResearch({ ...detail, submission: { id: 3, status: 'unanswered', reason: 'model_error' } }, true)).toBe(false);
});

it('keeps static instrument scenarios visibly conditional in original native results and star reports', async () => {
  const { ResearchAssessments } = await import('./research');
  const basis = { mode: 'STATIC_SCENARIO', ts_init_ns: '1795000000000000001', ts_event_ns: '1795000000000000000', scope: 'HYPOTHETICAL_STATIC_RULES_NOT_HISTORICALLY_VERIFIED' };
  const native = renderToStaticMarkup(createElement(NativeStrategyResultView, { value: { instrumentDefinition: basis, canonicalResult: {}, request: { instrument_definition: basis } } }));
  expect(visibleText(native)).toContain('静态规格情景'); expect(visibleText(native)).toContain('未验证当时真实规格'); expect(visibleText(native)).not.toContain('1795000000000000001');
  const report = renderToStaticMarkup(createElement(ResearchAssessments, { assessments: [{ taskId: 1, mode: 'SYNTHESIS', status: 'completed', configuration: 'CURRENT', error: null, result: { instrumentDefinitionAssumption: basis, judgment: { stars: 3, rationale: 'Conditional only' }, ratingStatus: 'RATED_CONDITIONAL_STATIC_SCENARIO' }, evidenceLinks: [] }] }));
  expect(visibleText(report)).toContain('条件性情景评级'); expect(visibleText(report)).toContain('历史合约规格未核实');

});

it('shows original equity availability and observation scope without inventing annual performance', () => {
  const value = { equityPerformance: { source: 'NAUTILUS_PORTFOLIO_SNAPSHOTS', status: 'UNAVAILABLE', reason: 'UNPRICED_PORTFOLIO_SNAPSHOTS', observed_utc_days: '27', return_days: '26', unpriced_snapshot_count: '1' }, canonicalResult: {}, request: {} };
  const html = renderToStaticMarkup(createElement(NativeStrategyResultView, { value }));
  expect(visibleText(html)).toContain('252 / 365 是年化频率，不是观测天数');
  expect(visibleText(html)).toContain('短样本年化是外推');
  expect(visibleText(html)).toContain('缺少权益证据时不替换为持仓收益');
  expect(visibleText(html)).toContain('部分账户快照缺少估值');
  expect(visibleText(html)).toContain('已观察天数'); expect(visibleText(html)).toContain('27');
});

it('configures the upstream Markdown renderer to escape HTML and limits link/image/code components', () => {
  const markdown = ResearchMarkdown({ content: '**重要结论**<script>bad</script>' });
  expect(markdown.props.escapeRawHtml).toBe(true); expect(markdown.props.components).toBe(safeMarkdownComponents);
  const link = safeMarkdownComponents.a({ href: 'https://example.com/report', children: '资料' });
  expect(link.props.rel).toBe('noopener noreferrer');
  expect(safeMarkdownComponents.a({ href: 'javascript:alert(1)', children: '资料' }).type).toBe('span');
  expect(safeMarkdownComponents.img({ alt: '行情图' }).type).toBe('span');
  expect(renderToStaticMarkup(safeMarkdownComponents.code({ lang: 'json', children: '{"internal_task":123}' }))).not.toContain('internal_task');
});
it('shows persisted roles distinctly and keeps the older-page action on an empty filtered page', () => {
  const html = renderToStaticMarkup(createElement(ResearchConversation, { messages: [{ entryId: 1, role: 'user', text: '原始问题，不是内部提示' }, { entryId: 2, role: 'assistant', text: '答复' }] }));
  expect(html).toContain('原始问题，不是内部提示'); expect(html).toContain('你'); expect(html).toContain('研究助手');
  expect(renderToStaticMarkup(createElement(ResearchConversation, { messages: [], researchId: 1, nextCursor: 'opaque' }))).toContain('加载更早的对话');
});

it('retains seen dialogue across sliding SSE pages and deduplicates older history', () => {
  const first = [{ entryId: 101, role: 'user' as const, text: 'middle' }, { entryId: 102, role: 'assistant' as const, text: 'latest' }];
  const history = mergeResearchMessages([{ entryId: 1, role: 'user', text: 'old' }], first);
  const shifted = mergeResearchMessages(history, [{ entryId: 102, role: 'assistant', text: 'latest' }, { entryId: 201, role: 'assistant', text: 'new' }]);
  expect(shifted.map(row => row.entryId)).toEqual([1, 101, 102, 201]);
  expect(mergeResearchMessages(shifted, []).map(row => row.entryId)).toEqual([1, 101, 102, 201]);
  const one = ResearchConversation({ researchId: 1, messages: first }); const two = ResearchConversation({ researchId: 2, messages: [] });
  expect(one.key).not.toBe(two.key);
});
it('replaces structured JSON in labeled, unlabeled, inline and plain messages without changing prose', () => {
  for (const text of ['```json\n{"task":1}\n```', '```\n{"task":1}\n```', '`{"task":1}`', '{"task":1}', '结果： {"task":{"id":1}}', '{"task":1} 与 {"task":2}', '说明：\n{\n "task": {"nested":1}\n}\n结束']) {
    expect(readableResearchText(text)).not.toContain('"task"'); expect(readableResearchText(text)).toContain('结构化数据已省略');
  }
  expect(readableResearchText('**原结论**：证据不足。')).toBe('**原结论**：证据不足。');
});
it('describes actual disposal reason and exact request assumptions without exposing JSON', () => {
  const html = renderToStaticMarkup(createElement(NativeStrategyResultView, { value: { canonicalResult: null, canonicalUnavailableReason: 'OMITTED_BY_DISPOSE_ON_COMPLETION_CONFIG', request: { event_start_ns: '1790000000000000001', event_end_ns: '1790000000000000002', venue: { starting_balances: ['5000.000000000000001 USDT'], maker_fee: '0.0001', taker_fee: '0.0002' } } } }));
  for (const text of ['本次配置选择运行结束后不保留扩展报告', '1790000000000000001', '5000.000000000000001 USDT', '0.0001']) expect(html).toContain(text);
  expect(html).not.toContain('event_start_ns');
});

it('keys document and new-dataset routes independently while preserving existing request conversion', () => {
  const first = Research({ route: { page: 'research', creating: false, id: '1', params: new URLSearchParams() } });
  const second = Research({ route: { page: 'research', creating: false, id: '2', params: new URLSearchParams() } });
  expect(first.key).not.toBe(second.key);
  const a = Research({ route: { page: 'research', creating: true, params: new URLSearchParams('dataset=a') } });
  const b = Research({ route: { page: 'research', creating: true, params: new URLSearchParams('dataset=b') } });
  expect(a.key).not.toBe(b.key);
});

it('uses read-only yellow Ant Design stars while keeping null distinct from actual zero', () => {
  const rated = renderToStaticMarkup(createElement(ResearchRating, { value: 3 }));
  expect(rated).toContain('ant-rate-disabled'); expect(rated).toContain('color:#fadb14'); expect(rated).toContain('3 / 5 星'); expect(rated).toContain('aria-hidden="true"');
  const zero = renderToStaticMarkup(createElement(ResearchRating, { value: 0 }));
  expect(zero).toContain('0 / 5 星'); expect(zero).toContain('ant-rate-disabled');
  const missing = renderToStaticMarkup(createElement(ResearchRating, { value: null }));
  expect(missing).toContain('未评级'); expect(missing).not.toContain('ant-rate'); expect(missing).not.toContain('0 / 5');
});

it('withholds unfinished structured output throughout streaming and inside non-JSON brackets', () => {
  const stream = '{"task":{"source":"internal","status":"running"}}';
  for (let end = 1; end <= stream.length; end++) {
    const visible = readableResearchText(stream.slice(0, end));
    expect(visible).not.toContain('{'); expect(visible).not.toContain('"task"'); expect(visible).toContain('结构化数据已省略');
  }
  for (const text of ['前言 [原报告 {"task":1}] 后文', '```\n{"task":', '[{"task":1}', '{"task":1]']) {
    expect(readableResearchText(text)).not.toContain('"task"');
  }
  expect(readableResearchText('[正常资料](https://example.com)')).toBe('[正常资料](https://example.com)');
  expect(readableResearchText('普通说明 {数学符号} 不改变。')).toBe('普通说明 {数学符号} 不改变。');
});

it('shows authoritative uncertainty with all tasks terminal and never replaces it with older completion', () => {
  const detail = { toolPolicy: 'NAUTILUS_ONLY', submission: { id: 9, status: 'done' }, final: { status: 'COMPUTED', nativeStrategyResult: {} }, tasks: [{ status: 'terminal', outcome: 'completed' }], executionUncertain: true, executionUncertainty: { taskId: 42, reason: 'nautilus_execution_unknown' } } as ResearchDetail;
  expect(canContinueResearch(detail, false)).toBe(false);
  const html = renderToStaticMarkup(createElement(ResearchUncertainty, { detail }));
  expect(html).toContain('执行结果尚未确认'); expect(html).not.toContain('研究正在进行'); expect(html).toContain('不会自动重复运行');
  expect(renderToStaticMarkup(createElement(ResearchUncertainty, { detail: { executionUncertain: false } }))).toBe('');
  expect(renderToStaticMarkup(createElement(ResearchUncertainty, { detail: { executionUncertain: true, executionUncertainty: { taskId: 42, reason: 'nautilus_cancellation_unknown' } } }))).toContain('停止请求的结果尚未确认');
});

it('preserves ordinary numeric citations but withholds standalone and structured JSON arrays', () => {
  expect(readableResearchText('已有研究 [1] 支持这一点，另见[2]。')).toBe('已有研究 [1] 支持这一点，另见[2]。');
  expect(readableResearchText('[1]: https://example.com/paper')).toBe('[1]: https://example.com/paper');
  for (const text of ['[1]', '[1,2]', '```json\n[1]\n```', '数组： [1,2]', '`[1]`', '[{"task":1}]']) expect(readableResearchText(text)).toContain('结构化数据已省略');
});

it('repairs a non-overlapping SSE window and rejects cursor rollback from a late older page', () => {
  const initial = { messages: [{ entryId: 1, role: 'user' as const, text: 'old' }, { entryId: 100, role: 'assistant' as const, text: 'seen' }], cursor: 'very-old', generation: 0 };
  const jumped = syncResearchWindow(initial, [{ entryId: 301, role: 'assistant', text: 'new window' }], 'middle-page');
  expect(jumped.cursor).toBe('middle-page'); expect(jumped.generation).toBe(1);
  const late = acceptResearchHistoryPage(jumped, { messages: [{ entryId: 2, role: 'assistant', text: 'older reply' }], nextCursor: null }, 0);
  expect(late.cursor).toBe('middle-page'); expect(late.messages.map(row => row.entryId)).toEqual([1, 2, 100, 301]);
  const middle = acceptResearchHistoryPage(late, { messages: [{ entryId: 201, role: 'assistant', text: 'previously unseen' }], nextCursor: 'next-middle' }, 1);
  expect(middle.cursor).toBe('next-middle'); expect(middle.messages.map(row => row.entryId)).toEqual([1, 2, 100, 201, 301]);
  const empty = acceptResearchHistoryPage(middle, { messages: [], nextCursor: 'still-more' }, 1);
  expect(empty.cursor).toBe('still-more'); expect(empty.messages).toEqual(middle.messages);
});

it('keeps the new cursor when an actual pending history promise resolves after SSE advances', async () => {
  let state = syncResearchWindow({ messages: [{ entryId: 100, role: 'assistant', text: 'seen' }], cursor: 'old-cursor', generation: 0 }, [], 'unused');
  type Page = { messages: NonNullable<ResearchDetail['messages']>; nextCursor: string | null };
  let release!: (page: Page) => void;
  const response = new Promise<Page>(resolve => { release = resolve; });
  const sentGeneration = state.generation;
  const pending = response.then(page => { state = acceptResearchHistoryPage(state, page, sentGeneration); });
  state = syncResearchWindow(state, [{ entryId: 301, role: 'assistant', text: 'new' }], 'gap-cursor');
  release({ messages: [{ entryId: 50, role: 'user', text: 'late historical response' }], nextCursor: null });
  await pending;
  expect(state.cursor).toBe('gap-cursor'); expect(state.messages.map(row => row.entryId)).toEqual([50, 100, 301]);
  state = acceptResearchHistoryPage(state, { messages: [{ entryId: 201, role: 'assistant', text: 'gap' }], nextCursor: 'continue' }, state.generation);
  state = acceptResearchHistoryPage(state, { messages: [{ entryId: 100, role: 'assistant', text: 'seen' }], nextCursor: null }, state.generation);
  expect(state.cursor).toBeNull(); expect(state.messages.map(row => row.entryId)).toEqual([50, 100, 201, 301]);
});

it('refills a changed all-internal SSE window without letting an older response erase its cursor', async () => {
  let state: ReturnType<typeof syncResearchWindow> = { messages: [{ entryId: 100, role: 'assistant' as const, text: 'seen' }], cursor: 'ancient-page' as string | null | undefined, serverCursor: 'old-window' as string | null | undefined, generation: 0 };
  type Page = { messages: NonNullable<ResearchDetail['messages']>; nextCursor: string | null };
  let release!: (page: Page) => void;
  const response = new Promise<Page>(resolve => { release = resolve; });
  const sentGeneration = state.generation;
  const pending = response.then(page => { state = acceptResearchHistoryPage(state, page, sentGeneration); });
  state = syncResearchWindow(state, [], 'behind-internal-window');
  expect(state.cursor).toBe('behind-internal-window'); expect(state.generation).toBe(1);
  release({ messages: [], nextCursor: null }); await pending;
  expect(state.cursor).toBe('behind-internal-window');
  state = acceptResearchHistoryPage(state, { messages: [{ entryId: 201, role: 'assistant', text: 'otherwise missing' }], nextCursor: 'continue-middle' }, 1);
  state = syncResearchWindow(state, [], 'behind-internal-window');
  expect(state.generation).toBe(1); expect(state.cursor).toBe('continue-middle');
  expect(state.messages.map(row => row.entryId)).toEqual([100, 201]);
});

it('advances an empty server window even when no visible message has ever been received', async () => {
  let state: ReturnType<typeof syncResearchWindow> = { messages: [], cursor: 'old-history', serverCursor: 'old-window', generation: 0 };
  type Page = Parameters<typeof acceptResearchHistoryPage>[1];
  let release!: (page: Page) => void;
  const response = new Promise<Page>(resolve => { release = resolve; });
  const sentGeneration = state.generation;
  const pending = response.then(page => { state = acceptResearchHistoryPage(state, page, sentGeneration); });
  state = syncResearchWindow(state, [], 'new-window-cursor');
  expect(state.cursor).toBe('new-window-cursor'); expect(state.generation).toBe(1);
  release({ messages: [], nextCursor: null }); await pending;
  expect(state.cursor).toBe('new-window-cursor');
  state = acceptResearchHistoryPage(state, { messages: [{ entryId: 201, role: 'assistant', text: 'first visible message in middle' }], nextCursor: null }, 1);
  expect(state.messages.map(row => row.entryId)).toEqual([201]); expect(state.cursor).toBeNull();
  const firstVisibleWindow = syncResearchWindow({ messages: [], cursor: 'old-history', serverCursor: 'old-window', generation: 0 }, [{ entryId: 301, role: 'assistant', text: 'new visible window' }], 'visible-window-cursor');
  expect(firstVisibleWindow.cursor).toBe('visible-window-cursor'); expect(firstVisibleWindow.generation).toBe(1);
});

it('research list shows unresolved execution rather than an earlier computed result', () => {
 const text=visibleText(renderList([{...study,status:'UNKNOWN'}]));
 expect(text).toContain('执行结果未确认');
 expect(text).not.toContain('已计算');
});
