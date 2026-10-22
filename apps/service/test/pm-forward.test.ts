import assert from 'node:assert/strict';
import { test } from 'node:test';
import { assessPmExecutionQualification, validatePmExecutionEvidence } from '../../../packages/contracts/src/pm-forward.ts';
import type { PmExecutionEvidence, PmExecutionQualificationInput } from '../../../packages/contracts/src/pm-forward.ts';
import { validatePmEntity } from '../../../packages/contracts/src/pm-validation.ts';

// Software-only contract examples, never brokerage/source certification.
function fixture(): PmExecutionQualificationInput {
  const proof: PmExecutionEvidence = { schemaVersion: 1, id: 'source-proof', sourceKind: 'PAPER_EXECUTION_MODEL',
    sourceRevision: 'source-revision', accountSourceId: 'source', nativeAccountId: 'VENUE-1', observationId: 'observation',
    sequence: '7', controlRevision: 'control-3', currency: 'USD', asOfNs: '100', availableAtNs: '105', validUntilNs: '180',
    methodRef: 'source-method', sourceEvidenceRef: 'original-provider-envelope', instruments: [{ instrumentId: 'BTC-USD', rulesRef: 'official-instrument',
      costs: { commissionRate: '0.001', slippageRate: '0.002', impactRate: '0.003', evidenceRef: 'cost-proof' },
      entry: { maximumBuyNotional: '20.000000000000000001', maximumSellNotional: '80', evidenceRef: 'entry-proof' },
      exit: { maximumNotional: '100.000000000000000001', evidenceRef: 'exit-proof' } }] };
  return { proof, proofEvidenceRef: 'received-transport', nativeAccountId: 'VENUE-1', mode: 'paper',
    accountBasis: { accountBindingId: 'binding', accountSourceId: 'source', observationId: 'observation', sequence: '7', controlRevision: 'control-3',
      observedAtNs: '100', availableAtNs: '110', currency: 'USD', equity: null, reserved: { currency: 'USD', amount: '0' },
      positionsEvidenceRef: 'received-account', outstandingCommandsRef: 'original-command-ledger' },
    decisionAtNs: '120', evaluatedAtNs: '125', nativeTargetValidUntilNs: '220', frozenTargetTtlNs: '100', liquidityValidUntilNs: '170',
    liquidity: [{ instrumentId: 'BTC-USD', currency: 'USD', eventNs: '100', availableNs: '105', notional: '9999999', lotNotional: '0.1', minimumNotional: '1', takerFee: '0.001' }],
    positions: [{ instrumentId: 'BTC-USD', currentNotional: { currency: 'USD', amount: '80' } }],
    targets: [{ instrumentId: 'BTC-USD', navWeight: '0.5', targetQuantity: null,
      currentNotional: { currency: 'USD', amount: '80' }, targetNotional: { currency: 'USD', amount: '100.000000000000000001' },
      netTradeNotional: { currency: 'USD', amount: '20.000000000000000001' }, quantityRuleRef: 'native-rules', riskContribution: null }],
    modeledCostRates: [{ instrumentId: 'BTC-USD', rate: '0.006' }] };
}
test('forward qualification preserves exact money and only shortens the original deadline', () => {
  const input = fixture(); const result = assessPmExecutionQualification(input);
  assert.equal(result.scope, 'CURRENT_SOURCE_EXECUTION_BOUNDS'); assert.equal(result.outcome, 'READY'); assert.equal(result.validUntilNs, '170');
  assert.deepEqual(result.instruments[0]!.estimatedTradingCost, { currency: 'USD', amount: '0.120000000000000000006' });
  input.evaluatedAtNs = '170'; assert.equal(assessPmExecutionQualification(input).outcome, 'NOT_READY');
  input.evaluatedAtNs = '125'; input.nativeTargetValidUntilNs = '225';
  assert.ok(assessPmExecutionQualification(input).reasonCodes.includes('execution_native_ttl_not_original_cutoff'));
});
test('missing impact is unknown; paper proof cannot qualify live or a cheaper native cost model', () => {
  const input = fixture(); const proof = input.proof as PmExecutionEvidence;
  proof.instruments[0]!.costs.impactRate = null;
  const result = assessPmExecutionQualification(input);
  assert.equal(result.outcome, 'NEEDS_EVIDENCE'); assert.equal(result.instruments[0]!.estimatedTradingCost, null);
  proof.instruments[0]!.costs.impactRate = '0.003'; input.mode = 'live';
  assert.ok(assessPmExecutionQualification(input).reasonCodes.includes('execution_source_environment_mismatch'));
  input.mode = 'paper'; input.modeledCostRates[0]!.rate = '0.003';
  assert.ok(assessPmExecutionQualification(input).reasonCodes.includes('execution_native_cost_model_mismatch:BTC-USD'));
});
test('bar turnover cannot replace executable entry/exit limits, including one-attodollar breaches', () => {
  const input = fixture(); const proof = input.proof as PmExecutionEvidence;
  proof.instruments[0]!.entry.maximumBuyNotional = '20'; proof.instruments[0]!.exit.maximumNotional = '100';
  const result = assessPmExecutionQualification(input); assert.equal(result.outcome, 'NOT_READY');
  assert.ok(result.reasonCodes.includes('execution_entry_limit_exceeded:BTC-USD'));
  assert.ok(result.reasonCodes.includes('execution_exit_limit_exceeded:BTC-USD'));
});
test('unknown inventory and omitted held assets do not become fresh cash', () => {
  const input = fixture(); input.positions = null; assert.equal(assessPmExecutionQualification(input).outcome, 'NEEDS_EVIDENCE');
  input.positions = [{ instrumentId: 'ETH-USD', currentNotional: { currency: 'USD', amount: '1' } }];
  const result = assessPmExecutionQualification(input); assert.equal(result.outcome, 'NOT_READY');
  assert.ok(result.reasonCodes.includes('execution_inventory_target_omitted:ETH-USD'));
  assert.ok(result.reasonCodes.includes('execution_current_inventory_mismatch:BTC-USD'));
});
test('source proof binds account identity and receipt while retaining its distinct execution clock', () => {
  for (const change of [
    (p: PmExecutionEvidence) => { p.sequence = '8'; },
    (p: PmExecutionEvidence) => { p.nativeAccountId = 'OTHER'; },
    (p: PmExecutionEvidence) => { p.availableAtNs = '111'; },
    (p: PmExecutionEvidence) => { p.asOfNs = '112'; p.availableAtNs = '113'; },
  ]) { const input = fixture(); change(input.proof as PmExecutionEvidence); assert.equal(assessPmExecutionQualification(input).outcome, 'NOT_READY'); }
  const input = fixture(); input.liquidity[0]!.availableNs = '111';
  assert.ok(assessPmExecutionQualification(input).reasonCodes.includes('execution_liquidity_basis_mismatch:BTC-USD'));
  const current = fixture(); const proof = current.proof as PmExecutionEvidence;
  proof.asOfNs = '150'; proof.availableAtNs = '151'; current.accountBasis.availableAtNs = '152';
  current.decisionAtNs = '155'; current.evaluatedAtNs = '156'; current.nativeTargetValidUntilNs = '255';
  assert.equal(assessPmExecutionQualification(current).outcome, 'READY');
  assert.equal(current.accountBasis.observedAtNs, '100'); assert.equal(proof.asOfNs, '150');
});
test('ambiguous proofs and invented ready fields reject instead of picking the first row', () => {
  const proof = fixture().proof as PmExecutionEvidence;
  assert.throws(() => validatePmExecutionEvidence({ ...proof, ready: true }), /shape_invalid/);
  proof.instruments.push(structuredClone(proof.instruments[0]!)); assert.throws(() => validatePmExecutionEvidence(proof), /duplicate_instrument/);
  const input = fixture(); input.targets = [...input.targets, input.targets[0]!]; assert.throws(() => assessPmExecutionQualification(input), /duplicate_instrument/);
  assert.equal(assessPmExecutionQualification({ ...fixture(), proof: null }).outcome, 'NEEDS_EVIDENCE');
});

