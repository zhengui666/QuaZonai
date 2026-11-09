import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { ConversationId } from '@earendil-works/pi-durable';
import { Study, Requests } from '../../research/src/documents.ts';
import { ReviewLedger } from '../../research/src/review-documents.ts';
import { Application, json } from '../src/facts.ts';
import { TickEconomicReplays } from '../src/economic-tick.ts';
import { ReviewAllocations } from '../src/review-allocation.ts';
import { context, setup, wat } from './support.ts';

type State = Awaited<ReturnType<typeof setup>>;
async function seed(state: State, count: number) {
  return state.service.research.harness.commit(async tx => {
    const ids: number[] = []; const requests = await tx.doc(Requests); const app = await tx.doc(Application);
    for (let i = 0; i < count; i++) {
      const { id } = await tx.createConversation({ ownership: { kind: 'ownerless' } }); ids.push(id);
      const input = { goal: `Full private research question ${i}`, model: state.model, data: { request: {} },
        specification: { version: 1 as const, name: i === 0 ? 'Oldest needle' : `Research ${String(i).padStart(3, '0')}`, question: `Question ${i}`, rationale: `Long rationale searchable-${i}`, search: { scope: 'single-factor' as const, maxMembers: 1, stop: 'selection' as const }, evaluation: { metric: 'pearson_ic' as const, direction: 'maximize' as const, scheme: 'registered-partitions' as const } },
        policy: { strategy: { instrumentId: 'TEST.SIM' } } };
      const study = await tx.doc(Study, id); study.input = input;
      const result = { partitions: [{ partition: 'HELDOUT', metric: { counts: { included: '20' }, pearson_ic: { value: i === 1 ? null : (i - 2) / 1000, status: i === 1 ? 'CONSTANT_SCORE' : 'AVAILABLE' } } }] };
      study.trials = [{ taskId: 1000 + i, proposal: { hypothesis: i === 0 ? 'Hidden hypothesis ancient-hypothesis-needle' : `Hidden hypothesis phrase ${i}`, model: wat }, status: 'COMPUTED', result }];
      study.final = { trialId: 1000 + i, status: 'COMPUTED', result };
      requests.requests[`request:record-${i}`] = { conversationId: id, input };
      app.researchRequests[`request:record-${i}`] = { original: { datasetId: 'controlled' }, input: {}, dataset: { id: 'controlled', name: 'Frozen dataset', currency: 'USD' } };
    }
    await tx.createConversation({ ownership: { kind: 'ownerless' } });
    return ids;
  }, context);
}
async function get(state: State, path: string, expected = 200) {
  const response = await state.service.app.inject({ url: `/api/v3/${path}`, headers: state.headers });
  assert.equal(response.statusCode, expected, response.body); return response.json();
}
function fact(id: string, name: string, researchId: number, capital = '1000', currency = 'USD', sharpe: number | null = null, drawdown: number | null = null) {
  const metrics = [
    { key: 'Sharpe Ratio (252 days)', label: 'Sharpe Ratio (252 days)', value: sharpe },
    { key: 'Max Drawdown', label: 'Max Drawdown', value: drawdown },
  ];
  return { input: { requestId: id, name, researchIds: [researchId], datasetId: 'controlled', capital },
    view: { id, name, researchIds: [researchId], datasetId: 'controlled', status: 'COMPUTED', capital, currency, createdAt: '2026-11-01T00:00:00.000Z', taskId: null,
      metrics, curve: [{ at: '2026-11-01T00:00:00.000Z', value: capital }], targets: [{ instrumentId: 'SECRET', weight: '1' }], error: null },
    evidence: [{ researchId, input: { goal: 'Frozen original source name' } }], policy: {}, native: null, target: null };
}

