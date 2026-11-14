/** Account-owner capital preparation. Transfers are always performed externally. */
export const CAPITAL_EXIT_VERSION = 2 as const;
export type CapitalExitCapability = 'SUPPORTED' | 'UNSUPPORTED' | 'BLOCKED';
export type CapitalExitAction = 'START' | 'PAUSE' | 'CANCEL' | 'RESUME' | 'RELEASE_RESERVATION' | 'RECONCILE_EXTERNAL_TRANSFER' | 'RECONCILE_WITHDRAWAL';
export type CapitalExitState = 'REQUESTED' | 'FENCING' | 'CANCELLING_OPENERS' | 'WAITING_CASH' | 'REDUCING' | 'WAITING_EVIDENCE' | 'READY' | 'PAUSING' | 'PAUSED' | 'RECOVERING' | 'CANCELLING_EXIT' | 'CANCELLED_RESERVED' | 'RECONCILING_WITHDRAWAL' | 'COMPLETED' | 'CLOSED' | 'BLOCKED';
export type CapitalPreparationState = 'WAITING_CASH' | 'REDUCING' | 'WAITING_EXTERNAL_TRANSFER' | 'READY' | 'STALE' | 'BLOCKED' | 'CLOSED';
export interface CapitalMoney { amount: string; currency: string }
export interface CapitalExitReductionLeg { instrument_id: string; maximum_reduction_quantity: string; minimum_sell_price: string }
export interface CapitalExitPacing { minimum_interval_seconds: number; maximum_slice_notional: string; maximum_participation: string }
export type CapitalExitPolicy = { kind: 'CASH_ONLY' } | { kind: 'NATURAL_RELEASE'; deadline: string } | {
  kind: 'BOUNDED_LIMIT'; deadline: string; legs: CapitalExitReductionLeg[];
  max_execution_cost: CapitalMoney & { reference_evidence_id: string }; pacing: CapitalExitPacing;
};
export interface CapitalExitScope { kind: 'AMOUNT'; amount: string; currency: string }
export interface CapitalExitActionAvailability { allowed: boolean; reason: string | null }
export type CapitalExitAvailableActions = Partial<Record<CapitalExitAction, CapitalExitActionAvailability>>;
export interface CapitalExitNativeOrderRef { native_client_order_id: string; native_strategy_id: string; native_instrument_id: string; original_event_refs?: string[] }
export interface CapitalExitInstrument {
  instrument_id: string; symbol: string; quantity: string; eligible_quantity: string; quantity_increment: string; price_increment: string;
  bid_price: string | null; ask_price: string | null; quote_asof: string | null; fee_rate: string | null; reference_evidence_id: string;
}
export interface CapitalExitPlanningContext {
  schema_version: 2; id: string; account_source_id: string; observation_id: string; account_control_revision: string;
  currency: string; environment: 'PAPER' | 'LIVE'; asof: string; valid_until: string;
  funds: { total_cash: CapitalMoney | null; free_cash: CapitalMoney | null; locked_cash: CapitalMoney | null; verified_withdrawable: CapitalMoney | null };
  instruments: CapitalExitInstrument[]; capability: CapitalExitCapability; reason_codes: string[];
  supported_policies: CapitalExitPolicy['kind'][];
}
export interface CapitalExitPreviewRequest {
  schema_version: 2; account_source_id: string; expected_source_observation_id: string; expected_account_control_revision: string;
  planning_context_id: string; scope: CapitalExitScope; policy: CapitalExitPolicy; resume_exit_id?: string; expected_exit_revision?: string;
}
export interface CapitalExitPreview {
  schema_version: 2; id: string; plan_artifact_id: string; account_source_id: string; original_observation_id: string;
  expected_account_control_revision: string | null; environment: 'PAPER' | 'LIVE'; scope: CapitalExitScope; policy: CapitalExitPolicy;
  capability: CapitalExitCapability; reason_codes: string[]; valid_until: string; resume_exit_id?: string; expected_exit_revision?: string;
  funds: { requested: CapitalMoney; remaining_managed_capital: CapitalMoney | null; estimated_execution_cost: CapitalMoney | null; existing_unrealized_pnl?: CapitalMoney | null;
    native_total_cash?: CapitalMoney | null; native_free_cash?: CapitalMoney | null; native_locked_cash?: CapitalMoney | null };
  proposed_cancellations: CapitalExitNativeOrderRef[]; retained_protective_orders: CapitalExitNativeOrderRef[]; reduction_legs: CapitalExitReductionLeg[];
  [key: string]: unknown;
}
export interface CapitalExitProgress {
  instrument_id: string; authorized_quantity: string; filled_quantity: string; remaining_quantity: string;
  active_order_ids: string[]; execution_cost: CapitalMoney | null; actual_fees?: CapitalMoney | null;
}
export interface CapitalExitEvent { id: string; at: string; kind: string; message: string; reference: string | null }
export interface CapitalExitView {
  schema_version: 2; id: string; revision: string; account_source_id: string; environment: 'PAPER' | 'LIVE'; state: CapitalExitState; preparation_state: CapitalPreparationState;
  account_control_revision: string; command_id: string; reason_codes: string[]; policy: CapitalExitPolicy; scope: CapitalExitScope;
  remaining_trading: 'FENCED_PENDING_TARGET' | 'BUDGET_AWARE';
  funds: { requested_amount: CapitalMoney; reserved_amount: CapitalMoney; staged_for_withdrawal_amount: CapitalMoney; waived_amount: CapitalMoney;
    released_cash_amount: CapitalMoney | null; verified_withdrawable_amount: CapitalMoney | null; reconciled_withdrawal_amount: CapitalMoney;
    withdrawability: 'UNVERIFIED' | 'VERIFIED' | 'STALE' | 'SIMULATED'; evidence_asof?: string | null; evidence_valid_until?: string | null; actual_fees?: CapitalMoney | null };
  action_availability: CapitalExitAvailableActions; progress: CapitalExitProgress[]; events: CapitalExitEvent[];
  created_at: string; updated_at: string; [key: string]: unknown;
}
export interface CapitalExitCommandReceipt { requestId: string; state: 'PENDING' | 'UNCERTAIN' | 'ACCEPTED' | 'REJECTED'; error: string | null; result?: unknown }
export interface CapitalExitStatus {
  capability: CapitalExitCapability; reason: string | null; accountSourceId: string | null; observationId: string | null;
  controlRevision: string | null; reserved: string | null; exits: CapitalExitView[]; commands?: CapitalExitCommandReceipt[];
  unsettledReservations?: { requestId: string; state: CapitalExitCommandReceipt['state']; reserved: string | null }[];
  supportedPolicies?: CapitalExitPolicy['kind'][]; availableActions?: CapitalExitAction[];
}
type ExistingCapitalExitCommand = { requestId: string; exitId: string; expectedRevision: string };
export type CapitalExitCommandInput = { requestId: string; action: 'START'; previewId: string }
  | (ExistingCapitalExitCommand & { action: 'PAUSE' | 'CANCEL' | 'RELEASE_RESERVATION' })
  | (ExistingCapitalExitCommand & { action: 'RESUME'; previewId: string })
  | (ExistingCapitalExitCommand & { action: 'RECONCILE_EXTERNAL_TRANSFER' | 'RECONCILE_WITHDRAWAL'; amount: string; currency: string; externalTransferRef: string });
