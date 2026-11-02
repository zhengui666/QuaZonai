import assert from 'node:assert/strict';
import { validateReviewPolicy } from '../../../packages/contracts/src/review-scoring.ts';

/** The original caller's fields remain exact; only the documented admission
 * addition is accepted. The entire frozen input is still compared on replay. */
export function assertFrozenInput(actual: unknown, original: unknown): void {
  const value = structuredClone(actual) as { policy: Record<string, unknown> };
  const expected = original as { policy: Record<string, unknown> };
  if (expected.policy.reviewPolicies === undefined && value.policy.reviewPolicies !== undefined) {
    const policies = value.policy.reviewPolicies as Record<string, unknown>;
    assert.equal(validateReviewPolicy(policies.FACTOR).targetKind, 'FACTOR');
    assert.equal(validateReviewPolicy(policies.PORTFOLIO).targetKind, 'PORTFOLIO');
    delete value.policy.reviewPolicies;
  }
  assert.deepEqual(value, original);
}
