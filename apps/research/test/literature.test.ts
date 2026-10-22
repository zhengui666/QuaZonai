import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import type { TranscriptContext } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { LiteratureSearch, LiteratureSearchResult, LiteratureView } from '../../../packages/contracts/src/literature.ts';
import { researchSpecification } from '../../../packages/contracts/src/research.ts';
import { openResearch } from '../src/host.ts';
import { researchInput } from '../src/input.ts';
import { LiteratureDoc } from '../src/literature-document.ts';
import { literatureTools } from '../src/literature-tools.ts';
import { researchExtension } from '../plugins/research.ts';
import wasm from '../plugins/factor-wasm.ts';

const context = BACKGROUND_CONTEXT;
const paper = { id: 'doi:10.0000/test-only', source: 'crossref' as const, title: 'Protocol fixture, not a real paper',
  authors: ['Test Author'], year: 2020, venue: 'Test fixture', doi: '10.0000/test-only', url: 'https://example.org/test-only',
  abstract: 'Conditional momentum may reverse after costs.', publishedAt: '2020-01-01' };
const result = (query: string): LiteratureSearchResult => ({ query, searchedAt: '2026-10-22T00:00:00Z', status: 'completed',
  sources: [{ source: 'crossref', status: 'ok', error: null }], papers: [paper] });
const call = (name: string, args: JsonObject) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
function latest(transcript: TranscriptContext, name: string) {
  const message = transcript.messages.filter(item => item.role === 'toolResult' && item.toolName === name).at(-1);
  assert.ok(message?.role === 'toolResult');
  return { error: message.isError, text: message.content.filter(part => part.type === 'text').map(part => part.text).join('\n') };
}
async function setup() {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-literature-'));
  const faux = fauxProvider({ models: [{ id: 'literature-protocol' }], tokensPerSecond: 100_000 });
  const models = createModels(); models.setProvider(faux.provider);
  const model = { provider: faux.provider.id, modelId: faux.getModel().id };
  return { directory, faux, models, database: join(directory, 'research.sqlite'),
    input: { goal: 'Investigate a public factor hypothesis.', publicLiteratureQueries: ['momentum transaction costs', '  MOMENTUM   transaction costs ', 'new configured query'], model, data: { sealed: { private: 'HELDOUT_PRIVATE_SENTINEL' } }, policy: { max_trials: 1 } },
    async cleanup() { await rm(directory, { recursive: true, force: true }); } };
}

