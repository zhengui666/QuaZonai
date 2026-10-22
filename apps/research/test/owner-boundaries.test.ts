import assert from 'node:assert/strict';
import { test } from 'node:test';
import { canonicalOwnerBoundary, explicitOwnerBoundaryProposals, normalizeOwnerBoundaryProposal, ownerBoundaryRefines } from '../../../packages/contracts/src/pm-intent.ts';

const boundary = { id: 'original', kind: 'LEVERAGE' as const, sourceStatement: 'No leverage.', effectiveFromNs: '0', value: { maximumGrossNavRatio: '1', borrowingAllowed: false } };
test('owner leverage constraints reject invalid values and cannot be relaxed through clarification', () => {
  for (const value of [{ maximumGrossNavRatio: '-1', borrowingAllowed: false }, { maximumGrossNavRatio: 1, borrowingAllowed: false },
    { maximumGrossNavRatio: '1', borrowingAllowed: 'false' }, { maximumGrossNavRatio: 'NaN', borrowingAllowed: false }]) {
    assert.throws(() => canonicalOwnerBoundary({ ...boundary, value }));
  }
  assert.equal(ownerBoundaryRefines(boundary, { ...boundary, value: { maximumGrossNavRatio: '0.5', borrowingAllowed: false } }), true);
  assert.equal(ownerBoundaryRefines(boundary, { ...boundary, value: { maximumGrossNavRatio: '2', borrowingAllowed: false } }), false);
  assert.equal(ownerBoundaryRefines(boundary, { ...boundary, value: { maximumGrossNavRatio: '1', borrowingAllowed: true } }), false);
  assert.equal(ownerBoundaryRefines(boundary, { ...boundary, sourceStatement: 'Borrow freely.' }), false);
});
test('model normalization is tied to an exact owner statement and preserves unknown or future-loss intent', () => {
  const context = { id: 'new', effectiveFromNs: '1', sourceStatements: ['Research allocations. No leverage.'] };
  assert.deepEqual(normalizeOwnerBoundaryProposal({ kind: 'LEVERAGE', sourceStatement: 'No leverage.' }, context)!.value, boundary.value);
  assert.throws(() => normalizeOwnerBoundaryProposal({ kind: 'LEVERAGE', sourceStatement: 'No leverage.', normalizedValue: { maximumGrossNavRatio: '2', borrowingAllowed: true } }, context), /normalization_conflict/);
  assert.throws(() => normalizeOwnerBoundaryProposal({ kind: 'LEVERAGE', sourceStatement: '不要杠杆' }, context), /source_not_in_original_goal/);
  assert.equal(normalizeOwnerBoundaryProposal({ kind: 'LOSS', sourceStatement: 'Future drawdown below 5%.' }, { ...context, sourceStatements: ['Future drawdown below 5%.'] }), null);
  assert.equal(normalizeOwnerBoundaryProposal({ kind: 'LEVERAGE', sourceStatement: 'Not no leverage.' }, { ...context, sourceStatements: ['Not no leverage.'] }), null);
});


test('finite source discovery preserves exact affirmative clauses and does not invent intent from substrings or questions', () => {
  assert.deepEqual(explicitOwnerBoundaryProposals(['Research returns. No leverage.', '研究账户。不要杠杆。']), [
    { kind: 'LEVERAGE', sourceStatement: 'No leverage.' }, { kind: 'LEVERAGE', sourceStatement: '不要杠杆。' },
  ]);
  for (const source of ['Not no leverage.', 'No leverage?', 'Do we want no leverage.', '不是不要杠杆。', 'Future drawdown below 5%.']) assert.deepEqual(explicitOwnerBoundaryProposals([source]), []);
});
