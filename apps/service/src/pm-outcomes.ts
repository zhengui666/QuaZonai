import type * as Pm from '../../../packages/contracts/src/pm.ts';
import type { CapturedAccount } from './pm-account.ts';
import type { PmRecord } from './pm-facts.ts';
import { digest, fact, gap, identity } from './pm-facts.ts';
import { json, nowNs, object, Problem } from './facts.ts';
import { addDecimal, subtractDecimal } from '../../../packages/contracts/src/pm-validation.ts';
import { accountEventKey, projectAccountEvents } from './pm-account-events.ts';
import type { AccountEventIdentity, CapturedAccountEvents } from './pm-account-events.ts';

function knownUnpricedSnapshot(artifact: CapturedAccount['artifact'] | null): boolean {
  if (!artifact?.snapshot) return false;
  const snapshot = object(artifact.snapshot); if (!snapshot.nativeObservation) return false;
  const native = object(object(snapshot.nativeObservation).snapshot);
  return native.is_stale === true || ['stale_instruments', 'stale_currencies', 'unpriced_instruments'].some(key => Array.isArray(native[key]) && native[key].length > 0);
}

/** Only the original registered cadence or original target expiry supplies a
 * maturity boundary. Unknown schedules do not become an invented five-second
 * financial observation window. */
function observationWindow(record: PmRecord, decision: Pm.PortfolioDecision, atNs: string): string | null {
  const explanation = object(decision.explanationFacts ?? {});
  const seed = typeof explanation.forwardSeedDecisionId === 'string' ? record.decisions[explanation.forwardSeedDecisionId] : null;
  const source = seed ? object(seed.explanationFacts) : explanation;
  if (typeof source.forwardAnchorNs === 'string' && typeof source.forwardTtlSeconds === 'string'
    && /^\d+$/.test(source.forwardAnchorNs) && /^\d+$/.test(source.forwardTtlSeconds)) {
    const ttl = BigInt(source.forwardTtlSeconds) * 1_000_000_000n, anchor = BigInt(source.forwardAnchorNs);
    if (ttl > 0n && BigInt(atNs) >= anchor) return `registered:${(BigInt(atNs) - anchor) / ttl}`;
  }
  return typeof decision.validUntilNs === 'string' && BigInt(atNs) >= BigInt(decision.validUntilNs) ? 'original_target_window_matured' : null;
}

/** Actual account events are a separate evidence stream from modeled replays.
 * A cash-flow-adjusted currency delta is neither strategy PnL nor a TWR.
 * Sampling history does not gate source acceptance or target invalidation:
 * Application retains the latest account snapshot, and the PM admission path
 * immediately fences a changed account basis while coalescing its latest
 * pending capture. A NAV tick need not append every historical attribution. */
