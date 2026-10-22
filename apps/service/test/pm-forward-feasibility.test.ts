import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile, mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { JsonObject } from '@earendil-works/pi-durable';
import { executeNative } from '../../research/plugins/native-portfolio.ts';
import { object } from '../src/facts.ts';
import { assessPmForwardFeasibility, selectPmForwardProposal } from '../src/pm-forward-feasibility.ts';
import { pmOperatingRule } from '../src/pm-operating-rule.ts';
import { compareDecimal, multiplyDecimal, subtractDecimal } from '../../../packages/contracts/src/pm-validation.ts';

// Actual Rust results from the original observed balances and market source.
// Variants below are separately declared test methods, not runtime parameter
// search, source certification or independent investment qualification.
const directory = process.env.QZ_PM_FORWARD_FIXTURE_DIR, binary = process.env.QZ_JOB_BINARY;
const options = { timeout: 120_000, skip: !directory || !binary ? 'Requires the actual native forward fixture and job.' : false };
async function fixture(): Promise<JsonObject> { return object(JSON.parse(await readFile(join(directory!, 'source.json'), 'utf8'))); }
async function run(source: JsonObject, request: JsonObject): Promise<JsonObject> {
  const scratch = await mkdtemp(join(tmpdir(), 'qz-forward-feasibility-'));
  try { return await executeNative(binary!, 'build-observed-spot', request,
    ['--catalog', String(source.catalog), '--objects', String(source.objectsDirectory)], scratch, BACKGROUND_CONTEXT); }
  finally { await rm(scratch, { recursive: true, force: true }); }
}
function choice(native: JsonObject) {
  const comparison = object(native.rounded_modeled_comparison), hold = Number(object(comparison.hold).objective_value), proposed = Number(object(comparison.proposed).objective_value);
  const rule = object(object(pmOperatingRule().decisionRule).numericalTie);
  return selectPmForwardProposal(assessPmForwardFeasibility(native), hold, proposed,
    Number(rule.absoluteTolerance) + Number(rule.relativeTolerance) * Math.max(Math.abs(hold), Math.abs(proposed)));
}

test('actual marked-price drift breaches HOLD bounds; original rounded native proposal restores them', options, async () => {
  const source = await fixture(), native = object(source.result), before = structuredClone(native);
  const assessed = assessPmForwardFeasibility(native);
  // The engine actually bought 4.9965 BTC at 100; the original cutoff mark
  // is 200, with 500 quote cash. It was not a manually edited starting weight.
  assert.equal(object(native.valuation).total, '1499.3');
  assert.equal(assessed.hold.status, 'INFEASIBLE');
  assert.ok(assessed.hold.reasonCodes.includes('HOLD:net_exposure_above_maximum'));
  assert.ok(assessed.hold.reasonCodes.includes('HOLD:cash_below_minimum'));
  assert.equal(assessed.proposed.status, 'FEASIBLE'); assert.equal(choice(native), 'REBALANCE');
  assert.equal(assessed.amounts!.totalModeledLinearCost, '0.069804');
  assert.equal(compareDecimal(String(assessed.amounts!.postFeeNav), subtractDecimal('1499.3', '0.069804')), 0);
  assert.deepEqual(native, before);
});

test('an infeasible original HOLD cannot win on a better native modeled objective', options, async () => {
  const source = await fixture(), request = structuredClone(object(source.request));
  // This explicitly registered test variant makes expected return dominate
  // variance. Re-run the actual native optimizer; do not alter its result.
  object(object(object(request.mandate).optimizer).parameters).risk_aversion = '0.01';
  const native = await run(source, request); assert.deepEqual(native.request, request); assert.ok(native.target);
  const comparison = object(native.rounded_modeled_comparison);
  assert.ok(Number(object(comparison.hold).objective_value) < Number(object(comparison.proposed).objective_value));
  const assessed = assessPmForwardFeasibility(native);
  assert.equal(assessed.hold.status, 'INFEASIBLE'); assert.equal(assessed.proposed.status, 'FEASIBLE');
  assert.equal(choice(native), 'REBALANCE', 'Hard-bound restoration precedes comparing objective values of feasible alternatives.');
});

test('a real rounded proposal that still exceeds the cap after modeled fees cannot claim restoration', options, async () => {
  const source = await fixture(), request = structuredClone(object(source.request));
  object(request.mandate).exposure_tolerance = '0.000001';
  const native = await run(source, request); assert.ok(native.target, 'The original continuous/native lot target remains available.');
  const assessed = assessPmForwardFeasibility(native);
  assert.equal(assessed.hold.status, 'INFEASIBLE'); assert.equal(assessed.proposed.status, 'INFEASIBLE');
  assert.ok(assessed.proposed.reasonCodes.includes('ROUNDED_POST_FEE_NAV:net_exposure_above_maximum'));
  const amounts = assessed.amounts!;
  assert.ok(compareDecimal(String((amounts.roundedInventory as string[])[0]),
    multiplyDecimal('0.600001', String(amounts.postFeeNav))) > 0, 'Independent exact scalar bound, without division or a floating tolerance.');
  assert.equal(choice(native), 'NEEDS_EVIDENCE');
});
