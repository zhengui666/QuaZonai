import assert from 'node:assert/strict';
import test from 'node:test';
import type { PortfolioTaskInput } from '../src/native.ts';
import { portfolioIndependentData } from '../src/science.ts';
import { createReviewPolicy } from '../../../packages/contracts/src/review-scoring.ts';

function input(): PortfolioTaskInput {
  return { id: 'portfolio', dataDir: '/tmp', catalog: '/component-sealed', binary: '/job', capital: '100', currency: 'USD', evidence: [],
    policy: { kind: 'factor-threshold' } as PortfolioTaskInput['policy'],
    review: { policy: createReviewPolicy('PORTFOLIO', 'policy', '1'), model: { provider: 'test', modelId: 'reviewer' }, createdAt: '2026-11-02T00:00:00Z' } };
}

test('new reviewed portfolios require a frozen independent dataset and never borrow component sealed', () => {
  const captured = input();
  assert.throws(() => portfolioIndependentData(captured), /portfolio_independent_dataset_required/);
  captured.review!.independentData = { catalog: '/fresh-unread', request: { independent: true } };
  assert.equal(portfolioIndependentData(captured), captured.review!.independentData);
  assert.equal(captured.catalog, '/component-sealed');
});

test('legacy admitted portfolio keeps its original independent consumer semantics', () => {
  const captured = input(); delete captured.review;
  assert.equal(portfolioIndependentData(captured), undefined);
});

test('an independent frozen remote binding cannot redirect the original worker', () => {
  const captured = input();
  captured.review!.independentData = { catalog: '/fresh', nautilusTarget: { id: 'other', url: 'https://other.example' } };
  assert.throws(() => portfolioIndependentData(captured), /portfolio_independent_target_mismatch/);
  captured.binary = { id: 'same', url: 'https://native.example' };
  captured.review!.independentData.nautilusTarget = { id: 'same', url: 'https://native.example' };
  assert.ok(portfolioIndependentData(captured));
});

test('catalog replay requires an independent original scope reference', () => {
  const captured = input(); captured.policy = { kind: 'catalog-ensemble' } as PortfolioTaskInput['policy'];
  captured.review!.independentData = { catalog: '/fresh' };
  assert.throws(() => portfolioIndependentData(captured), /portfolio_independent_catalog_scope_required/);
});
