import type { JsonObject } from '@earendil-works/pi-durable';
import { compareDecimal } from '../../../packages/contracts/src/pm-validation.ts';
import { counter, decimal, json, object, Problem, text } from './facts.ts';
import type { CapturedAccount } from './pm-account.ts';

export interface NativeObservedSpotStart extends JsonObject {
  schema_version: 1; snapshot_evidence_ref: string; source_native_version: '0.69.0'; snapshot: JsonObject; available_ns: string;
  open_orders_evidence_ref: string; open_orders_account_id: string; open_orders_asof_ns: string; open_orders_available_ns: string; open_orders_count: 0;
}
const refuse = (code: string): never => { throw new Problem(`pm_account_${code}`, 409); };
const array = (value: unknown, name: string): unknown[] => Array.isArray(value) ? value : refuse(`${name}_required`);
const exactKeys = (value: JsonObject, names: readonly string[], name: string) => {
  if (Object.keys(value).some(key => !names.includes(key)) || names.some(key => !(key in value))) refuse(`${name}_shape_invalid`);
};
function money(value: unknown): { amount: string; currency: string } {
  const item = object(value, 'native_money'); exactKeys(item, ['amount', 'currency'], 'native_money');
  return { amount: decimal(item.amount, 'native_money'), currency: text(item.currency, 'native_currency') };
}

/** Lossless adapter for an actually received official source observation.
 * The old equity/cash/positions transport cannot reconstruct native balances,
 * prove no pending orders, or move a current account into past validation.
 */
