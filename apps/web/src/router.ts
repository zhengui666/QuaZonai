import { useEffect, useState } from 'react';
export type Workspace = 'pm' | 'research' | 'data' | 'factors' | 'parameter-scans' | 'portfolios' | 'observations' | 'settings' | 'extensions';
export interface Route { page: Workspace; id?: string; creating: boolean; params: URLSearchParams; unknown?: boolean }
const pages = new Set<Workspace>(['pm', 'research', 'data', 'parameter-scans', 'portfolios', 'observations', 'settings', 'extensions']);
export function currentRoute(): Route {
  const [path = '', query = ''] = location.hash.replace(/^#\/?/, '').split('?');
  const [name, id, extra] = path.split('/');
  const page = pages.has(name as Workspace) ? name as Workspace : 'research';
  const params = new URLSearchParams(query);
  const unknown = !!name && !pages.has(name as Workspace) || extra !== undefined || !!id && ['data', 'settings', 'extensions'].includes(page);
  try { return { page, ...(id && id !== 'new' ? { id: decodeURIComponent(id) } : {}), creating: id === 'new', params, unknown }; }
  catch { return { page, creating: false, params, unknown: true }; }
}
export function useRoute() {
  const [route, setRoute] = useState(currentRoute);
  useEffect(() => {
    const update = () => { setRoute(currentRoute()); };
    update(); window.addEventListener('hashchange', update);
    return () => window.removeEventListener('hashchange', update);
  }, []);
  return route;
}
export function navigate(path: string, replace = false) {
  if (!replace) location.hash = path;
  else { history.replaceState(null, '', `#${path}`); window.dispatchEvent(new HashChangeEvent('hashchange')); }
}
export function href(path: string) { return `#${path}`; }

const tableScrollers = () => [...document.querySelectorAll<HTMLElement>('.workspace-table .ant-table-content, .workspace-table .ant-table-body')];
const listPositions = new Map<string, { x: number; y: number; tables: number[]; target?: string }>();
/** Remember only collection navigation; details keep their complete query in the URL. */
export function rememberListNavigation(target?: string) {
  const route = currentRoute();
  if (route.id || route.creating) return;
  const focused = document.activeElement instanceof HTMLAnchorElement ? document.activeElement.getAttribute('href') ?? undefined : undefined;
  listPositions.set(location.hash, { x: window.scrollX, y: window.scrollY, tables: tableScrollers().map(element => element.scrollLeft), target: target ?? focused });
}
/** May be retried after the collection query settles and rows mount. */
export function restoreListNavigation(route: Route, settled = false) {
  if (route.id || route.creating) return false;
  const position = listPositions.get(location.hash);
  if (!position) return false;
  window.scrollTo(position.x, position.y);
  if (settled) {
    const key = location.hash;
    // AntD measures and initializes its scroll containers after the collection mounts.
    // Restore after that layout work, retaining the position through the initial route effect.
    requestAnimationFrame(() => requestAnimationFrame(() => {
      if (location.hash !== key || listPositions.get(key) !== position) return;
      const target = position.target ? [...document.querySelectorAll<HTMLAnchorElement>('a[href]')].find(anchor => anchor.getAttribute('href') === position.target) : undefined;
      (target ?? document.getElementById('main-content'))?.focus({ preventScroll: true });
      tableScrollers().forEach((element, index) => { element.scrollLeft = position.tables[index] ?? 0; });
      window.scrollTo(position.x, position.y); listPositions.delete(key);
    }));
  }
  return true;
}
