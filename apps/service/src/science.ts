import { ScientificStage } from './scientific-stages.ts';
import { retainNativeRemoteUncertainty, withNativeRemoteTracking, withNativeProgress } from '../../research/src/native-execution.ts';
import { defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import { Application, json, object, observation, portfolio, Problem } from './facts.ts';
import { nativeCurve } from './projections.ts';
import type { PortfolioProgress, EquityPoint } from '../../../packages/contracts/src/index.ts';
import { assertPortfolioIndependentContract, computePortfolio, nativeJob } from './native.ts';
import type { PortfolioTaskInput } from './native.ts';
import { acceptReceipt, acceptSnapshot, downstream, isPreTransportRejection } from './downstream.ts';
import type { ObservationTaskInput } from './downstream.ts';
import { AdjustAccount, acceptAllocationReceipt } from './automatic.ts';
import { accountKey, CapitalExitCommand } from './capital-exit.ts';
import { classifyExecutionError, executionDiagnostic } from '../../research/src/execution-errors.ts';
import { assertAccountTargetControl } from './account-target-control.ts';
import { PmDocument, pmState } from './pm-facts.ts';
import { reviewPortfolio, reservePortfolioReviewScience } from './portfolio-review.ts';
import { researchRuntime } from '../../research/src/research-runtime.ts';
import { checkExposure } from '../../research/plugins/native-exposure.ts';
import { canonical } from '../../research/src/documents.ts';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import type { ReviewCheck, ReviewMetric, ReviewReport } from '../../../packages/contracts/src/review.ts';
import { reviewIndependentOutcomeCheck, scoreReview } from '../../../packages/contracts/src/review-scoring.ts';
import { allocationCapacityCheck } from './review-allocation.ts';

/** A new reviewed configuration must use its own frozen unread dataset. */
export function portfolioIndependentData(input: PortfolioTaskInput): JsonObject | undefined {
  if (!input.review) return undefined;
  const data = input.review.independentData;
  if (!data) throw new Problem('portfolio_independent_dataset_required', 422);
  if (data.nautilusTarget !== undefined && canonical(data.nautilusTarget) !== canonical(json(input.binary))) throw new Problem('portfolio_independent_target_mismatch', 422);
  assertPortfolioIndependentContract(input, data);
  return data;
}

const errorCode = (error: unknown) => error instanceof Problem ? error.message : 'operation_failed';
export const BuildPortfolio = defineTask<PortfolioTaskInput, { phase: 'compute' }, null>({
  name: 'qz.portfolio', version: 1, initial: () => ({ phase: 'compute' }),
  phases: { async compute(task, runtime, context) {
    return withNativeRemoteTracking(runtime.taskId, runtime, context, async () => {
    try {
      const review = await reviewPortfolio(task.input, researchRuntime(runtime), context);
      if (review) {
        await runtime.commit(async tx => {
          const facts = await tx.doc(Application); const fact = portfolio(facts.portfolios[task.input.id]);
          fact.view.review = review;
          if (review.decision !== 'PASS') {
            fact.view.status = 'REVIEW_BLOCKED'; fact.view.targets = []; fact.target = null;
            fact.view.progress = { phase: 'REVIEW_BLOCKED', updatedAt: new Date().toISOString() };
          }
          facts.portfolios[task.input.id] = json(fact); return undefined;
        }, context);
        if (review.decision !== 'PASS') {
          await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: null } }), context);
          return;
        }
      }
      const independentData = portfolioIndependentData(task.input);
      if (independentData) {
        // Grant original data read capability only after reserving the permanent
        // exposure. Recovery retains this Task identity and never refunds a read.
        await checkExposure(researchRuntime(runtime), context, independentData, true);
      }
      let phase = 'FORECASTING'; let lastSnapshotCount = 0;
      const report = async (progress: PortfolioProgress, curve?: EquityPoint[]) => {
        phase = progress.phase;
        await runtime.commit(async tx => {
          const facts = await tx.doc(Application); const fact = portfolio(facts.portfolios[task.input.id]);
          fact.view.progress = progress;
          if (curve) fact.view.curve = curve;
          facts.portfolios[task.input.id] = json(fact); return undefined;
        }, context);
      };
      const result = await withNativeProgress(async snapshots => {
        if (phase !== 'SIMULATING' || !snapshots.length || snapshots.length === lastSnapshotCount) return;
        const curve = nativeCurve({ canonical_result: { portfolio_snapshots: snapshots } }, task.input.currency);
        await report({ phase, updatedAt: new Date().toISOString() }, curve); lastSnapshotCount = snapshots.length;
      }, async () => {
        if (!independentData) return computePortfolio(task.input, context, report);
        const sources = Array.isArray(task.input.evidence) ? task.input.evidence : [task.input.evidence];
        const maximumCalls = sources.reduce((sum, source) => {
          const proposal = object(object(source.selected, 'selected_trial').proposal, 'selected_proposal');
          const members = Array.isArray(proposal.members) ? proposal.members.length : 1;
          return sum + members + (members > 1 ? 1 : 0);
        }, 3);
        // Original task-keyed reservation/results survive restart. Incomplete
        // remote effects remain under the existing receipt reconciliation path.
        const saved = await reservePortfolioReviewScience(task.input, researchRuntime(runtime), context, 'independent', maximumCalls, { kind: 'portfolio', input: task.input, partition: 'HELDOUT', data: independentData });
        return saved as unknown as Awaited<ReturnType<typeof computePortfolio>>;
      });
      let qualifiedReview: ReviewReport | null = review;
      let independentEvidence: JsonObject | null = null;
      if (review && independentData) {
        const reference = `portfolio:${task.input.id}:${review.target.version}:independent`;
        let check: ReviewCheck = { id: 'independent_outcome', status: 'UNKNOWN', reason: 'fresh_independent_statistical_evidence_missing', evidenceRefs: [reference] };
        let directory: string | undefined;
        try {
          directory = await mkdtemp(join(task.input.dataDir, 'portfolio-independent-'));
          const request = structuredClone(object(result.native.simulationRequest, 'independent_shared_account_request'));
          request.target_points = (request.target_points as JsonObject[]).map(point => ({ ...point, cash_weight: '1', targets: (point.targets as JsonObject[]).map(target => ({ ...target, weight: '0' })) }));
          const cash = await reservePortfolioReviewScience(task.input, researchRuntime(runtime), context, 'independent-cash', 1, { kind: 'native', command: 'simulate', input: request, args: ['--catalog', String(independentData.catalog)] });
          const diagnostic = await reservePortfolioReviewScience(task.input, researchRuntime(runtime), context, 'independent-outcome', 1, { kind: 'account-metrics', candidate: object(result.native.simulation), baseline: cash, evidenceRef: reference });
          const metrics = diagnostic.metrics as unknown as ReviewMetric[];
          check = reviewIndependentOutcomeCheck(review.policy, metrics, reference);
          const constraints = allocationCapacityCheck(json(result.capacity), task.input.capital, object(result.native.simulation), reference);
          if (constraints.status === 'FAIL' || check.status === 'PASS' && constraints.status !== 'PASS') check = { ...check, status: constraints.status, reason: constraints.reason };
          independentEvidence = { cash, diagnostic, validationReviewId: review.id, validationReview: json(review), constraints: json(constraints), qualification: 'ONCE_ONLY_FROZEN_PORTFOLIO_INDEPENDENT_OUTCOME' };
        } catch (error) {
          await retainNativeRemoteUncertainty(error, runtime.taskId, runtime, context);
          if (runtime.signal.aborted) throw error;
          independentEvidence = { validationReviewId: review.id, validationReview: json(review), status: 'INCOMPLETE', reason: error instanceof Error ? error.message : 'independent_evidence_unavailable' };
        } finally { if (directory) await rm(directory, { recursive: true, force: true }); }
        // No model observes this result. Keep Validation scores unchanged and
        // apply the additional once-only hard gate deterministically.
        qualifiedReview = scoreReview({ id: `${review.id}:independent`, target: { ...review.target, sourceEvidenceRefs: [...review.target.sourceEvidenceRefs, reference, `review:${review.id}`] }, policy: review.policy,
          metrics: review.metrics, checks: [...review.checks.filter(item => item.id !== 'independent_outcome'), check],
          analyses: [...review.analyses, { kind: 'INDEPENDENT_OUTCOME', status: check.status === 'UNKNOWN' ? 'INCOMPLETE' : 'COMPLETE', evidenceRef: reference, reason: check.reason, metrics: ['account.net_return_lower', 'data.coverage'] }], narrative: review.narrative, createdAt: review.createdAt });
      }
      await runtime.commit(async (tx) => {
        const facts = await tx.doc(Application); const fact = portfolio(facts.portfolios[task.input.id]);
        const approved = !qualifiedReview || qualifiedReview.decision === 'PASS';
        fact.native = independentEvidence ? { ...result.native, independentQualification: independentEvidence } : result.native; fact.target = approved ? result.target : null;
        if (qualifiedReview) fact.view.review = qualifiedReview;
        fact.view.curve = result.curve; fact.view.metrics = result.metrics; fact.view.capacity = result.capacity;
        fact.view.targets = approved ? ((result.target?.targets ?? []) as unknown[]).map((item) => { const target = object(item); return { instrumentId: String(target.instrument_id), weight: String(target.weight) }; }) : [];
        fact.view.progress = { phase: approved ? 'COMPLETED' : 'REVIEW_BLOCKED', updatedAt: new Date().toISOString() };
        fact.view.status = approved ? 'COMPUTED' : 'REVIEW_BLOCKED'; fact.view.error = null; fact.view.errorDetail = null;
        facts.portfolios[task.input.id] = json(fact);
        return { status: 'terminal', outcome: { status: 'completed', result: null } };
      }, context);
    } catch (error) {
      await retainNativeRemoteUncertainty(error, runtime.taskId, runtime, context);
        if (runtime.signal.aborted) throw error; // suspend preserves the same admitted native input
      const diagnostic = executionDiagnostic(error, 'operation_failed');
      const detail = classifyExecutionError(error, 'backtest_failed', { operation: 'portfolio-backtest' });
      await runtime.commit(async (tx) => {
        const facts = await tx.doc(Application); const fact = portfolio(facts.portfolios[task.input.id]);
        fact.view.progress = { phase: 'FAILED', updatedAt: new Date().toISOString() };
        fact.view.status = 'FAILED'; fact.view.error = diagnostic; fact.view.errorDetail = detail; facts.portfolios[task.input.id] = json(fact);
        return { status: 'terminal', outcome: { status: 'failed', error: { message: diagnostic, detail: { ...detail } } } };
      }, context);
    }
    });
  } },
  async abort(task, runtime, context) {
    await runtime.commit(async (tx) => {
      const facts = await tx.doc(Application); const fact = portfolio(facts.portfolios[task.input.id]);
      fact.view.progress = { phase: 'STOPPED', updatedAt: new Date().toISOString() };
      fact.view.status = 'STOPPED'; facts.portfolios[task.input.id] = json(fact);
      return { status: 'terminal', outcome: { status: 'aborted' } };
    }, context);
  },
});