test('official durable research saves retrieved sources and attributed notes, reuses receipts and preserves them across restart', { timeout: 15_000 }, async () => {
  const state = await setup(); let searches = 0; let searchId = 0;
  const extension = researchExtension(async query => { searches++; return result(query); });
  const note = () => ({ searchId, paperId: paper.id, claim: 'Test claim based only on the abstract.', relation: 'contradicts',
    limitations: 'Protocol fixture; no full-text or financial evidence.', excerpt: 'may reverse after costs' });
  state.faux.setResponses([
    call('research_brief', {}),
    call('research_literature_search', { queryIndex: 0, limit: 2 }),
    transcript => {
      const receipt = JSON.parse(latest(transcript, 'research_literature_search').text) as LiteratureSearch;
      searchId = receipt.id; assert.deepEqual(receipt.papers, [paper]);
      return call('research_literature_note', note());
    },
    call('research_literature_search', { queryIndex: 1, limit: 8 }),
    transcript => {
      assert.equal(JSON.parse(latest(transcript, 'research_literature_search').text).id, searchId);
      return call('research_literature_note', note());
    },
    () => call('research_literature_note', { ...note(), excerpt: 'Unretrieved profitable outcome' }),
    transcript => {
      const invalid = latest(transcript, 'research_literature_note');
      assert.ok(invalid.error); assert.match(invalid.text, /literature_excerpt_not_in_abstract/);
      return call('research_literature_note', { ...note(), paperId: 'invented-paper' });
    },
    transcript => {
      assert.match(latest(transcript, 'research_literature_note').text, /literature_paper_not_retrieved/);
      return call('research_literature', {});
    },
    transcript => {
      const saved = JSON.parse(latest(transcript, 'research_literature').text) as LiteratureView;
      assert.equal(saved.searches.length, 1); assert.equal(saved.notes.length, 1);
      assert.equal(saved.notes[0]!.searchId, searchId); assert.equal(saved.notes[0]!.excerpt, note().excerpt);
      return call('factor_evaluate', { hypothesis: 'Literature still requires an actual scientific trial.', model: '(module)' });
    },
    call('research_finish', { reason: 'Original single trial exhausted after missing scientific input.' }),
  ]);
  let host = await openResearch({ ...state, extensions: [extension, wasm] });
  try {
    const receipt = await host.start('literature-original', state.input);
    assert.equal((await (await host.harness.submission(receipt.submissionId, context))!.wait(context)).status, 'done');
    const saved = await host.harness.snapshot(LiteratureDoc, receipt.conversationId, context);
    assert.equal(searches, 1); assert.equal(saved!.searches.length, 1); assert.equal(saved!.notes.length, 1);
    const facts = (await host.state(receipt.conversationId)).research!;
    assert.equal(facts.trials.length, 1); assert.equal(facts.trials[0]!.status, 'FAILED');
    assert.equal(facts.final!.status, 'STOPPED'); assert.equal(facts.final!.result, null);
    const before = await host.view(receipt.conversationId);
    assert.equal(JSON.stringify(before.entries).includes('HELDOUT_PRIVATE_SENTINEL'), false);

    // Even direct native tool dispatch after a final cannot fetch or append notes.
    state.faux.setResponses([fauxAssistantMessage([
      fauxToolCall('research_literature_search', { queryIndex: 2 }),
      fauxToolCall('research_literature_note', { ...note(), claim: 'Late new claim' }),
    ], { stopReason: 'toolUse' })]);
    const conversation = (await host.harness.conversation(receipt.conversationId, context))!;
    const late = await conversation.submit({ type: 'input', content: 'TEST_ONLY final tool guards', requestId: 'late-final-guards' }, context);
    await (await host.harness.submission(late.id, context))!.wait(context);
    assert.equal(searches, 1);
    assert.deepEqual(await host.harness.snapshot(LiteratureDoc, receipt.conversationId, context), saved);
    const calls = state.faux.state.callCount;
    await host.close(); host = await openResearch({ ...state, extensions: [extension, wasm] });
    assert.deepEqual(await host.start('literature-original', state.input), receipt);
    assert.deepEqual(await host.harness.snapshot(LiteratureDoc, receipt.conversationId, context), saved);
    assert.equal(state.faux.state.callCount, calls); assert.equal(searches, 1);
  } finally { await host.close(); await state.cleanup(); }
});

test('unavailable literature is retained without blocking or qualifying a scientific attempt', { timeout: 15_000 }, async () => {
  const state = await setup(); let searches = 0;
  const extension = researchExtension(async query => { searches++; return { ...result(query), status: 'unavailable', papers: [],
    sources: [{ source: 'crossref', status: 'error', error: 'http_429' }, { source: 'arxiv', status: 'error', error: 'timeout' }] }; });
  state.faux.setResponses([
    call('research_literature_search', { queryIndex: 0 }),
    call('research_literature_search', { queryIndex: 0 }),
    transcript => {
      const receipt = JSON.parse(latest(transcript, 'research_literature_search').text) as LiteratureSearch;
      assert.equal(receipt.status, 'unavailable'); assert.deepEqual(receipt.papers, []);
      return call('factor_evaluate', { hypothesis: 'Unverified hypothesis; sources unavailable.', model: '(module)' });
    },
    call('research_finish', { reason: 'Original trial exhausted; missing scientific input.' }),
  ]);
  const host = await openResearch({ ...state, extensions: [extension, wasm] });
  try {
    const receipt = await host.start('unavailable-literature', state.input);
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    assert.equal(searches, 1);
    const literature = (await host.harness.snapshot(LiteratureDoc, receipt.conversationId, context))!;
    assert.equal(literature.searches.length, 1); assert.equal(literature.searches[0]!.status, 'unavailable'); assert.deepEqual(literature.notes, []);
    const facts = (await host.state(receipt.conversationId)).research!;
    assert.equal(facts.trials.length, 1); assert.equal(facts.trials[0]!.status, 'FAILED');
    assert.equal(facts.final!.status, 'STOPPED'); assert.equal(facts.final!.result, null);
  } finally { await host.close(); await state.cleanup(); }
});

