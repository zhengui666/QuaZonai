import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import { openResearch } from '../../research/src/host.ts';
import { createCapacity, capacityRequest, CapacityRequests } from '../src/capacity-tasks.ts';
import { configDefaults } from '../src/config.ts';
import type { PortfolioFact } from '../src/facts.ts';

const assumptions = { participation: '0.01', maximumAgeSeconds: 120, scenarios: ['1000.000000000000000001'] };
const portfolio = { view: { id: 'original-portfolio', currency: 'USD', status: 'COMPUTED' }, policy: { kind: 'catalog-ensemble' } } as unknown as PortfolioFact;

test('capacity GET is read-only and explicit request survives close/reopen without another task', async () => {
  const dataDir = await mkdtemp(join(tmpdir(), 'qz-managed-capacity-'));
  const config = configDefaults({ dataDir }); const database = join(dataDir, 'capacity.sqlite');
  let capacity = createCapacity(config);
  let host = await openResearch({ database, models: createModels(), extensions: [capacity.extension] }); capacity.connect(host);
  try {
    const missing = await capacity.read('PORTFOLIO', portfolio.view.id);
    assert.equal(missing.reason, 'capacity_not_assessed');
    assert.equal(await host.harness.snapshot(CapacityRequests, BACKGROUND_CONTEXT), undefined);
    const input = capacityRequest('PORTFOLIO', portfolio.view.id, assumptions, 'original-request');
    const result = await capacity.startPortfolio(portfolio, input);
    assert.equal(result.reason, 'catalog_ensemble_capacity_unavailable');
    assert.equal(result.scenarios[0]!.capital, assumptions.scenarios[0]);
    const before = await host.harness.snapshot(CapacityRequests, BACKGROUND_CONTEXT);
    assert.equal(Object.keys(before!.records).length, 1);
    assert.deepEqual(await capacity.startPortfolio(portfolio, input), result);
    await assert.rejects(capacity.startPortfolio(portfolio, { ...input, assumptions: { ...assumptions, participation: '0.02' } }), /request_id_conflict/);
    await host.close();
    capacity = createCapacity(config);
    host = await openResearch({ database, models: createModels(), extensions: [capacity.extension] }); capacity.connect(host);
    assert.deepEqual(await capacity.startPortfolio(portfolio, input), result);
    assert.deepEqual(await host.harness.snapshot(CapacityRequests, BACKGROUND_CONTEXT), before);
    assert.deepEqual(await capacity.read('PORTFOLIO', portfolio.view.id), result);
  } finally { await host.close(); await rm(dataDir, { recursive: true, force: true }); }
});

test('legacy capacity bodies derive stable identity while explicit new requests remain independent', () => {
  const input = capacityRequest('FACTOR', 'r1.t2.m0', assumptions);
  assert.equal(input.requestId, capacityRequest('FACTOR', 'r1.t2.m0', structuredClone(assumptions)).requestId);
  assert.notEqual(input.requestId, capacityRequest('FACTOR', 'r1.t2.m0', { ...assumptions, maximumAgeSeconds: 121 }).requestId);
  assert.notEqual(input.requestId, capacityRequest('PORTFOLIO', 'r1.t2.m0', assumptions).requestId);
  assert.equal(capacityRequest('FACTOR', 'r1.t2.m0', assumptions, 'new-calculation').requestId, 'new-calculation');
});
