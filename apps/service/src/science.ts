import { defineExtension, defineTask } from '@earendil-works/pi-durable';
import { Application, json, object, observation, portfolio, Problem } from './facts.ts';
import { computePortfolio } from './native.ts';
import type { PortfolioTaskInput } from './native.ts';
import { acceptReceipt, acceptSnapshot, downstream, isPreTransportRejection } from './downstream.ts';
import type { ObservationTaskInput } from './downstream.ts';
import { AdjustAccount, acceptAllocationReceipt } from './automatic.ts';
import { accountKey, CapitalExitCommand } from './capital-exit.ts';

const errorCode = (error: unknown) => error instanceof Problem ? error.message : 'operation_failed';
export const BuildPortfolio = defineTask<PortfolioTaskInput, { phase: 'compute' }, null>({
  name: 'qz.portfolio', version: 1, initial: () => ({ phase: 'compute' }),
  phases: { async compute(task, runtime, context) {
    try {
      const result = await computePortfolio(task.input, context);
      await runtime.commit(async (tx) => {
        const facts = await tx.doc(Application); const fact = portfolio(facts.portfolios[task.input.id]);
        fact.native = result.native; fact.target = result.target;
        fact.view.curve = result.curve; fact.view.metrics = result.metrics; fact.view.capacity = result.capacity;
        fact.view.targets = (result.target.targets as unknown[]).map((item) => { const target = object(item); return { instrumentId: String(target.instrument_id), weight: String(target.weight) }; });
        fact.view.status = 'COMPUTED'; fact.view.error = null;
        facts.portfolios[task.input.id] = json(fact);
        return { status: 'terminal', outcome: { status: 'completed', result: null } };
      }, context);
    } catch (error) {
      if (runtime.signal.aborted) throw error; // suspend preserves the same admitted native input
      await runtime.commit(async (tx) => {
        const facts = await tx.doc(Application); const fact = portfolio(facts.portfolios[task.input.id]);
        fact.view.status = 'FAILED'; fact.view.error = errorCode(error); facts.portfolios[task.input.id] = json(fact);
        return { status: 'terminal', outcome: { status: 'failed', error: { message: errorCode(error) } } };
      }, context);
    }
  } },
  async abort(task, runtime, context) {
    await runtime.commit(async (tx) => {
      const facts = await tx.doc(Application); const fact = portfolio(facts.portfolios[task.input.id]);
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
        fact.view.receipt!.status = 'UNCERTAIN'; fact.view.status = 'UNCERTAIN'; facts.observations[task.input.id] = json(fact);
        return { status: 'running', checkpoint: { phase: 'reconcile' } };
      }, context);
      if (stopped) return;
      try {
        const receipt = await downstream(task.input, 'deliver', runtime.signal);
        await runtime.commit(async (tx) => {
          const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]);
          acceptReceipt(fact, receipt); facts.observations[task.input.id] = json(fact);
          return { status: 'running', checkpoint: { phase: 'observe' } };
        }, context);
      } catch (error) {
        if (runtime.signal.aborted) throw error;
        await runtime.commit(async (tx) => {
          const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]);
          if (isPreTransportRejection(error)) {
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
