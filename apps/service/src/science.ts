import { retainNativeRemoteUncertainty, withNativeRemoteTracking, withNativeProgress } from '../../research/src/native-execution.ts';
import { defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import { Application, json, object, observation, portfolio, Problem } from './facts.ts';
import { nativeCurve } from './projections.ts';
import type { PortfolioProgress, EquityPoint } from '../../../packages/contracts/src/index.ts';
import { computePortfolio } from './native.ts';
import type { PortfolioTaskInput } from './native.ts';
import { acceptReceipt, acceptSnapshot, downstream, isPreTransportRejection } from './downstream.ts';
import type { ObservationTaskInput } from './downstream.ts';
import { AdjustAccount, acceptAllocationReceipt } from './automatic.ts';
import { accountKey, CapitalExitCommand } from './capital-exit.ts';
import { classifyExecutionError, executionDiagnostic } from '../../research/src/execution-errors.ts';
import { readCatalogScope } from '../../research/plugins/native-catalog-scope.ts';
import { assertAccountTargetControl } from './account-target-control.ts';
import { PmDocument, pmState } from './pm-facts.ts';
import { reviewPortfolio, reservePortfolioReviewScience } from './portfolio-review.ts';
import { researchRuntime } from '../../research/src/research-runtime.ts';
import { checkExposure } from '../../research/plugins/native-exposure.ts';
import { canonical } from '../../research/src/documents.ts';

/** A new reviewed configuration must use its own frozen unread dataset. */
export function portfolioIndependentData(input: PortfolioTaskInput): JsonObject | undefined {
  if (!input.review) return undefined;
  const data = input.review.independentData;
  if (!data) throw new Problem('portfolio_independent_dataset_required', 422);
  if (data.nautilusTarget !== undefined && canonical(data.nautilusTarget) !== canonical(json(input.binary))) throw new Problem('portfolio_independent_target_mismatch', 422);
  if (input.policy.kind === 'catalog-ensemble' && !data.catalog_scope) throw new Problem('portfolio_independent_catalog_scope_required', 422);
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
      let scope: JsonObject | undefined;
      if (task.input.policy.kind === 'catalog-ensemble') {
        if (independentData) {
          const reference = object(independentData.catalog_scope, 'independent_catalog_scope');
          const originalScope = await runtime.getTask(reference.taskId as TaskId<JsonObject>, context);
          if (!originalScope || object(originalScope.input, 'independent_scope_input').catalog !== independentData.catalog) throw new Problem('portfolio_independent_catalog_scope_mismatch', 422);
          scope = await readCatalogScope(runtime, reference, context);
        } else {
        const source = Array.isArray(task.input.evidence) ? task.input.evidence[0] : task.input.evidence;
        const final = object(object(source!.final, 'independent_final').result, 'independent_result');
        let reference = final.catalog_scope;
        if (!reference) {
          const original = await runtime.getTask(final.evidenceTaskId as TaskId<JsonObject>, context);
          reference = object(object(original?.input, 'portfolio_input').data, 'study_data').catalog_scope;
        }
        scope = await readCatalogScope(runtime, object(reference, 'catalog_scope'), context);
        }
      }
      let phase = 'FORECASTING'; let lastSnapshotCount = 0;
      const report = async (progress: PortfolioProgress, curve?: EquityPoint[]) => {
        phase = progress.phase;
        await runtime.commit(async tx => {
          const facts = await tx.doc(Application); const fact = portfolio(facts.portfolios[task.input.id]);
          fact.view.status = 'RUNNING'; fact.view.progress = progress;
          if (curve) fact.view.curve = curve;
          facts.portfolios[task.input.id] = json(fact); return undefined;
        }, context);
      };
      const result = await withNativeProgress(async snapshots => {
        if (phase !== 'SIMULATING' || !snapshots.length || snapshots.length === lastSnapshotCount) return;
        const curve = nativeCurve({ canonical_result: { portfolio_snapshots: snapshots } }, task.input.currency);
        await report({ phase, updatedAt: new Date().toISOString() }, curve); lastSnapshotCount = snapshots.length;
      }, async () => {
        if (!independentData) return computePortfolio(task.input, context, scope, report);
        const sources = Array.isArray(task.input.evidence) ? task.input.evidence : [task.input.evidence];
        const maximumCalls = sources.reduce((sum, source) => {
          const proposal = object(object(source.selected, 'selected_trial').proposal, 'selected_proposal');
          const members = Array.isArray(proposal.members) ? proposal.members.length : 1;
          return sum + members + (members > 1 ? 1 : 0);
        }, 3);
        // Original task-keyed reservation/results survive restart. Incomplete
        // remote effects remain under the existing receipt reconciliation path.
        const saved = await reservePortfolioReviewScience(task.input, researchRuntime(runtime), context, 'independent', maximumCalls,
          async () => json(await computePortfolio(task.input, context, scope, report, { partition: 'HELDOUT', data: independentData })));
        return saved as unknown as Awaited<ReturnType<typeof computePortfolio>>;
      });
      await runtime.commit(async (tx) => {
        const facts = await tx.doc(Application); const fact = portfolio(facts.portfolios[task.input.id]);
        fact.native = result.native; fact.target = result.target;
        fact.view.curve = result.curve; fact.view.metrics = result.metrics; fact.view.capacity = result.capacity;
        fact.view.targets = ((result.target?.targets ?? []) as unknown[]).map((item) => { const target = object(item); return { instrumentId: String(target.instrument_id), weight: String(target.weight) }; });
        fact.view.progress = { phase: 'COMPLETED', updatedAt: new Date().toISOString() };
        fact.view.status = 'COMPUTED'; fact.view.error = null; fact.view.errorDetail = null;
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
export const science = defineExtension({ name: 'qz.science', tasks: [BuildPortfolio, ObserveAccount, AdjustAccount, CapitalExitCommand] });
