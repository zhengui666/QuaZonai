import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it } from 'vitest';
import type { PortfolioView } from '../../../packages/contracts/src/index.ts';
import { BacktestProgress } from './backtest-progress';
const render = (data: Partial<PortfolioView>, compact = false) => renderToStaticMarkup(createElement(BacktestProgress, { portfolio: { status: 'RUNNING', curve: [], ...data } as PortfolioView, compact }));
it('shows actual member counts without estimating a percentage', () => {
  const markup = render({ progress: { phase: 'FORECASTING', completed: 1, total: 3, updatedAt: '2026-10-29T00:00:00Z' } });
  expect(markup).toContain('生成成员预测');
  expect(markup).toContain('1 / 3');
  expect(markup).not.toContain('%');
  expect(markup).toContain('首个权益快照');
});
it('labels a running curve provisional and omits finished list progress', () => {
  expect(render({ curve: [{ at: '2026-10-29T00:00:00Z', value: '100' }] })).toContain('回测尚未结束');
  expect(render({ status: 'COMPUTED' }, true)).toBe('');
});

it('translates recorded queued phase', () => {
  expect(render({ status: 'QUEUED', progress: { phase: 'QUEUED', updatedAt: '2026-10-29T00:00:00Z' } }, true)).toContain('等待回测');
});
