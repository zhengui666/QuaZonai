import { createElement, useState } from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { SettingsView } from '../../../packages/contracts/src/index.ts';
import i18n from './i18n';
import { shortError } from './api';

// Exercise the real React/query/i18next state and PATCH handler without a browser.
// Ant Design's portal and DOM behavior are outside this component-level test.
vi.mock('antd', () => ({ Select: (props: Record<string, unknown>) => createElement('select', props) }));
vi.mock('./ui', () => ({
  ErrorNotice: ({ error }: { error?: unknown }) => error ? createElement('p', { role: 'alert' }, shortError(error)) : null,
  Loading: () => createElement('p', null, 'Loading'),
}));
vi.mock('./model-settings', () => ({ default: () => null }));
vi.mock('./extensions', () => ({ default: () => null }));
vi.mock('./instance-settings', () => ({ default: () => null }));
vi.mock('./account-settings', () => ({ default: () => null }));
vi.mock('./data', () => ({ default: () => null }));
import Settings from './settings';

const original: SettingsView = { defaultModel: { provider: 'original', modelId: 'original-model' }, thinkingLevel: 'high', serviceTier: 'priority', tickBookCores: 8 };
let renderer: ReactTestRenderer | undefined;
let cache: QueryClient;
beforeEach(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  await i18n.changeLanguage('en');
  cache = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity }, mutations: { retry: false } } });
});
afterEach(async () => {
  if (renderer) await act(async () => { renderer!.unmount(); });
  renderer = undefined;
  cache.clear();
  await i18n.changeLanguage('en');
  vi.unstubAllGlobals();
});

// A mounted editor subscribes to the same language changes, with independent
// unsaved React state. This checks rerender preservation, not route navigation.
function DraftEditor() {
  const { t } = useTranslation('core');
  const [draft, setDraft] = useState('');
  return createElement('input', { 'data-testid': 'draft', 'aria-label': t('Research'), value: draft, onChange: (event: { target: { value: string } }) => setDraft(event.target.value) });
}
async function mount(value: SettingsView = original) {
  cache.setQueryData(['/settings'], value);
  await act(async () => {
    renderer = create(createElement(QueryClientProvider, { client: cache },
      createElement(Settings, { connect: () => {}, route: { page: 'settings', creating: false, params: new URLSearchParams() } }),
      createElement(DraftEditor)));
  });
}
const select = () => renderer!.root.findByType('select');
const draft = () => renderer!.root.findByProps({ 'data-testid': 'draft' });

it('defaults historical settings to English without an extra preference or request', async () => {
  const fetch = vi.fn(); vi.stubGlobal('fetch', fetch);
  await mount();
  expect(i18n.language).toBe('en');
  expect(select().props.value).toBe('en');
  expect(renderer!.root.findByType('h2').children.join('')).toBe('General');
  expect(cache.getQueryCache().getAll().map(query => query.queryKey)).toEqual([['/settings']]);
  expect(fetch).not.toHaveBeenCalled();
});

it('applies only the successful PATCH receipt to the shared cache and locale, preserving drafts', async () => {
  let complete!: (value: Response) => void;
  const fetch = vi.fn(() => new Promise<Response>(resolve => { complete = resolve; }));
  vi.stubGlobal('fetch', fetch);
  await mount();
  const bytes = '用户原始草稿\n9007199254740993.0000000001';
  await act(async () => { draft().props.onChange({ target: { value: bytes } }); });
  await act(async () => { select().props.onChange('zh-CN'); });
  expect(fetch).toHaveBeenCalledTimes(1);
  expect(fetch.mock.calls[0]).toEqual(['/api/v3/settings', expect.objectContaining({ method: 'PATCH', body: '{"language":"zh-CN"}', credentials: 'same-origin' })]);
  expect(select().props.disabled).toBe(true);
  expect(i18n.language).toBe('en');
  expect(cache.getQueryData(['/settings'])).toEqual(original);
  const saved = { ...original, language: 'zh-CN' as const };
  await act(async () => { complete(Response.json(saved)); });
  await vi.waitFor(() => expect(i18n.language).toBe('zh-CN'));
  // Flush TanStack's queued observer notification without changing component identity.
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 0)); });
  expect(select().props.value).toBe('zh-CN');
  expect(select().props.disabled).toBe(false);
  expect(cache.getQueryData(['/settings'])).toEqual(saved);
  expect(cache.getQueryCache().getAll().map(query => query.queryKey)).toEqual([['/settings']]);
  expect(draft().props.value).toBe(bytes);
  expect(renderer!.root.findByType('h2').children.join('')).toBe('通用');
  expect(fetch).toHaveBeenCalledTimes(1);
});

