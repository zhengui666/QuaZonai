import { EnvHttpProxyAgent, fetch } from 'undici';
import { XMLParser, XMLValidator } from 'fast-xml-parser';
import { setTimeout as sleep } from 'node:timers/promises';
import type { LiteraturePaper, LiteratureSearchResult, LiteratureSource } from '../../../packages/contracts/src/literature.ts';

type ResponseBody = { ok: boolean; status: number; headers: { get(name: string): string | null }; body: {
  cancel(): Promise<void>; getReader(): { read(): Promise<{ done: boolean; value?: Uint8Array }>; cancel(): Promise<void>; releaseLock(): void };
} | null };
type Fetcher = (url: URL, options: { signal: AbortSignal; headers: Record<string, string>; redirect: 'error' }) => Promise<ResponseBody>;
type Candidate = { paper: LiteraturePaper; keys: string[] };
type ArxivQueue = <T>(operation: () => Promise<T>, signal?: AbortSignal) => Promise<T>;
type SearchOptions = { fetcher: Fetcher; timeoutMs?: number; maxResponseBytes?: number; arxivQueue?: ArxivQueue };
const SOURCES: LiteratureSource[] = ['crossref', 'arxiv', 'semanticscholar'];
const xml = new XMLParser({ ignoreAttributes: false, removeNSPrefix: true, parseTagValue: false, trimValues: true });
class SourceError extends Error {}

function cancellable<T>(promise: Promise<T>, signal?: AbortSignal): Promise<T> {
  if (!signal) return promise;
  signal.throwIfAborted();
  return new Promise((resolve, reject) => {
    const abort = () => { signal.removeEventListener('abort', abort); reject(signal.reason); };
    signal.addEventListener('abort', abort, { once: true });
    promise.then(value => { signal.removeEventListener('abort', abort); resolve(value); }, error => { signal.removeEventListener('abort', abort); reject(error); });
  });
}

/** One process-wide arXiv lane: hold it through response consumption, not only headers. */
export function createArxivQueue(options: { intervalMs?: number; now?: () => number; wait?: (ms: number, signal?: AbortSignal) => Promise<void> } = {}): ArxivQueue {
  const interval = options.intervalMs ?? 3000; const now = options.now ?? (() => performance.now());
  const wait = options.wait ?? (async (ms, signal) => { await sleep(ms, undefined, { signal }); });
  if (!Number.isFinite(interval) || interval < 0) throw new Error('literature_pacing_invalid');
  let tail = Promise.resolve(); let started: number | undefined;
  return async <T>(operation: () => Promise<T>, signal?: AbortSignal): Promise<T> => {
    signal?.throwIfAborted();
    const previous = tail; let release!: () => void;
    const current = new Promise<void>(resolve => { release = resolve; });
    // A cancelled waiter releases its own slot, but cannot let successors pass
    // the request that still owns the preceding slot.
    tail = previous.then(() => current);
    try {
      await cancellable(previous, signal);
      while (started !== undefined && started + interval > now()) await wait(started + interval - now(), signal);
      signal?.throwIfAborted();
      const result = operation(); started = now();
      return await result;
    } finally { release(); }
  };
}
const sharedArxivQueue = createArxivQueue();
let arxivDispatcher: EnvHttpProxyAgent | undefined;

