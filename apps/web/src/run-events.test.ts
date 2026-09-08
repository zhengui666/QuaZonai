import { describe, expect, it } from 'vitest';
import { decodeRunEvent } from './run-events';
const run = '01990000-0000-7000-8000-000000000001';
function frame(seq = '1', event = 'run.state_changed', patch: Record<string, unknown> = {}) {
  return { id: `${run}:${seq}`, event, data: JSON.stringify({
    schema_version: 1, run_id: run, seq, attempt_id: null, event_type: event,
    occurred_at: '2026-09-08T00:00:00Z', payload: { schema_version: 1, state: 'RUNNING', reason: 'RUNTIME_RUNNING' }, ...patch,
  }) };
}
describe('durable event cursor boundary', () => {
  it('accepts the actual seven-field Rust event', () => {
    expect(decodeRunEvent(frame(), run, '0')?.seq).toBe('1');
  });
  it('keeps counters above the JavaScript integer range exact', () => {
    expect(decodeRunEvent(frame('9007199254740993'), run, '9007199254740992')?.seq).toBe('9007199254740993');
  });
  it('deduplicates replayed events without state changes', () => {
    expect(decodeRunEvent(frame('1'), run, '1')).toBeNull();
  });
  it('rejects missing events instead of inventing progress', () => {
    expect(() => decodeRunEvent(frame('3'), run, '1')).toThrow('事件序号不连续');
  });
  it('keeps compatible unknown events without treating their payload as state', () => {
    const result = decodeRunEvent(frame('1', 'run.future_note', { payload: { schema_version: 1, diagnostic: 'fixture' } }), run, '0');
    expect(result?.event_type).toBe('run.future_note');
    expect(result?.payload).not.toHaveProperty('state');
  });
  it.each([
    { schema_version: 2 }, { seq: 1 }, { seq: '01' }, { unexpected: 'field' },
    { run_id: '01990000-0000-7000-8000-000000000002' },
    { payload: { schema_version: 1, state: 'INVENTED', reason: 'RUNTIME_RUNNING' } },
    { payload: { schema_version: 2 } }, { occurred_at: 'not a timestamp' },
  ])('rejects malformed or cross-run event %j', patch => {
    expect(() => decodeRunEvent(frame('1', 'run.state_changed', patch), run, '0')).toThrow();
  });
  it('requires the exact SSE id and name', () => {
    expect(() => decodeRunEvent({ ...frame(), id: '1' }, run, '0')).toThrow();
    expect(() => decodeRunEvent({ ...frame(), event: 'different' }, run, '0')).toThrow();
  });
  it('stops on reset-required without consuming a cursor', () => {
    expect(() => decodeRunEvent({ id: '', event: 'reset-required', data: '{}' }, run, '0')).toThrow('重新载入快照');
  });
  it('limits payload by UTF-8 bytes, not character count', () => {
    expect(() => decodeRunEvent(frame('1', 'run.future_note', { payload: { schema_version: 1, note: '研'.repeat(24000) } }), run, '0')).toThrow();
  });
});
