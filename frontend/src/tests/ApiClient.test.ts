import { afterEach, describe, expect, it, vi } from 'vitest';
import { answerIdeaDraft, apiRequest, createIdeaDraft, normalizeList, startIdeaDraft } from '../lib/api/client';

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('apiRequest failures', () => {
  it('marks non-JSON HTTP failures as localizable fallbacks', async () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.resolve(new Response('proxy unavailable', { status: 502 }))));

    await expect(apiRequest('/api/v1/example')).rejects.toMatchObject({
      failure: { kind: 'http', status: 502 },
      status: 502,
    });
  });

  it('marks network failures as localizable fallbacks', async () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.reject(new TypeError('Failed to fetch'))));

    await expect(apiRequest('/api/v1/example')).rejects.toMatchObject({
      failure: { kind: 'network' },
      status: 0,
      code: undefined,
    });
  });

  it('marks malformed successful JSON responses as localizable fallbacks', async () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.resolve(new Response('{', {
      status: 200,
      headers: { 'content-type': 'application/json' },
    }))));

    await expect(apiRequest('/api/v1/example')).rejects.toMatchObject({
      failure: { kind: 'decode' },
      status: 200,
      code: undefined,
    });
  });

  it('marks successful non-JSON responses as decode failures', async () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.resolve(new Response('<!doctype html>', {
      status: 200,
      headers: { 'content-type': 'text/html' },
    }))));

    await expect(apiRequest('/api/v1/example')).rejects.toMatchObject({
      failure: { kind: 'decode' },
      status: 200,
      code: undefined,
    });
  });

  it('preserves API-authored messages in structured failures', async () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.resolve(new Response(JSON.stringify({
      error: { message: 'Origin policy denied.' },
    }), {
      status: 403,
      headers: { 'content-type': 'application/json' },
    }))));

    await expect(apiRequest('/api/v1/example')).rejects.toMatchObject({
      failure: { kind: 'api', message: 'Origin policy denied.' },
      status: 403,
    });
  });
});

describe('list contracts', () => {
  it('keeps legacy array and envelope list responses', () => {
    expect(normalizeList<string>(['one'])).toEqual(['one']);
    expect(normalizeList<string>({ items: ['two'] })).toEqual(['two']);
    expect(normalizeList<string>({ data: ['three'] })).toEqual(['three']);
  });

  it('rejects malformed list envelopes instead of showing an empty list', () => {
    expect(() => normalizeList<string>({ items: null } as unknown as { items?: string[] })).toThrow(expect.objectContaining({
      code: 'CONTRACT_MISMATCH',
      failure: { kind: 'contract', message: 'Expected a list response with an items or data array.' },
    }));
  });
});

describe('Idea Draft client', () => {
  it('uses the v2 draft, answers, and start endpoints', async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ id: 'draft-1' }), { status: 201 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ id: 'draft-1' }), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ id: 'program-1', state: 'ACTIVE' }), { status: 200 }));
    vi.stubGlobal('fetch', fetchMock);

    await createIdeaDraft({ original_idea_text: 'Test a research boundary.' });
    await answerIdeaDraft('draft-1', { answers: { horizon: '1D' }, expected_revision: 2 });
    await startIdeaDraft('draft-1', { expected_revision: 3 });

    expect(fetchMock.mock.calls.map(([path]) => path)).toEqual([
      '/api/v1/idea-drafts',
      '/api/v1/idea-drafts/draft-1/answers',
      '/api/v1/idea-drafts/draft-1/start',
    ]);
    expect(JSON.parse(fetchMock.mock.calls[1]?.[1]?.body as string)).toEqual({
      answers: { horizon: '1D' },
      expected_revision: 2,
    });
  });
});