function record(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
}
function list(value: unknown): unknown[] { return Array.isArray(value) ? value : value == null ? [] : [value]; }
function text(value: unknown, maximum = 500): string | null {
  if (typeof value !== 'string') return null;
  const clean = value.replace(/[\u0000-\u001f\u007f]/g, ' ').replace(/\s+/g, ' ').trim();
  return clean ? clean.slice(0, maximum) : null;
}
function year(value: unknown): number | null {
  return typeof value === 'number' && Number.isInteger(value) && value >= 1000 && value <= 9999 ? value : null;
}
function doi(value: unknown): string | null {
  const clean = text(value, 2048)?.replace(/^https?:\/\/(?:dx\.)?doi\.org\//i, '').replace(/^doi:\s*/i, '').toLowerCase();
  return clean && clean.length <= 200 && /^10\.\d{4,9}\/[^\s<>]+$/.test(clean) ? clean : null;
}
function doiUrl(value: string): string { return `https://doi.org/${value.split('/').map(encodeURIComponent).join('/')}`; }
function arxivId(value: unknown): string | null {
  const clean = text(value)?.replace(/^https?:\/\/(?:export\.)?arxiv\.org\/(?:abs|pdf)\//i, '').replace(/\.pdf$/i, '');
  return clean && /^(?:\d{4}\.\d{4,5}|[a-z-]+(?:\.[A-Z]{2})?\/\d{7})(?:v\d+)?$/.test(clean) ? clean : null;
}
function arxivKey(value: string): string { return `arxiv:${value.replace(/v\d+$/, '')}`; }
function date(value: unknown): string | null {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}(?:T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z)?$/.test(value)) return null;
  const parsed = new Date(value);
  return Number.isFinite(parsed.getTime()) && parsed.toISOString().slice(0, 10) === value.slice(0, 10) ? value : null;
}
function partsDate(value: unknown): { year: number | null; publishedAt: string | null } {
  const parts = list(record(value)['date-parts'])[0];
  if (!Array.isArray(parts) || year(parts[0]) === null) return { year: null, publishedAt: null };
  const y = parts[0] as number;
  if (parts.length === 1) return { year: y, publishedAt: String(y) };
  const month = parts[1];
  if (!Number.isInteger(month) || month < 1 || month > 12) return { year: y, publishedAt: null };
  const prefix = `${y}-${String(month).padStart(2, '0')}`;
  if (parts.length === 2) return { year: y, publishedAt: prefix };
  return { year: y, publishedAt: date(`${prefix}-${String(parts[2]).padStart(2, '0')}`) };
}
function plainAbstract(value: unknown): string | null {
  // Crossref abstracts can contain JATS markup. Do not render that markup.
  return text(typeof value === 'string' ? value.replace(/<[^>]*>/g, ' ') : value, 8000);
}
function authors(values: unknown, field: 'name' | 'crossref'): string[] {
  return list(values).slice(0, 30).flatMap(item => {
    const row = record(item);
    const name = text(field === 'crossref' ? [row.given, row.family].filter(part => typeof part === 'string').join(' ') || row.name : row.name, 200);
    return name ? [name] : [];
  });
}
function crossref(value: unknown): Candidate[] {
  const items = record(record(value).message).items;
  if (!Array.isArray(items)) throw new SourceError('invalid_response');
  const candidates = items.slice(0, 20).flatMap(item => {
    const row = record(item); const title = text(list(row.title)[0]); const identifier = doi(row.DOI);
    if (!title || !identifier) return [];
    return [{ paper: { id: `doi:${identifier}`, source: 'crossref' as const, title, authors: authors(row.author, 'crossref'),
      ...partsDate(row.published), venue: text(list(row['container-title'])[0]), doi: identifier, url: doiUrl(identifier), abstract: plainAbstract(row.abstract) }, keys: [`doi:${identifier}`] }];
  });
  if (items.length && !candidates.length) throw new SourceError('invalid_response');
  return candidates;
}
function arxiv(body: string): Candidate[] {
  if (/<!DOCTYPE|<!ENTITY/i.test(body) || XMLValidator.validate(body) !== true) throw new SourceError('invalid_response');
  const document = record(xml.parse(body));
  if (!('feed' in document)) throw new SourceError('invalid_response');
  const entries = list(record(document.feed).entry);
  const candidates = entries.slice(0, 20).flatMap(item => {
    const row = record(item); const title = text(row.title); const identifier = arxivId(row.id);
    if (!title || !identifier) return [];
    const publishedAt = date(row.published); const identifierDoi = doi(row.doi);
    return [{ paper: { id: arxivKey(identifier), source: 'arxiv' as const, title, authors: authors(row.author, 'name'),
      year: publishedAt ? year(Number(publishedAt.slice(0, 4))) : null, publishedAt, venue: text(row.journal_ref), doi: identifierDoi,
      url: `https://arxiv.org/abs/${identifier}`, abstract: text(row.summary, 8000) },
      keys: [arxivKey(identifier), ...(identifierDoi ? [`doi:${identifierDoi}`] : [])] }];
  });
  if (entries.length && !candidates.length) throw new SourceError('invalid_response');
  return candidates;
}
function semanticScholar(value: unknown): Candidate[] {
  const items = record(value).data;
  if (!Array.isArray(items)) throw new SourceError('invalid_response');
  const candidates = items.slice(0, 20).flatMap(item => {
    const row = record(item); const title = text(row.title); const identifier = text(row.paperId);
    if (!title || !identifier || !/^[a-f\d]{40}$/i.test(identifier)) return [];
    const external = record(row.externalIds); const identifierDoi = doi(external.DOI); const preprint = arxivId(external.ArXiv);
    return [{ paper: { id: `semanticscholar:${identifier}`, source: 'semanticscholar' as const, title,
      authors: authors(row.authors, 'name'), year: year(row.year), venue: text(row.venue), doi: identifierDoi,
      url: identifierDoi ? doiUrl(identifierDoi) : preprint ? `https://arxiv.org/abs/${preprint}` : `https://www.semanticscholar.org/paper/${identifier}`,
      abstract: text(row.abstract, 8000), publishedAt: date(row.publicationDate) },
      keys: [`semanticscholar:${identifier}`, ...(identifierDoi ? [`doi:${identifierDoi}`] : []), ...(preprint ? [arxivKey(preprint)] : [])] }];
  });
  if (items.length && !candidates.length) throw new SourceError('invalid_response');
  return candidates;
}
function endpoint(source: LiteratureSource, query: string, limit: number): URL {
  const url = new URL(source === 'crossref' ? 'https://api.crossref.org/works' : source === 'arxiv' ? 'https://export.arxiv.org/api/query' : 'https://api.semanticscholar.org/graph/v1/paper/search');
  const params: Record<string, string> = source === 'crossref' ? { 'query.bibliographic': query, rows: String(limit), select: 'DOI,title,author,published,container-title,abstract' }
    : source === 'arxiv' ? { search_query: arxivQuery(query), start: '0', max_results: String(limit), sortBy: 'relevance', sortOrder: 'descending' }
      : { query, limit: String(limit), fields: 'paperId,title,authors,year,venue,url,externalIds,abstract,publicationDate' };
  url.search = new URLSearchParams(params).toString(); return url;
}
function arxivQuery(query: string): string {
  // User punctuation separates literal keywords; no field, group, wildcard or
  // Boolean syntax is copied into the upstream query language.
  const terms = query.match(/[\p{L}\p{N}]+(?:[-_.][\p{L}\p{N}]+)*/gu);
  if (!terms?.length) throw new SourceError('invalid_query');
  return terms.map(term => `all:"${term.replace(/["\\]/g, '\\$&')}"`).join(' AND ');
}
async function boundedBody(response: ResponseBody, maximum: number, signal: AbortSignal): Promise<string> {
  if (Number(response.headers.get('content-length')) > maximum) { await response.body?.cancel(); throw new SourceError('response_too_large'); }
  if (!response.body) throw new SourceError('invalid_response');
  const reader = response.body.getReader(); const chunks: Uint8Array[] = []; let size = 0;
  const abort = () => { void reader.cancel().catch(() => {}); };
  signal.addEventListener('abort', abort, { once: true });
  try {
    for (;;) {
      signal.throwIfAborted(); const item = await reader.read(); if (item.done) break;
      if (!item.value) throw new SourceError('invalid_response');
      size += item.value.byteLength;
      if (size > maximum) throw new SourceError('response_too_large');
      chunks.push(item.value);
    }
  } finally { signal.removeEventListener('abort', abort); await reader.cancel().catch(() => {}); reader.releaseLock(); }
  return Buffer.concat(chunks).toString('utf8');
}
function balanced(groups: Candidate[][], limit: number): LiteraturePaper[] {
  // Join proven identifiers, including a DOI/arXiv bridge from a third source.
  // Equal titles alone do not prove that two papers are the same work.
  const parents = new Map<string, string>();
  function root(key: string): string {
    const parent = parents.get(key); if (!parent || parent === key) return key;
    const value = root(parent); parents.set(key, value); return value;
  }
  for (const candidate of groups.flat()) for (const key of candidate.keys.slice(1)) parents.set(root(key), root(candidate.keys[0]!));
  const representatives = new Map<string, Candidate>();
  for (const candidate of groups.flat()) {
    const identity = root(candidate.keys[0]!); const previous = representatives.get(identity);
    // Keep the complete observed record and its real source, rather than attach
    // another provider's abstract to metadata it did not return.
    if (!previous || previous.paper.abstract === null && candidate.paper.abstract !== null) representatives.set(identity, candidate);
  }
  const seen = new Set<string>(); const result: LiteraturePaper[] = []; const positions = groups.map(() => 0);
  while (result.length < limit) {
    const before = result.length;
    for (const [index, group] of groups.entries()) {
      while (positions[index]! < group.length) {
        const candidate = group[positions[index]!]!; positions[index]!++;
        const identity = root(candidate.keys[0]!); if (seen.has(identity)) continue;
        seen.add(identity); result.push(representatives.get(identity)!.paper); break;
      }
      if (result.length === limit) break;
    }
    if (result.length === before) break;
  }
  return result;
}

