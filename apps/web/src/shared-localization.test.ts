import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, it } from 'vitest';
import i18n from './i18n';
import { reasonText, ExecutionFailure } from './diagnostics';
import { NativeStrategyResultView, unavailableReason } from './research-results';
import { TickReplayProgress } from './tick-economic-replays';
import { factDisplayLabel, factMetricKey } from './metric-glossary';
import type { ResearchDetail } from '../../../packages/contracts/src/index.ts';
import type { TickEconomicReplaySummary } from '../../../packages/contracts/src/economic-tick.ts';

afterEach(async () => { await i18n.changeLanguage('en'); });

it('switches shared diagnostic copy while preserving raw identities and supplied messages', async () => {
  await i18n.changeLanguage('en');
  const english = reasonText('task_failed');
  expect(english).toContain('original task');
  expect(reasonText('unknown_code', '原始诊断')).toBe('原始诊断');
  const markup = renderToStaticMarkup(createElement(ExecutionFailure, { error: 'unknown_original_code', identity: 'task/123' }));
  expect(markup).toContain('Calculation incomplete');
  expect(markup).toContain('unknown_original_code');
  expect(markup).toContain('task/123');
  await i18n.changeLanguage('zh-CN');
  expect(reasonText('task_failed')).not.toBe(english);
  expect(reasonText('unknown_code', '原始诊断')).toBe('原始诊断');
});

it('renders result interface copy in either language without translating original fields', async () => {
  const value = {
    request: { instrument_id: 'BTC-USDC-PERP.HYPERLIQUID', strategy: { name: '用户原始策略名称' } },
    nativeVersion: '0.69.0', equityPerformance: { status: 'UNAVAILABLE', reason: 'NO_COMPLETED_NATIVE_RESULT' },
    canonicalResult: null, canonicalUnavailableReason: 'NO_RETAINED_ENGINE', nativeResults: [],
  } as unknown as NonNullable<ResearchDetail['final']>['nativeStrategyResult'];
  await i18n.changeLanguage('en');
  const english = renderToStaticMarkup(createElement(NativeStrategyResultView, { value }));
  expect(english).toContain('Backtest results');
  expect(english).toContain('用户原始策略名称');
  expect(english).toContain('BTC-USDC-PERP.HYPERLIQUID');
  expect(unavailableReason('NO_COMPLETED_NATIVE_RESULT')).toBe('No completed native run result is available yet.');
  await i18n.changeLanguage('zh-CN');
  const chinese = renderToStaticMarkup(createElement(NativeStrategyResultView, { value }));
  expect(chinese).toContain('回测结果');
  expect(chinese).toContain('用户原始策略名称');
  expect(chinese).toContain('BTC-USDC-PERP.HYPERLIQUID');
});

it('updates progress and historical fact labels at render time, not module import', async () => {
  const replay = { status: 'RUNNING', progress: { phase: 'REPLAYING', completed: 1, total: 4, updatedAt: null } } as unknown as TickEconomicReplaySummary;
  await i18n.changeLanguage('en');
  expect(renderToStaticMarkup(createElement(TickReplayProgress, { replay }))).toContain('Native economic replay');
  expect(factDisplayLabel('总权益')).toBe('Total equity');
  expect(factMetricKey('Total equity')).toBe('equity');
  expect(factDisplayLabel('用户自定义标签')).toBe('用户自定义标签');
  await i18n.changeLanguage('zh-CN');
  expect(renderToStaticMarkup(createElement(TickReplayProgress, { replay }))).toContain('原生经济回放');
  expect(factDisplayLabel('用户自定义标签')).toBe('用户自定义标签');
});
