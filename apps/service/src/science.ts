import { AccountStrategyCommand } from './account-strategy.ts';
import { CapitalExitWatch } from './capital-watch.ts';
import { retainNativeRemoteUncertainty } from '../../research/src/native-execution.ts';
import { defineExtension, defineTask } from '@earendil-works/pi-durable';
import { Application, json, object, observation, portfolio, Problem } from './facts.ts';
import { acceptReceipt, acceptSnapshot, downstream } from './downstream.ts';
import type { ObservationTaskInput } from './downstream.ts';
import { AdjustAccount, acceptAllocationReceipt } from './automatic.ts';
import { accountKey, CapitalExitCommand } from './capital-exit.ts';

const errorCode = (error: unknown) => error instanceof Problem ? error.message : 'operation_failed';
export const ObserveAccount = defineTask<ObservationTaskInput & { refresh: boolean; accountOnly?: boolean }, { phase: 'deliver' | 'reconcile' | 'observe' }, null>({
  name: 'qz.observation', version: 1, initial: (input) => ({ phase: input.accountOnly ? 'observe' : input.refresh ? 'reconcile' : 'deliver' }),
  phases: {
    async deliver(task, runtime, context) {
      await runtime.commit(async tx => {
        const app = await tx.doc(Application); const fact = observation(app.observations[task.input.id]);
        if (fact.view.receipt?.status === 'UNCERTAIN') return { status: 'running', checkpoint: { phase: 'reconcile' } };
        if (fact.view.receipt?.status === 'PENDING') fact.view.receipt.status = 'NOT_OFFERED';
        fact.view.error = 'native_strategy_only'; app.observations[task.input.id] = json(fact);
        return { status: 'terminal', outcome: { status: 'completed', result: null } };
      }, context);
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
export const science = defineExtension({ name: 'qz.science', tasks: [AccountStrategyCommand, ObserveAccount, AdjustAccount, CapitalExitCommand, CapitalExitWatch] });
