import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { conversationsOf, jsonObjectInput, modelsOf, nativeId, nativeText, policyInput, receiptOf, ResearchIntent, tasksOf, usageOf, viewOf } from './research-native';
import { NativeResult, NativeStatus, NativeUsage, ResearchFinal, ResearchTrials } from './research-results';

// Synthetic presentation data only; no provider calls or fabricated backend evidence.
const input = { goal: 'Demo / test-only plugin-general research', model: { provider: 'test-provider', modelId: 'test-model' },
  data: { arbitrary_plugin: { ref: 'owner-reference', count: '9007199254740993' } }, policy: { max_trials: 2 } };
const metric = { counts: { observed: '9007199254740993', missing: '2' }, pearson_ic: { status: 'INSUFFICIENT', value: null } };
const result = { request: { owner_reference: 'test-only' }, native_versions: { factor: 'official-test-version' }, methods: { pearson_ic: 'native' },
  consumed_fuel: '12', partitions: [{ partition: 'VALIDATION', start_ns: '1234567890123456789', end_ns: '1234567890123456790', metric,
    assets: [{ instrument_id: 'owner-defined-instrument', metric }] }] };

describe('minimal native research projections', () => {
  it('keeps numeric safe integer native IDs including zero, not UUIDs or coerced strings', () => {
    for (const id of [0, 1, Number.MAX_SAFE_INTEGER]) expect(nativeId(id)).toBe(true);
    for (const id of ['1', '01990000-0000-7000-8000-000000000001', -1, 1.5, Number.MAX_SAFE_INTEGER + 1, null]) expect(nativeId(id)).toBe(false);
    expect(receiptOf({ conversationId: 0, submissionId: 1, native_extra: true })).toEqual({ conversationId: 0, submissionId: 1 });
    expect(() => receiptOf({ conversationId: '0', submissionId: 1 })).toThrow();
  });
  it('keeps arbitrary plugin input and an exact captured body across a retry', () => {
    const original = structuredClone(input); const intent = new ResearchIntent(original, 'one-form-intent');
    const bytes = JSON.stringify(intent.body); original.goal = 'changed'; original.data.arbitrary_plugin.ref = 'changed';
    expect(JSON.stringify(intent.body)).toBe(bytes);
    expect(intent.body).toEqual({ requestId: 'one-form-intent', input });
    expect(new ResearchIntent(input).body.requestId).not.toBe(new ResearchIntent(input).body.requestId);
  });
  it('requires explicit JSON objects, without inserting datasets or a policy', () => {
    expect(jsonObjectInput('{"plugin":{"ref":"owner-defined"}}', '数据')).toEqual({ plugin: { ref: 'owner-defined' } });
    expect(policyInput('{}')).toEqual({});
    for (const text of ['', 'null', '[]', 'true', '1', '{', '{"n":1e999}']) expect(() => jsonObjectInput(text, '数据')).toThrow();
    for (const max_trials of [0, 1, Number.MAX_SAFE_INTEGER]) expect(policyInput(JSON.stringify({ max_trials }))).toEqual({ max_trials });
    for (const max_trials of [-1, 1.5, '2', null, Number.MAX_SAFE_INTEGER + 1]) expect(() => policyInput(JSON.stringify({ max_trials }))).toThrow('非负');
  });
  it('preserves native cursor objects and complete conversation ancestry', () => {
    const raw = { items: [{ id: 0, parent: { conversationId: 1, at: 2 } }], cursor: { order: 'descending', offset: 8 } };
    const page = conversationsOf(raw); expect(page.raw).toBe(raw); expect(page.cursor).toEqual(raw.cursor); expect(page.items).toEqual(raw.items);
    expect(() => conversationsOf({ items: [{ id: '0' }] })).toThrow();
    expect(() => conversationsOf({ items: [], cursor: 'opaque-string' })).toThrow();
    expect(conversationsOf({ items: [] }).cursor).toBeUndefined();
  });
  it('rejects a different selected conversation but retains native docs and unknown plugin fields', () => {
    const raw = { conversation: { id: 7 }, entries: [{ kind: 'custom', data: { preserved: true } }], docs: { 'pi.live': {} },
      research: { input, trials: [], final: null, arbitrary_plugin_fact: { native: true } } };
    expect(viewOf(raw, 7).raw).toBe(raw);
    expect(viewOf(raw, 7).research.arbitrary_plugin_fact).toEqual({ native: true });
    expect(() => viewOf(raw, 8)).toThrow();
  });
  it('uses actual model availability and native task snapshots rather than assumed outcomes', () => {
    expect(modelsOf([])).toEqual([]);
    expect(modelsOf([{ provider: 'test', modelId: 'custom', name: 'Demo model', reasoning: true }])).toHaveLength(1);
    expect(() => modelsOf([{ provider: 'test', modelId: 'custom' }])).toThrow();
    const raw = { scheduling: 'paused', tasks: [{ record: { id: 1 }, state: { kind: 'waiting', on: [2] } }], submissions: [] };
    expect(tasksOf(raw)).toEqual(raw); expect(usageOf({ models: {}, tools: {} }).models).toEqual({});
    expect(() => usageOf({ models: [] })).toThrow();
  });
  it('never converts null, absent values or decimal strings into numeric zero', () => {
    expect(nativeText(null)).toBe('未报告（null）'); expect(nativeText(undefined)).toBe('未报告');
    expect(nativeText(0)).toBe('0'); expect(nativeText('9007199254740993')).toBe('9007199254740993');
  });
});

