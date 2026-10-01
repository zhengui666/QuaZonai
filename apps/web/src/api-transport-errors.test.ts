import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiFailure, Intent, makeClient } from './api';

afterEach(() => { vi.unstubAllGlobals(); });

describe('HTTP transport failures keep read and write meanings distinct', () => {
  it.each(['GET', 'HEAD', 'POST', 'PUT', 'PATCH', 'DELETE'])('classifies %s without replay or raw error disclosure', async method => {
    const fetch = vi.fn(async (_request: Request) => { throw new TypeError('private transport details'); });
    vi.stubGlobal('fetch', fetch);
    const client = makeClient('http://localhost');
    // Exercise the official middleware with every method, including HEAD for
    // which the current business schema has no route. No server is contacted.
    client.use({ onRequest: ({ request }) => new Request(request, { method }) });
    const reading = ['GET', 'HEAD'].includes(method);
    await expect(client.GET('/api/v2/projects')).rejects.toMatchObject({
      code: reading ? 'NETWORK_READ_FAILED' : 'NETWORK_UNKNOWN',
      message: reading ? '连接中断，未能读取数据；请重试' : '连接中断，提交结果未知；请重试当前操作',
    });
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(fetch.mock.calls[0]![0].method).toBe(method);
  });

  it.each(['GET', 'POST'])('preserves aborts and existing failures for %s', async method => {
    for (const failure of [new DOMException('cancelled', 'AbortError'), new ApiFailure('OFFLINE', '离线，操作未提交')]) {
      const fetch = vi.fn(async () => { throw failure; }); vi.stubGlobal('fetch', fetch);
      const client = makeClient('http://localhost');
      client.use({ onRequest: ({ request }) => new Request(request, { method }) });
      await expect(client.GET('/api/v2/projects')).rejects.toBe(failure);
      expect(fetch).toHaveBeenCalledTimes(1);
    }
  });

  it('retains an uncertain write body and key until an explicit same-intent retry', async () => {
    const fetch = vi.fn(async (_request: Request) => { throw new TypeError('lost ACK'); }); vi.stubGlobal('fetch', fetch);
    const client = makeClient('http://localhost'); const intent = new Intent();
    const body = { schema_version: 1 as const, name: 'Research', description: '' };
    const headers = intent.headers('POST', '/api/v2/projects', body);
    await expect(client.POST('/api/v2/projects', { body, params: { header: headers } })).rejects.toMatchObject({ code: 'NETWORK_UNKNOWN' });
    await Promise.resolve(); expect(fetch).toHaveBeenCalledTimes(1);
    await expect(client.POST('/api/v2/projects', { body, params: { header: intent.headers('POST', '/api/v2/projects', body) } })).rejects.toMatchObject({ code: 'NETWORK_UNKNOWN' });
    expect(fetch).toHaveBeenCalledTimes(2);
    const first = fetch.mock.calls[0]![0]; const second = fetch.mock.calls[1]![0];
    expect(first.headers.get('Idempotency-Key')).toBe(second.headers.get('Idempotency-Key'));
    expect(await first.text()).toBe(await second.text());
  });
});
