import { expect, it, vi } from 'vitest';
import { api, ApiError, shortError } from './api';
import { capitalInput } from './capacity';

it('capacity authoring preserves native decimal boundary strings and rejects out-of-range values', () => {
  const fields = { participation: '0.01', maximumAgeSeconds: '60' };
  for (const scenarios of ['99999999999999999999.999999999999999999', '0.000000000000000001', '0'.repeat(63) + '1', '1.' + '0'.repeat(62)]) expect(capitalInput({ ...fields, scenarios }).scenarios).toEqual([scenarios]);
  for (const scenarios of ['100000000000000000000', '0'.repeat(64) + '1', '0.0000000000000000001']) expect(() => capitalInput({ ...fields, scenarios })).toThrow('research_capital_scenarios_invalid');
  expect(shortError('research_capital_scenarios_invalid')).toContain('64'); expect(shortError('research_criterion_insufficient')).toContain('独立指标不可用'); expect(shortError('account_snapshot_conflict')).toContain('快照事实冲突');
});

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