export function observePmOutcome(record: PmRecord, account: CapturedAccount) {
  const ledgerKey = `account-event-ledger:${account.basis.accountBindingId}`;
  const ledger = record.artifacts[ledgerKey] ? object(record.artifacts[ledgerKey]) : {};
  const prior = Array.isArray(ledger.identities) ? ledger.identities as unknown as AccountEventIdentity[] : [];
  let events: CapturedAccountEvents | null = null; let integrityFailure: string | null = null;
  try {
    events = projectAccountEvents(account, prior);
    if (events?.newEventKeys.length) {
      const merged = new Map(prior.map(item => [accountEventKey(item), item]));
      for (const item of events.identities) merged.set(accountEventKey(item), item);
      record.artifacts[ledgerKey] = json({ identities: [...merged.values()], evidenceRefs: [...new Set([...(Array.isArray(ledger.evidenceRefs) ? ledger.evidenceRefs as string[] : []), events.evidence.id])] });
    }
  } catch (error) {
    integrityFailure = error instanceof Problem ? error.message : 'account_event_projection_failed';
    gap(record, 'account_event_integrity', integrityFailure, 'REFRESH_SOURCE');
  }
  const retainSource = () => {
    record.evidence[account.evidence.id] = account.evidence; record.artifacts[account.evidence.objectKey] = account.artifact;
    if (events) { record.evidence[events.evidence.id] = events.evidence; record.artifacts[events.evidence.objectKey] = events.artifact; }
  };
  // Real source events remain durable even when no strategy was delivered.
  // Advancing an empty coverage envelope alone is not a new account event.
  if (events?.newEventKeys.length) retainSource();
  const completeness = digest(json({ complete: events?.complete ?? null, integrityFailure }));
  const decisions = Object.values(record.decisions).filter(item => item.accountBasis?.accountBindingId === account.basis.accountBindingId
    && item.targetCommandId !== null && item.targetCommandId !== undefined && record.commands[item.targetCommandId]
    && record.commands[item.targetCommandId]!.state !== 'NOT_OFFERED');
  const latest = decisions.at(-1);
  for (const decision of decisions) {
    const original = decision.accountBasis!;
    if (BigInt(account.basis.observedAtNs) <= BigInt(original.observedAtNs)) continue;
    const command = record.commands[decision.targetCommandId!]!;
    const key = `outcome-sampling:${decision.id}`; const previous = record.artifacts[key] ? object(record.artifacts[key]) : null;
    const window = observationWindow(record, decision, account.basis.observedAtNs);
    const matchedNewExecution = !!events?.executions.some(event => event.clientTargetId === command.idempotencyKey
      && events!.newEventKeys.includes(accountEventKey({ accountSourceId: events!.accountSourceId, nativeAccountId: events!.nativeAccountId, kind: 'EXECUTION', id: event.id })));
    const unresolved = ['PENDING', 'UNCERTAIN'].includes(command.state)
      || Object.values(record.retirements ?? {}).some(item => item.reconciliationRequired && item.outstandingTargetCommandIds.includes(command.id));
    const matured = window !== null && previous?.window !== window;
    const receiptChanged = previous !== null && previous.receiptState !== command.state;
    // Older settled commands get their pending maturity or actual matched fill;
    // they do not accumulate every future account heartbeat or later strategy's
    // account delta. Never-offered research candidates are not attributed.
    if (decision !== latest && !unresolved && !matchedNewExecution && !receiptChanged && !(matured && previous?.matured !== true)) continue;
    const sampleRequired = previous === null || matchedNewExecution || !!events?.newEventKeys.length
      || previous.completeness !== completeness || receiptChanged || matured;
    if (!sampleRequired) continue;
    // Target receipt settlement is independent of the account source stream.
    // Reconciliation may advance UNCERTAIN/PENDING while the original account
    // sequence, native clock and payload remain exactly unchanged. Bind the
    // immutable first-received receipt observation, not a new account identity
    // or the mutable latest-receipt pointer. Legacy records without that clock
    // become known conservatively at this observation, never backdated.
    const receiptClock = record.artifacts[`target-receipt-clock:${command.id}`];
    const observedRef = typeof receiptClock?.observationRef === 'string' ? receiptClock.observationRef : null;
    let observedReceipt = observedRef ? record.artifacts[observedRef] : null;
    let receiptObservationRef = observedReceipt?.commandId === command.id
      && typeof observedReceipt.availableAtNs === 'string' && /^\d+$/.test(observedReceipt.availableAtNs) ? observedRef : null;
    if (!receiptObservationRef) {
      const retained = command.externalReceiptRef ? record.artifacts[command.externalReceiptRef] ?? null : null;
      receiptObservationRef = identity('target-state-observation', [command.id, command.state, retained ? digest(retained) : null]);
      record.artifacts[receiptObservationRef] ??= json({ commandId: command.id, receiptState: command.state,
        receiptRef: command.externalReceiptRef ?? null, receipt: retained, availableAtNs: nowNs(),
        clockBasis: 'FIRST_LOCAL_OBSERVATION_ORIGINAL_RECEIVE_CLOCK_UNAVAILABLE' });
      observedReceipt = record.artifacts[receiptObservationRef]!;
    }
    const receiptAvailableAtNs = String(observedReceipt!.availableAtNs);
    const availableAtNs = BigInt(receiptAvailableAtNs) > BigInt(account.basis.availableAtNs) ? receiptAvailableAtNs : account.basis.availableAtNs;
    const sampling = { sourceRevision: account.evidence.sourceRevision, completeness,
      receiptState: command.state, receiptObservationRef, window, matured: previous?.matured === true || matured, observedAtNs: account.basis.observedAtNs };
    const accountObservationId = identity('outcome', [decision.id, account.evidence.sourceRevision]);
    const id = identity('outcome', [decision.id, account.evidence.sourceRevision, command.state, receiptObservationRef]);
    if (record.outcomes[id]) { record.artifacts[key] = sampling; continue; }
    retainSource();
    if (receiptObservationRef) record.evidence[receiptObservationRef] ??= { id: receiptObservationRef, sourceKind: observedReceipt?.clockBasis ? 'DERIVED' : 'ACCOUNT', role: 'OBSERVED', runtime: null,
      objectKey: receiptObservationRef, contentDigest: digest(observedReceipt), mediaType: 'application/json', sourceRevision: receiptObservationRef,
      eventInterval: null, availableAtNs: receiptAvailableAtNs, clockPrecision: 'NANOSECOND',
      schemaId: observedReceipt?.clockBasis ? 'qz.pm.legacy-target-state-observation/1' : 'qz.pm.target-receipt-observation/1', visibility: 'OWNER_ONLY' };
    const interval = { startNs: original.observedAtNs, endExclusiveNs: (BigInt(account.basis.observedAtNs) + 1n).toString() };
    const makeMetric = (key: string, label: string, value: string | null, reasons: string[], refs: string[]): Pm.MetricObservation => ({
      definition: { id: `qz.metric.${key}/1`, key: `account.${key}`, definitionVersion: '1', label, unit: 'CURRENCY',
        valueEncoding: 'DECIMAL', direction: 'DIAGNOSTIC', aggregation: key === 'observed_equity' ? 'LAST_AUTHORITATIVE_SNAPSHOT' : 'EXACT_ACCOUNT_INTERVAL_CURRENCY_DELTA',
        calendarId: null, annualizationDays: null, horizonRef: 'OBSERVED_ACCOUNT_INTERVAL', requiredInputs: key === 'observed_equity' ? ['authoritative_account_equity'] : ['matched_original_equity_snapshots', 'source_confirmed_complete_external_cash_flows'] },
      status: value !== null ? 'AVAILABLE' : 'UNAVAILABLE', value, currency: account.basis.currency, reasonCodes: reasons, role: 'OBSERVED', interval,
      asOfNs: account.basis.observedAtNs, availableAtNs: account.basis.availableAtNs, origin: key === 'observed_equity' ? 'ACCOUNT' : 'DERIVED',
      methodRef: key === 'observed_equity' ? null : 'qz.account-flow-adjusted-currency-delta/1', evidenceRefs: refs, sampleCount: null, expectedCount: null,
      uncertainty: null, raw: null });
    const closingUnpriced = knownUnpricedSnapshot(account.artifact);
    const metric = makeMetric('observed_equity', 'Observed account equity', closingUnpriced ? null : account.basis.equity?.amount ?? null,
      closingUnpriced ? ['native_account_valuation_stale_or_unpriced'] : account.basis.equity ? [] : ['authoritative_equity_unavailable'], [account.evidence.id]);
    const metrics = [metric];
    const reasons = ['observed_account_changes_are_not_identified_strategy_pnl'];
    if (receiptChanged && previous.sourceRevision === account.evidence.sourceRevision) reasons.push('receipt_settlement_without_new_account_observation');
    // Account-only metrics keep their original identity across receipt changes.
    // ACK/REJECTED cannot create another equity, execution or cash-flow fact.
    const normalizationRef = identity('cashflow-normalization', accountObservationId);
    const coverage = !!events && events.complete.cashFlows && BigInt(events.interval.startNs) <= BigInt(original.observedAtNs) + 1n
      && BigInt(events.interval.endExclusiveNs) >= BigInt(interval.endExclusiveNs);
    const flows = events?.cashFlows.filter(flow => BigInt(flow.eventAtNs) > BigInt(original.observedAtNs) && BigInt(flow.eventAtNs) <= BigInt(account.basis.observedAtNs)) ?? [];
    const priorEvidence = record.evidence[original.positionsEvidenceRef]; const priorArtifact = priorEvidence ? record.artifacts[priorEvidence.objectKey] : null;
    const binding = (artifact: typeof account.artifact | null) => {
      const snapshot = artifact?.snapshot && object(artifact.snapshot); const native = snapshot && snapshot.nativeObservation && object(snapshot.nativeObservation);
      return native && native.binding ? object(native.binding) : null;
    };
    const beforeBinding = binding(priorArtifact ?? null); const afterBinding = binding(account.artifact);
    const sameAccount = original.accountSourceId === account.basis.accountSourceId && original.accountBindingId === account.basis.accountBindingId
      && beforeBinding !== null && afterBinding !== null && ['project_id', 'environment', 'native_account_id', 'native_trader_id'].every(key => beforeBinding[key] !== undefined && beforeBinding[key] === afterBinding[key]);
    const currencyMatches = original.currency === account.basis.currency && flows.every(flow => flow.currency === account.basis.currency);
    const pricingKnownInvalid = closingUnpriced || knownUnpricedSnapshot(priorArtifact ?? null);
    const available = coverage && sameAccount && currencyMatches && !pricingKnownInvalid && original.equity !== null && account.basis.equity !== null;
    const totalFlows = available ? flows.reduce((total, flow) => addDecimal(total, flow.amount), '0') : null;
    const adjustedChange = available ? subtractDecimal(subtractDecimal(account.basis.equity!.amount, original.equity!.amount), totalFlows!) : null;
    const normalizationReasons = available ? ['account_currency_delta_not_strategy_pnl_or_time_weighted_return']
      : [!coverage ? 'complete_external_cashflow_coverage_unavailable' : !sameAccount ? 'matched_original_native_account_binding_unavailable' : pricingKnownInvalid ? 'native_account_valuation_stale_or_unpriced' : !currencyMatches ? 'cashflow_currency_conversion_unavailable' : 'matched_reported_equity_unavailable'];
    const refs = [original.positionsEvidenceRef, account.evidence.id, ...(events ? [events.evidence.id] : [])];
    const normalization = json({ method: 'qz.account-flow-adjusted-currency-delta/1', interval, status: available ? 'AVAILABLE' : 'UNAVAILABLE',
      accountSourceId: account.basis.accountSourceId, sourceEvidenceRefs: refs, originalEquity: original.equity, closingEquity: account.basis.equity,
      externalNetCashFlow: totalFlows, adjustedEquityChange: adjustedChange, currency: account.basis.currency, reasonCodes: normalizationReasons,
      scope: 'WHOLE_ACCOUNT_NOT_STRATEGY_ATTRIBUTION', returnMethod: null });
    record.artifacts[normalizationRef] = normalization;
    record.evidence[normalizationRef] = { id: normalizationRef, sourceKind: 'DERIVED', role: 'OBSERVED', runtime: null, objectKey: normalizationRef,
      contentDigest: digest(normalization), mediaType: 'application/json', sourceRevision: account.evidence.sourceRevision, eventInterval: interval,
      availableAtNs: account.basis.availableAtNs, clockPrecision: 'NANOSECOND', schemaId: 'qz.account-cashflow-normalization/1', visibility: 'OWNER_ONLY' };
    const adjusted = makeMetric('external_flow_adjusted_equity_change', 'Account equity change excluding external cash flows', adjustedChange,
      adjustedChange !== null ? [] : normalizationReasons, [normalizationRef]);
    metrics.push(adjusted); reasons.push(...normalizationReasons);
    const actualExecutions = events?.executions.filter(event => BigInt(event.eventAtNs) > BigInt(original.observedAtNs)
      && BigInt(event.eventAtNs) <= BigInt(account.basis.observedAtNs)) ?? [];
    const matchedExecutions = command ? actualExecutions.filter(event => event.clientTargetId === command.idempotencyKey) : [];
    reasons.push(events ? events.complete.executions ? 'source_execution_coverage_declared' : 'partial_source_execution_coverage' : 'actual_executions_not_provided');
    const executionRef = identity('execution-attribution', accountObservationId);
    record.artifacts[executionRef] = json({ decisionId: decision.id, commandId: command?.id ?? null, sourceEvidenceRef: events?.evidence.id ?? null,
      matchedExecutionIds: matchedExecutions.map(event => event.id), unmatchedExecutionIds: actualExecutions.filter(event => !matchedExecutions.includes(event)).map(event => event.id),
      completeness: events?.complete.executions ?? null, identification: 'REPORTED_TARGET_ID_ONLY_NOT_PNL_CAUSALITY' });
    record.evidence[executionRef] = { ...record.evidence[normalizationRef]!, id: executionRef, objectKey: executionRef,
      contentDigest: digest(record.artifacts[executionRef]), schemaId: 'qz.account-execution-attribution/1' };
    const attributionId = identity('attribution', id);
    const outcome: Pm.ObservedOutcome = { ...fact(id), decisionId: decision.id, targetCommandId: decision.targetCommandId,
      accountBasis: account.basis, interval, availableAtNs, receiptState: command?.state ?? 'NOT_OFFERED',
      reconciliationStatus: matchedExecutions.length || command?.state === 'ACKNOWLEDGED' ? 'PARTIAL' : 'UNMATCHED', realizedMetrics: metrics, attributionEvidenceRef: attributionId,
      matureLabelEvidenceRef: null, reasonCodes: reasons };
    const comparison = record.comparisons[decision.comparisonRef];
    if (comparison) record.attributions[attributionId] = { ...fact(attributionId), observedOutcomeId: id, decisionId: decision.id,
      scope: { ...comparison.scope, id: identity('observed-scope', id), interval, role: 'OBSERVED', capital: account.basis.equity },
      cashFlowNormalizationRef: normalizationRef, actualObservationRefs: [...refs, executionRef, ...(receiptObservationRef ? [receiptObservationRef] : [])], counterfactualEvidenceRefs: [],
      components: [{ kind: 'DATA', metric, identification: 'UNIDENTIFIED', methodRef: null },
        { kind: 'CAPITAL', metric: adjusted, identification: 'UNIDENTIFIED', methodRef: 'qz.account-flow-adjusted-currency-delta/1' }],
      aggregation: 'NON_ADDITIVE_COUNTERFACTUAL', residual: null, limitations: reasons };
    else outcome.attributionEvidenceRef = null;
    record.outcomes[id] = outcome;
    record.artifacts[key] = sampling;
  }
}
