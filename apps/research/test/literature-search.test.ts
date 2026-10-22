import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createLiteratureSearcher } from '../src/literature-search.ts';

const atom = (entries = '') => `<?xml version="1.0" encoding="UTF-8"?><feed xmlns="http://www.w3.org/2005/Atom" xmlns:arxiv="http://arxiv.org/schemas/atom">${entries}</feed>`;
const entry = (id: string, extra = '') => `<entry><id>http://arxiv.org/abs/${id}</id><title>Momentum &amp; risk</title><author><name>Jane Doe</name></author><summary>Observed &lt;signal&gt; effects.</summary><published>2023-06-01T12:00:00Z</published>${extra}</entry>`;
const cross = (suffix: string, extra = {}) => ({ DOI: `10.1234/${suffix}`, title: [`Paper ${suffix}`], author: [{ given: 'Jane', family: 'Doe' }], published: { 'date-parts': [[2021]] }, ...extra });
const semantic = (id: string, extra = {}) => ({ paperId: id.repeat(40), title: 'Prediction', authors: [{ name: 'John Smith' }], year: 2022, ...extra });
const response = (value: unknown) => new Response(JSON.stringify(value), { headers: { 'content-type': 'application/json' } });
function empty(url: URL) {
  return url.hostname === 'export.arxiv.org' ? new Response(atom()) : response(url.hostname === 'api.crossref.org' ? { message: { items: [] } } : { data: [] });
}

test('parses three scholarly formats, preserves observed metadata and balances source results', async () => {
  const seen: URL[] = [];
  const search = createLiteratureSearcher({ fetcher: async (url, options) => {
    seen.push(url); assert.equal(url.protocol, 'https:'); assert.equal(options.redirect, 'error');
    assert.equal(options.headers.authorization, undefined);
    if (url.hostname === 'api.crossref.org') return response({ message: { items: [cross('one', { 'container-title': ['Journal'], abstract: '<jats:p>Original observation.</jats:p>' }), cross('two'), cross('three')] } });
    if (url.hostname === 'export.arxiv.org') return new Response(atom(entry('2306.00001v2')));
    return response({ data: [semantic('a', { abstract: 'Independent limitation.', publicationDate: '2022-07-20', venue: 'Conference' })] });
  } });
  const result = await search(' momentum 中文 ', 3);
  assert.equal(result.status, 'completed'); assert.equal(result.query, 'momentum 中文');
  assert.equal(seen.length, 3); assert.ok(result.sources.every(source => source.status === 'ok'));
  assert.deepEqual(result.papers.map(paper => paper.source), ['crossref', 'arxiv', 'semanticscholar']);
  assert.equal(result.papers[0]!.publishedAt, '2021'); assert.equal(result.papers[0]!.year, 2021);
  assert.deepEqual(result.papers[0]!.authors, ['Jane Doe']); assert.equal(result.papers[0]!.abstract, 'Original observation.');
  assert.equal(result.papers[1]!.title, 'Momentum & risk'); assert.equal(result.papers[1]!.abstract, 'Observed <signal> effects.');
  assert.equal(result.papers[1]!.url, 'https://arxiv.org/abs/2306.00001v2');
  assert.equal(result.papers[2]!.doi, null); assert.equal(result.papers[2]!.publishedAt, '2022-07-20');
  assert.equal(seen.find(url => url.hostname === 'api.crossref.org')!.searchParams.get('query.bibliographic'), 'momentum 中文');
});

test('deduplicates verified DOI/arXiv identities across sources without collapsing equal titles', async () => {
  const search = createLiteratureSearcher({ fetcher: async url => {
    if (url.hostname === 'api.crossref.org') return response({ message: { items: [cross('same', { title: ['Momentum & risk'] }), cross('different', { title: ['Momentum & risk'] })] } });
    if (url.hostname === 'export.arxiv.org') return new Response(atom(entry('2306.00001v1') + entry('2306.00001v2')));
    return response({ data: [semantic('a', { externalIds: { DOI: 'https://doi.org/10.1234/SAME', ArXiv: '2306.00001' } })] });
  } });
  const result = await search('momentum', 10);
  assert.equal(result.papers.length, 2);
  assert.deepEqual(result.papers.map(paper => paper.id), ['doi:10.1234/different', 'arxiv:2306.00001']);
});

