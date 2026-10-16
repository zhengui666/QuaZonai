import { expect, it, vi } from 'vitest';
import { api, ApiError, shortError } from './api';
import { captureResearchIntent, clearResearchIntent, readResearchIntent } from './request-intent';

it('persists the exact research request across a failed send and a fresh retry', async () => {
  const values = new Map<string, string>();
  const storage = { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); }, removeItem: (key: string) => { values.delete(key); } };
  const input = { goal: '测试研究', model: { provider: 'test', modelId: 'test-model' }, datasetId: 'test-data', maxTrials: 2 };
  const captured = captureResearchIntent(input, storage); input.goal = '已修改';
  const fetch = vi.fn().mockRejectedValueOnce(new TypeError('offline')).mockResolvedValueOnce(Response.json({ conversationId: 7, submissionId: 9 }, { status: 202 }));
  vi.stubGlobal('fetch', fetch);
  try {
    await expect(api('/research', { method: 'POST', rawBody: captured.raw })).rejects.toBeInstanceOf(ApiError);
    const recovered = readResearchIntent(storage)!;
    expect(recovered.raw).toBe(captured.raw); expect(recovered.value.goal).toBe('测试研究');
    await expect(api('/research', { method: 'POST', rawBody: recovered.raw })).resolves.toEqual({ conversationId: 7, submissionId: 9 });
    expect(fetch.mock.calls.map(call => call[1].body)).toEqual([captured.raw, captured.raw]);
    expect(fetch.mock.calls[0]![0]).toBe('/api/v3/research'); expect(fetch.mock.calls[0]![1].credentials).toBe('same-origin');
    expect(shortError('{"error":"raw payload"}')).toBe('请求失败');
    clearResearchIntent(storage); expect(readResearchIntent(storage)).toBeUndefined();
  } finally { vi.unstubAllGlobals(); }
});
