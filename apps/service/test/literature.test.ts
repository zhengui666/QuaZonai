import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { LiteratureView, ResearchDetail } from '../../../packages/contracts/src/index.ts';
import { LiteratureDoc } from '../../research/src/literature-document.ts';
import { Study } from '../../research/src/documents.ts';
import { context, setup, studied } from './support.ts';

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
