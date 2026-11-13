import assert from 'node:assert/strict';
import test from 'node:test';
import * as review from '../../../packages/contracts/src/review-scoring.ts';
import * as forward from '../../../packages/contracts/src/pm-forward.ts';

test('retired review mathematics and PM execution qualification are not executable exports', () => {
  for (const name of ['scoreReview', 'deriveReviewPolicyFromReference', 'reviewIndependentOutcomeCheck', 'buildReviewAllocationRequest']) assert.equal(name in review, false);
  assert.equal('assessPmExecutionQualification' in forward, false);
  assert.equal('validatePmExecutionEvidence' in forward, false);
  assert.equal(typeof review.validateReviewPolicy, 'function');
});
