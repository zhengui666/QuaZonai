import { beforeEach as beforeLocaleTest, afterEach as afterLocaleTest } from 'vitest';
import i18n from './i18n';
beforeLocaleTest(async () => { await i18n.changeLanguage('zh-CN'); });
afterLocaleTest(async () => { await i18n.changeLanguage('en'); });
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { thinkingLabel, thinkingOptions, thinkingUnavailableReason, reasoningModeDescription } from './model-labels';
const reads = vi.hoisted(() => [] as Array<Record<string, unknown>>);
vi.mock('@tanstack/react-query', async importOriginal => ({ ...await importOriginal<typeof import('@tanstack/react-query')>(), useQuery: (options: Record<string, unknown>) => { reads.push(options); return {}; } }));
import { useModelCatalog } from './data-hooks';
function Catalog() { useModelCatalog(); return null; }
describe('provider reasoning presentation', () => {
  it('uses English effort labels, including Ultra, without changing provider values', () => {
    expect(['off', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max', 'ultra', 'default'].map(thinkingLabel)).toEqual(['Off', 'Minimal', 'Low', 'Medium', 'High', 'XHigh', 'Max', 'Ultra', 'Default']);
    expect(thinkingLabel('future-provider-level')).toBe('future-provider-level');
  });
  it('shows advertised unavailable Ultra disabled with its reason and does not mutate capabilities', () => {
    const model = { thinkingLevels: ['high', 'max'], unavailableThinkingLevels: [{ level: 'ultra', reason: '服务暂不接受 Ultra' }] };
    expect(thinkingOptions(model)).toEqual([{ value: 'high', label: 'High' }, { value: 'max', label: 'Max' }, { value: 'ultra', label: 'Ultra', disabled: true, title: '服务暂不接受 Ultra' }]);
    expect(model.thinkingLevels).toEqual(['high', 'max']);
    expect(thinkingUnavailableReason(model)).toBe('Ultra 不可用：服务暂不接受 Ultra');
    expect(thinkingUnavailableReason()).toBeUndefined();
    expect(thinkingOptions({ ...model, thinkingLevels: ['max', 'ultra'] }).filter(option => option.value === 'ultra')).toEqual([{ value: 'ultra', label: 'Ultra', disabled: true, title: '服务暂不接受 Ultra' }]);
    expect(thinkingOptions()).toEqual([]);
  });
  it('keeps Ultra separate from the configured model effort and enables only declared orchestration', () => {
    const model = { thinkingLevels: ['max'], reasoningModes: [{ level: 'ultra', effectiveLevel: 'max', mode: 'PI_MULTI_AGENT' as const }], unavailableThinkingLevels: [{ level: 'ultra', reason: '原生不支持' }] };
    expect(thinkingOptions(model)).toEqual([{ value: 'max', label: 'Max' }, { value: 'ultra', label: 'Ultra' }]);
    expect(thinkingUnavailableReason(model)).toBeUndefined();
    expect(reasoningModeDescription(model, 'ultra')).toContain('底层模型配置 Max');
    expect(reasoningModeDescription(model, 'ultra')).toContain('仅分析、审查和综合');
    expect(reasoningModeDescription(model, 'max')).toBeUndefined();
    expect(model.thinkingLevels).toEqual(['max']);
  });
  it('automatically reads the same model endpoint once per minute while visible', () => {
    reads.length = 0; renderToStaticMarkup(createElement(Catalog));
    expect(reads[0]).toMatchObject({ queryKey: ['/models'], refetchInterval: 60000, refetchIntervalInBackground: false });
    expect(reads[0]).not.toHaveProperty('refetchOnWindowFocus', false);
    expect(reads[0]).not.toHaveProperty('initialData');
  });
});

it('localizes reasoning explanations at call time but keeps provider efforts in English', async () => {
 const model={thinkingLevels:['max'],reasoningModes:[{level:'ultra',effectiveLevel:'max',mode:'PI_MULTI_AGENT' as const}]};
 await i18n.changeLanguage('en');
 expect(reasoningModeDescription(model,'ultra')).toContain('underlying model effort Max');
 expect(thinkingLabel('ultra')).toBe('Ultra');
 await i18n.changeLanguage('zh-CN');
 expect(reasoningModeDescription(model,'ultra')).toContain('底层模型配置 Max');
 expect(thinkingLabel('ultra')).toBe('Ultra');
});
