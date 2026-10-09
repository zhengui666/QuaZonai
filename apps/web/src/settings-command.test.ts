import { describe, expect, it, vi } from 'vitest';
import { ApiFailure } from './api';
import { SettingsCommand } from './settings-command';

const path = '/api/v2/projects';
const body = { schema_version: 1, name: '原始登记', description: 'Keep the complete original request.' };
const receipt = '018fc823-8e40-7000-8000-000000000001';
const rejections = [
  new ApiFailure('OFFLINE', 'not sent'),
  ...([['AUTH_REQUIRED', 401], ['CONFLICT', 409]] as const).map(([code, status]) => new ApiFailure(code, 'rejected', status, {
    type: 'about:blank', title: 'rejected', status, code, detail: 'rejected', request_id: receipt,
    field_errors: [], retryable: false, safe_next_actions: [],
  })),
  new ApiFailure('LOCAL_VALIDATION_ERROR', 'invalid input'),
];

function submission(command: SettingsCommand, value = body) {
  const requests: Request[] = [];
  const response = vi.fn<() => Promise<string>>();
  const request = vi.fn(() => {
    requests.push(new Request(`http://localhost${path}`, {
      method: 'POST', headers: command.intent.headers('POST', path, value), body: JSON.stringify(value),
    }));
    return response();
  });
  return { request, response, requests };
}

async function expectSameRequest(first: Request, retry: Request) {
  expect(retry.method).toBe(first.method);
  expect(retry.url).toBe(first.url);
  expect(first.headers.get('Idempotency-Key')).toBeTruthy();
  expect(retry.headers.get('Idempotency-Key')).toBe(first.headers.get('Idempotency-Key'));
  expect(await retry.clone().text()).toBe(await first.clone().text());
}

