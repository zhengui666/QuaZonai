import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { describe, expect, it } from 'vitest';
import type { ListPage, ResearchListItem } from '../../../packages/contracts/src/lists.ts';
import Research, { NativeResearchContinuation, nativeResearchInput, ResearchConversation, canContinueResearch, NativeStrategyResultView } from './research';
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
  expect(html).toContain('Agent 对话'); expect(html).toContain('请选择周期：');
  expect(html).toContain('&lt;script&gt;'); expect(html).not.toContain('<script>');
  expect(renderToStaticMarkup(createElement(ResearchConversation, { messages: [] }))).toBe('');
});

it('shows the official strategy report without custom indicator semantics or invented performance', () => {
  const value = { request: { instrument_id: 'TEST.SIM', available_start_ns: '1790000000000000001', strategy: { kind: 'EMA_CROSS', trade_size: '0.500000000000000001' } },
    nativeVersion: '0.69.0', scientificClaim: 'HISTORICAL_NATIVE_BACKTEST_NOT_INDEPENDENT_QUALIFICATION',
    canonicalResult: { statistics: { returns: { 'Sharpe Ratio': '3ff0000000000000' } }, diagnostics: ['<script>not markup</script>'] }, dataRowsByType: { QuoteTick: '9007199254740993' }, notes: ['原生费用假设'] };
  const html = renderToStaticMarkup(createElement(NativeStrategyResultView, { value }));
  const visible = visibleText(html);
  for (const exact of ['0.69.0', '1790000000000000001', '0.500000000000000001', '9007199254740993', '3ff0000000000000', '原生费用假设']) expect(visible).toContain(exact);
  expect(visible).toContain('不在浏览器重算绩效');
  expect(visible).not.toContain('最后一条原生指标观测'); expect(visible).not.toContain('现金期');
  expect(html).toContain('&lt;script&gt;'); expect(html).not.toContain('<script>');
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
  expect(html).toContain('/api/v3/research/7/evidence/41'); expect(html).toContain('CPCV_NOT_RUN');
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
  expect(visibleText(native)).toContain('静态规格情景'); expect(visibleText(native)).toContain('未验证当时真实规格'); expect(visibleText(native)).toContain('1795000000000000001');
  const report = renderToStaticMarkup(createElement(ResearchAssessments, { assessments: [{ taskId: 1, mode: 'SYNTHESIS', status: 'completed', configuration: 'CURRENT', error: null, result: { instrumentDefinitionAssumption: basis, judgment: { stars: 3, rationale: 'Conditional only' }, ratingStatus: 'RATED_CONDITIONAL_STATIC_SCENARIO' }, evidenceLinks: [] }] }));
  expect(visibleText(report)).toContain('条件性情景评级'); expect(visibleText(report)).toContain('历史合约规格未核实');

});
