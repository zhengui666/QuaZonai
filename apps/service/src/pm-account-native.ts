import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import { mkdir, mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import type { Money } from '../../../packages/contracts/src/pm.ts';
import { addDecimal, compareDecimal } from '../../../packages/contracts/src/pm-validation.ts';
import { executeNative } from '../../research/plugins/native-portfolio.ts';
import { canonical } from '../../research/src/documents.ts';
import { counter, decimal, json, nowNs, object, Problem, text } from './facts.ts';
import type { CapturedAccount } from './pm-account.ts';

export interface NativeObservedSpotStart extends JsonObject {
  schema_version: 1; snapshot_evidence_ref: string; source_native_version: '0.63.0'; snapshot: JsonObject; available_ns: string;
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
export function toNativeObservedSpotStart(capture: CapturedAccount, options: { selectionStartNs: string }): NativeObservedSpotStart {
  const transport = object(capture.artifact.snapshot, 'captured_account_snapshot');
  const original = object(transport.nativeObservation, 'native_account_observation');
  exactKeys(original, ['schema_version', 'binding', 'sequence', 'dropped_events', 'observed_at_ns', 'connection', 'snapshot'], 'native_observation');
  if (original.schema_version !== 1 || original.connection !== 'CONNECTED') refuse('native_observation_unavailable');
  if (counter(original.dropped_events, 'dropped_events') !== '0') refuse('native_observation_has_gap');
  const binding = object(original.binding, 'native_binding');
  exactKeys(binding, ['schema_version', 'project_id', 'environment', 'native_trader_id', 'native_session_id', 'native_account_id', 'native_version'], 'native_binding');
  if (binding.schema_version !== 1 || binding.native_version !== '0.63.0') refuse('native_version_unsupported');
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
    && capitalPermission?.version === 1 && capitalPermission.readable === true;
  if (!readableControl || control.accountSourceId !== capture.basis.accountSourceId || control.observationId !== capture.basis.observationId
    || control.controlRevision !== capture.basis.controlRevision || transport.controlRevision !== capture.basis.controlRevision) refuse('capital_control_not_matched');
  if (!capture.basis.reserved || capture.basis.reserved.currency !== capture.basis.currency || compareDecimal(capture.basis.reserved.amount, '0') !== 0) refuse('capital_reserved_or_unknown');
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
  return { schema_version: 1, snapshot_evidence_ref: capture.evidence.id, source_native_version: '0.63.0', snapshot: json(native), available_ns: available,
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

export interface ObservedSpotComparisonInput {
  capture: CapturedAccount; nativeDatasetRevisionId: string; datasetRevision: { id: string; contentRef: string }; catalog: string; binary: NativeTarget; dataDir: string;
  operation: { kind: 'SIMULATION'; simulation: JsonObject } | { kind: 'STUDY'; study: JsonObject; objectsDirectory: string };
}
export interface ObservedSpotLeg { status: 'COMPLETED' | 'FAILED'; result: JsonObject | null; reason: string | null }
export interface ObservedSpotComparisonResult {
  request: JsonObject; original: JsonObject; hold: ObservedSpotLeg; rebalance: ObservedSpotLeg;
  datasetBinding: { datasetRevisionId: string; datasetContentRef: string; nativeDatasetRevisionId: string };
  accountEvidenceRef: string; limitations: string[];
}

export interface ObservedSpotValuationInput extends Omit<ObservedSpotComparisonInput, 'operation'> { selection: JsonObject; settings: JsonObject }
export interface ObservedSpotValuationResult {
  request: JsonObject; original: JsonObject; reportValue: Money; asOfNs: string; availableAtNs: string;
  datasetBinding: ObservedSpotComparisonResult['datasetBinding']; accountEvidenceRef: string;
}

/** Derived inventory value from original balances and original as-of prices.
 * Publication/receiver availability remains later than economic valuation time;
 * this result never overwrites the native source equity or cash fields.
 */
export async function valueObservedSpotAccount(input: ObservedSpotValuationInput, context: Context): Promise<ObservedSpotValuationResult> {
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(input.nativeDatasetRevisionId)) refuse('native_dataset_revision_uuid_required');
  const datasetBinding = { datasetRevisionId: text(input.datasetRevision.id, 'pm_dataset_revision'),
    datasetContentRef: text(input.datasetRevision.contentRef, 'pm_dataset_content_ref'), nativeDatasetRevisionId: input.nativeDatasetRevisionId };
  const initial = toNativeObservedSpotStart(input.capture, { selectionStartNs: input.capture.basis.availableAtNs });
  const policy = object(input.settings.multi_currency_spot_cash, 'explicit_multi_currency_spot_policy');
  if (policy.schema_version !== 1 || policy.mode !== 'MULTI_CURRENCY_CASH' || input.settings.account_kind !== 'CASH'
    || policy.report_currency !== input.capture.basis.currency || input.settings.base_currency !== input.capture.basis.currency) refuse('report_currency_or_cash_policy_mismatch');
  const asOfNs = counter(initial.snapshot.ts_event, 'native_snapshot_clock');
  if (counter(input.selection.decision_cutoff_ns, 'valuation_price_cutoff') !== asOfNs) refuse('valuation_price_cutoff_mismatch');
  const request = json({ schema_version: 1, dataset_revision_id: input.nativeDatasetRevisionId, initial_state: initial, selection: input.selection, settings: input.settings });
  const root = join(input.dataDir, 'scratch'); await mkdir(root, { recursive: true }); const directory = await mkdtemp(join(root, 'pm-observed-valuation-'));
  try {
    const original = await executeNative(input.binary, 'value-observed-spot', request, ['--catalog', input.catalog], directory, context);
    if (original.schema_version !== 1 || original.kind !== 'OBSERVED_SPOT_VALUATION' || original.basis !== 'DERIVED_REPORT_CURRENCY_INVENTORY_VALUE'
      || canonical(original.request!) !== canonical(request) || original.report_currency !== input.capture.basis.currency || original.asof_ns !== asOfNs) refuse('native_valuation_basis_mismatch');
    const total = decimal(original.total, 'native_report_value'); const sourceAvailable = counter(original.available_ns, 'native_valuation_available');
    if (compareDecimal(total, '0') < 0 || BigInt(sourceAvailable) < BigInt(initial.available_ns)) refuse('native_valuation_amount_or_clock_invalid');
    object(original.source_evidence, 'native_valuation_source_evidence');
    const balances = array(initial.snapshot.balances, 'native_balances').map(value => money(object(value).total));
    const legs = array(original.legs, 'native_valuation_legs').map(value => object(value));
    const seen = new Set<string>(); let sum = '0';
    for (const leg of legs) {
      const currency = text(leg.currency, 'valuation_currency'); const balance = balances.find(value => value.currency === currency);
      if (seen.has(currency) || !balance || compareDecimal(decimal(leg.native_total, 'native_balance'), balance.amount) !== 0) refuse('native_valuation_balance_mismatch');
      seen.add(currency); const reportValue = decimal(leg.report_value, 'native_report_leg');
      if (compareDecimal(reportValue, '0') < 0) refuse('native_valuation_amount_or_clock_invalid'); sum = addDecimal(sum, reportValue);
      for (const field of ['price_event_ns', 'price_available_ns']) if (leg[field] !== null && BigInt(counter(leg[field], field)) > BigInt(asOfNs)) refuse('native_valuation_future_price');
    }
    if (seen.size !== balances.length || compareDecimal(sum, total) !== 0) refuse('native_valuation_conservation_failed');
    const availableAtNs = nowNs(); if (BigInt(sourceAvailable) > BigInt(availableAtNs)) refuse('native_valuation_source_from_future');
    return { request, original, datasetBinding, reportValue: { amount: total, currency: input.capture.basis.currency }, asOfNs, availableAtNs, accountEvidenceRef: input.capture.evidence.id };
  } finally { await rm(directory, { recursive: true, force: true }); }
}

/** Run both candidates from one original observed balance state. The wrapper
 * validates its own output binding; no failed leg becomes an economic loser.
 * Study members/objects and the caller's scientific budget are admitted upstream.
 */
export async function compareObservedSpotAccount(input: ObservedSpotComparisonInput, context: Context): Promise<ObservedSpotComparisonResult> {
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(input.nativeDatasetRevisionId)) refuse('native_dataset_revision_uuid_required');
  const datasetBinding = { datasetRevisionId: text(input.datasetRevision.id, 'pm_dataset_revision'),
    datasetContentRef: text(input.datasetRevision.contentRef, 'pm_dataset_content_ref'), nativeDatasetRevisionId: input.nativeDatasetRevisionId };
  const study = input.operation.kind === 'STUDY';
  const operation = json(input.operation.kind === 'STUDY' ? input.operation.study : input.operation.simulation);
  // Both native request structs declare settlements with serde(default,
  // skip_serializing_if = "Vec::is_empty"). Emit that exact wire form before
  // comparing the echoed basis; nonempty payouts and all other fields retain
  // strict identity and the caller's original request is never rewritten.
  if (Array.isArray(operation.settlements) && operation.settlements.length === 0) delete operation.settlements;
  const selection = object(study ? operation.source_selection : operation.selection, 'native_selection');
  const settings = object(study ? operation.execution_settings : operation.settings, 'native_execution_settings');
  const policy = object(settings.multi_currency_spot_cash, 'explicit_multi_currency_spot_policy');
  if (policy.schema_version !== 1 || policy.mode !== 'MULTI_CURRENCY_CASH' || settings.account_kind !== 'CASH'
    || policy.report_currency !== input.capture.basis.currency || settings.base_currency !== input.capture.basis.currency) refuse('report_currency_or_cash_policy_mismatch');
  const start = counter(study ? operation.evaluation_start_ns : selection.event_start_ns, 'native_replay_start');
  const end = counter(selection.event_end_ns, 'native_replay_end'); if (BigInt(start) >= BigInt(end)) refuse('replay_interval_invalid');
  const initial = toNativeObservedSpotStart(input.capture, { selectionStartNs: start });
  const request = json({ schema_version: 1, dataset_revision_id: input.nativeDatasetRevisionId, initial_state: initial, [study ? 'study' : 'simulation']: operation });
  const root = join(input.dataDir, 'scratch'); await mkdir(root, { recursive: true }); const directory = await mkdtemp(join(root, 'pm-observed-spot-'));
  try {
    const args = ['--catalog', input.catalog]; if (input.operation.kind === 'STUDY') args.push('--objects', input.operation.objectsDirectory);
    const original = await executeNative(input.binary, study ? 'study-observed-spot' : 'compare-observed-spot', request, args, directory, context);
    if (original.schema_version !== 1 || original.kind !== (study ? 'OBSERVED_SPOT_STUDY_COMPARISON' : 'OBSERVED_SPOT_ACCOUNT_COMPARISON')
      || original.basis !== 'HISTORICAL_REPLAY_FROM_OBSERVED_SPOT_BALANCES' || canonical(original.request!) !== canonical(request)) refuse('native_comparison_basis_mismatch');
    const leg = (value: unknown, allowStudyDiagnostics = false): ObservedSpotLeg => {
      const item = object(value, 'native_comparison_leg');
      if (item.status === 'COMPLETED' && item.reason === null) return { status: 'COMPLETED', result: object(item.result, 'native_comparison_result'), reason: null };
      if (item.status === 'FAILED' && typeof item.reason === 'string' && item.reason.trim()
        && (item.result === null || allowStudyDiagnostics && typeof item.result === 'object' && !Array.isArray(item.result))) {
        return { status: 'FAILED', result: item.result === null ? null : object(item.result), reason: item.reason };
      }
      return refuse('native_comparison_leg_invalid');
    };
    return { request, original, datasetBinding, hold: leg(original.hold), rebalance: leg(original.rebalance, study), accountEvidenceRef: input.capture.evidence.id,
      limitations: ['historical_replay_from_observed_balances_is_not_forecast_or_realized_account_pnl', 'native_fee_acceptance_and_source_limitations_remain_authoritative',
        'new_snapshot_or_open_orders_invalidates_current_account_delivery_basis'] };
  } finally { await rm(directory, { recursive: true, force: true }); }
}