export function toNativeObservedSpotStart(capture: CapturedAccount, options: { selectionStartNs: string; reservationMode?: 'SEPARATE_TARGET_BUDGET' }): NativeObservedSpotStart {
  const transport = object(capture.artifact.snapshot, 'captured_account_snapshot');
  const original = object(transport.nativeObservation, 'native_account_observation');
  exactKeys(original, ['schema_version', 'binding', 'sequence', 'dropped_events', 'observed_at_ns', 'connection', 'snapshot'], 'native_observation');
  if (original.schema_version !== 1 || original.connection !== 'CONNECTED') refuse('native_observation_unavailable');
  if (counter(original.dropped_events, 'dropped_events') !== '0') refuse('native_observation_has_gap');
  const binding = object(original.binding, 'native_binding');
  exactKeys(binding, ['schema_version', 'project_id', 'environment', 'native_trader_id', 'native_session_id', 'native_account_id', 'native_version'], 'native_binding');
  if (binding.schema_version !== 1 || binding.native_version !== '0.69.0') refuse('native_version_unsupported');
  for (const field of ['project_id', 'native_trader_id', 'native_session_id', 'native_account_id']) text(binding[field], field);
  const mode = object(capture.artifact.observation).mode;
  if (binding.environment !== (mode === 'paper' ? 'PAPER' : mode === 'live' ? 'LIVE' : null)) refuse('native_environment_mismatch');
  const native = object(original.snapshot, 'native_snapshot');
  exactKeys(native, ['account_id', 'account_type', 'base_currency', 'balances', 'margins', 'unrealized_pnls', 'realized_pnls', 'total_equity',
    'base_currency_equity', 'is_stale', 'stale_instruments', 'stale_currencies', 'unpriced_instruments', 'event_id', 'ts_event', 'ts_init'], 'native_snapshot');
  if (native.account_id !== binding.native_account_id) refuse('native_account_identity_mismatch');
  if (native.account_type !== 'CASH' || native.base_currency !== null) refuse('native_multi_currency_cash_required');
  if (native.is_stale !== false || ['margins', 'stale_instruments', 'stale_currencies', 'unpriced_instruments'].some(key => array(native[key], key).length !== 0)) refuse('native_state_not_complete');
  text(native.event_id, 'native_event_id');
  const sequence = counter(original.sequence, 'native_sequence');
  const nativeClock = counter(native.ts_event, 'native_snapshot_clock');
  const nativeReceived = counter(native.ts_init, 'native_snapshot_received_clock');
  const heartbeat = counter(original.observed_at_ns, 'native_observed_clock');
  if (sequence !== capture.basis.sequence || transport.sequence !== sequence || nativeClock !== capture.basis.observedAtNs || transport.nativeClockNs !== nativeClock) refuse('native_snapshot_identity_mismatch');
  if (BigInt(nativeClock) > BigInt(nativeReceived) || BigInt(heartbeat) < BigInt(nativeReceived)) refuse('native_clock_order_invalid');
  const received = counter(capture.artifact.snapshotReceivedAtNs, 'snapshot_received_clock');
  const controlReceived = counter(capture.artifact.capitalControlReceivedAtNs, 'capital_control_received_clock');
  const capabilityReceived = capture.artifact.capitalCapabilities === undefined ? null
    : counter(object(capture.artifact.capitalCapabilities, 'capital_capabilities').receivedAtNs, 'capital_capabilities_received_clock');
  const available = [received, controlReceived, ...(capabilityReceived === null ? [] : [capabilityReceived])]
    .reduce((left, right) => BigInt(left) > BigInt(right) ? left : right);
  const start = counter(options.selectionStartNs, 'replay_start');
  if (available !== capture.basis.availableAtNs || available !== capture.evidence.availableAtNs || BigInt(heartbeat) > BigInt(received)
    || BigInt(nativeClock) > BigInt(received) || BigInt(available) > BigInt(start)) refuse('snapshot_not_available_before_replay');
  if (transport.accountSourceId !== capture.basis.accountSourceId || transport.observationId !== capture.basis.observationId) refuse('source_identity_required');
  const control = object(capture.artifact.capitalControl, 'capital_control');
  const capabilityDocument = capture.artifact.capitalCapabilities ? object(object(capture.artifact.capitalCapabilities).document) : null;
  const capitalPermission = capabilityDocument?.capitalExit ? object(capabilityDocument.capitalExit) : null;
  const readableControl = control.capability === 'SUPPORTED' || control.capability === 'UNSUPPORTED' && control.reason === 'capital_exit_initiation_unavailable'
    && capitalPermission?.version === 2 && capitalPermission.readable === true;
  if (!readableControl || control.accountSourceId !== capture.basis.accountSourceId || control.observationId !== capture.basis.observationId
    || control.controlRevision !== capture.basis.controlRevision || transport.controlRevision !== capture.basis.controlRevision) refuse('capital_control_not_matched');
  // Reservations are a control constraint, not an invented native balance.
  // The raw observed account is retained for inference; target admission carries
  // its separate managed budget and the receiver prevents spending the reserve.
  if (!capture.basis.reserved || capture.basis.reserved.currency !== capture.basis.currency
    || compareDecimal(capture.basis.reserved.amount, '0') < 0
    || options.reservationMode !== 'SEPARATE_TARGET_BUDGET' && compareDecimal(capture.basis.reserved.amount, '0') !== 0) refuse('capital_reserved_or_unknown');
  const balances = array(native.balances, 'native_balances'); if (!balances.length) refuse('native_balances_required');
  const currencies = new Set<string>();
  for (const value of balances) {
    const balance = object(value, 'native_balance'); exactKeys(balance, ['total', 'locked', 'free'], 'native_balance');
    const total = money(balance.total), locked = money(balance.locked), free = money(balance.free);
    if (currencies.has(total.currency) || total.currency !== locked.currency || total.currency !== free.currency) refuse('native_balance_currency_conflict');
    currencies.add(total.currency);
    if (compareDecimal(total.amount, '0') < 0 || compareDecimal(locked.amount, '0') !== 0 || compareDecimal(total.amount, free.amount) !== 0) refuse('locked_or_borrowed_balance_unsupported');
  }
  for (const field of ['unrealized_pnls', 'realized_pnls', 'total_equity']) for (const value of array(native[field], field)) money(value);
  if (native.base_currency_equity !== null) money(native.base_currency_equity);
  const orders = object(transport.openOrders, 'open_orders_report');
  exactKeys(orders, ['schema_version', 'account_id', 'asof_ns', 'available_ns', 'count'], 'open_orders');
  if (orders.schema_version !== 1 || orders.count !== 0) refuse('no_open_orders_proof_required');
  const orderClock = counter(orders.asof_ns, 'open_orders_clock'); const orderAvailable = counter(orders.available_ns, 'open_orders_available');
  if (orders.account_id !== native.account_id || orderClock !== nativeClock) refuse('open_orders_basis_mismatch');
  if (BigInt(orderAvailable) < BigInt(orderClock) || BigInt(orderAvailable) > BigInt(received)) refuse('open_orders_clock_invalid');
  return { schema_version: 1, snapshot_evidence_ref: capture.evidence.id, source_native_version: binding.native_version as '0.69.0', snapshot: json(native), available_ns: available,
    open_orders_evidence_ref: capture.evidence.id, open_orders_account_id: String(orders.account_id), open_orders_asof_ns: orderClock,
    open_orders_available_ns: received, open_orders_count: 0 };
}

/** A zero normalized position list cannot hide a non-report-currency balance.
 * Missing raw balances or outstanding-order evidence leaves retirement open.
 */
export function retirementAccountFlat(capture: CapturedAccount): { flat: boolean | null; evidenceRefs: string[]; reasonCodes: string[] } {
  try {
    const initial = toNativeObservedSpotStart(capture, { selectionStartNs: capture.basis.availableAtNs });
    const snapshot = object(capture.artifact.snapshot);
    const positions = array(snapshot.positions, 'position_quantities');
    const positionResidual = positions.some(value => compareDecimal(decimal(object(value).quantity, 'position_quantity'), '0') !== 0);
    const balanceResidual = array(initial.snapshot.balances, 'native_balances').some(value => {
      const total = money(object(value).total); return total.currency !== capture.basis.currency && compareDecimal(total.amount, '0') !== 0;
    });
    return { flat: !positionResidual && !balanceResidual, evidenceRefs: [capture.evidence.id], reasonCodes: [positionResidual || balanceResidual ? 'original_account_has_residual_exposure' : 'original_balances_positions_and_zero_orders_confirm_flat'] };
  } catch (error) {
    return { flat: null, evidenceRefs: [capture.evidence.id], reasonCodes: [error instanceof Error ? error.message : 'account_flatness_evidence_unavailable'] };
  }
}
