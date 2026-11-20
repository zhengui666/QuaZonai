import i18n from './i18n';
import { beforeEach as useChineseInterface, afterEach as restoreDefaultInterface } from 'vitest';
// Keep legacy Chinese-language regression assertions explicit; new language tests cover the English default.
useChineseInterface(async () => { await i18n.changeLanguage('zh-CN'); });
restoreDefaultInterface(async () => { await i18n.changeLanguage('en'); });
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it, vi } from 'vitest';
import Portfolios from './portfolio';

vi.mock('./delete-record', () => ({ DeleteRecord: () => null }));
vi.mock('./data-hooks', () => ({
  usePortfolioDetail: () => ({ isPending: false, data: {
    id: 'p-original', name: '原始账户目标', status: 'COMPUTED', taskId: null,
    datasetId: 'native-data', capital: '1000.000000000000000001', currency: 'USD',
    createdAt: '2026-11-12T00:00:00Z', factors: [], curve: [], metrics: [],
    targets: [{ instrumentId: 'BTCUSDT.BINANCE', weight: '0.25' }],
  } }),
  useResource: () => ({ isPending: false, data: [{ id: 'o-original', portfolioId: 'p-original', name: '已有账户', mode: 'paper', connectionId: 'binding' }] }),
}));
const render = (creating: boolean, params = new URLSearchParams(), id = creating ? undefined : 'p-original') => renderToStaticMarkup(createElement(Portfolios, { route: { page: 'portfolios', creating, id, params } }));
it('removes retired portfolio construction and review allocation routes', () => {
  for (const html of [render(true), render(false, new URLSearchParams('view=allocations'))]) {
    expect(html).toContain('该自研组合创建入口已移除');
    expect(html).toContain('#/research/new');
    expect(html).not.toContain('<form');
  }
});
it('returns a neutral notice for old tick list and detail links without opening a portfolio', () => {
  const params = new URLSearchParams('view=tick');
  const list = renderToStaticMarkup(createElement(Portfolios, { route: { page: 'portfolios', creating: false, params } }));
  for (const html of [list, render(false, params, 'old-tick-id')]) {
    expect(html).toContain('旧版 Tick 经济回放页面已退役');
    expect(html).toContain('href="#/portfolios"');
    expect(html).not.toContain('原始账户目标');
    expect(html).not.toContain('tick-economic-replays');
  }
});
it('retains real target delivery, exact capital, and account capital management links', () => {
  const html = render(false);
  expect(html).toContain('1000.000000000000000001');
  expect(html).toContain('BTCUSDT.BINANCE');
  expect(html).not.toContain('交付目标');
  expect(html).toContain('历史目标（只读）');
  expect(html).toContain('#/observations/o-original?section=capital');
  expect(html).not.toContain('重评原组合容量');
  expect(html).not.toContain('评审结论');
  expect(html).not.toContain('创建组合');
});
