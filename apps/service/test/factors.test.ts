import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { researchSpecification } from '../../../packages/contracts/src/research.ts';
import { researchInput } from '../../research/src/input.ts';
import { Study } from '../../research/src/documents.ts';
import { factorViews } from '../src/factors.ts';
import { context, setup, studied, wat } from './support.ts';

const specification = researchSpecification({ version: 1, name: 'Shared-account research', question: 'Does the ensemble improve account returns?', rationale: 'Projection protocol only',
  evaluation: { metric: 'Sharpe Ratio (252 days)', direction: 'maximize', target: 1, scheme: 'registered-partitions' },
  search: { scope: 'ensemble', maxMembers: 2, stop: 'budget-or-selection' } });
const proposal = { members: [{ model: wat, weight: '0.25' }, { model: wat, weight: '0.75' }] };
const account = (partition: string): JsonObject => ({ partitions: [], portfolio: { partition, strategy_kind: 'catalog-ensemble',
  statistics: [{ native_key: 'Sharpe Ratio (252 days)', value: 2 }] } });

test('catalog ensemble account metrics remain on the research and are never attributed to members without diagnostics', async () => {
  const state = await setup();
  try {
    const { receipt } = await studied(state);
    for (const components of [undefined, []]) {
      await state.service.research.harness.commit(async tx => {
        const study = await tx.doc(Study, receipt.conversationId);
        study.input!.specification = specification;
        study.input!.goal = specification.question;
        study.trials[0]!.proposal = proposal;
        study.trials[0]!.result = { ...account('VALIDATION'), ...(components ? { components } : {}) };
        study.final!.result = account('HELDOUT');
      }, context);
      const factorsResponse = await state.service.app.inject({ url: '/api/v3/factors', headers: state.headers });
      assert.equal(factorsResponse.statusCode, 200, factorsResponse.body);
      const factors = factorsResponse.json();
      assert.equal(factors.length, 2);
      for (const factor of factors) {
        assert.deepEqual(factor.metrics, []);
        assert.equal(factor.criterion, null);
        assert.equal(factor.evidence, 'component_only');
        assert.equal(factor.combinable, false);
      }
      const detailResponse = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers });
      assert.equal(detailResponse.statusCode, 200, detailResponse.body);
      const detail = detailResponse.json();
      assert.equal(detail.final.metrics.find((metric: { key: string }) => metric.key === 'HELDOUT.portfolio.Sharpe Ratio (252 days)').value, 2);
      assert.equal(detail.criterion.status, 'met');
    }
  } finally { await state.cleanup(); }
});

test('real member diagnostics retain their own values without inheriting the shared account criterion', () => {
  const result = { ...account('HELDOUT'), components: [{ summary: { partitions: [{ partition: 'HELDOUT', metric: { counts: { included: '2' }, pearson_ic: { value: 0.1 } } }] } }] };
  const input = researchInput({ goal: specification.question, specification, model: { provider: 'test', modelId: 'test' }, data: {}, policy: { max_trials: 1 } });
  const factors = factorViews(3, { input, trials: [{ taskId: 17, status: 'COMPUTED', proposal, result }], final: { status: 'COMPUTED', trialId: 17, result } });
  assert.equal(factors[0]!.metrics.find(metric => metric.key === 'HELDOUT.pearson_ic')!.value, 0.1);
  assert.equal(factors[0]!.metrics.some(metric => metric.key.includes('portfolio')), false);
  assert.equal(factors[0]!.criterion!.status, 'insufficient');
  assert.equal(factors[0]!.criterion!.value, null);
  assert.deepEqual(factors[1]!.metrics, []);
  assert.equal(factors[1]!.criterion, null);
});

test('legacy single-factor results keep their metrics and qualification', () => {
  const result = { partitions: [{ partition: 'HELDOUT', metric: { counts: { included: '2' }, pearson_ic: { value: 0.2 } } }] };
  const input = researchInput({ goal: 'Legacy factor', model: { provider: 'test', modelId: 'test' }, data: {}, policy: { max_trials: 1, strategy: {} } });
  const study = { input, trials: [{ taskId: 17, status: 'COMPUTED', proposal: { model: wat }, result }], final: { status: 'COMPUTED', trialId: 17, result } };
  const factor = factorViews(3, study)[0]!;
  assert.equal(factor.metrics.find(metric => metric.key === 'HELDOUT.pearson_ic')!.value, 0.2);
  assert.equal(factor.criterion, null);
  assert.equal(factor.evidence, 'independent');
  assert.equal(factor.combinable, true);
});
