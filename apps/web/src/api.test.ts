import { afterEach, expect, it, vi } from 'vitest';
import { api, ApiError, shortError } from './api';
import i18n from './i18n';
afterEach(async () => { await i18n.changeLanguage('en'); });

it('sends the exact captured request bytes on an explicit retry after a failed send', async () => {
  const raw = JSON.stringify({ requestId: 'request-original', goal: '测试研究', model: { provider: 'test', modelId: 'test-model' }, datasetId: 'test-data' });
  const fetch = vi.fn().mockRejectedValueOnce(new TypeError('offline')).mockResolvedValueOnce(Response.json({ conversationId: 7, submissionId: 9 }, { status: 202 }));
  vi.stubGlobal('fetch', fetch);
  try {
    await expect(api('/research', { method: 'POST', rawBody: raw })).rejects.toBeInstanceOf(ApiError);
    await expect(api('/research', { method: 'POST', rawBody: raw })).resolves.toEqual({ conversationId: 7, submissionId: 9 });
    expect(fetch.mock.calls.map(call => call[1].body)).toEqual([raw, raw]);
    expect(fetch.mock.calls[0]![0]).toBe('/api/v3/research'); expect(fetch.mock.calls[0]![1].credentials).toBe('same-origin');
    expect(shortError('{"error":"raw payload"}')).toBe('Request failed.');
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

it('localizes known errors at display time without changing codes or original diagnostics', async () => {
  await i18n.changeLanguage('en');
  const error = new ApiError('Original diagnostic', 404, 'dataset_not_found');
  expect(shortError(error)).toBe('The dataset does not exist.');
  await i18n.changeLanguage('zh-CN');
  expect(shortError(error)).toBe('数据集不存在');
  expect(error.code).toBe('dataset_not_found');
  expect(error.message).toBe('Original diagnostic');
  expect(shortError('原始用户内容')).toBe('原始用户内容');
  await i18n.changeLanguage('en');
  expect(shortError(error)).toBe('The dataset does not exist.');
});
