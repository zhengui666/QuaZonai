import { defineTask } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { runReviewScience } from './review-science-adapter.ts';
import { retainNativeRemoteUncertainty, withNativeRemoteTracking } from './native-execution.ts';

/** Host-created immutable plan, never a model-supplied script or metric table. */
export interface ReviewNativePlan {
  kind: 'FACTOR' | 'SELECTION' | 'PORTFOLIO'; source: JsonObject; analysis: JsonObject;
  evidenceRef: string; target: NativeTarget;
}
export const ReviewNativeTask = defineTask<ReviewNativePlan, { phase: 'compute' }, JsonObject>({
  name: 'qz.review.science', version: 1, initial: () => ({ phase: 'compute' }),
  phases: { async compute(task, runtime, context) {
    return withNativeRemoteTracking(task.id, runtime, context, async () => {
      try {
        const result = await runReviewScience({ ...task.input, context });
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed',
          result: JSON.parse(JSON.stringify(result)) as JsonObject } }), context);
      } catch (error) {
        await retainNativeRemoteUncertainty(error, task.id, runtime, context);
        if (runtime.signal.aborted) throw error;
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: 'review_science_unavailable' } } }), context);
      }
    });
  } },
  async abort(_task, runtime, context) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context); },
});
