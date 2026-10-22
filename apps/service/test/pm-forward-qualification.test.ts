import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { JsonObject } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import type { PmExecutionEvidence } from '../../../packages/contracts/src/pm-forward.ts';
import { compareDecimal } from '../../../packages/contracts/src/pm-validation.ts';
import { qualifyPmForward } from '../src/pm-forward-qualification.ts';
import { forwardSeedDecision } from '../src/pm-forward.ts';
import { pmOperatingRule } from '../src/pm-operating-rule.ts';
import { object } from '../src/facts.ts';
import type { PmRecord } from '../src/pm-facts.ts';
import { fact } from '../src/pm-facts.ts';
import { pmArtifactDigest } from '../src/pm-evidence.ts';

// Independent consumer tests: original exported Rust result, controlled owner,
// method and source-proof inputs. These tests do NOT establish real method
// qualification, source certification, live authorization or transport delivery.
const directory = process.env.QZ_PM_FORWARD_FIXTURE_DIR;
const options = { skip: directory ? false : 'Requires exported original native forward result.' };
type Input = Parameters<typeof qualifyPmForward>[0];
const rows = (value: unknown) => { assert.ok(Array.isArray(value)); return value.map(value => object(value)); };
const same = (actual: unknown, expected: unknown) => assert.equal(compareDecimal(String(actual), String(expected)), 0);

