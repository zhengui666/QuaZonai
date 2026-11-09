const detailKeys = ['tab', 'trial'];
const navigationKeys = new Set(['view', 'section', ...detailKeys]);
const controlKeys = new Set(['q', 'page', 'pageSize', 'sort', 'order']);

/** Old bookmarks used translated state labels; the API now uses canonical status values. */
export function normalizeResearchState(params: URLSearchParams) {
  const result = new URLSearchParams(params);
  const legacy = result.get('state');
  if (!result.has('status') && legacy) {
    const statuses: Record<string, string> = { '进行中': 'RUNNING,STARTED,QUEUED', '已计算': 'COMPUTED', '失败': 'FAILED', '已停止': 'STOPPED', '待处理': 'IDLE' };
    if (statuses[legacy]) result.set('status', statuses[legacy]);
  }
  result.delete('state');
  return result;
}
export function isPlainNavigationClick(event: Pick<MouseEvent, 'button' | 'metaKey' | 'ctrlKey' | 'shiftKey' | 'altKey'>) {
  return event.button === 0 && !event.metaKey && !event.ctrlKey && !event.shiftKey && !event.altKey;
}

export function listParameters(params: URLSearchParams) {
  const result = new URLSearchParams(params);
  for (const key of detailKeys) result.delete(key);
  return result;
}
export function listQuery(params: URLSearchParams, defaultSort?: string) {
  const result = listParameters(params);
  for (const key of navigationKeys) result.delete(key);
  if (!result.has('page')) result.set('page', '1');
  if (!result.has('pageSize')) result.set('pageSize', '20');
  if (!result.has('sort') && defaultSort) { result.set('sort', defaultSort); result.set('order', 'desc'); }
  result.sort();
  return result.toString();
}
export function filterEntries(params: URLSearchParams) {
  return [...params.entries()].filter(([key, value]) => value && !navigationKeys.has(key) && !controlKeys.has(key));
}
export function changeListParameters(params: URLSearchParams, patch: Record<string, string | number | undefined>, resetPage = true) {
  const result = listParameters(params);
  for (const [key, value] of Object.entries(patch)) {
    if (value === undefined || value === '') result.delete(key);
    else result.set(key, String(value));
  }
  const currency = result.get('currency');
  if (!currency || currency.includes(',') || currency === 'unknown') {
    if (result.get('sort') === 'capital') { result.delete('sort'); result.delete('order'); }
    result.delete('capitalMin'); result.delete('capitalMax');
  }
  const metric = result.get('metric');
  if (result.get('sort') === 'criterionValue' && (!metric || metric.includes(',') || metric === 'unknown')) { result.delete('sort'); result.delete('order'); }
  if (resetPage) result.delete('page');
  return result;
}