test('list HTTP queries search all original records before pagination and retain lightweight stable summaries', async () => {
  const state = await setup();
  try {
    const ids = await seed(state, 105);
    const old = await get(state, 'research'); assert.ok(Array.isArray(old.items)); assert.ok(old.cursor);
    const page = await get(state, 'research/list'); assert.equal(page.total, 105); assert.equal(page.items.length, 20); assert.equal(page.items[0].id, ids.at(-1));
    assert.equal(page.facets.datasetId[0].label, 'Frozen dataset');
    assert.deepEqual((await get(state, 'research/list?q=Oldest')).items.map((row: { id: number }) => row.id), [ids[0]]);
    assert.deepEqual((await get(state, 'research/list?q=searchable-0')).items.map((row: { id: number }) => row.id), [ids[0]]);
    assert.equal((await get(state, 'research/list?scope=single-factor&status=COMPUTED&modality=BAR')).total, 105);
    const sorted = await get(state, 'research/list?sort=title&order=asc&page=2'); assert.equal(sorted.items[0].title, 'Research 020');
    assert.equal((await get(state, 'research/list?page=999')).page, 6);
    const searched = await get(state, 'factors/list?q=ancient-hypothesis-needle'); assert.equal(searched.total, 1);
    const ascending = await get(state, 'factors/list?sort=independentIc&order=asc&pageSize=100'); assert.equal(ascending.items[0].researchId, ids[0]); assert.equal(ascending.items[0].independentIc, -0.002);
    const last = await get(state, 'factors/list?sort=independentIc&order=asc&page=6'); assert.equal(last.items.at(-1).independentIc, null);
    const reverseLast = await get(state, 'factors/list?sort=independentIc&order=desc&page=6'); assert.equal(reverseLast.items.at(-1).independentIc, null);
    assert.equal((await get(state, 'factors/list?icMin=-0.002&icMax=0')).total, 2);
    const text = JSON.stringify(searched); assert.ok(!text.includes(wat)); assert.ok(!text.includes('Hidden hypothesis')); assert.ok(!text.includes('modelSource'));
    assert.equal((await get(state, 'factors')).length, 105);
    await get(state, 'research/list?sort=criterionValue', 400);
    assert.equal((await get(state, 'research/list?sort=criterionValue&metric=pearson_ic')).total, 105);
    for (const suffix of ['page=0', 'page=abc', 'pageSize=3', 'sort=anything', 'order=anything']) await get(state, `research/list?${suffix}`, 400);
    await get(state, 'factors/list?icMin=NaN', 400); await get(state, 'factors/list?icMin=1&icMax=-1', 400);
    const unauthenticated = await state.service.app.inject({ url: '/api/v3/research/list' }); assert.equal(unauthenticated.statusCode, 401);
  } finally { await state.cleanup(); }
});