async function fixture(): Promise<Input> {
  const source = object(JSON.parse(await readFile(join(directory!, 'source.json'), 'utf8')));
  assert.equal(source.evidenceKind, 'CONTROLLED_NATIVE_FORWARD_MODEL_TEST_NOT_LIVE_QUALIFICATION');
  const native = object(source.result); assert.deepEqual(native.request, source.request);
  assert.equal(native.kind, 'OBSERVED_SPOT_FORWARD_BUILD'); assert.equal(native.target_block_reason, null);
  const request = object(native.request), initial = object(request.initial_state), snapshot = object(initial.snapshot);
  const target = object(native.target), valuation = object(native.valuation), cutoff = String(target.asof_ns);
  const expiry = (BigInt(cutoff) + 10_000_000_000n).toString();
  const currency = String(valuation.report_currency), money = (amount: unknown) => ({ currency, amount: String(amount) });
  const trades = rows(native.trade_estimates), assets = rows(object(native.build).input && object(object(native.build).input).assets);
  const basis: Pm.AccountBasis = { accountBindingId: 'test:binding', accountSourceId: 'test:source', observationId: 'test:observation',
    sequence: '7', controlRevision: 'test:control', observedAtNs: String(snapshot.ts_event), availableAtNs: cutoff,
    currency, equity: null, reserved: money('0'), positionsEvidenceRef: 'test:captured-account', outstandingCommandsRef: 'test:commands' };
  const assetTargets: Pm.AssetTarget[] = rows(target.targets).map(row => {
    const trade = trades.find(trade => trade.instrument_id === row.instrument_id)!; assert.ok(trade);
    return { instrumentId: String(row.instrument_id), navWeight: String(row.weight), targetQuantity: null,
      currentNotional: money(trade.current_notional), targetNotional: money(trade.target_notional),
      netTradeNotional: money(trade.net_trade_notional), quantityRuleRef: 'test:original-native', riskContribution: null };
  });
  const decision: Pm.PortfolioDecision = { ...fact('test:decision', cutoff), cycleId: 'test:cycle', mandateRevisionId: 'test:mandate',
    strategyVersionId: 'test:strategy', ensembleRevisionId: 'test:ensemble', strategyAllocations: [], policyRevisionId: 'test:policy',
    basisRevision: 'test:basis', incumbentDecisionId: 'test:seed', accountBasis: basis, researchStartingStateRef: null,
    action: 'REBALANCE', status: 'CANDIDATE', assetTargets,
    capital: { equity: money(valuation.total), reserved: money('0'), ownerLiquidityReserve: money('0'), deployableBudget: money(valuation.total),
      committedCapital: money('899.58'), uncommittedCash: money('599.72'), projectedTradingCash: money('0'), capacityEstimateIds: [], sizingEvidenceRef: 'test:original-native' },
    comparisonRef: 'test:comparison', solverEvidenceRef: 'test:original-native', jointReplayEvidenceRef: null,
    reasonCodes: [], explanationFacts: {}, validUntilNs: String(target.valid_until_ns), nextInvalidationDependencies: [], targetCommandId: null };
  // Control flags isolate this consumer's boundary. A real HTTP positive must
  // acquire these from its original independent task, and is tested separately.
  const record = { mandate: { operatingAuthority: 'AUTHORIZED_TARGETS', unresolvedIntent: [] }, gaps: {},
    policy: { validDomain: { qualificationProtocol: { maximumPromotion: 'SHADOW', liveTargetAuthorization: false } } } } as unknown as PmRecord;
  const seedDecision = { ...decision, id: 'test:seed', explanationFacts: {
    economicQualification: { outcome: 'READY' }, independentEvidenceRef: 'test:controlled-independent-input' } };
  const proof: PmExecutionEvidence = { schemaVersion: 1, id: 'test:execution-proof', sourceKind: 'PAPER_EXECUTION_MODEL',
    sourceRevision: 'test:execution-revision', accountSourceId: basis.accountSourceId, nativeAccountId: String(snapshot.account_id),
    observationId: basis.observationId, sequence: basis.sequence, controlRevision: basis.controlRevision, currency,
    asOfNs: cutoff, availableAtNs: cutoff, validUntilNs: expiry, methodRef: 'test:execution-model', sourceEvidenceRef: 'test:original-source',
    instruments: trades.map(trade => {
      const definition = rows(native.instrument_definitions).find(row => Object.values(row).some(value => value && typeof value === 'object' && !Array.isArray(value) && value.id === trade.instrument_id))!;
      assert.ok(definition); const asset = assets.find(asset => asset.instrument_id === trade.instrument_id)!; assert.ok(asset);
      return { instrumentId: String(trade.instrument_id), rulesRef: `sha256:${pmArtifactDigest(definition)}`,
        costs: { commissionRate: String(asset.transaction_cost_rate), slippageRate: '0', impactRate: '0', evidenceRef: 'test:cost-proof' },
        entry: { maximumBuyNotional: '1000', maximumSellNotional: '99.72', evidenceRef: 'test:entry-proof' },
        exit: { maximumNotional: '899.58', evidenceRef: 'test:exit-proof' } };
    }) };
  const capture: JsonObject = { observation: { mode: 'paper' }, capitalCapabilities: { document: { automaticTargets: { supported: true, revisionBound: true, snapshotBound: true } } },
    snapshot: { nativeObservation: { binding: { native_account_id: snapshot.account_id! } }, execution: { validUntilNs: expiry,
      pmEvidence: proof as unknown as JsonObject, liquidity: trades.map(trade => ({ instrumentId: trade.instrument_id!, currency,
        eventNs: trade.price_event_ns!, availableNs: trade.price_available_ns!, notional: '100000', lotNotional: '0.02', minimumNotional: '0', takerFee: '0.0007' })) } } };
  return { record, decision, seedDecision, native, capture, dataset: { id: 'test:dataset-revision', contentRef: 'test:original-source-digest', qualityStatus: 'UNVERIFIED' } as unknown as Pm.DatasetRevision,
    sourceManifest: { controlledConsumerTest: true }, forwardEvidenceRef: 'test:original-native', assessedAtNs: (BigInt(cutoff) + 1n).toString() };
}
const proof = (input: Input) => object(object(input.capture.snapshot).execution).pmEvidence as unknown as PmExecutionEvidence;