test('missing fields stay null and metadata is bounded without inventing publication dates', async () => {
  const search = createLiteratureSearcher({ fetcher: async url => url.hostname === 'api.semanticscholar.org'
    ? response({ data: [semantic('b', { title: 'x'.repeat(600), abstract: 'a'.repeat(9000), authors: [], year: null, publicationDate: '2023-02-30', url: 'javascript:alert(1)' })] }) : empty(url) });
  const result = await search('research'); const paper = result.papers[0]!;
  assert.equal(paper.title.length, 500); assert.equal(paper.abstract!.length, 8000);
  assert.equal(paper.year, null); assert.equal(paper.publishedAt, null); assert.equal(paper.venue, null);
  assert.deepEqual(paper.authors, []); assert.match(paper.url, /^https:\/\/www\.semanticscholar\.org\/paper\//);
});

test('identifier duplicates retain the original source record that actually contains an abstract', async () => {
  const search = createLiteratureSearcher({ fetcher: async url => {
    if (url.hostname === 'api.crossref.org') return response({ message: { items: [cross('same')] } });
    if (url.hostname === 'export.arxiv.org') return new Response(atom(entry('2306.00001v2', '<arxiv:doi>10.1234/SAME</arxiv:doi>')));
    return empty(url);
  } });
  const result = await search('research');
  assert.equal(result.papers.length, 1);
  assert.equal(result.papers[0]!.source, 'arxiv');
  assert.equal(result.papers[0]!.abstract, 'Observed <signal> effects.');
  assert.equal(result.papers[0]!.title, 'Momentum & risk');
  assert.equal(result.papers[0]!.year, 2023);
  assert.equal(result.papers[0]!.url, 'https://arxiv.org/abs/2306.00001v2');
});

test('partial failures, all failures and genuine empty successful results remain distinct', async () => {
  const none = await createLiteratureSearcher({ fetcher: async url => empty(url) })('empty');
  assert.equal(none.status, 'no_results'); assert.deepEqual(none.papers, []);
  const partial = await createLiteratureSearcher({ fetcher: async url => url.hostname === 'api.crossref.org'
    ? new Response('private upstream detail', { status: 429 }) : empty(url) })('empty');
  assert.equal(partial.status, 'partial'); assert.equal(partial.sources[0]!.error, 'rate_limited');
  const unavailable = await createLiteratureSearcher({ fetcher: async () => new Response('private upstream detail', { status: 403 }) })('empty');
  assert.equal(unavailable.status, 'unavailable'); assert.ok(unavailable.sources.every(source => source.error === 'access_denied'));
  assert.ok(!JSON.stringify([partial, unavailable]).includes('private upstream detail'));
});

test('malformed JSON, Atom errors and XML entities are source errors, never no-results evidence', async () => {
  for (const body of ['<feed><entry>', '<!DOCTYPE feed [<!ENTITY secret "untrusted">]><feed/>', atom('<entry><id>https://export.arxiv.org/api/errors</id><title>Error</title></entry>')]) {
    const result = await createLiteratureSearcher({ fetcher: async url => url.hostname === 'export.arxiv.org' ? new Response(body) : new Response('invalid JSON') })('input');
    assert.equal(result.status, 'unavailable'); assert.ok(result.sources.every(source => source.error === 'invalid_response'));
  }
});

test('timeouts are bounded public failures and caller cancellation propagates separately', async () => {
  const waitForAbort: Parameters<typeof createLiteratureSearcher>[0]['fetcher'] = async (_url, options) => new Promise((_resolve, reject) => {
    options.signal.addEventListener('abort', () => reject(options.signal.reason), { once: true });
  });
  const timed = await createLiteratureSearcher({ fetcher: waitForAbort, timeoutMs: 10 })('timeout');
  assert.equal(timed.status, 'unavailable'); assert.ok(timed.sources.every(source => source.error === 'timeout'));
  const controller = new AbortController(); const reason = new Error('owner cancelled');
  const pending = createLiteratureSearcher({ fetcher: waitForAbort })('cancel', 5, controller.signal);
  controller.abort(reason); await assert.rejects(pending, error => error === reason);
  await assert.rejects(createLiteratureSearcher({ fetcher: async () => { throw new Error('must not fetch'); } })('cancel', 5, controller.signal), error => error === reason);
});

test('response limits apply both to declared bytes and streamed bodies, and stalled bodies cancel', async () => {
  let cancelled = 0;
  for (const declared of [true, false]) {
    const result = await createLiteratureSearcher({ maxResponseBytes: 100, fetcher: async () => new Response(new ReadableStream({
      start(controller) { controller.enqueue(new Uint8Array(101)); }, cancel() { cancelled++; },
    }), { headers: declared ? { 'content-length': '101' } : {} }) })('bounded');
    assert.ok(result.sources.every(source => source.error === 'response_too_large'));
  }
  assert.equal(cancelled, 6);
  const stalled = await createLiteratureSearcher({ timeoutMs: 10, fetcher: async () => new Response(new ReadableStream({ cancel() { cancelled++; } })) })('stalled');
  assert.ok(stalled.sources.every(source => source.error === 'timeout')); assert.equal(cancelled, 9);
});

test('invalid query and result count fail before transport', async () => {
  const search = createLiteratureSearcher({ fetcher: async () => { throw new Error('must not fetch'); } });
  for (const query of ['', ' ', 'a'.repeat(501), 'test\0']) await assert.rejects(search(query), /literature_query_invalid/);
  for (const limit of [0, 21, 1.5]) await assert.rejects(search('query', limit), /literature_limit_invalid/);
});