test('portfolio, replay and allocation summaries preserve source names, exact capital and native performance sorting', async () => {
  const state = await setup();
  try {
    const [researchId] = await seed(state, 1);
    await state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application);
      app.portfolios['lower'] = json(fact('lower', 'Lower', researchId!, '9007199254740993.000000000000000001', 'USD', -2, -0.3));
      app.portfolios['higher'] = json(fact('higher', 'Higher', researchId!, '9007199254740993.000000000000000002', 'USD', -1, -0.1));
      app.portfolios['missing'] = json(fact('missing', 'Missing', researchId!, '2000', 'EUR'));
      app.portfolios['deleted'] = json({ ...fact('deleted', 'Deleted', researchId!), deletedAt: '2026-11-01T00:00:00Z' });
      (await tx.doc(TickEconomicReplays)).runs['tick'] = json({ original: { datasetId: 'controlled', factorId: `r${researchId}.t1000.m0`, request: { scenario: {} } },
        view: { id: 'tick', requestId: 'tick-request', datasetId: 'controlled', factorId: `r${researchId}.t1000.m0`, taskId: 123456, status: 'COMPUTED', scientificQualification: 'UNQUALIFIED_EXPLORATORY', result: null, error: null } });
      (await tx.doc(ReviewAllocations)).records['allocation'] = json({ view: { id: 'allocation', requestId: 'request', name: 'Combined', status: 'COMPUTED', portfolioIds: ['lower', 'higher'], taskId: null,
        consumedScientificCalls: 100, weights: null, review: null, error: null, createdAt: '2026-11-01T00:00:00.000Z' }, sources: [{ view: { id: 'lower', name: 'Frozen Lower' } }, { view: { id: 'higher', name: 'Frozen Higher' } }] });
    }, context);
    const first = await get(state, 'portfolios/list?q=Frozen%20original%20source'); assert.equal(first.total, 3);
    assert.equal(first.facets.source[0].label, 'Frozen original source name'); assert.equal(first.items[0].datasetName, 'Frozen dataset');
    assert.equal((await get(state, 'portfolios/list?sort=sharpe&order=desc')).items[0].id, 'higher');
    assert.equal((await get(state, 'portfolios/list?sort=sharpe&order=asc')).items[0].id, 'lower');
    assert.equal((await get(state, 'portfolios/list?sort=maxDrawdown&order=asc')).items[0].id, 'higher');
    assert.equal((await get(state, 'portfolios/list?sort=sharpe&order=asc')).items.at(-1).id, 'missing');
    assert.equal((await get(state, 'portfolios/list?currency=USD&sort=capital&order=asc')).items[0].id, 'lower');
    assert.equal((await get(state, 'portfolios/list?currency=USD&capitalMin=9007199254740993.000000000000000002')).total, 1);
    await get(state, 'portfolios/list?sort=capital', 400); await get(state, 'portfolios/list?capitalMin=100', 400);
    await get(state, 'portfolios/list?createdFrom=2026-02-31', 400); await get(state, 'portfolios/list?createdFrom=2026-11-02&createdTo=2026-11-01', 400);
    assert.equal((await get(state, 'portfolios/list?createdFrom=2026-11-01&createdTo=2026-11-01')).total, 3);
    for (const name of ['curve', 'targets', 'metrics', 'evidence', 'taskId']) assert.ok(!JSON.stringify(first.items).includes(`"${name}"`));
    const tick = await get(state, 'tick-economic-replays/list?q=Oldest'); assert.equal(tick.total, 1); assert.equal(tick.items[0].createdAt, null); assert.equal(tick.items[0].name, 'Oldest needle');
    assert.ok(!('result' in tick.items[0])); assert.ok(!('requestId' in tick.items[0])); assert.ok(!('executionModel' in tick.items[0]));
    const allocation = await get(state, 'review-allocations/list?q=Frozen%20Lower'); assert.equal(allocation.total, 1); assert.equal(allocation.items[0].memberCount, 2); assert.ok(!('consumedScientificCalls' in allocation.items[0]));
    assert.equal((await get(state, 'portfolios')).length, 3);
    assert.ok(Array.isArray(await get(state, 'tick-economic-replays'))); assert.ok(Array.isArray(await get(state, 'review-allocations')));
  } finally { await state.cleanup(); }
});

test('list projections follow Durable commits, recover a failed read, avoid repeated scans and rebuild after restart', async () => {
  const state = await setup();
  try {
    const ids = await seed(state, 2);
    await get(state, 'research/list');
    const host = state.service.research; const read = host.listState; let reads = 0;
    host.listState = async id => { reads++; return read(id); };
    await get(state, 'research/list?q=Oldest'); await get(state, 'research/list?sort=title'); await get(state, 'factors/list'); assert.equal(reads, 0);
    await host.harness.commit(async tx => {
      const study = await tx.doc(Study, ids[0] as ConversationId); study.input!.specification!.name = 'Updated source'; study.final!.status = 'FAILED';
      (await tx.doc(ReviewLedger, ids[0] as ConversationId)).records['review'] = { trialId: 1000, report: { decision: 'FAIL' } };
    }, context);
    let fail = true; host.listState = async id => { if (fail) { fail = false; throw new Error('transient_read'); } return read(id); };
    await get(state, 'research/list', 500);
    const recovered = await get(state, 'research/list?q=Updated'); assert.equal(recovered.total, 1); assert.equal(recovered.items[0].status, 'FAILED');
    const factor = await get(state, `factors/list?research=${ids[0]}`); assert.equal(factor.items[0].combinable, false); assert.equal(factor.items[0].reviewDecision, 'FAIL');
    await host.harness.commit(async tx => { (await tx.doc(Study, ids[0] as ConversationId)).deletedAt = '2026-11-09T00:00:00Z'; }, context);
    assert.equal((await get(state, 'research/list')).total, 1); assert.equal((await get(state, 'factors/list')).total, 1);
    await state.restart(); assert.equal((await get(state, 'research/list')).total, 1); assert.equal((await get(state, 'factors/list')).total, 1);
    assert.equal((await get(state, 'research/list?q=Updated')).total, 0);
  } finally { await state.cleanup(); }
});