export const ObserveAccount = defineTask<ObservationTaskInput & { refresh: boolean; accountOnly?: boolean }, { phase: 'deliver' | 'reconcile' | 'observe' }, null>({
  name: 'qz.observation', version: 1, initial: (input) => ({ phase: input.accountOnly ? 'observe' : input.refresh ? 'reconcile' : 'deliver' }),
  phases: {
    async deliver(task, runtime, context) {
      // The durable checkpoint is moved BEFORE the unsafe POST. After a crash only GET reconciliation may run.
      let stopped = false;
      await runtime.commit(async (tx) => {
        const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]);
        if (fact.view.status === 'STOPPED') {
          stopped = true; fact.view.receipt!.status = 'NOT_OFFERED'; facts.observations[task.input.id] = json(fact);
          return { status: 'terminal', outcome: { status: 'aborted' } };
        }
        try {
          assertAccountTargetControl(facts, pmState(await tx.doc(PmDocument)), fact.connection,
            { kind: 'LEGACY', observationId: fact.view.id, commandId: task.input.receiptId });
        } catch (error) {
          stopped = true; fact.view.receipt!.status = 'NOT_OFFERED'; fact.view.status = 'FAILED'; fact.view.error = errorCode(error);
          facts.observations[task.input.id] = json(fact);
          return { status: 'terminal', outcome: { status: 'failed', error: { message: errorCode(error) } } };
        }
        fact.view.receipt!.status = 'UNCERTAIN'; fact.view.status = 'UNCERTAIN'; facts.observations[task.input.id] = json(fact);
        return { status: 'running', checkpoint: { phase: 'reconcile' } };
      }, context);
      if (stopped) return;
      let checkingController = true;
      try {
        await runtime.commit(async tx => {
          const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]);
          if (fact.view.status === 'STOPPED') throw new Problem('observation_stopped', 409);
          assertAccountTargetControl(facts, pmState(await tx.doc(PmDocument)), fact.connection,
            { kind: 'LEGACY', observationId: fact.view.id, commandId: task.input.receiptId });
          return undefined;
        }, context);
        checkingController = false;
        const receipt = await downstream(task.input, 'deliver', runtime.signal);
        await runtime.commit(async (tx) => {
          const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]);
          acceptReceipt(fact, receipt); facts.observations[task.input.id] = json(fact);
          return { status: 'running', checkpoint: { phase: 'observe' } };
        }, context);
      } catch (error) {
        await retainNativeRemoteUncertainty(error, runtime.taskId, runtime, context);
        if (runtime.signal.aborted) throw error;
        await runtime.commit(async (tx) => {
          const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]);
          if (checkingController || isPreTransportRejection(error)) {
            fact.view.receipt!.status = 'NOT_OFFERED'; if (fact.view.status !== 'STOPPED') fact.view.status = 'FAILED';
          }
          fact.view.error = errorCode(error); facts.observations[task.input.id] = json(fact);
          return { status: 'terminal', outcome: { status: 'failed', error: { message: errorCode(error) } } };
        }, context);
      }
    },
    async reconcile(task, runtime, context) {
      try {
        const receipt = await downstream(task.input, 'receipt', runtime.signal);
        await runtime.commit(async (tx) => {
          const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]);
          if (task.input.allocation) acceptAllocationReceipt(fact, receipt); else acceptReceipt(fact, receipt);
          fact.view.error = null; facts.observations[task.input.id] = json(fact);
          if (fact.view.status === 'STOPPED') return { status: 'terminal', outcome: { status: 'completed', result: null } };
          return { status: 'running', checkpoint: { phase: 'observe' } };
        }, context);
      } catch (error) {
        await retainNativeRemoteUncertainty(error, runtime.taskId, runtime, context);
        if (runtime.signal.aborted) throw error;
        await runtime.commit(async (tx) => {
          const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]);
          fact.view.error = errorCode(error); facts.observations[task.input.id] = json(fact);
          return { status: 'terminal', outcome: { status: 'failed', error: { message: errorCode(error) } } };
        }, context);
      }
    },
    async observe(task, runtime, context) {
      try {
        const snapshot = await downstream(task.input, 'account', runtime.signal);
        await runtime.commit(async (tx) => {
          const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]);
          const key = accountKey(fact.connection, facts.accountControls);
          const bindings = Object.values(facts.observations).map(observation).filter(item => accountKey(item.connection, facts.accountControls) === key);
          acceptSnapshot(fact, snapshot, bindings); fact.view.error = null; facts.observations[task.input.id] = json(fact);
          return { status: 'terminal', outcome: { status: 'completed', result: null } };
        }, context);
      } catch (error) {
        await retainNativeRemoteUncertainty(error, runtime.taskId, runtime, context);
        if (runtime.signal.aborted) throw error;
        await runtime.commit(async (tx) => {
          const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]);
          fact.view.error = errorCode(error); facts.observations[task.input.id] = json(fact);
          return { status: 'terminal', outcome: { status: 'failed', error: { message: errorCode(error) } } };
        }, context);
      }
    },
  },
  async abort(task, runtime, context) {
    await runtime.commit(async (tx) => {
      const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]);
      fact.view.status = 'STOPPED'; // local observation stop is NOT a remote revocation
      if (task.state.checkpoint.phase === 'deliver') fact.view.receipt!.status = 'NOT_OFFERED';
      facts.observations[task.input.id] = json(fact);
      return { status: 'terminal', outcome: { status: 'aborted' } };
    }, context);
  },
});
export const science = defineExtension({ name: 'qz.science', tasks: [BuildPortfolio, ObserveAccount, AdjustAccount, CapitalExitCommand, ScientificStage] });
