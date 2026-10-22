import type { JsonObject } from '@earendil-works/pi-durable';
import type { EvidenceRef, Interval, Money } from '../../../packages/contracts/src/pm.ts';
import { compareDecimal } from '../../../packages/contracts/src/pm-validation.ts';
import { counter, decimal, json, object, Problem, text } from './facts.ts';
import { digest, identity } from './pm-facts.ts';
import type { CapturedAccount } from './pm-account.ts';

export interface AccountCashFlow {
  id: string; eventAtNs: string; availableAtNs: string; currency: string; amount: string;
  kind: 'DEPOSIT' | 'WITHDRAWAL' | 'TRANSFER' | 'CORRECTION';
}
export interface AccountExecution {
  id: string; eventAtNs: string; availableAtNs: string; instrumentId: string; orderId: string; clientTargetId: string | null;
  side: 'BUY' | 'SELL'; quantity: string; price: string; currency: string; fee: Money | null;
}
export interface AccountEventIdentity {
  accountSourceId: string; nativeAccountId: string; kind: 'CASH_FLOW' | 'EXECUTION'; id: string; contentDigest: string;
}
export interface CapturedAccountEvents {
  accountSourceId: string; nativeAccountId: string; interval: Interval;
  /** Original source publication and actual local availability are distinct. */
  sourceAvailableAtNs: string; receivedAtNs: string;
  complete: { cashFlows: boolean; executions: boolean };
  cashFlows: AccountCashFlow[]; executions: AccountExecution[];
  identities: AccountEventIdentity[]; newEventKeys: string[];
  evidence: EvidenceRef; artifact: JsonObject;
}

const refuse = (code: string): never => { throw new Problem(`pm_account_events_${code}`, 409); };
function fields(value: JsonObject, expected: readonly string[], name: string): void {
  if (Object.keys(value).some(key => !expected.includes(key)) || expected.some(key => !(key in value))) refuse(`${name}_shape_invalid`);
}
export function accountEventKey(value: Pick<AccountEventIdentity, 'accountSourceId' | 'nativeAccountId' | 'kind' | 'id'>): string {
  return identity('account-event', [value.accountSourceId, value.nativeAccountId, value.kind, value.id]);
}

/** Project an explicitly complete/partial source envelope, never infer events
 * from a balance delta. Missing legacy envelopes return null, not empty facts.
 * Event availableAtNs is the original publication clock; consumers must also
 * require evidence.availableAtNs (actual capture availability) at their cutoff.
 * Prior identities let durable consumers reject conflicting re-publications;
 * identical retries keep one event and do not acquire a second newEventKey.
 */