export interface CapitalExitPreviewInput { requestId: string; amount: string; currency: string; contextId: string; policy: CapitalExitPolicy; resumeExitId?: string; expectedExitRevision?: string }
export interface CapitalExitStart { schema_version: 2; preview_id: string; expected_account_control_revision: string; acknowledged_plan_artifact_id: string; expected_source_observation_id: string }
export type CapitalExitNativeAction = { schema_version: 2; expected_revision: string; action: Exclude<CapitalExitAction, 'START'>;
  preview_id?: string; user_reported_amount?: string; currency?: string; external_transfer_ref?: string };
export interface CapitalExitCapabilities {
  version: 2; supported: boolean; readable: boolean; reason?: string;
  policies: CapitalExitPolicy['kind'][]; actions: CapitalExitAction[];
}

function record(value: unknown, name: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(name + '_object_required');
  return value as Record<string, unknown>;
}
function string(value: unknown, name: string): string {
  if (typeof value !== 'string' || !value.trim()) throw new Error(name + '_required');
  return value;
}
function oneOf(value: unknown, options: readonly string[], name: string) {
  if (typeof value !== 'string' || !options.includes(value)) throw new Error(name + '_invalid');
}
function keys(value: Record<string, unknown>, allowed: string[], name: string) {
  if (Object.keys(value).some(key => !allowed.includes(key))) throw new Error(name + '_unknown_field');
}
export function capitalDecimalUnits(value: unknown, name = 'capital_amount'): bigint {
  if (typeof value !== 'string' || value.length > 64 || !/^-?\d+(?:\.\d{1,18})?$/.test(value)) throw new Error(name + '_decimal_required');
  const negative = value.startsWith('-'); const [integer, fraction = ''] = (negative ? value.slice(1) : value).split('.');
  const units = BigInt(integer + fraction.padEnd(18, '0')) * (negative ? -1n : 1n);
  if (units <= -(10n ** 38n) || units >= 10n ** 38n) throw new Error(name + '_out_of_range');
  return units;
}
function amount(value: unknown, name: string, positive = false) {
  const units = capitalDecimalUnits(value, name); if (positive ? units <= 0n : units < 0n) throw new Error(name + '_invalid');
}
function revision(value: unknown, name: string) {
  if (typeof value !== 'string' || !/^[1-9]\d*$/.test(value) || BigInt(value) > 9223372036854775807n) throw new Error(name + '_invalid');
}
function timestamp(value: unknown, name: string) { if (typeof value !== 'string' || !Number.isFinite(Date.parse(value))) throw new Error(name + '_invalid'); }
function money(value: unknown, name: string, currency?: string) {
  const v = record(value, name); amount(v.amount, name); string(v.currency, 'currency'); if (currency && v.currency !== currency) throw new Error(name + '_currency_mismatch');
}
function leg(value: unknown) {
  const v = record(value, 'reduction_leg'); keys(v, ['instrument_id', 'maximum_reduction_quantity', 'minimum_sell_price'], 'reduction_leg');
  string(v.instrument_id, 'instrument_id'); amount(v.maximum_reduction_quantity, 'maximum_reduction_quantity', true); amount(v.minimum_sell_price, 'minimum_sell_price', true);
}
export function validateCapitalPolicy(value: unknown): asserts value is CapitalExitPolicy {
  const v = record(value, 'capital_policy'); oneOf(v.kind, ['CASH_ONLY', 'NATURAL_RELEASE', 'BOUNDED_LIMIT'], 'capital_policy');
  if (v.kind === 'CASH_ONLY') { keys(v, ['kind'], 'capital_policy'); return; }
  timestamp(v.deadline, 'deadline');
  if (v.kind === 'NATURAL_RELEASE') { keys(v, ['kind', 'deadline'], 'capital_policy'); return; }
  keys(v, ['kind', 'deadline', 'legs', 'max_execution_cost', 'pacing'], 'capital_policy');
  if (!Array.isArray(v.legs) || !v.legs.length) throw new Error('capital_reduction_legs_required');
  v.legs.forEach(leg); if (new Set(v.legs.map(x => record(x, 'leg').instrument_id)).size !== v.legs.length) throw new Error('capital_reduction_leg_duplicate');
  const cost = record(v.max_execution_cost, 'execution_cost'); money(cost, 'execution_cost'); string(cost.reference_evidence_id, 'reference_evidence_id');
  const pacing = record(v.pacing, 'pacing'); keys(pacing, ['minimum_interval_seconds', 'maximum_slice_notional', 'maximum_participation'], 'pacing');
  if (!Number.isSafeInteger(pacing.minimum_interval_seconds) || Number(pacing.minimum_interval_seconds) < 1) throw new Error('capital_interval_invalid');
  amount(pacing.maximum_slice_notional, 'maximum_slice_notional', true); const participation = capitalDecimalUnits(pacing.maximum_participation, 'maximum_participation');
  if (participation <= 0n || participation > 10n ** 18n) throw new Error('maximum_participation_invalid');
}
function scope(value: unknown) { const v = record(value, 'scope'); keys(v, ['kind', 'amount', 'currency'], 'scope'); oneOf(v.kind, ['AMOUNT'], 'scope'); amount(v.amount, 'scope_amount', true); string(v.currency, 'currency'); }
function policyCurrency(value: unknown, currency: unknown) {
  const policy = record(value, 'capital_policy');
  if (policy.kind === 'BOUNDED_LIMIT' && record(policy.max_execution_cost, 'execution_cost').currency !== currency) throw new Error('capital_execution_cost_currency_mismatch');
}
/** Shared boundary parser: no science process, order engine, or financial side effect. */
export function validateCapitalDocument(kind: string, value: unknown): void {
  const v = record(value, 'capital_document'); if (v.schema_version !== CAPITAL_EXIT_VERSION) throw new Error('capital_schema_version_invalid');
  if (kind === 'REQUEST') {
    keys(v, ['schema_version', 'account_source_id', 'expected_source_observation_id', 'expected_account_control_revision', 'planning_context_id', 'scope', 'policy', 'resume_exit_id', 'expected_exit_revision'], 'capital_request');
    for (const name of ['account_source_id', 'expected_source_observation_id', 'planning_context_id']) string(v[name], name);
    revision(v.expected_account_control_revision, 'account_control_revision'); scope(v.scope); validateCapitalPolicy(v.policy); policyCurrency(v.policy, record(v.scope, 'scope').currency);
    if (v.resume_exit_id !== undefined || v.expected_exit_revision !== undefined) { string(v.resume_exit_id, 'resume_exit_id'); revision(v.expected_exit_revision, 'expected_exit_revision'); }
    return;
  }
  if (kind === 'START') {
    keys(v, ['schema_version', 'preview_id', 'expected_account_control_revision', 'acknowledged_plan_artifact_id', 'expected_source_observation_id'], 'capital_start');
    for (const name of ['preview_id', 'acknowledged_plan_artifact_id', 'expected_source_observation_id']) string(v[name], name);
    revision(v.expected_account_control_revision, 'account_control_revision'); return;
  }
  if (kind === 'ACTION') {
    oneOf(v.action, ['PAUSE', 'CANCEL', 'RESUME', 'RELEASE_RESERVATION', 'RECONCILE_EXTERNAL_TRANSFER', 'RECONCILE_WITHDRAWAL'], 'capital_action'); revision(v.expected_revision, 'exit_revision');
    const extra = v.action === 'RESUME' ? ['preview_id'] : ['RECONCILE_EXTERNAL_TRANSFER', 'RECONCILE_WITHDRAWAL'].includes(String(v.action)) ? ['user_reported_amount', 'currency', 'external_transfer_ref'] : [];
    keys(v, ['schema_version', 'expected_revision', 'action', ...extra], 'capital_action');
    if (v.action === 'RESUME') string(v.preview_id, 'preview_id');
    if (['RECONCILE_EXTERNAL_TRANSFER', 'RECONCILE_WITHDRAWAL'].includes(String(v.action))) { amount(v.user_reported_amount, 'withdrawal_amount', true); string(v.currency, 'currency'); string(v.external_transfer_ref, 'external_transfer_ref'); }
    return;
  }
  if (kind === 'CONTEXT') {
    for (const name of ['id', 'account_source_id', 'observation_id', 'currency']) string(v[name], name);
    revision(v.account_control_revision, 'account_control_revision'); timestamp(v.asof, 'asof'); timestamp(v.valid_until, 'valid_until');
    oneOf(v.environment, ['PAPER', 'LIVE'], 'environment'); oneOf(v.capability, ['SUPPORTED', 'UNSUPPORTED', 'BLOCKED'], 'capability');
    if (!Array.isArray(v.instruments) || !Array.isArray(v.supported_policies) || !Array.isArray(v.reason_codes)) throw new Error('capital_context_invalid');
    const funds = record(v.funds, 'funds'); for (const name of ['total_cash', 'free_cash', 'locked_cash', 'verified_withdrawable']) if (funds[name] != null) money(funds[name], name, String(v.currency));
    for (const row of v.instruments) { const r = record(row, 'instrument'); string(r.instrument_id, 'instrument_id'); string(r.reference_evidence_id, 'reference_evidence_id'); for (const name of ['quantity', 'eligible_quantity']) amount(r[name], name); for (const name of ['quantity_increment', 'price_increment']) amount(r[name], name, true); }
    return;
  }
  if (kind === 'PREVIEW') {
    for (const name of ['id', 'plan_artifact_id', 'account_source_id', 'original_observation_id']) string(v[name], name);
    if (v.expected_account_control_revision != null) revision(v.expected_account_control_revision, 'account_control_revision');
    timestamp(v.valid_until, 'valid_until'); scope(v.scope); validateCapitalPolicy(v.policy); oneOf(v.capability, ['SUPPORTED', 'UNSUPPORTED', 'BLOCKED'], 'capability');
    const funds = record(v.funds, 'funds'); const requestScope = record(v.scope, 'scope'); money(funds.requested, 'requested', String(requestScope.currency));
    if (capitalDecimalUnits(record(funds.requested, 'requested').amount) !== capitalDecimalUnits(requestScope.amount)) throw new Error('capital_preview_amount_mismatch');
    policyCurrency(v.policy, requestScope.currency);
    for (const name of ['remaining_managed_capital', 'estimated_execution_cost', 'native_total_cash', 'native_free_cash', 'native_locked_cash']) if (funds[name] != null) money(funds[name], name, String(requestScope.currency));
    if (!Array.isArray(v.proposed_cancellations) || !Array.isArray(v.reduction_legs) || !Array.isArray(v.reason_codes)) throw new Error('capital_preview_invalid');
    v.reduction_legs.forEach(leg);
    if (v.resume_exit_id !== undefined || v.expected_exit_revision !== undefined) { string(v.resume_exit_id, 'resume_exit_id'); revision(v.expected_exit_revision, 'expected_exit_revision'); }
    return;
  }
  if (kind === 'VIEW') {
    for (const name of ['id', 'account_source_id', 'command_id']) string(v[name], name); revision(v.revision, 'exit_revision'); revision(v.account_control_revision, 'account_control_revision');
    oneOf(v.state, ['REQUESTED', 'FENCING', 'CANCELLING_OPENERS', 'WAITING_CASH', 'REDUCING', 'WAITING_EVIDENCE', 'READY', 'PAUSING', 'PAUSED', 'RECOVERING', 'CANCELLING_EXIT', 'CANCELLED_RESERVED', 'RECONCILING_WITHDRAWAL', 'COMPLETED', 'CLOSED', 'BLOCKED'], 'capital_state');
    oneOf(v.preparation_state, ['WAITING_CASH', 'REDUCING', 'WAITING_EXTERNAL_TRANSFER', 'READY', 'STALE', 'BLOCKED', 'CLOSED'], 'preparation_state');
    const f = record(v.funds, 'funds'); const currency = String(record(f.requested_amount, 'requested').currency);
    for (const name of ['requested_amount', 'reserved_amount', 'staged_for_withdrawal_amount', 'reconciled_withdrawal_amount', 'waived_amount']) money(f[name], name, currency);
    for (const name of ['released_cash_amount', 'verified_withdrawable_amount']) if (f[name] != null) money(f[name], name, currency);
    if (f.actual_fees != null) { const fees = record(f.actual_fees, 'actual_fees'); capitalDecimalUnits(fees.amount, 'actual_fees'); if (fees.currency !== currency) throw new Error('capital_actual_fee_currency_mismatch'); }
    const units = (name: string) => capitalDecimalUnits(record(f[name], name).amount, name);
    if (units('requested_amount') !== units('reserved_amount') + units('staged_for_withdrawal_amount') + units('reconciled_withdrawal_amount') + units('waived_amount')) throw new Error('capital_obligation_mismatch');
    oneOf(f.withdrawability, ['UNVERIFIED', 'VERIFIED', 'STALE', 'SIMULATED'], 'withdrawability');
    oneOf(v.environment, ['PAPER', 'LIVE'], 'environment');
    const requestedScope = record(v.scope, 'scope');
    if (capitalDecimalUnits(requestedScope.amount) !== units('requested_amount') || requestedScope.currency !== currency) throw new Error('capital_scope_funds_mismatch');
    if (f.released_cash_amount != null && capitalDecimalUnits(record(f.released_cash_amount, 'released_cash').amount) > units('reserved_amount')) throw new Error('capital_released_cash_exceeds_reservation');
    if (f.verified_withdrawable_amount != null && capitalDecimalUnits(record(f.verified_withdrawable_amount, 'withdrawable').amount) > units('reserved_amount') + units('staged_for_withdrawal_amount')) throw new Error('capital_withdrawable_exceeds_obligation');
    if (f.withdrawability === 'VERIFIED') { timestamp(f.evidence_asof, 'evidence_asof'); timestamp(f.evidence_valid_until, 'evidence_valid_until'); }

    if (!Array.isArray(v.progress) || !Array.isArray(v.events) || !Array.isArray(v.reason_codes)) throw new Error('capital_progress_invalid');
    for (const [action, availability] of Object.entries(record(v.action_availability, 'action_availability'))) {
      oneOf(action, ['START', 'PAUSE', 'CANCEL', 'RESUME', 'RELEASE_RESERVATION', 'RECONCILE_EXTERNAL_TRANSFER', 'RECONCILE_WITHDRAWAL'], 'capital_action');
      const available = record(availability, 'action_availability'); if (typeof available.allowed !== 'boolean' || available.reason !== null && typeof available.reason !== 'string') throw new Error('capital_action_availability_invalid');
    }
    oneOf(v.remaining_trading, ['FENCED_PENDING_TARGET', 'BUDGET_AWARE'], 'remaining_trading');
    validateCapitalPolicy(v.policy); scope(v.scope); policyCurrency(v.policy, currency); return;
  }
  throw new Error('capital_document_kind_invalid');
}
