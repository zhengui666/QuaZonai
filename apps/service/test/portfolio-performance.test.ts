import assert from 'node:assert/strict';
import test from 'node:test';
import type { PortfolioFact } from '../src/facts.ts';
import { portfolioPerformance } from '../src/portfolio-performance.ts';
import { portfolioView } from '../src/projections.ts';

function fixture(days = 365): PortfolioFact {
  return { input: { requestId: 'original', name: 'original', datasetId: 'data', capital: '100', researchIds: [1] },
    policy: {}, evidence: [], target: null,
    view: { id: 'p', name: 'original', datasetId: 'data', researchIds: [1], status: 'COMPUTED', capital: '100', currency: 'USD', createdAt: '2026-01-01T00:00:00Z', taskId: 1,
      metrics: [], curve: [], targets: [], error: null },
    native: { simulationRequest: { selection: { event_start_ns: '1', event_end_ns: '2' } }, simulation: { statistics: [
      { native_key: `Sharpe Ratio (${days} days)`, value: 1.5 },
      { native_key: 'Max Drawdown', value: -0.125 },
      { native_key: 'Calendar Geometric Daily Return', value: 0.001 },
      { native_key: 'Calendar Expected Days', value: 30 },
      { native_key: 'Calendar Observed Return Days', value: 30 },
      { native_key: 'Calendar Partial Days', value: 2 },
    ] } } };
}
function update(fact: PortfolioFact, key: string, value: number | null) {
  const rows = (fact.native!.simulation as { statistics: { native_key: string; value: number | null }[] }).statistics;
  rows.find(row => row.native_key === key)!.value = value;
}

test('shared-account summary keeps native Sharpe, signed drawdown and complete-day annualization distinct', () => {
  for (const days of [252, 365]) {
    const fact = fixture(days); const summary = portfolioPerformance(fact);
    assert.equal(summary.sharpe, 1.5); assert.equal(summary.maxDrawdown, 0.125);
    assert.ok(Math.abs(summary.annualizedReturn! - ((1.001 ** days) - 1)) < 1e-12);
    assert.equal(summary.annualizationDays, days);
    assert.equal(summary.basis.partialDays, 2);
    assert.equal(summary.basis.annualizedReturn, 'COMPLETE_UTC_DAYS_GEOMETRIC');
    assert.equal(summary.basis.sharpe, 'NATIVE_ACCOUNT_DAILY_RETURNS');
    assert.deepEqual(portfolioView(fact).performance, summary, 'detail and list consume the same projection');
  }
});
test('missing days do not invent annual returns or erase available native statistics', () => {
  const fact = fixture(); update(fact, 'Calendar Observed Return Days', 29);
  assert.equal(portfolioPerformance(fact).annualizedReturn, null);
  assert.equal(portfolioPerformance(fact).sharpe, 1.5);
  update(fact, 'Calendar Observed Return Days', 30); update(fact, 'Calendar Geometric Daily Return', null);
  assert.equal(portfolioPerformance(fact).annualizedReturn, null);
});
test('zero, complete loss, overflow, and unfinished execution retain their meaning', () => {
  const fact = fixture(); update(fact, 'Calendar Geometric Daily Return', 0);
  assert.equal(portfolioPerformance(fact).annualizedReturn, 0);
  update(fact, 'Calendar Geometric Daily Return', -1);
  assert.equal(portfolioPerformance(fact).annualizedReturn, -1);
  update(fact, 'Calendar Geometric Daily Return', 1e10);
  assert.equal(portfolioPerformance(fact).annualizedReturn, null);
  fact.view.status = 'RUNNING';
  assert.equal(portfolioPerformance(fact).sharpe, null);
  assert.equal(portfolioPerformance(fact).maxDrawdown, null);
});
test('historical metrics keep their known convention, without replacing gaps with equity approximations', () => {
  const fact = fixture(); fact.native = null;
  fact.view.metrics = [{ key: 'Sharpe Ratio (252 days)', label: 'native', value: -0.4 }, { key: 'Max Drawdown', label: 'native', value: -0.2 }];
  const summary = portfolioPerformance(fact);
  assert.equal(summary.sharpe, -0.4); assert.equal(summary.annualizationDays, 252);
  assert.equal(summary.maxDrawdown, 0.2); assert.equal(summary.annualizedReturn, null);
  fact.view.metrics.push({ key: 'Sharpe Ratio (365 days)', label: 'conflicting', value: 2 });
  assert.equal(portfolioPerformance(fact).sharpe, null);
  assert.equal(portfolioPerformance(fact).annualizationDays, null);
});
test('spot CASH statistics belong to the original fee-inclusive spot report, never top-level position statistics', () => {
  const fact = fixture(); const simulation = fact.native!.simulation as Record<string, unknown>;
  simulation.spot_cash_report = { statistics: simulation.statistics };
  simulation.statistics = [{ native_key: 'Sharpe Ratio (252 days)', value: 99 }];
  const summary = portfolioPerformance(fact);
  assert.equal(summary.sharpe, 1.5); assert.equal(summary.annualizationDays, 365);
  assert.equal(summary.maxDrawdown, 0.125); assert.ok(summary.annualizedReturn! > 0);
  assert.equal(summary.basis.sharpe, 'NATIVE_SPOT_CASH_COMPLETE_DAY_RETURNS');
  assert.equal(portfolioView(fact).metrics.find(row => row.key === 'Max Drawdown')?.value, -0.125);
  assert.equal(portfolioView(fact).metrics.find(row => row.key === 'Sharpe Ratio (365 days)')?.value, 1.5);
  simulation.spot_cash_report = { statistics: [] };
  assert.equal(portfolioPerformance(fact).sharpe, null, 'missing spot evidence cannot fall back to a different source');
});
