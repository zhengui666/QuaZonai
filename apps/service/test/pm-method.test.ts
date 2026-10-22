import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { pmAllocationMethod, pmNativeOptimizer, validatePmAllocationMethod } from '../src/pm-method.ts';
import { pmOperatingRule, validatePmOperatingRule } from '../src/pm-operating-rule.ts';
import { assertPmForwardMethod } from '../src/pm-forward-native.ts';
import { forwardSeedDecision } from '../src/pm-forward.ts';
import type { PmRecord } from '../src/pm-facts.ts';

// Method contract tests only. No optimizer result, independent qualification,
// account evidence or return is fabricated by these protocol fixtures.
test('numerical output is pre-registered only for observed funded CASH and preserves the original optimizer parameters', () => {
  const method = pmAllocationMethod(); const original = structuredClone(method);
  assert.equal(method.id, 'qz.pm.owner-nav-constrained-mean-variance/3');
  const native = pmNativeOptimizer(new Proxy(method, {}), true);
  const parameters = native.parameters as JsonObject;
  assert.equal(parameters.cash_numerical_output, 'VALIDATED_FEASIBLE_SEGMENT_V1');
  const { cash_numerical_output: _protocol, ...unchanged } = parameters;
  assert.deepEqual(unchanged, (method.optimizer as JsonObject).parameters,
    'Output representation does not change risk aversion, solver tolerance or solver iterations.');
  assert.deepEqual(pmNativeOptimizer(method, false), method.optimizer,
    'Other domains retain strict original output; CASH-only repair cannot silently apply to MARGIN.');
  parameters.solver_tolerance = '1';
  assert.deepEqual(method, original, 'Native request construction cannot mutate the retained original policy.');
  const rule = pmOperatingRule();
  assert.equal(rule.id, 'qz.pm.frozen-current-spot-operating/3');
  assert.equal(rule.allocationMethodRef, method.id);
  assert.deepEqual(rule.numericalOutput, method.numericalOutput);
});

test('old methods and old seed requests cannot acquire the new numerical protocol by filling defaults', () => {
  const method = pmAllocationMethod();
  const legacy: JsonObject = { ...method, id: 'qz.pm.owner-nav-constrained-mean-variance/2' }; delete legacy.numericalOutput;
  assert.throws(() => validatePmAllocationMethod(legacy), /pm_unregistered_allocation_method/);
  assert.throws(() => pmNativeOptimizer(legacy, true), /pm_unregistered_allocation_method/);
  assert.throws(() => validatePmAllocationMethod({ ...method, numericalOutput: null }), /pm_unregistered_allocation_method/);
  const study: JsonObject = { mandate: { constraints: { cash_funding: 'PRESERVE_MIN_CASH_AFTER_LINEAR_TURNOVER_V1' }, optimizer: pmNativeOptimizer(method, true) } };
  const seed = { allocationMethod: method, study };
  assert.doesNotThrow(() => assertPmForwardMethod(seed));
  const oldRequest = structuredClone(study);
  delete (((oldRequest.mandate as JsonObject).optimizer as JsonObject).parameters as JsonObject).cash_numerical_output;
  assert.throws(() => assertPmForwardMethod({ ...seed, study: oldRequest }), /forward_original_numerical_output_method_missing_or_changed/);
  assert.throws(() => assertPmForwardMethod({ ...seed, allocationMethod: legacy }), /pm_unregistered_allocation_method/);
  const changed = structuredClone(study);
  (((changed.mandate as JsonObject).optimizer as JsonObject).parameters as JsonObject).solver_tolerance = '0.0001';
  assert.throws(() => assertPmForwardMethod({ ...seed, study: changed }), /forward_original_numerical_output_method_missing_or_changed/);
  const legacyRule: JsonObject = { ...pmOperatingRule(), id: 'qz.pm.frozen-current-spot-operating/2' }; delete legacyRule.numericalOutput;
  assert.throws(() => validatePmOperatingRule(legacyRule), /pm_original_operating_rule_missing_or_changed/);
  const record = { mandate: { capitalSource: { kind: 'ACCOUNT' } }, policy: { validDomain: { operatingRule: legacyRule } } } as unknown as PmRecord;
  assert.equal(forwardSeedDecision(record), null, 'An older policy remains unqualified without faulting the original monitor or reading a fabricated seed.');
});
