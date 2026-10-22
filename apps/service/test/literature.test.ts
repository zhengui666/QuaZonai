import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { LiteratureView, ResearchDetail } from '../../../packages/contracts/src/index.ts';
import { LiteratureDoc } from '../../research/src/literature-document.ts';
import { Requests, Study } from '../../research/src/documents.ts';
import { Application } from '../src/facts.ts';
import { context, fauxAssistantMessage, fauxToolCall, setup, studied } from './support.ts';

test('research HTTP retains retrieved sources and interpretations after restart without changing science', async () => {
  const state = await setup();
  try {
    const { receipt } = await studied(state);
    const get = async () => {
      const response = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers });
      assert.equal(response.statusCode, 200, response.body);
      return response.json<ResearchDetail>();
    };
    const original = await get();
    assert.ok(original.literature === undefined || original.literature.searches.length === 0);
    const science = await state.service.research.harness.snapshot(Study, receipt.conversationId, context);
    const beforeFactors = await state.service.app.inject({ url: '/api/v3/factors', headers: state.headers });
    assert.equal(beforeFactors.statusCode, 200, beforeFactors.body);
    const literature: LiteratureView = {
      searches: [{ id: 123, query: 'public factor concept', searchedAt: '2026-10-22T09:00:00.000Z', status: 'partial',
        sources: [{ source: 'arxiv', status: 'ok', error: null }, { source: 'semanticscholar', status: 'error', error: 'http_429' }],
        papers: [{ id: 'arxiv:TEST_ONLY', source: 'arxiv', title: 'TEST_ONLY retrieved paper', authors: ['Test Author'],
          year: 2026, venue: null, doi: null, url: 'https://arxiv.org/abs/TEST_ONLY',
          abstract: 'A controlled abstract for HTTP projection.', publishedAt: '2026-10-01T00:00:00Z' }],
      }],
      notes: [{ id: 124, searchId: 123, paperId: 'arxiv:TEST_ONLY', claim: 'A model interpretation of the retrieved abstract.',
        relation: 'background', limitations: 'Protocol fixture only; no scientific evidence.', excerpt: 'controlled abstract' }],
    };
    await state.service.research.harness.commit(async tx => {
      Object.assign(await tx.doc(LiteratureDoc, receipt.conversationId), structuredClone(literature));
    }, context);

    const observed = await get();
    assert.deepEqual(observed.literature, literature);
    assert.deepEqual(observed.trialRecords, original.trialRecords);
    assert.deepEqual(observed.final, original.final);
    assert.deepEqual(await state.service.research.harness.snapshot(Study, receipt.conversationId, context), science);
    const afterFactors = await state.service.app.inject({ url: '/api/v3/factors', headers: state.headers });
    assert.deepEqual(afterFactors.json(), beforeFactors.json());
    const modelCalls = state.faux.state.callCount;

    await state.restart();
    assert.deepEqual((await get()).literature, literature);
    assert.deepEqual((await state.service.research.state(receipt.conversationId)).literature, literature);
    assert.deepEqual((await state.service.research.view(receipt.conversationId)).literature, literature);
    assert.equal(state.faux.state.callCount, modelCalls);
  } finally { await state.cleanup(); }
});

test('both research admission surfaces freeze approved public queries and replay the original input', async () => {
  const state = await setup();
  try {
    const queries = ['  public market microstructure  ', 'price discovery evidence'];
    const post = (payload: object) => state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload });
    for (const mode of ['raw', 'dataset'] as const) {
      const requestId = `literature-public-${mode}`;
      const raw = { goal: 'Private controlled hypothesis', model: state.model, data: structuredClone(state.options.config.datasets[0]!.data), policy: { max_trials: 1 }, publicLiteratureQueries: queries };
      const payload = mode === 'raw' ? { requestId, input: raw }
        : { requestId, goal: raw.goal, model: state.model, datasetId: 'controlled', maxTrials: 1, publicLiteratureQueries: queries };
      state.faux.setResponses([fauxAssistantMessage(fauxToolCall('controlled_trial', {}), { stopReason: 'toolUse' })]);
      const response = await post(payload);
      assert.equal(response.statusCode, 202, response.body);
      const receipt = response.json();
      await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
      const original = (await state.service.research.harness.snapshot(Study, receipt.conversationId, context))!;
      assert.deepEqual(original.input!.publicLiteratureQueries, queries);
      const detail = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers });
      assert.equal(detail.statusCode, 200, detail.body);
      assert.deepEqual(detail.json<ResearchDetail>().input.publicLiteratureQueries, queries);
      const modelCalls = state.faux.state.callCount;

      const replay = await post(payload);
      assert.equal(replay.statusCode, 202, replay.body);
      assert.deepEqual(replay.json(), receipt);
      assert.equal(state.faux.state.callCount, modelCalls);
      const changed = mode === 'raw' ? { ...payload, input: { ...raw, publicLiteratureQueries: ['changed public query'] } }
        : { ...payload, publicLiteratureQueries: ['changed public query'] };
      const conflict = await post(changed);
      assert.equal(conflict.statusCode, 409, conflict.body);
      assert.equal(conflict.json().error, 'request_id_conflict');
      assert.equal(state.faux.state.callCount, modelCalls);
      assert.deepEqual(await state.service.research.harness.snapshot(Study, receipt.conversationId, context), original);
    }
    const legacy = await studied(state, 'literature-legacy-no-approved-query');
    const original = (await state.service.research.harness.snapshot(Study, legacy.receipt.conversationId, context))!;
    assert.equal(original.input!.publicLiteratureQueries, undefined);
    const detail = await state.service.app.inject({ url: `/api/v3/research/${legacy.receipt.conversationId}`, headers: state.headers });
    assert.equal(detail.statusCode, 200, detail.body);
    assert.equal(detail.json<ResearchDetail>().input.publicLiteratureQueries, undefined);
  } finally { await state.cleanup(); }
});

test('invalid approved public queries are rejected before admission on both research surfaces', async () => {
  const state = await setup();
  try {
    const invalid: unknown[] = [[], ['one', 'two', 'three', 'four'], ['  '], ['x'.repeat(501)], ['query\nprivate'], ['query\u0000private'], 'query', null];
    const originalFacts = (await state.service.research.harness.snapshot(Application, context))!.researchRequests;
    const originalRequests = await state.service.research.harness.snapshot(Requests, context);
    for (const mode of ['raw', 'dataset'] as const) {
      for (const [index, publicLiteratureQueries] of invalid.entries()) {
        const requestId = `invalid-literature-public-${mode}-${index}`;
        const raw = { goal: 'Private controlled hypothesis', model: state.model, data: structuredClone(state.options.config.datasets[0]!.data), policy: { max_trials: 1 }, publicLiteratureQueries };
        const payload = mode === 'raw' ? { requestId, input: raw }
          : { requestId, goal: raw.goal, model: state.model, datasetId: 'controlled', maxTrials: 1, publicLiteratureQueries };
        const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload });
        assert.equal(response.statusCode, 400, `${mode} query ${index}: ${response.body}`);
        assert.equal(state.faux.state.callCount, 0);
        assert.deepEqual((await state.service.research.harness.snapshot(Application, context))!.researchRequests, originalFacts);
        assert.deepEqual(await state.service.research.harness.snapshot(Requests, context), originalRequests);
      }
    }
    assert.equal((await state.service.research.list()).items.length, 0);
  } finally { await state.cleanup(); }
});