it('retains the previous language, settings and draft on save failure and displays the error', async () => {
  await i18n.changeLanguage('zh-CN');
  const saved = { ...original, language: 'zh-CN' as const };
  const fetch = vi.fn().mockResolvedValue(Response.json({ error: 'unavailable' }, { status: 503 }));
  vi.stubGlobal('fetch', fetch);
  await mount(saved);
  await act(async () => { draft().props.onChange({ target: { value: 'draft stays unchanged' } }); });
  await act(async () => { select().props.onChange('en'); });
  await vi.waitFor(() => expect(select().props.disabled).toBe(false));
  expect(i18n.language).toBe('zh-CN');
  expect(select().props.value).toBe('zh-CN');
  expect(cache.getQueryData(['/settings'])).toEqual(saved);
  expect(draft().props.value).toBe('draft stays unchanged');
  expect(renderer!.root.findByProps({ role: 'alert' }).children.join('')).toBe(shortError('unavailable'));
  expect(fetch).toHaveBeenCalledTimes(1);
});


it('cancels settings reads both before and during PATCH so late old GETs cannot overwrite the receipt', async () => {
  // Unlike the basic cached-render cases, use normal stale queries so mounting
  // starts a real GET. Explicit refetch below models polling during the PATCH.
  cache.setDefaultOptions({ queries: { retry: false, staleTime: 0 }, mutations: { retry: false } });
  const reads: { finish: (value: Response) => void; signal: AbortSignal }[] = [];
  let finishPatch!: (value: Response) => void;
  const fetch = vi.fn((_url: string, options: RequestInit) => {
    if (options.method === 'PATCH') return new Promise<Response>(resolve => { finishPatch = resolve; });
    // Deliberately ignore abort in the transport: TanStack cancellation must
    // also prevent a late resolved response from publishing stale data.
    return new Promise<Response>(resolve => { reads.push({ finish: resolve, signal: options.signal as AbortSignal }); });
  });
  vi.stubGlobal('fetch', fetch);
  await mount();
  expect(reads).toHaveLength(1);
  await act(async () => { select().props.onChange('zh-CN'); });
  expect(reads[0]!.signal.aborted).toBe(true);
  expect(fetch.mock.calls.filter(([, options]) => options.method === 'PATCH')).toHaveLength(1);
  let polling!: Promise<void>;
  await act(async () => { polling = cache.refetchQueries({ queryKey: ['/settings'], exact: true }); });
  expect(reads).toHaveLength(2);
  expect(reads[1]!.signal.aborted).toBe(false);
  const saved = { ...original, language: 'zh-CN' as const };
  await act(async () => { finishPatch(Response.json(saved)); });
  await vi.waitFor(() => expect(i18n.language).toBe('zh-CN'));
  expect(reads[1]!.signal.aborted).toBe(true);
  await act(async () => {
    for (const read of reads) read.finish(Response.json({ ...original, language: 'en' }));
    await polling;
    await new Promise(resolve => setTimeout(resolve, 0));
  });
  expect(cache.getQueryData(['/settings'])).toEqual(saved);
  expect(select().props.value).toBe('zh-CN');
  expect(i18n.language).toBe('zh-CN');
  expect(fetch).toHaveBeenCalledTimes(3);
});
