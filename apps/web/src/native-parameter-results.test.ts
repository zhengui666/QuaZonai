import i18n from './i18n';
import { beforeEach as useChineseInterface, afterEach as restoreDefaultInterface } from 'vitest';
// Keep legacy Chinese-language regression assertions explicit; new language tests cover the English default.
useChineseInterface(async () => { await i18n.changeLanguage('zh-CN'); });
restoreDefaultInterface(async () => { await i18n.changeLanguage('en'); });
import { createElement } from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, expect, it, vi } from 'vitest';
const calls = vi.hoisted(() => [] as string[]);
vi.mock('./api', async importOriginal => ({ ...await importOriginal<typeof import('./api')>(), api: async (path: string) => { calls.push(path); return path.includes('cursor=') ? { items: [{ taskId: 21, status: 'failed', evidenceHref: '/api/v3/research/7/evidence/21' }], nextCursor: null } : { items: [{ taskId: 1, status: 'completed', evidenceHref: '/api/v3/research/7/evidence/1' }], nextCursor: 'next/page' }; } }));
vi.mock('antd', async importOriginal => {
  const original = await importOriginal<typeof import('antd')>();
  return { ...original,
    Button: ({children, onClick, disabled}: any) => createElement('button', { onClick, disabled }, children),
    Table: ({ dataSource, columns }: any) => createElement('div', {}, dataSource.map((row: any) => createElement('div', {key: row.taskId}, columns.map((column: any, index: number) => createElement('span', {key: index}, column.render ? column.render(row[column.dataIndex], row) : row[column.dataIndex]))))),
  };
});
import { NativeBacktestHistory } from './research';
let renderer: ReactTestRenderer | undefined; let client: QueryClient;
const settle = async () => { await act(async () => { await new Promise(resolve => setTimeout(resolve, 15)); }); };
afterEach(async () => { if(renderer) await act(async () => renderer!.unmount()); client?.clear(); vi.unstubAllGlobals(); });
it('owner can page all native task links, return to prior results and refresh without running a backtest', async () => {
  calls.length = 0; vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  await act(async () => { renderer = create(createElement(QueryClientProvider, {client}, createElement(NativeBacktestHistory, {researchId: 7, active: false}))); }); await settle();
  const button = (text: string) => renderer!.root.findAllByType('button').find(node => node.children.includes(text))!;
  const link = () => renderer!.root.findByType('a').props.href;
  expect(link()).toBe('/api/v3/research/7/evidence/1');
  expect(button('上一页').props.disabled).toBe(true);
  await act(async () => button('下一页').props.onClick()); await settle();
  expect(link()).toBe('/api/v3/research/7/evidence/21');
  expect(button('下一页').props.disabled).toBe(true);
  await act(async () => button('上一页').props.onClick()); await settle();
  expect(link()).toBe('/api/v3/research/7/evidence/1');
  await act(async () => button('刷新').props.onClick()); await settle();
  expect(calls).toContain('/research/7/native-backtests?cursor=next%2Fpage');
  expect(calls.every(path => path.startsWith('/research/7/native-backtests'))).toBe(true);
  expect(JSON.stringify(renderer!.toJSON())).toContain('不代表排名或样本外验证');
});