describe('SettingsCommand preserves an unknown original submission', () => {
  it.each(rejections)('retains the original request, key and callback after a $code retry rejection', async rejection => {
    const command = new SettingsCommand(`retry:${rejection.code}`, '登记');
    const original = submission(command);
    original.response.mockRejectedValueOnce(new ApiFailure('NETWORK_UNKNOWN', 'lost receipt'))
      .mockRejectedValueOnce(rejection).mockResolvedValueOnce(receipt);
    const complete = vi.fn();
    const clear = vi.spyOn(command.intent, 'clear');
    await command.submit(original.request, complete);
    expect(command.getSnapshot()).toMatchObject({ pending: false, unknown: true, receipts: [] });

    const competing = submission(command, { ...body, name: 'Changed input', description: 'A different request.' });
    competing.response.mockResolvedValue('different receipt');
    const competingComplete = vi.fn();
    await command.submit(competing.request, competingComplete);
    expect(command.getSnapshot()).toMatchObject({ pending: false, unknown: true, receipts: [], error: rejection });
    expect(clear).not.toHaveBeenCalled();
    expect(complete).not.toHaveBeenCalled();
    await expectSameRequest(original.requests[0]!, original.requests[1]!);

    command.retry();
    await vi.waitFor(() => expect(command.getSnapshot().pending).toBe(false));
    expect(original.request).toHaveBeenCalledTimes(3);
    await expectSameRequest(original.requests[0]!, original.requests[2]!);
    expect(await original.requests[2]!.clone().text()).toBe(JSON.stringify(body));
    expect(command.getSnapshot()).toEqual({ pending: false, unknown: false, receipts: [receipt], error: undefined });
    expect(clear).toHaveBeenCalledTimes(1);
    expect(complete).toHaveBeenCalledTimes(1);
    expect(competing.request).not.toHaveBeenCalled();
    expect(competingComplete).not.toHaveBeenCalled();
    command.retry();
    expect(original.request).toHaveBeenCalledTimes(3);
    expect(complete).toHaveBeenCalledTimes(1);
  });

  it.each(rejections)('allows a first $code rejection to clear the intent and accept a new submission', async rejection => {
    const command = new SettingsCommand(`first:${rejection.code}`, '登记');
    const first = submission(command);
    first.response.mockRejectedValueOnce(rejection);
    const complete = vi.fn();
    const clear = vi.spyOn(command.intent, 'clear');
    await command.submit(first.request, complete);
    expect(command.getSnapshot()).toEqual({ pending: false, unknown: false, receipts: [], error: rejection });
    expect(clear).toHaveBeenCalledTimes(1);
    command.retry();
    expect(first.request).toHaveBeenCalledTimes(1);

    const next = submission(command);
    next.response.mockResolvedValueOnce(receipt);
    const nextComplete = vi.fn();
    await command.submit(next.request, nextComplete);
    expect(next.requests[0]!.headers.get('Idempotency-Key')).not.toBe(first.requests[0]!.headers.get('Idempotency-Key'));
    expect(complete).not.toHaveBeenCalled();
    expect(nextComplete).toHaveBeenCalledTimes(1);
    expect(command.getSnapshot()).toEqual({ pending: false, unknown: false, receipts: [receipt], error: undefined });
  });

  it('keeps every consecutive failure unknown until the original receipt arrives', async () => {
    const command = new SettingsCommand('consecutive', '登记');
    const original = submission(command);
    const failures = [new ApiFailure('NETWORK_UNKNOWN', 'lost receipt'), ...rejections,
      new ApiFailure('UNAVAILABLE', 'server failure', 503), new ApiFailure('HTTP_CONTRACT_ERROR', 'invalid receipt', 201),
      new ApiFailure('RESPONSE_VALIDATOR_UNAVAILABLE', 'unvalidated receipt'), new Error('unexpected failure'),
      new DOMException('cancelled', 'AbortError')];
    for (const failure of failures) original.response.mockRejectedValueOnce(failure);
    original.response.mockResolvedValueOnce(receipt);
    const complete = vi.fn();
    const clear = vi.spyOn(command.intent, 'clear');
    for (const failure of failures) {
      await command.submit(original.request, complete);
      expect(command.getSnapshot()).toEqual({ pending: false, unknown: true, receipts: [], error: failure });
      await expectSameRequest(original.requests[0]!, original.requests.at(-1)!);
      expect(clear).not.toHaveBeenCalled();
      expect(complete).not.toHaveBeenCalled();
    }
    await expectSameRequest(original.requests[0]!, original.requests[2]!);
    await command.submit(original.request, complete);
    expect(command.getSnapshot()).toEqual({ pending: false, unknown: false, receipts: [receipt], error: undefined });
    expect(clear).toHaveBeenCalledTimes(1);
    expect(complete).toHaveBeenCalledTimes(1);
  });

  it('confirms a successful first request and completes once', async () => {
    const command = new SettingsCommand('success', '登记');
    const original = submission(command);
    original.response.mockResolvedValueOnce(receipt);
    const complete = vi.fn();
    const clear = vi.spyOn(command.intent, 'clear');
    await command.submit(original.request, complete);
    command.retry();
    expect(command.getSnapshot()).toEqual({ pending: false, unknown: false, receipts: [receipt], error: undefined });
    expect(original.request).toHaveBeenCalledTimes(1);
    expect(clear).toHaveBeenCalledTimes(1);
    expect(complete).toHaveBeenCalledTimes(1);
  });

  it('absorbs double clicks while the request and completion are pending', async () => {
    const command = new SettingsCommand('double-click', '登记');
    const original = submission(command);
    let resolve!: (value: string) => void;
    original.response.mockImplementationOnce(() => new Promise<string>(done => { resolve = done; }));
    let finish!: () => void;
    const complete = vi.fn(() => new Promise<void>(done => { finish = done; }));
    const competing = submission(command, { ...body, name: 'Double click' });
    const competingComplete = vi.fn();
    const pending = command.submit(original.request, complete);
    await command.submit(competing.request, competingComplete);
    expect(command.getSnapshot()).toMatchObject({ pending: true, unknown: false, receipts: [] });
    expect(original.request).toHaveBeenCalledTimes(1);
    expect(competing.request).not.toHaveBeenCalled();
    resolve(receipt);
    await vi.waitFor(() => expect(complete).toHaveBeenCalledTimes(1));
    expect(command.getSnapshot()).toMatchObject({ pending: true, unknown: false, receipts: [receipt] });
    await command.submit(competing.request, competingComplete);
    expect(competing.request).not.toHaveBeenCalled();
    finish();
    await pending;
    command.retry();
    expect(command.getSnapshot()).toEqual({ pending: false, unknown: false, receipts: [receipt], error: undefined });
    expect(original.request).toHaveBeenCalledTimes(1);
    expect(complete).toHaveBeenCalledTimes(1);
    expect(competingComplete).not.toHaveBeenCalled();
  });

  it('absorbs double retry clicks and completes the unknown original once', async () => {
    const command = new SettingsCommand('double-retry', '登记');
    const original = submission(command);
    let resolve!: (value: string) => void;
    original.response.mockRejectedValueOnce(new ApiFailure('NETWORK_UNKNOWN', 'lost receipt'))
      .mockImplementationOnce(() => new Promise<string>(done => { resolve = done; }));
    const complete = vi.fn();
    await command.submit(original.request, complete);
    command.retry(); command.retry();
    expect(command.getSnapshot()).toMatchObject({ pending: true, unknown: true, receipts: [] });
    expect(original.request).toHaveBeenCalledTimes(2);
    resolve(receipt);
    await vi.waitFor(() => expect(command.getSnapshot().pending).toBe(false));
    await expectSameRequest(original.requests[0]!, original.requests[1]!);
    command.retry();
    expect(original.request).toHaveBeenCalledTimes(2);
    expect(complete).toHaveBeenCalledTimes(1);
    expect(command.getSnapshot()).toEqual({ pending: false, unknown: false, receipts: [receipt], error: undefined });
  });

  it('keeps a confirmed receipt distinct from a later UI refresh failure', async () => {
    const command = new SettingsCommand('refresh-failure', '登记');
    const original = submission(command);
    original.response.mockRejectedValueOnce(new ApiFailure('NETWORK_UNKNOWN', 'lost receipt')).mockResolvedValueOnce(receipt);
    const refreshError = new ApiFailure('NETWORK_UNKNOWN', 'refresh failed after confirmation');
    const complete = vi.fn(async () => { throw refreshError; });
    const clear = vi.spyOn(command.intent, 'clear');
    await command.submit(original.request, complete);
    await command.submit(original.request, complete);
    expect(command.getSnapshot()).toEqual({ pending: false, unknown: false, receipts: [receipt], error: refreshError });
    command.retry();
    expect(original.request).toHaveBeenCalledTimes(2);
    expect(complete).toHaveBeenCalledTimes(1);
    expect(clear).toHaveBeenCalledTimes(1);
    command.acknowledgeError();
    expect(command.getSnapshot()).toEqual({ pending: false, unknown: false, receipts: [receipt], error: undefined });
  });
});
