import { ApiFailure, isCounter } from './api';
import type { Schema } from './api';

const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const states = new Set(['QUEUED', 'DISPATCHING', 'RUNNING', 'RECONCILING', 'CANCEL_REQUESTED', 'SUCCEEDED', 'FAILED', 'CANCELLED']);
const reasons = new Set(['ADMITTED', 'DISPATCH_RESERVED', 'RUNTIME_RUNNING', 'LEASE_TAKEN_OVER', 'CANCEL_REQUESTED', 'CANCELLED_BEFORE_DISPATCH', 'RUNTIME_SUCCEEDED', 'RUNTIME_FAILED', 'RUNTIME_CANCELLED', 'RESULT_DISCARDED_AFTER_CANCEL', 'DEADLINE_EXCEEDED']);
function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
function envelope(value: unknown): value is Schema['RunEventV1'] {
  if (!object(value) || Object.keys(value).length !== 7 || value.schema_version !== 1
    || typeof value.run_id !== 'string' || !uuid.test(value.run_id)
    || typeof value.seq !== 'string' || !isCounter(value.seq)
    || (value.attempt_id !== null && (typeof value.attempt_id !== 'string' || !uuid.test(value.attempt_id)))
    || typeof value.event_type !== 'string' || !/^[a-z][a-z0-9_.]{0,119}$/.test(value.event_type)
    || typeof value.occurred_at !== 'string' || !/Z$/.test(value.occurred_at) || Number.isNaN(Date.parse(value.occurred_at))
    || !object(value.payload) || value.payload.schema_version !== 1
    || new TextEncoder().encode(JSON.stringify(value.payload)).length > 65_536) return false;
  if (value.event_type === 'run.created' || value.event_type === 'run.state_changed') {
    return Object.keys(value.payload).length === 3 && typeof value.payload.state === 'string'
      && states.has(value.payload.state) && typeof value.payload.reason === 'string' && reasons.has(value.payload.reason);
  }
  return true;
}
/** A compatible future event advances only the cursor, never synthesizes state. */
export function decodeRunEvent(frame: { id: string; event: string; data: string }, run: string, previous: string): Schema['RunEventV1'] | null {
  if (frame.event === 'reset-required') throw new ApiFailure('EVENT_RESET_REQUIRED', '事件流要求重新载入快照并核验登录状态。');
  if (!frame.data) return null;
  if (new TextEncoder().encode(frame.data).length > 100_000) throw new ApiFailure('CONTRACT_VERSION_UNSUPPORTED', '事件体超出合同限制。');
  let value: unknown;
  try { value = JSON.parse(frame.data); } catch { throw new ApiFailure('CONTRACT_VERSION_UNSUPPORTED', '事件不是有效 JSON。'); }
  if (!envelope(value) || value.run_id !== run || frame.id !== `${run}:${value.seq}` || frame.event !== value.event_type) {
    throw new ApiFailure('CONTRACT_VERSION_UNSUPPORTED', '事件合同、运行编号或事件游标不一致。');
  }
  if (BigInt(value.seq) <= BigInt(previous)) return null;
  if (BigInt(value.seq) !== BigInt(previous) + 1n) throw new ApiFailure('EVENT_GAP', '事件序号不连续，请重新载入运行快照。');
  return value;
}