test('owner cancellation propagates to retrieval and does not store a successful or unavailable receipt', { timeout: 15_000 }, async () => {
  const state = await setup(); let entered!: () => void; let aborted = false;
  const running = new Promise<void>(resolve => { entered = resolve; });
  const extension = researchExtension(async (_query, _limit, signal) => {
    assert.ok(signal); signal.throwIfAborted(); entered();
    return await new Promise<LiteratureSearchResult>((_resolve, reject) => signal.addEventListener('abort', () => { aborted = true; reject(signal.reason); }, { once: true }));
  });
  state.faux.setResponses([call('research_literature_search', { queryIndex: 0 })]);
  const host = await openResearch({ ...state, extensions: [extension] });
  try {
    const receipt = await host.start('cancel-literature', state.input); await running;
    await host.stop(receipt.conversationId);
    assert.equal(aborted, true);
    const saved = await host.harness.snapshot(LiteratureDoc, receipt.conversationId, context);
    assert.deepEqual(saved, { searches: [], notes: [] });
    assert.equal((await host.state(receipt.conversationId)).research!.trials.length, 0);
  } finally { await host.close(); await state.cleanup(); }
});

test('explicit retry retains the failed receipt and original-task reentry still returns its original failure', { timeout: 15_000 }, async () => {
  const state = await setup(); let searches = 0; let originalId = 0; let recoveredId = 0;
  const search = async (query: string): Promise<LiteratureSearchResult> => {
    searches++;
    return searches === 1 ? { ...result(query), status: 'unavailable', papers: [], sources: [{ source: 'crossref', status: 'error', error: 'http_429' }] } : result(query);
  };
  const [searchTool] = literatureTools(search);
  const replay = defineExtension({ name: 'test-literature-reentry', tools: [defineTool({
    name: 'original_search_reentry', description: 'TEST_ONLY reenter the original committed tool identity.', parameters: Type.Object({}), replay: 'safe',
    async execute(_args, api, ctx) {
      return searchTool.execute({ queryIndex: 0, retryFailed: true }, { ...api, taskId: originalId as TaskId }, ctx);
    },
  })] });
  state.faux.setResponses([
    call('research_literature_search', { queryIndex: 0 }),
    transcript => {
      const receipt = JSON.parse(latest(transcript, 'research_literature_search').text) as LiteratureSearch;
      originalId = receipt.id; assert.equal(receipt.status, 'unavailable');
      return call('research_literature_search', { queryIndex: 0 });
    },
    transcript => {
      assert.equal(JSON.parse(latest(transcript, 'research_literature_search').text).id, originalId); assert.equal(searches, 1);
      return call('research_literature_search', { queryIndex: 0, retryFailed: true });
    },
    transcript => {
      const receipt = JSON.parse(latest(transcript, 'research_literature_search').text) as LiteratureSearch;
      recoveredId = receipt.id; assert.notEqual(recoveredId, originalId); assert.equal(receipt.status, 'completed');
      return call('research_literature_search', { queryIndex: 0, retryFailed: true });
    },
    transcript => {
      assert.equal(JSON.parse(latest(transcript, 'research_literature_search').text).id, recoveredId); assert.equal(searches, 2);
      return call('original_search_reentry', {});
    },
    transcript => {
      const original = JSON.parse(latest(transcript, 'original_search_reentry').text) as LiteratureSearch;
      assert.equal(original.id, originalId); assert.equal(original.status, 'unavailable'); assert.equal(searches, 2);
      return call('research_finish', { reason: 'Protocol fixture complete.' });
    },
  ]);
  let host = await openResearch({ ...state, extensions: [researchExtension(search), replay] });
  try {
    const receipt = await host.start('retry-literature', state.input);
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const saved = (await host.harness.snapshot(LiteratureDoc, receipt.conversationId, context))!;
    assert.deepEqual(saved.searches.map(item => [item.id, item.status]), [[originalId, 'unavailable'], [recoveredId, 'completed']]);
    await host.close(); host = await openResearch({ ...state, extensions: [researchExtension(search), replay] });
    assert.deepEqual(await host.start('retry-literature', state.input), receipt);
    assert.deepEqual(await host.harness.snapshot(LiteratureDoc, receipt.conversationId, context), saved);
    assert.equal(searches, 2);
  } finally { await host.close(); await state.cleanup(); }
});

