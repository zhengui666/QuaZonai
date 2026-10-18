import { useEffect, useState } from 'react';
export type Workspace = 'research' | 'data' | 'factors' | 'portfolios' | 'observations' | 'settings' | 'extensions';
export interface Route { page: Workspace; id?: string; creating: boolean; params: URLSearchParams; unknown?: boolean }
const pages = new Set<Workspace>(['research', 'data', 'factors', 'portfolios', 'observations', 'settings', 'extensions']);
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
  useEffect(() => { const update = () => setRoute(currentRoute()); window.addEventListener('hashchange', update); return () => window.removeEventListener('hashchange', update); }, []);
  return route;
}
export function navigate(path: string, replace = false) {
  if (!replace) location.hash = path;
  else { history.replaceState(null, '', `#${path}`); window.dispatchEvent(new HashChangeEvent('hashchange')); }
}
export function href(path: string) { return `#${path}`; }
