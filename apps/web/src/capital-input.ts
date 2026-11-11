import type { CapitalExitAction, CapitalExitPlanningContext, CapitalExitPreview, CapitalExitPreviewInput, CapitalExitStatus, CapitalExitView } from '../../../packages/contracts/src/capital-exit.ts';
import { decimalUnits } from './commands';

export interface CapitalPlanFields {
  amount: string; policy: 'CASH_ONLY' | 'NATURAL_RELEASE' | 'BOUNDED_LIMIT'; deadline?: string;
  maxCost?: string; maxSlice?: string; minimumInterval?: string; participation?: string;
  legs?: { instrumentId: string; quantity: string; minimumPrice: string }[];
}
function positive(value: string | undefined, field: string, zero = false): string {
  const units = decimalUnits(value ?? '');
  if (units === undefined || (zero ? units < 0n : units <= 0n)) throw new Error(field);
  return value!;
}
function decimalString(units: bigint) {
  const fraction = (units % 10n ** 18n).toString().padStart(18, '0').replace(/0+$/, '');
  return `${units / 10n ** 18n}${fraction ? `.${fraction}` : ''}`;
}
/** A visible suggestion, never an automatic increase to satisfy minimum orders. */
export function suggestedCapitalSlice(amount: string, free: string | null | undefined, reserved: string | null | undefined): string | undefined {
  const requested = decimalUnits(amount), cash = decimalUnits(free ?? ''), held = decimalUnits(reserved ?? '');
  if (requested === undefined || cash === undefined || held === undefined) return;
  const idle = cash > held ? cash - held : 0n;
  return requested > idle ? decimalString((requested - idle) / 10n) : undefined;
}
export function capitalPreviewInput(fields: CapitalPlanFields, context: CapitalExitPlanningContext, now = Date.now(), resume?: CapitalExitView): Omit<CapitalExitPreviewInput, 'requestId'> {
  if (context.capability !== 'SUPPORTED' || !Number.isFinite(Date.parse(context.valid_until)) || Date.parse(context.valid_until) <= now) throw new Error('capital_exit_context_expired');
  const amount = positive(fields.amount, 'capital_exit_amount_invalid');
  if (!context.supported_policies.includes(fields.policy)) throw new Error('capital_exit_policy_unsupported');
  if (resume && (context.account_source_id !== resume.account_source_id || context.environment !== resume.environment || context.currency !== resume.scope.currency
    || decimalUnits(amount) !== decimalUnits(resume.funds.reserved_amount.amount))) throw new Error('capital_exit_resume_basis_changed');
  if (resume && !capitalAction(resume, 'RESUME').allowed) throw new Error(capitalAction(resume, 'RESUME').reason ?? 'capital_exit_action_unavailable');
  const base = { amount, currency: context.currency, contextId: context.id, ...(resume ? { resumeExitId: resume.id, expectedExitRevision: resume.revision } : {}) };
  if (fields.policy === 'CASH_ONLY') return { ...base, policy: { kind: 'CASH_ONLY' } };
  const deadlineMs = Date.parse(fields.deadline ?? '');
  if (!Number.isFinite(deadlineMs) || deadlineMs <= now) throw new Error('capital_exit_deadline_invalid');
  const deadline = new Date(deadlineMs).toISOString();
  if (fields.policy === 'NATURAL_RELEASE') return { ...base, policy: { kind: 'NATURAL_RELEASE', deadline } };
  if (!fields.legs?.length) throw new Error('capital_exit_legs_required');
  const seen = new Set<string>();
  const legs = fields.legs.map(row => {
    const instrument = context.instruments.find(item => item.instrument_id === row.instrumentId);
    if (!instrument || seen.has(row.instrumentId)) throw new Error('capital_exit_instrument_invalid');
    seen.add(row.instrumentId);
    const quantity = positive(row.quantity, 'capital_exit_quantity_invalid');
    const price = positive(row.minimumPrice, 'capital_exit_price_invalid');
    const qty = decimalUnits(quantity)!, step = decimalUnits(instrument.quantity_increment);
    const priceStep = decimalUnits(instrument.price_increment);
    if (qty > (decimalUnits(instrument.eligible_quantity) ?? 0n) || !step || qty % step !== 0n) throw new Error('capital_exit_quantity_increment_invalid');
    if (!priceStep || decimalUnits(price)! % priceStep !== 0n) throw new Error('capital_exit_price_increment_invalid');
    return { instrument_id: row.instrumentId, maximum_reduction_quantity: quantity, minimum_sell_price: price };
  });
  const interval = fields.minimumInterval ?? '5';
  if (!/^[1-9]\d*$/.test(interval) || !Number.isSafeInteger(Number(interval)) || Number(interval) < 5) throw new Error('capital_exit_interval_invalid');
  const participation = positive(fields.participation ?? '0.1', 'capital_exit_participation_invalid');
  if (decimalUnits(participation)! > 100_000_000_000_000_000n) throw new Error('capital_exit_participation_invalid');
  return { ...base, policy: { kind: 'BOUNDED_LIMIT', deadline, legs,
    max_execution_cost: { amount: positive(fields.maxCost, 'capital_exit_cost_invalid', true), currency: context.currency, reference_evidence_id: context.id },
    pacing: { minimum_interval_seconds: Number(interval), maximum_slice_notional: positive(fields.maxSlice, 'capital_exit_slice_invalid'), maximum_participation: participation } } };
}
export function capitalPreviewIsCurrent(preview: CapitalExitPreview | undefined, status: CapitalExitStatus | undefined, now = Date.now()): boolean {
  return !!preview && !!status && preview.capability === 'SUPPORTED' && Date.parse(preview.valid_until) > now
    && preview.account_source_id === status.accountSourceId
    && preview.expected_account_control_revision === status.controlRevision;
}
export function capitalResumePreviewMatches(exit: CapitalExitView, preview: CapitalExitPreview | undefined, status: CapitalExitStatus | undefined, previewForExitId: string | undefined, now = Date.now()): boolean {
  return previewForExitId === exit.id && preview?.resume_exit_id === exit.id && preview.expected_exit_revision === exit.revision && capitalPreviewIsCurrent(preview, status, now)
    && preview?.account_source_id === exit.account_source_id && preview.environment === exit.environment && preview.scope.currency === exit.scope.currency
    && decimalUnits(preview.scope.amount) === decimalUnits(exit.funds.reserved_amount.amount);
}
export function capitalAction(exit: CapitalExitView, action: CapitalExitAction) {
  return exit.action_availability[action] ?? { allowed: false, reason: 'capital_exit_action_unavailable' };
}
export function capitalEvidenceIsCurrent(exit: CapitalExitView, now = Date.now()): boolean {
  return exit.funds.withdrawability === 'VERIFIED' && !!exit.funds.verified_withdrawable_amount && !!exit.funds.evidence_valid_until
    && Date.parse(exit.funds.evidence_valid_until) > now;
}