test('owner strategy pause prevents a qualified method from becoming a new operating seed', () => {
  const policy = { id: 'policy:original', validDomain: { operatingRule: pmOperatingRule() } };
  const seed = { id: 'decision:original', policyRevisionId: policy.id, strategyVersionId: 'strategy:original', capital: {}, ensembleRevisionId: 'ensemble:original',
    action: 'REBALANCE', status: 'CURRENT', reasonCodes: [] as string[], explanationFacts: { forwardSeedEvidenceRef: 'evidence:seed', economicQualification: { outcome: 'READY' }, independentEvidenceRef: 'evidence:independent' } };
  const record = { mandate: { capitalSource: { kind: 'ACCOUNT', accountBindingId: 'binding:original' } }, policy, decisions: { [seed.id]: seed },
    artifacts: { [`independent-input:${seed.id}`]: { policy: structuredClone(policy) } },
    evidence: { 'evidence:independent': { role: 'INDEPENDENT' } }, retirements: {} } as unknown as PmRecord;
  assert.equal(forwardSeedDecision(record)?.id, seed.id, 'Controlled precondition: the original method is eligible before the owner pauses it.');
  record.artifacts['strategy-control:strategy:original'] = { action: 'pause', requestId: 'owner:pause' };
  record.decisions[seed.id]!.status = 'REVOKED';
  record.decisions[seed.id]!.reasonCodes.push('owner_strategy_paused');
  assert.equal(forwardSeedDecision(record), null, 'Account/source refresh cannot revoke the owner’s pause.');
  record.artifacts['strategy-control:strategy:original'] = { action: 'resume', requestId: 'owner:resume' };
  assert.equal(forwardSeedDecision(record)?.id, seed.id, 'An explicit resume reuses still-qualified method evidence without another sealed evaluation.');
  assert.equal(record.decisions[seed.id]!.status, 'REVOKED', 'Resuming the method must not rewrite an old revoked delivery decision.');
  object(object(record.decisions[seed.id]!.explanationFacts).economicQualification).outcome = 'NOT_READY';
  assert.equal(forwardSeedDecision(record), null, 'Resume cannot rehabilitate a scientifically disqualified method.');
});

test('forward consumer uses actual rounded trade and remaining inventory for source entry/exit bounds', options, async () => {
  const input = await fixture(), before = structuredClone(input.native), original = input.decision.assetTargets[0]!;
  const trade = rows(input.native.trade_estimates)[0]!;
  assert.ok(compareDecimal(String(trade.post_rounding_notional), original.targetNotional.amount) > 0,
    'The exported native sell leaves strictly more inventory than its continuous target.');
  assert.ok(compareDecimal(String(trade.net_trade_notional), String(trade.rounded_trade_notional)) < 0);
  const qualified = qualifyPmForward(input);
  assert.equal(qualified.assessment.outcome, 'READY'); assert.equal(qualified.assessment.stage, 'PAPER');
  assert.equal(Object.hasOwn(qualified, 'capacity'), false, 'Current source bounds cannot manufacture a historical capital replay.');
  const rounded = rows(qualified.artifact.roundedExecutionTargets)[0]!;
  same(object(rounded.targetNotional).amount, trade.post_rounding_notional);
  same(object(rounded.netTradeNotional).amount, trade.rounded_trade_notional);
  const estimated = rows(object(qualified.artifact.execution).instruments)[0]!;
  same(object(estimated.estimatedTradingCost).amount, trade.modeled_linear_cost);
  // The continuous target passes this exact bound, but the true rounded exit
  // inventory breaches it. No relative tolerance may hide the breach.
  proof(input).instruments[0]!.exit.maximumNotional = original.targetNotional.amount;
  const blocked = qualifyPmForward(input); assert.equal(blocked.assessment.outcome, 'NOT_READY');
  assert.ok(blocked.assessment.reasonCodes.includes(`execution_exit_limit_exceeded:${original.instrumentId}`));
  assert.deepEqual(input.native, before, 'Qualification cannot repair original native evidence.');
});

