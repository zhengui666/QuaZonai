import assert from 'node:assert/strict';
import { test } from 'node:test';
import { pmOperatingRule, pmOperatingRuleIdentity, validatePmOperatingRule } from '../src/pm-operating-rule.ts';
import type { JsonObject } from '@earendil-works/pi-durable';

// Contract validation only: no fabricated model, native economic evidence,
// qualification, account state or execution authorization is introduced.
test('a missing historical operating rule cannot acquire current qualification through a default', () => {
  for (const value of [undefined, null, {}, { id: 'qz.pm.frozen-current-spot-operating/1' }, { id: 'qz.pm.frozen-current-spot-operating/2' }]) {
    assert.throws(() => validatePmOperatingRule(value), /original_operating_rule_missing_or_changed/);
    assert.throws(() => pmOperatingRuleIdentity(value), /original_operating_rule_missing_or_changed/);
  }
});

test('operating rule identity preserves its original timing, fixed inputs and economic method', () => {
  const original = pmOperatingRule(); const identity = pmOperatingRuleIdentity(original);
  const reordered = Object.fromEntries(Object.entries(original).reverse());
  assert.equal(pmOperatingRuleIdentity(reordered), identity, 'object key order cannot create another policy identity');
  assert.equal(pmOperatingRuleIdentity(new Proxy(original, {})), identity, 'transaction JSON proxies retain their original rule identity');
  const invalid = [
    { ...original, id: 'qz.pm.frozen-current-spot-operating/1' },
    { ...original, id: 'qz.pm.frozen-current-spot-operating/2' },
    { ...original, numericalOutput: null },
    { ...original, numericalOutput: { ...(original.numericalOutput as JsonObject), acceptance: 'ALLOW_NEGATIVE_POST_FEE_CASH' } },
    { ...original, cashFunding: null },
    { ...original, cashFunding: { ...(original.cashFunding as JsonObject), solver: 'SCALE_TARGET_AFTER_SOLVE' } },
    { ...original, fixedInputs: ['FIT_A_NEW_ENSEMBLE'] },
    { ...original, clockRule: { ...(original.clockRule as JsonObject), actualCutoff: 'BACKDATE_TO_NOMINAL_GRID' } },
    { ...original, decisionRule: { ...(original.decisionRule as JsonObject), forecastUnit: 'DAILY_RETURN' } },
    { ...original, decisionRule: { ...(original.decisionRule as JsonObject), direction: 'MAXIMIZE' } },
    { ...original, decisionRule: { ...(original.decisionRule as JsonObject), feasibilityPriority: null } },
    { ...original, decisionRule: { ...(original.decisionRule as JsonObject), feasibilityPriority: {
      ...((original.decisionRule as JsonObject).feasibilityPriority as JsonObject), infeasibleHold: 'IGNORE_WHEN_OBJECTIVE_IS_BETTER' } } },
    { ...original, decisionRule: { ...(original.decisionRule as JsonObject), feasibilityPriority: {
      ...((original.decisionRule as JsonObject).feasibilityPriority as JsonObject), proposal: 'UNROUNDED_OPTIMIZER_WEIGHTS' } } },
    { ...original, researchBoundary: { ...(original.researchBoundary as JsonObject), changedFixedInput: 'REUSE_EXISTING_QUALIFICATION' } },
    { ...original, qualificationBoundary: { ...(original.qualificationBoundary as JsonObject), authorization: 'SEND_TARGETS_ON_MODELED_IMPROVEMENT' } },
    { ...original, ownerUtilityClaim: true },
  ];
  for (const value of invalid) assert.throws(() => validatePmOperatingRule(value), /original_operating_rule_missing_or_changed/);
  const validated = validatePmOperatingRule(original); (validated.clockRule as JsonObject).actualCutoff = 'changed';
  assert.equal(pmOperatingRuleIdentity(original), identity, 'a consumer cannot mutate the retained original through the validated clone');
  assert.deepEqual(pmOperatingRule(), original);
});