test('visible execution estimate preserves exact rounded funds without replacing the continuous model target', () => {
  const continuous = fixture().targets[0]!;
  const target = { ...continuous, executionEstimate: { resultingQuantity: '5', resultingNotional: { currency: 'USD', amount: '100' },
    netTradeQuantity: '1', netTradeNotional: { currency: 'USD', amount: '20' }, evidenceRef: 'original-rounded-native-trades' } };
  const validated = validatePmEntity('AssetTarget', target);
  assert.equal(validated.targetNotional.amount, '100.000000000000000001');
  assert.equal(validated.executionEstimate!.resultingNotional.amount, '100');
  assert.equal(validated.targetQuantity, null);
  assert.throws(() => validatePmEntity('AssetTarget', { ...target, executionEstimate: { ...target.executionEstimate,
    resultingNotional: { currency: 'USD', amount: '100.000000000000000001' } } }), /estimated_trade_not_conserved/);
  assert.throws(() => validatePmEntity('AssetTarget', { ...target, executionEstimate: { ...target.executionEstimate,
    netTradeNotional: { currency: 'EUR', amount: '20' } } }), /currency_mismatch/);
});

test('execution estimate cannot swap buy/sell signs or give negative quantity to a positive marked holding', () => {
  const target = { ...fixture().targets[0]!, executionEstimate: { resultingQuantity: '5', resultingNotional: { currency: 'USD', amount: '100' },
    netTradeQuantity: '1', netTradeNotional: { currency: 'USD', amount: '20' }, evidenceRef: 'original-rounded-native-trades' } };
  for (const field of ['resultingQuantity', 'netTradeQuantity'] as const) assert.throws(() => validatePmEntity('AssetTarget', {
    ...target, executionEstimate: { ...target.executionEstimate, [field]: '-1' } }), /estimated_quantity_direction_mismatch/);
  const short = { ...target, navWeight: '-0.5', targetNotional: { currency: 'USD', amount: '-100' }, netTradeNotional: { currency: 'USD', amount: '-180' },
    executionEstimate: { ...target.executionEstimate, resultingQuantity: '-5', resultingNotional: { currency: 'USD', amount: '-100' },
      netTradeQuantity: '-9', netTradeNotional: { currency: 'USD', amount: '-180' } } };
  assert.doesNotThrow(() => validatePmEntity('AssetTarget', short), 'A generic signed DTO does not confer unsupported short-selling capability on the CASH native consumer.');
});
