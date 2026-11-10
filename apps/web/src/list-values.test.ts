import { describe, expect, it } from 'vitest';
import { changeListParameters, filterEntries, isPlainNavigationClick, listParameters, listQuery } from './list-values';

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
  it.each([undefined, '', 'USD,USDT', 'unknown'])('removes amount bounds and sorting when a currency chip becomes %s', currency => {
    const source = new URLSearchParams('page=3&currency=USD&capitalMin=100&capitalMax=999&sort=capital&order=asc&view=strategies');
    const result = changeListParameters(source, { currency });
    for (const key of ['capitalMin', 'capitalMax', 'sort', 'order', 'page']) expect(result.has(key)).toBe(false);
    expect(result.get('view')).toBe('strategies');
  });
  it.each([undefined, '', 'pearson_ic,Sharpe Ratio (365 days)', 'unknown'])('removes criterion sorting when a metric chip becomes %s', metric => {
    const result = changeListParameters(new URLSearchParams('metric=pearson_ic&sort=criterionValue&order=desc&page=3'), { metric });
    for (const key of ['sort', 'order', 'page']) expect(result.has(key)).toBe(false);
  });
  it('clears dependent sorts atomically through the same path used by clear-all', () => {
    for (const source of [new URLSearchParams('q=x&currency=USD&capitalMin=1&sort=capital&order=asc&page=2'), new URLSearchParams('q=x&metric=pearson_ic&sort=criterionValue&order=desc&page=2')]) {
      const patch = Object.fromEntries([...filterEntries(source).map(([key]) => [key, undefined]), ['q', undefined]]);
      expect(changeListParameters(source, patch).toString()).toBe('');
    }
  });
  it('preserves valid single-currency and single-metric sorting when changing unrelated filters', () => {
    expect(changeListParameters(new URLSearchParams('currency=USD&capitalMin=1&sort=capital&order=asc'), { q: 'test' }).get('sort')).toBe('capital');
    expect(changeListParameters(new URLSearchParams('metric=pearson_ic&sort=criterionValue&order=desc'), { q: 'test' }).get('sort')).toBe('criterionValue');
  });
  it('only remembers navigation that leaves this tab, preventing later polling from stealing focus', () => {
    const primary = { button: 0, metaKey: false, ctrlKey: false, shiftKey: false, altKey: false };
    expect(isPlainNavigationClick(primary)).toBe(true);
    for (const modifier of ['metaKey', 'ctrlKey', 'shiftKey', 'altKey']) expect(isPlainNavigationClick({ ...primary, [modifier]: true })).toBe(false);
    expect(isPlainNavigationClick({ ...primary, button: 1 })).toBe(false);
    expect(isPlainNavigationClick({ ...primary, button: 2 })).toBe(false);
  });
});
