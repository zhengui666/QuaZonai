import { expect, it, vi } from 'vitest';
import { api, ApiError, shortError } from './api';

it('sends the exact captured request bytes on an explicit retry after a failed send', async () => {
  const raw = JSON.stringify({ requestId: 'request-original', goal: '测试研究', model: { provider: 'test', modelId: 'test-model' }, datasetId: 'test-data' });
  const fetch = vi.fn().mockRejectedValueOnce(new TypeError('offline')).mockResolvedValueOnce(Response.json({ conversationId: 7, submissionId: 9 }, { status: 202 }));
  vi.stubGlobal('fetch', fetch);
  try {
    await expect(api('/research', { method: 'POST', rawBody: raw })).rejects.toBeInstanceOf(ApiError);
    await expect(api('/research', { method: 'POST', rawBody: raw })).resolves.toEqual({ conversationId: 7, submissionId: 9 });
    expect(fetch.mock.calls.map(call => call[1].body)).toEqual([raw, raw]);
    expect(fetch.mock.calls[0]![0]).toBe('/api/v3/research'); expect(fetch.mock.calls[0]![1].credentials).toBe('same-origin');
    expect(shortError('{"error":"raw payload"}')).toBe('请求失败');
  } finally { vi.unstubAllGlobals(); }
});

it('uses explicit JSON for bodyless mutations while GET remains bodyless', async () => {
 const fetch=vi.fn().mockImplementation(()=>Promise.resolve(Response.json({})));
 vi.stubGlobal('fetch',fetch);
 try{await api('/fixture',{method:'DELETE'});await api('/fixture');
  expect(fetch.mock.calls[0]![1].headers).toEqual({'Content-Type':'application/json'});expect(fetch.mock.calls[0]![1].body).toBe('{}');
  expect(fetch.mock.calls[1]![1].body).toBeUndefined();
 }finally{vi.unstubAllGlobals();}
});
