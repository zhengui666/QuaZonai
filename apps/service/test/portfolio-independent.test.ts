import assert from 'node:assert/strict';
import test from 'node:test';
import type { PortfolioTaskInput } from '../src/native.ts';
import { portfolioIndependentData } from '../src/science.ts';
import { createReviewPolicy } from '../../../packages/contracts/src/review-scoring.ts';
import { object } from '../src/facts.ts';

function request() { return { schema_version: 1, train_end_ns: '3', validation_end_ns: '6', forecast: { schema_version: 1, selection: { schema_version: 1, bar_types: ['ASSET.SIM-1-DAY-LAST-EXTERNAL'], event_start_ns: '0', event_end_ns: '9', decision_cutoff_ns: '9', maximum_rows: 100 }, parameters: { schema_version: 1, fast_period: 2, slow_period: 3, label_horizon_observations: 1 } } }; }

function input(): PortfolioTaskInput {
  return { id: 'portfolio', dataDir: '/tmp', catalog: '/component-sealed', binary: '/job', capital: '100', currency: 'USD', evidence: [{ selected: { result: { request: request() } } }],
    policy: { name: 'fixed method', kind: 'factor-threshold', threshold: 0, investedWeight: '1', targetTtlNs: '100', instrumentId: 'ASSET.SIM', simulationSettings: { base_currency: 'USD' } },
    review: { policy: createReviewPolicy('PORTFOLIO', 'policy', '1'), model: { provider: 'test', modelId: 'reviewer' }, createdAt: '2026-11-02T00:00:00Z' } };
}

test('new reviewed portfolios require a frozen independent dataset and never borrow component sealed', () => {
  const captured = input();
  assert.throws(() => portfolioIndependentData(captured), /portfolio_independent_dataset_required/);
  captured.review!.independentData = { catalog: '/fresh-unread', request: request() };
  assert.equal(portfolioIndependentData(captured), captured.review!.independentData);
  assert.equal(captured.catalog, '/component-sealed');
});

test('legacy admitted portfolio keeps its original independent consumer semantics', () => {
  const captured = input(); delete captured.review;
  assert.equal(portfolioIndependentData(captured), undefined);
});

test('an independent frozen remote binding cannot redirect the original worker', () => {
  const captured = input();
  captured.review!.independentData = { catalog: '/fresh', request: request(), nautilusTarget: { id: 'other', url: 'https://other.example' } };
  assert.throws(() => portfolioIndependentData(captured), /portfolio_independent_target_mismatch/);
  captured.binary = { id: 'same', url: 'https://native.example' };
  captured.review!.independentData.nautilusTarget = { id: 'same', url: 'https://native.example' };
  assert.ok(portfolioIndependentData(captured));
});

test('fresh windows cannot change reviewed forecast parameters or the BAR horizon', () => {
  const captured = input(); const original = request();
  captured.review!.independentData = { catalog: '/fresh', request: original };
  original.forecast.parameters.fast_period = 4;
  assert.throws(() => portfolioIndependentData(captured), /portfolio_independent_forecast_contract_changed/);
  original.forecast.parameters.fast_period = 2; original.forecast.parameters.label_horizon_observations = 2;
  assert.throws(() => portfolioIndependentData(captured), /portfolio_independent_forecast_contract_changed/);
  original.forecast.parameters.label_horizon_observations = 1; original.forecast.selection.bar_types = ['ASSET.SIM-1-HOUR-LAST-EXTERNAL'];
  assert.throws(() => portfolioIndependentData(captured), /portfolio_independent_forecast_contract_changed/);
});

test('a frozen independent dataset retains legacy fuel without changing the reviewed scientific method', () => {
  for (const total_fuel of ['1', '100000000']) {
    const captured = input();
    const original = request();
    captured.review!.independentData = { catalog: '/fresh', request: { ...original,
      forecast: { ...original.forecast, parameters: { ...original.forecast.parameters, total_fuel } } } };
    const frozen = structuredClone(captured);
    assert.equal(portfolioIndependentData(captured), captured.review!.independentData);
    assert.deepEqual(captured, frozen, 'Validation must retain both the native result and frozen independent fuel history');
    const changed = structuredClone(captured);
    const data = changed.review!.independentData!;
    const parameters = object(object(object(data.request).forecast).parameters);
    parameters.fast_period = 4;
    assert.throws(() => portfolioIndependentData(changed), /portfolio_independent_forecast_contract_changed/);
  }
});

test('catalog replay requires an independent original scope reference', () => {
  const captured = input(); captured.policy = { kind: 'catalog-ensemble' } as PortfolioTaskInput['policy'];
  captured.review!.independentData = { catalog: '/fresh' };
  assert.throws(() => portfolioIndependentData(captured), /portfolio_independent_catalog_scope_required/);
});
