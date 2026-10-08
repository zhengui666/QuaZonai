import { describe, expect, it, vi } from 'vitest';
import { ApiFailure } from './api';
import { createExitResultReader, ExitSession, money, positiveDecimal, previewCanStart, remainingReduction, withdrawableDisplay, type ExitRequest, type ExitResult } from './capital-exit-state';
import { id, source, intent, preview, exit, amount, time, until } from './capital-exit-fixtures';
const request: ExitRequest = { kind: 'START', project: id, source, body: { schema_version: 1, preview_id: id, expected_account_control_revision: '9007199254740993', acknowledged_plan_artifact_id: id, expected_source_observation_id: id } };
const result: ExitResult = { kind: 'INTENT', value: exit };
function memory() { const values = new Map<string, string>(); return { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); } }; }

describe('capital exit recoverable operation', () => {
  it('absorbs repeated clicks while pending and after one validated receipt', async () => {
    let resolve!: (result: ExitResult) => void;
    const send = vi.fn(() => new Promise<ExitResult>(done => { resolve = done; }));
    const session = new ExitSession('test', memory());
    const first = session.submit(request, send); await session.submit(request, send);
    expect(send).toHaveBeenCalledTimes(1); expect(session.getSnapshot().pending).toBe(true);
    resolve(result); await first; await session.submit(request, send);
    expect(send).toHaveBeenCalledTimes(1); expect(session.getSnapshot().lastIntent).toBe(intent);
  });
  it('retains exact body and key across lost response, close, refresh and changed caller data', async () => {
    const storage = memory(); const session = new ExitSession('test', storage);
    const failed = vi.fn(async () => { throw new ApiFailure('NETWORK_UNKNOWN', 'unknown'); });
    await session.submit(request, failed); const original = session.getSnapshot().operation!;
    expect(original.request).toMatchObject({ body: { expected_account_control_revision: '9007199254740993' } });
    const restored = new ExitSession('test', storage, { project: id, source });
    expect(restored.getSnapshot().unknown).toBe(true); expect(failed).toHaveBeenCalledTimes(1);
    const send = vi.fn(async () => result);
    await restored.submit({ ...request, body: { ...request.body, expected_account_control_revision: '2' } }, send);
    expect(send).toHaveBeenCalledWith(original.request, original.key);
    expect(new ExitSession('test', storage).getSnapshot()).toMatchObject({ unknown: false, lastIntent: intent });
  });
  it.each([
    ['OFFLINE', 0], ['AUTH_REQUIRED', 401], ['FORBIDDEN', 403], ['STALE_PREVIEW', 409],
  ] as const)('keeps an unknown original request through a %s retry and reload until its receipt resolves', async (code, status) => {
    const storage = memory(); const first = new ExitSession('test', storage);
    await first.submit(request, async () => { throw new ApiFailure('NETWORK_UNKNOWN', 'original result lost'); });
    const original = first.getSnapshot().operation!;
    const retained = storage.getItem('test');
    const retry = new ExitSession('test', storage, { project: id, source });
    const problem = status === 0 ? undefined : { code, detail: 'retry rejected', status, request_id: id, field_errors: [], title: 'retry rejected', type: 'about:blank', retryable: false, safe_next_actions: [] };
    const rejected = vi.fn(async () => { throw new ApiFailure(code, 'retry rejected', status, problem); });
    const competing: ExitRequest = { kind: 'ACTION', project: id, source, id: intent,
      body: { schema_version: 1, action: 'PAUSE', expected_revision: '2' } };
    await retry.submit(competing, rejected);
    expect(rejected).toHaveBeenCalledWith(original.request, original.key);
    expect(retry.getSnapshot()).toMatchObject({ pending: false, unknown: true, operation: original });
    expect(storage.getItem('test')).toBe(retained);
    const reopened = new ExitSession('test', storage, { project: id, source });
    expect(reopened.getSnapshot()).toMatchObject({ pending: false, unknown: true, operation: original });
    const confirmed = vi.fn(async () => result);
    await reopened.submit(undefined, confirmed);
    expect(confirmed).toHaveBeenCalledWith(original.request, original.key);
    expect(reopened.getSnapshot()).toMatchObject({ pending: false, unknown: false, operation: undefined, lastIntent: intent });
    expect(new ExitSession('test', storage).getSnapshot()).toMatchObject({ unknown: false, lastIntent: intent });
  });
  it('can clear an offline first attempt with no prior unknown outcome', async () => {
    const storage = memory(); const session = new ExitSession('test', storage);
    await session.submit(request, async () => { throw new ApiFailure('OFFLINE', 'not sent'); });
    expect(session.getSnapshot()).toMatchObject({ pending: false, unknown: false, operation: undefined });
    expect(new ExitSession('test', storage).getSnapshot()).toMatchObject({ unknown: false, operation: undefined });
    const send = vi.fn(async () => result);
    await session.submit(request, send);
    expect(send).toHaveBeenCalledTimes(1); expect(session.getSnapshot().lastIntent).toBe(intent);
  });
  it('restores an interrupted pending write as unknown without auto-submission', async () => {
    const storage = memory(); const session = new ExitSession('test', storage);
    void session.submit(request, () => new Promise(() => {}));
    const restored = new ExitSession('test', storage);
    expect(restored.getSnapshot()).toMatchObject({ pending: false, unknown: true });
    expect(restored.getSnapshot().operation?.request).toEqual(request);
  });
  it('blocks unreadable or account-mismatched recovery records instead of forgetting them', async () => {
    const storage = memory(); storage.setItem('test', '{');
    const bad = new ExitSession('test', storage); const send = vi.fn(async () => result);
    await bad.submit(request, send); expect(send).not.toHaveBeenCalled(); expect(bad.getSnapshot().unknown).toBe(true);
    const storage2 = memory(); const original = new ExitSession('test', storage2);
    await original.submit(request, async () => { throw new Error('lost'); });
    const crossed = new ExitSession('test', storage2, { project: intent, source });
    await crossed.submit(request, send); expect(send).not.toHaveBeenCalled();
  });
  it('restores the same unknown operation past the former serialized recovery limit', async () => {
    const storage = memory();
    const first = new ExitSession('test', storage);
    await first.submit(request, async () => { throw new ApiFailure('NETWORK_UNKNOWN', 'response lost'); });
    const original = first.getSnapshot().operation!;
    storage.setItem('test', ' '.repeat(65537) + storage.getItem('test')!);
    const restored = new ExitSession('test', storage, { project: id, source });
    expect(restored.getSnapshot().unknown).toBe(true);
    expect(restored.getSnapshot().operation).toEqual(original);
    expect(restored.getSnapshot().error).toBeUndefined();
    const stillUnknown = vi.fn(async () => { throw new ApiFailure('OFFLINE', 'not sent'); });
    await restored.submit({ ...request, body: { ...request.body, expected_account_control_revision: '2' } }, stillUnknown);
    expect(stillUnknown).toHaveBeenCalledWith(original.request, original.key);
    expect(restored.getSnapshot()).toMatchObject({ unknown: true, operation: original });
    expect(new ExitSession('test', storage, { project: id, source }).getSnapshot().operation).toEqual(original);
  });
  it('does not send when original key cannot be persisted', async () => {
    const session = new ExitSession('test', { getItem: () => null, setItem: () => { throw new Error('storage denied'); } });
    const send = vi.fn(async () => result); await session.submit(request, send);
    expect(send).not.toHaveBeenCalled(); expect(session.getSnapshot().pending).toBe(false);
  });
  it('a foreign receipt remains unknown rather than marking this account successful', async () => {
    const session = new ExitSession('test', memory());
    await session.submit(request, async () => ({ kind: 'INTENT', value: { ...exit, account_source_id: intent } }));
    expect(session.getSnapshot().unknown).toBe(true); expect(session.getSnapshot().result).toBeUndefined();
  });
  it('validated 409 rejects old preview without silently starting another plan', async () => {
    const session = new ExitSession('test', memory());
    const problem = { code: 'STALE_PREVIEW', detail: 'refresh', status: 409, request_id: id, field_errors: [], current_revision: '2', title: 'conflict', type: 'about:blank', retryable: false, safe_next_actions: ['REFRESH'] };
    const send = vi.fn(async () => { throw new ApiFailure('STALE_PREVIEW', 'refresh', 409, problem); });
    await session.submit(request, send);
    expect(session.getSnapshot()).toMatchObject({ pending: false, unknown: false, operation: undefined });
    expect(send).toHaveBeenCalledTimes(1);
  });
});
describe('capital exit Drawer completion selection', () => {
  const historicalId = '018fc823-8e40-7000-8000-000000000004';
  it('keeps explicitly selected history B when the source session still holds consumed result A', () => {
    const read = createExitResultReader(historicalId, result);
    let selected = historicalId;
    const completion = read(result);
    if (completion?.kind === 'INTENT') selected = completion.value.id;
    expect(selected).toBe(historicalId);
    expect(read(result)).toBeUndefined();
  });
  it('applies newly completed work once after an explicit historical selection', () => {
    const read = createExitResultReader(historicalId, result);
    expect(read(result)).toBeUndefined();
    const newer: ExitResult = { kind: 'INTENT', value: { ...exit, id: historicalId, revision: '9007199254740994' } };
    expect(read(newer)).toBe(newer);
    expect(read(newer)).toBeUndefined();
    const newPreview: ExitResult = { kind: 'PREVIEW', value: preview };
    expect(read(newPreview)).toBe(newPreview);
    expect(read(newPreview)).toBeUndefined();
  });
  it('restores latest A when reopening without an explicit historical selection', () => {
    const read = createExitResultReader(undefined, result);
    expect(read(result)).toBe(result);
    expect(read(result)).toBeUndefined();
  });
  it('does not clear account-wide unknown work when history B is opened or a competing action is attempted', async () => {
    const storage = memory(); const session = new ExitSession('test', storage);
    await session.submit(request, async () => result);
    const pending: ExitRequest = { kind: 'ACTION', project: id, source, id: intent,
      body: { schema_version: 1, action: 'PAUSE', expected_revision: exit.revision } };
    await session.submit(pending, async () => { throw new ApiFailure('NETWORK_UNKNOWN', 'action outcome lost'); });
    const original = session.getSnapshot().operation!;
    const read = createExitResultReader(historicalId, session.getSnapshot().result);
    expect(read(session.getSnapshot().result)).toBeUndefined();
    expect(session.getSnapshot().unknown).toBe(true);
    const competing: ExitRequest = { kind: 'ACTION', project: id, source, id: historicalId,
      body: { schema_version: 1, action: 'CANCEL', expected_revision: '2' } };
    const stillUnknown = vi.fn(async () => { throw new ApiFailure('OFFLINE', 'retry not sent'); });
    await session.submit(competing, stillUnknown);
    expect(stillUnknown).toHaveBeenCalledWith(original.request, original.key);
    expect(session.getSnapshot()).toMatchObject({ unknown: true, operation: original });
    const confirmed: ExitResult = { kind: 'INTENT', value: { ...exit, revision: '9007199254740994' } };
    await session.submit(undefined, async () => confirmed);
    expect(read(session.getSnapshot().result)).toBe(confirmed);
    expect(session.getSnapshot().unknown).toBe(false);
  });
});
describe('capital exit display authority', () => {
  it('rejects stale, unsupported, wrong-observation and missing-revision plans', () => {
    const now = Date.parse(time);
    expect(previewCanStart(preview, now, id)).toBe(true);
    expect(previewCanStart({ ...preview, capability: 'BLOCKED' }, now, id)).toBe(false);
    expect(previewCanStart({ ...preview, environment: 'LIVE', capability: 'SUPPORTED' }, now, id)).toBe(true);
    expect(previewCanStart(preview, Date.parse(until), id)).toBe(false);
    expect(previewCanStart(preview, now, source)).toBe(false);
    expect(previewCanStart({ ...preview, expected_account_control_revision: null }, now, id)).toBe(false);
  });
  it('never labels paper funds as verified real withdrawal even with an inconsistent status', () => {
    const now = Date.parse(time);
    expect(withdrawableDisplay(exit, now)).toEqual({ label: '模拟可用现金', value: '10 USD', state: 'SIMULATED' });
    expect(withdrawableDisplay({ ...exit, funds: { ...exit.funds, withdrawability: 'VERIFIED' } }, now).value).toBe('暂无有效模拟证据');
    expect(withdrawableDisplay(exit, Date.parse(until)).value).toBe('暂无有效模拟证据');
  });
  it('removes live ready amount immediately on expiry, failed refresh or absent proof', () => {
    const live = { ...exit, environment: 'LIVE' as const, funds: { ...exit.funds, withdrawability: 'VERIFIED' as const } };
    expect(withdrawableDisplay(live, Date.parse(time)).value).toBe('10 USD');
    expect(withdrawableDisplay(live, Date.parse(until)).value).not.toBe('10 USD');
    expect(withdrawableDisplay(live, Date.parse(time), false).value).not.toBe('10 USD');
    expect(withdrawableDisplay({ ...live, funds: { ...live.funds, evidence_asof: null } }, Date.parse(time)).value).not.toBe('10 USD');
  });
  it('keeps absent funds unknown and exact money beyond Number precision', () => {
    expect(money(null)).toBe('暂无证据'); expect(money(amount)).toBe('9007199254740993.123456789 USD');
    expect(remainingReduction(amount, { amount: '9007199254740993.123456788', currency: 'USD' })).toBe('0.000000001 USD');
    expect(remainingReduction({ amount: '100', currency: 'USD' }, { amount: '0', currency: 'USD' })).toBe('100 USD');
    expect(remainingReduction(amount, null)).toBe('暂无证据');
    expect(positiveDecimal('0')).toBe(false); expect(positiveDecimal('-1')).toBe(false); expect(positiveDecimal('0.000000001')).toBe(true);
  });
});