describe('scientific owner-visible native summaries', () => {
  it('shows exact native partitions, counts, ns bounds, null IC and full JSON', () => {
    const html = renderToStaticMarkup(<NativeResult value={result} label="Demo native result JSON" />);
    for (const text of ['VALIDATION', '9007199254740993', '1234567890123456789', 'INSUFFICIENT', '未报告（null）', '逐资产指标', 'native_versions', 'methods', 'request']) expect(html).toContain(text);
    expect(html).not.toContain('证据通过');
    const zero = renderToStaticMarkup(<NativeResult value={{ partitions: [{ partition: 'TRAIN', metric: { pearson_ic: { status: 'DEFINED', value: 0 } } }] }} label="Demo zero" />);
    expect(zero).toContain('<strong>0</strong>');
  });
  it('retains plugin results even without the factor summary projection', () => {
    const html = renderToStaticMarkup(<NativeResult value={{ plugin_fact: 'native-custom-result' }} label="Demo plugin JSON" />);
    expect(html).toContain('native-custom-result'); expect(html).toContain('Demo plugin JSON');
  });
  it('shows trial failures/rejections, proposals and computation status without an invented pass', () => {
    const html = renderToStaticMarkup(<ResearchTrials trials={[{ taskId: 8, proposal: { hypothesis: 'Demo hypothesis' }, status: 'FAILED', error: 'native_reason', result: null }]} />);
    for (const text of ['Demo hypothesis', 'FAILED', 'native_reason', 'id="trial-8"', '完整提案', '尚无计算摘要（null）']) expect(html).toContain(text);
    expect(renderToStaticMarkup(<NativeStatus value="PLUGIN_NEW_STATUS" />)).toContain('PLUGIN_NEW_STATUS');
  });
  it('keeps the actual selected trial, compared set and final measurements owner-visible', () => {
    const html = renderToStaticMarkup(<ResearchFinal value={{ taskId: 9, trialId: 8, comparedTrials: [7, 8], reason: 'native_selection_reason', status: 'COMPUTED', result }} />);
    for (const text of ['href="#trial-8"', 'native_selection_reason', '[7,8]', '所有者最终结果', '不作为模型调优反馈']) expect(html).toContain(text);
    expect(renderToStaticMarkup(<ResearchFinal value={null} />)).toContain('尚未记录最终选择或结果');
  });
  it('shows only reported usage with absent reasoning/cost distinct from zero', () => {
    const html = renderToStaticMarkup(<NativeUsage value={{ models: { 'test/model': { input: 0, output: 12, reasoning: null, cost: { total: null } } }, tools: {} }} label="Demo usage" />);
    expect(html).toContain('未报告（null）'); expect(html).toContain('未报告'); expect(html).toContain('test/model');
    expect(renderToStaticMarkup(<NativeUsage value={{ models: {}, tools: {} }} label="Demo empty usage" />)).toContain('不推定零用量');
  });
});
