import { afterEach, describe, expect, it, vi } from 'vitest';
import { apiRequest } from '../lib/api/client';

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
