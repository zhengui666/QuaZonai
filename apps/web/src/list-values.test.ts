import { describe, expect, it } from 'vitest';
import { changeListParameters, filterEntries, isPlainNavigationClick, listParameters, listQuery, normalizeResearchState } from './list-values';

describe('collection navigation parameters', () => {
  it('preserves list conditions and the portfolio view while removing detail selections', () => {
    const source = new URLSearchParams('q=中文&page=3&sort=name&order=asc&view=tick&tab=inputs&trial=8&datasetId=old');
    expect(Object.fromEntries(listParameters(source))).toEqual({ q: '中文', page: '3', sort: 'name', order: 'asc', view: 'tick', datasetId: 'old' });
    expect(source.get('trial')).toBe('8');
    const query = new URLSearchParams(listQuery(source));
    expect(query.get('view')).toBeNull();
    expect(query.get('pageSize')).toBe('20');
    expect(query.get('q')).toBe('中文');
  });
  it('resets pagination for changed conditions and keeps it for paging', () => {
    const source = new URLSearchParams('page=8&pageSize=50&q=old&status=FAILED&view=allocations');
    const changed = changeListParameters(source, { q: 'new', status: undefined });
    expect(changed.get('page')).toBeNull();
    expect(changed.get('pageSize')).toBe('50');
    expect(changed.get('status')).toBeNull();
    expect(changed.get('view')).toBe('allocations');
    expect(changeListParameters(source, { page: 9 }, false).get('page')).toBe('9');
  });
  it('exposes all active business filters, including unknown deep-link values', () => {
    expect(filterEntries(new URLSearchParams('q=x&page=2&pageSize=20&sort=name&order=desc&view=tick&tab=evidence&research=101&datasetId=retired')))
      .toEqual([['research', '101'], ['datasetId', 'retired']]);
  });
  it('canonicalizes equivalent API cache identities', () => {
    expect(listQuery(new URLSearchParams('status=FAILED&q=x'))).toBe(listQuery(new URLSearchParams('q=x&status=FAILED')));
  });
  it.each([['进行中', 'RUNNING,STARTED,QUEUED'], ['已计算', 'COMPUTED'], ['失败', 'FAILED'], ['已停止', 'STOPPED'], ['待处理', 'IDLE'], ['全部状态', null]])('keeps legacy %s bookmarks searchable', (state, status) => {
    const result = normalizeResearchState(new URLSearchParams({ q: '中文', state, page: '3' }));
    expect(result.get('status')).toBe(status);
    expect(result.has('state')).toBe(false);
    expect(result.get('q')).toBe('中文');
    expect(result.get('page')).toBe('3');
    expect(new URLSearchParams(listQuery(result)).has('state')).toBe(false);
  });
  it('keeps an explicit canonical status when a bookmark also has an old state', () => {
    expect(normalizeResearchState(new URLSearchParams('status=STOPPED&state=失败')).get('status')).toBe('STOPPED');
  });
  it('only remembers navigation that leaves this tab, preventing later polling from stealing focus', () => {
    const primary = { button: 0, metaKey: false, ctrlKey: false, shiftKey: false, altKey: false };
    expect(isPlainNavigationClick(primary)).toBe(true);
    for (const modifier of ['metaKey', 'ctrlKey', 'shiftKey', 'altKey']) expect(isPlainNavigationClick({ ...primary, [modifier]: true })).toBe(false);
    expect(isPlainNavigationClick({ ...primary, button: 1 })).toBe(false);
    expect(isPlainNavigationClick({ ...primary, button: 2 })).toBe(false);
  });
});