export function projectAccountEvents(capture: CapturedAccount, prior: readonly AccountEventIdentity[] = []): CapturedAccountEvents | null {
  const snapshot = object(capture.artifact.snapshot, 'captured_account_snapshot');
  if (snapshot.accountEvents === undefined || snapshot.accountEvents === null) return null;
  const envelope = object(snapshot.accountEvents, 'account_events');
  fields(envelope, ['schemaVersion', 'accountSourceId', 'nativeAccountId', 'interval', 'availableAtNs', 'complete', 'cashFlows', 'executions'], 'envelope');
  if (envelope.schemaVersion !== 1) refuse('schema_unsupported');
  const source = text(envelope.accountSourceId, 'events_account_source'); const account = text(envelope.nativeAccountId, 'events_native_account');
  const original = object(snapshot.nativeObservation, 'events_native_observation'); const binding = object(original.binding, 'events_native_binding');
  const native = object(original.snapshot, 'events_native_snapshot');
  if (source !== capture.basis.accountSourceId || snapshot.accountSourceId !== source || account !== binding.native_account_id || account !== native.account_id
    || String(original.sequence) !== capture.basis.sequence || snapshot.sequence !== capture.basis.sequence || snapshot.observationId !== capture.basis.observationId
    || original.connection !== 'CONNECTED' || counter(original.dropped_events, 'events_dropped_events') !== '0') refuse('account_identity_or_continuity_mismatch');
  const intervalRaw = object(envelope.interval, 'events_interval'); fields(intervalRaw, ['startNs', 'endExclusiveNs'], 'interval');
  const interval = { startNs: counter(intervalRaw.startNs, 'events_start'), endExclusiveNs: counter(intervalRaw.endExclusiveNs, 'events_end') };
  const sourceAvailableAtNs = counter(envelope.availableAtNs, 'events_available');
  const receivedAtNs = counter(capture.artifact.snapshotReceivedAtNs, 'events_received');
  const nativeEvent = counter(native.ts_event, 'events_native_asof'), nativePublished = counter(native.ts_init, 'events_native_published');
  const nativeHeartbeat = counter(original.observed_at_ns, 'events_native_heartbeat');
  const mode = object(capture.artifact.observation).mode;
  if (original.schema_version !== 1 || binding.schema_version !== 1 || binding.native_version !== '0.63.0'
    || binding.environment !== (mode === 'paper' ? 'PAPER' : mode === 'live' ? 'LIVE' : null)
    || nativeEvent !== capture.basis.observedAtNs || snapshot.nativeClockNs !== nativeEvent
    || BigInt(nativeEvent) > BigInt(nativePublished) || BigInt(nativePublished) > BigInt(nativeHeartbeat) || BigInt(nativeHeartbeat) > BigInt(receivedAtNs)) refuse('native_clock_or_version_invalid');
  if (BigInt(interval.startNs) >= BigInt(interval.endExclusiveNs) || BigInt(interval.endExclusiveNs) > BigInt(capture.basis.observedAtNs) + 1n
    || BigInt(interval.endExclusiveNs) > BigInt(sourceAvailableAtNs) + 1n || BigInt(sourceAvailableAtNs) > BigInt(receivedAtNs)
    || BigInt(receivedAtNs) > BigInt(capture.evidence.availableAtNs)) refuse('envelope_clock_invalid');
  const completeness = object(envelope.complete, 'events_completeness'); fields(completeness, ['cashFlows', 'executions'], 'complete');
  if (typeof completeness.cashFlows !== 'boolean' || typeof completeness.executions !== 'boolean') refuse('completeness_unknown');
  const complete = { cashFlows: completeness.cashFlows as boolean, executions: completeness.executions as boolean };
  const rawCashFlows = envelope.cashFlows, rawExecutions = envelope.executions;
  if (!Array.isArray(rawCashFlows) || !Array.isArray(rawExecutions)) return refuse('event_arrays_required');
  const previous = new Map<string, string>();
  for (const item of prior) {
    const key = accountEventKey(item); const old = previous.get(key);
    if (old !== undefined && old !== item.contentDigest) refuse('prior_identity_conflict');
    previous.set(key, item.contentDigest);
  }
  const current = new Map<string, string>(); const identities: AccountEventIdentity[] = []; const newEventKeys: string[] = [];
  const register = (kind: AccountEventIdentity['kind'], event: AccountCashFlow | AccountExecution): boolean => {
    const item: AccountEventIdentity = { accountSourceId: source, nativeAccountId: account, kind, id: event.id, contentDigest: digest(event) };
    const key = accountEventKey(item); const old = previous.get(key); const duplicate = current.get(key);
    if (old !== undefined && old !== item.contentDigest || duplicate !== undefined && duplicate !== item.contentDigest) refuse('event_identity_conflict');
    if (duplicate !== undefined) return false;
    current.set(key, item.contentDigest); identities.push(item); if (old === undefined) newEventKeys.push(key); return true;
  };
  const eventClock = (item: JsonObject) => {
    const eventAtNs = counter(item.eventAtNs, 'event_clock'), availableAtNs = counter(item.availableAtNs, 'event_available');
    if (BigInt(eventAtNs) < BigInt(interval.startNs) || BigInt(eventAtNs) >= BigInt(interval.endExclusiveNs)
      || BigInt(availableAtNs) < BigInt(eventAtNs) || BigInt(availableAtNs) > BigInt(sourceAvailableAtNs)) refuse('event_clock_invalid');
    return { id: text(item.id, 'event_id'), eventAtNs, availableAtNs };
  };
  const cashFlows: AccountCashFlow[] = [];
  for (const value of rawCashFlows) {
    const item = object(value, 'cash_flow'); fields(item, ['id', 'eventAtNs', 'availableAtNs', 'currency', 'amount', 'kind'], 'cash_flow');
    if (!['DEPOSIT', 'WITHDRAWAL', 'TRANSFER', 'CORRECTION'].includes(String(item.kind))) refuse('cash_flow_kind_invalid');
    const event: AccountCashFlow = { ...eventClock(item), currency: text(item.currency, 'cash_flow_currency'), amount: decimal(item.amount, 'cash_flow_amount'), kind: item.kind as AccountCashFlow['kind'] };
    if (event.kind === 'DEPOSIT' && compareDecimal(event.amount, '0') <= 0 || event.kind === 'WITHDRAWAL' && compareDecimal(event.amount, '0') >= 0) refuse('cash_flow_sign_invalid');
    if (register('CASH_FLOW', event)) cashFlows.push(event);
  }
  const executions: AccountExecution[] = [];
  for (const value of rawExecutions) {
    const item = object(value, 'account_execution'); fields(item, ['id', 'eventAtNs', 'availableAtNs', 'instrumentId', 'orderId', 'clientTargetId', 'side', 'quantity', 'price', 'currency', 'fee'], 'execution');
    if (item.side !== 'BUY' && item.side !== 'SELL') refuse('execution_side_invalid');
    const fee = item.fee === null ? null : object(item.fee, 'execution_fee'); if (fee) fields(fee, ['amount', 'currency'], 'fee');
    const event: AccountExecution = { ...eventClock(item), instrumentId: text(item.instrumentId, 'execution_instrument'), orderId: text(item.orderId, 'execution_order'),
      clientTargetId: item.clientTargetId === null ? null : text(item.clientTargetId, 'execution_target'), side: item.side as AccountExecution['side'],
      quantity: decimal(item.quantity, 'execution_quantity'), price: decimal(item.price, 'execution_price'), currency: text(item.currency, 'execution_currency'),
      fee: fee === null ? null : { amount: decimal(fee.amount, 'execution_fee'), currency: text(fee.currency, 'execution_fee_currency') } };
    if (compareDecimal(event.quantity, '0') <= 0 || compareDecimal(event.price, '0') <= 0) refuse('execution_amount_invalid');
    if (register('EXECUTION', event)) executions.push(event);
  }
  const artifact = json({ accountEvidenceRef: capture.evidence.id, envelope, receivedAtNs });
  const contentDigest = digest(artifact); const sourceRevision = identity('account-events', artifact);
  const evidence: EvidenceRef = { id: identity('evidence', sourceRevision), sourceKind: 'ACCOUNT', role: 'OBSERVED', runtime: null,
    objectKey: sourceRevision, contentDigest, mediaType: 'application/json', sourceRevision, eventInterval: interval,
    availableAtNs: capture.evidence.availableAtNs, clockPrecision: 'NANOSECOND', schemaId: 'qz.account-events/1', visibility: 'OWNER_ONLY' };
  return { accountSourceId: source, nativeAccountId: account, interval, sourceAvailableAtNs, receivedAtNs, complete,
    cashFlows, executions, identities, newEventKeys, evidence, artifact };
}