test('only frozen owner queries leave private research; model text and out-of-range indexes cannot reach the search adapter', { timeout: 15_000 }, async () => {
  const state = await setup(); const sent: string[] = [];
  const extension = researchExtension(async query => { sent.push(query); return result(query); });
  const queries = ['  public momentum  costs  ', 'public reversal'];
  const specification = researchSpecification({ version: 1, name: 'Private research', question: 'GOAL_PRIVATE_SENTINEL', rationale: 'RATIONALE_PRIVATE_SENTINEL',
    evaluation: { metric: 'pearson_ic', direction: 'maximize', scheme: 'registered-partitions' }, search: { scope: 'single-factor', maxMembers: 1, stop: 'budget-or-selection' } });
  const input = researchInput({ ...state.input, goal: specification.question, specification, publicLiteratureQueries: queries,
    data: { ...state.input.data, confidential: 'DATA_PRIVATE_SENTINEL' } });
  const withQueries = (value: string[]) => { const changed = structuredClone(input); changed.publicLiteratureQueries = value; return changed; };
  state.faux.setResponses([
    call('research_brief', {}),
    transcript => {
      const brief = JSON.parse(latest(transcript, 'research_brief').text);
      assert.deepEqual(brief.literature.publicQueries, queries.map((query, queryIndex) => ({ queryIndex, query })));
      return call('research_literature_search', { queryIndex: 0, query: 'MODEL_PRIVATE_SENTINEL' });
    },
    transcript => {
      assert.ok(latest(transcript, 'research_literature_search').error); assert.deepEqual(sent, []);
      return call('research_literature_search', { queryIndex: 2 });
    },
    transcript => {
      assert.match(latest(transcript, 'research_literature_search').text, /literature_query_index_invalid/); assert.deepEqual(sent, []);
      return call('research_literature_search', { queryIndex: -1 });
    },
    transcript => {
      assert.ok(latest(transcript, 'research_literature_search').error); assert.deepEqual(sent, []);
      return call('research_literature_search', { queryIndex: 0 });
    },
    transcript => {
      assert.equal(JSON.parse(latest(transcript, 'research_literature_search').text).query, queries[0]);
      return call('research_literature_search', { queryIndex: 1 });
    },
    () => {
      assert.deepEqual(sent, queries); assert.equal(sent.some(query => query.includes('PRIVATE_SENTINEL')), false);
      return call('factor_evaluate', { hypothesis: 'Original private hypothesis with public literature context.', model: '(module)' });
    },
    call('research_finish', { reason: 'Original single scientific attempt exhausted.' }),
  ]);
  let host = await openResearch({ ...state, extensions: [extension, wasm] });
  try {
    await assert.rejects(host.start('invalid-query-admission', withQueries(['invalid\nquery'])), /research_public_literature_queries_invalid/);
    const receipt = await host.start('frozen-public-queries', input);
    assert.equal((await (await host.harness.submission(receipt.submissionId, context))!.wait(context)).status, 'done');
    assert.deepEqual(sent, queries);
    await assert.rejects(host.start('frozen-public-queries', withQueries(['changed public query'])), /request_id_conflict/);
    await host.close(); host = await openResearch({ ...state, extensions: [extension, wasm] });
    assert.deepEqual(await host.start('frozen-public-queries', input), receipt);
    assert.deepEqual((await host.state(receipt.conversationId)).research!.input!.publicLiteratureQueries, queries);
    await assert.rejects(host.start('frozen-public-queries', withQueries([...queries].reverse())), /request_id_conflict/);
    assert.deepEqual(sent, queries);
  } finally { await host.close(); await state.cleanup(); }
});