test('forward consumer keeps shadow method qualification below the live authorization ceiling', options, async () => {
  const input = await fixture(); object(input.capture.observation).mode = 'live'; proof(input).sourceKind = 'LIVE_EXECUTION_SOURCE';
  input.dataset.qualityStatus = 'AVAILABLE';
  const result = qualifyPmForward(input); assert.equal(result.assessment.outcome, 'NEEDS_EVIDENCE');
  assert.ok(result.assessment.reasonCodes.includes('original_method_independent_protocol_authorizes_shadow_only'));
  assert.ok(!result.assessment.nextPermittedActions.includes('DELIVER_REVISION_BOUND_TARGET'));
});

test('forward consumer preserves original rule identity and missing source or impact evidence', options, async () => {
  const missingSource = await fixture(); missingSource.sourceManifest = null;
  assert.ok(qualifyPmForward(missingSource).assessment.reasonCodes.includes('current_original_market_receipt_coverage_missing'));
  const unknownCost = await fixture(); proof(unknownCost).instruments[0]!.costs.impactRate = null;
  assert.equal(qualifyPmForward(unknownCost).assessment.outcome, 'NEEDS_EVIDENCE');
  const changedRule = await fixture(); proof(changedRule).instruments[0]!.rulesRef = 'sha256:different-venue-rules';
  assert.equal(qualifyPmForward(changedRule).assessment.outcome, 'NOT_READY');
  const changedAccount = await fixture(); proof(changedAccount).sequence = '8';
  assert.equal(qualifyPmForward(changedAccount).assessment.outcome, 'NOT_READY');
  for (const flag of [undefined, false]) {
    const controlOnly = await fixture(); const caps = object(object(object(controlOnly.capture.capitalCapabilities).document).automaticTargets);
    if (flag === undefined) delete caps.snapshotBound; else caps.snapshotBound = flag;
    assert.notEqual(qualifyPmForward(controlOnly).assessment.outcome, 'READY', 'A control-revision fence does not prove an atomic inventory-snapshot fence.');
  }
});

test('forward consumer expiry is the earliest original source deadline and cannot renew with assessment time', options, async () => {
  const input = await fixture(), first = qualifyPmForward(input);
  assert.equal(first.assessment.outcome, 'READY');
  const nativePriceAge = BigInt(String(object(object(object(input.native.request).execution_settings).multi_currency_spot_cash).maximum_price_age_ns));
  const priceDeadline = rows(input.native.trade_estimates).map(trade => BigInt(String(trade.price_event_ns)) + nativePriceAge)
    .reduce((a, b) => a < b ? a : b).toString();
  assert.ok(BigInt(priceDeadline) < BigInt(proof(input).validUntilNs));
  assert.equal(first.validUntilNs, priceDeadline, 'A source proof or looser forecast age cannot extend original native price validity.');
  input.assessedAtNs = first.validUntilNs;
  const expired = qualifyPmForward(input); assert.equal(expired.assessment.outcome, 'NOT_READY');
  assert.equal(expired.validUntilNs, first.validUntilNs);
  const longProof = await fixture(); const later = (BigInt(String(object(longProof.native.target).valid_until_ns)) + 1n).toString();
  proof(longProof).validUntilNs = later; object(object(longProof.capture.snapshot).execution).validUntilNs = later;
  const fixed = qualifyPmForward(longProof); const deadlines = rows(fixed.artifact.deadlines);
  const expected = deadlines.map(row => String(row.validUntilNs)).reduce((a, b) => BigInt(a) < BigInt(b) ? a : b);
  assert.equal(fixed.validUntilNs, expected);
  assert.ok(BigInt(fixed.validUntilNs) < BigInt(String(object(longProof.native.target).valid_until_ns)), 'Original account/input age can expire before native TTL.');
  longProof.assessedAtNs = fixed.validUntilNs; assert.equal(qualifyPmForward(longProof).assessment.outcome, 'NOT_READY');
});