/** Testable transport boundary; endpoints and metadata parsing remain production code. */
export function createLiteratureSearcher(options: SearchOptions) {
  const timeoutMs = options.timeoutMs ?? 12_000; const maximum = options.maxResponseBytes ?? 2 * 1024 * 1024;
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || !Number.isSafeInteger(maximum) || maximum < 1) throw new Error('literature_options_invalid');
  return async (query: string, limit = 5, signal?: AbortSignal): Promise<LiteratureSearchResult> => {
    signal?.throwIfAborted();
    if (typeof query !== 'string' || !query.trim() || query.trim().length > 500 || /[\u0000-\u001f\u007f]/.test(query)) throw new Error('literature_query_invalid');
    if (!Number.isInteger(limit) || limit < 1 || limit > 20) throw new Error('literature_limit_invalid');
    query = query.trim(); const searchedAt = new Date().toISOString();
    const sourceSearch = async (source: LiteratureSource) => {
      const timeout = new AbortController(); const timer = setTimeout(() => timeout.abort(), timeoutMs);
      const combined = signal ? AbortSignal.any([signal, timeout.signal]) : timeout.signal;
      try {
        const response = await options.fetcher(endpoint(source, query, limit), { signal: combined, redirect: 'error',
          headers: { accept: source === 'arxiv' ? 'application/atom+xml' : 'application/json', 'user-agent': 'QuaZonai/3.0 (research literature)' } });
        combined.throwIfAborted();
        if (!response.ok) {
          await response.body?.cancel();
          throw new SourceError(response.status === 429 ? 'rate_limited' : [401, 403].includes(response.status) ? 'access_denied' : 'http_error');
        }
        const body = await boundedBody(response, maximum, combined); combined.throwIfAborted();
        let candidates: Candidate[];
        try { candidates = source === 'arxiv' ? arxiv(body) : source === 'crossref' ? crossref(JSON.parse(body)) : semanticScholar(JSON.parse(body)); }
        catch { throw new SourceError('invalid_response'); }
        return { source, status: 'ok' as const, error: null, candidates };
      } catch (error) {
        signal?.throwIfAborted();
        return { source, status: 'error' as const, error: timeout.signal.aborted ? 'timeout' : error instanceof SourceError ? error.message : 'network_unavailable', candidates: [] };
      } finally { clearTimeout(timer); }
    };
    // Queue time is not an upstream request timeout. Other sources remain
    // parallel while arXiv waits its turn, and cancellation removes queued work.
    const results = await Promise.all(SOURCES.map(source => source === 'arxiv'
      ? (options.arxivQueue ?? sharedArxivQueue)(() => sourceSearch(source), signal)
      : sourceSearch(source)));
    signal?.throwIfAborted();
    const papers = balanced(results.map(result => result.candidates), limit); const succeeded = results.filter(result => result.status === 'ok').length;
    return { query, searchedAt, sources: results.map(({ source, status, error }) => ({ source, status, error })), papers,
      status: succeeded === 0 ? 'unavailable' : succeeded < SOURCES.length ? 'partial' : papers.length ? 'completed' : 'no_results' };
  };
}

export async function searchLiterature(query: string, limit = 5, signal?: AbortSignal): Promise<LiteratureSearchResult> {
  const dispatcher = new EnvHttpProxyAgent();
  try { return await createLiteratureSearcher({ fetcher: (url, options) => fetch(url, { ...options,
    dispatcher: url.hostname === 'export.arxiv.org' ? arxivDispatcher ??= new EnvHttpProxyAgent({ connections: 1 }) : dispatcher,
  }) })(query, limit, signal); }
  finally { await dispatcher.close(); }
}
