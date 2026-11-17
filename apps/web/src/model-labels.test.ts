import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { thinkingLabel, thinkingOptions, thinkingUnavailableReason } from './model-labels';
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
  it('automatically reads the same model endpoint once per minute while visible', () => {
    reads.length = 0; renderToStaticMarkup(createElement(Catalog));
    expect(reads[0]).toMatchObject({ queryKey: ['/models'], refetchInterval: 60000, refetchIntervalInBackground: false });
    expect(reads[0]).not.toHaveProperty('refetchOnWindowFocus', false);
    expect(reads[0]).not.toHaveProperty('initialData');
  });
});