test('legacy input without public queries exposes capability absence, keeps old receipts readable and still runs science', { timeout: 15_000 }, async () => {
  const state = await setup(); let requests = 0;
  const extension = researchExtension(async query => { requests++; return result(query); });
  const { publicLiteratureQueries: _queries, ...input } = state.input;
  state.faux.setResponses([
    call('research_brief', {}),
    transcript => {
      assert.deepEqual(JSON.parse(latest(transcript, 'research_brief').text).literature.publicQueries, []);
      return call('research_literature_search', { queryIndex: 0 });
    },
    transcript => {
      const response = latest(transcript, 'research_literature_search'); assert.equal(response.error, false);
      assert.deepEqual(JSON.parse(response.text), { status: 'unavailable', reason: 'public_queries_not_configured', sources: [], papers: [] });
      return call('factor_evaluate', { hypothesis: 'No public query configured; retain scientific attempt.', model: '(module)' });
    },
    call('research_finish', { reason: 'Original scientific attempt exhausted without public queries.' }),
  ]);
  const host = await openResearch({ ...state, extensions: [extension, wasm] });
  try {
    const receipt = await host.start('legacy-no-public-queries', input);
    assert.equal((await (await host.harness.submission(receipt.submissionId, context))!.wait(context)).status, 'done');
    assert.equal(requests, 0);
    assert.equal(await host.harness.snapshot(LiteratureDoc, receipt.conversationId, context), undefined);
    const study = (await host.state(receipt.conversationId)).research!;
    assert.equal(study.trials.length, 1); assert.equal(study.trials[0]!.status, 'FAILED'); assert.equal(study.final!.status, 'STOPPED');
    const oldReceipt = { ...result('original historical query'), id: 700 };
    await host.harness.commit(async tx => { (await tx.doc(LiteratureDoc, receipt.conversationId)).searches.push(oldReceipt); }, context);
    state.faux.setResponses([fauxAssistantMessage([
      fauxToolCall('research_literature', {}), fauxToolCall('research_literature_search', { queryIndex: 0, retryFailed: true }),
    ], { stopReason: 'toolUse' })]);
    const conversation = (await host.harness.conversation(receipt.conversationId, context))!;
    const read = await conversation.submit({ type: 'input', content: 'TEST_ONLY read original historical receipt', requestId: 'legacy-literature-read' }, context);
    await (await host.harness.submission(read.id, context))!.wait(context);
    const entries = JSON.stringify((await host.view(receipt.conversationId)).entries);
    assert.ok(entries.includes('original historical query')); assert.ok(entries.includes('public_queries_not_configured'));
    assert.deepEqual((await host.harness.snapshot(LiteratureDoc, receipt.conversationId, context))!.searches, [oldReceipt]);
    assert.equal(requests, 0);
  } finally { await host.close(); await state.cleanup(); }
});
