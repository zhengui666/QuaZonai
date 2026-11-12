import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { defineTask } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { ResearchHost } from '../../research/src/host.ts';
import { canonical } from '../../research/src/documents.ts';
import { accountStrategyCommand } from '../../../packages/contracts/src/account-strategy.ts';
import { Application, json, object, observation, Problem } from './facts.ts';
import type { ConnectionConfig } from './config.ts';
import { accountKey, assertEnvelope } from './capital-exit.ts';
import { ensureCapitalWatch } from './capital-watch.ts';
import { downstreamRequest } from './downstream.ts';
import { assertBusinessAdmission } from './pm-maintenance.ts';

interface CommandRecord {
  original: JsonObject; connection: ConnectionConfig; currency: string; observationId: string;
  status: 'PENDING' | 'UNCERTAIN' | 'COMPLETED'; receipt: JsonObject | null; error: string | null; taskId: number;
}
const context = BACKGROUND_CONTEXT;
const commandRecord = (value: unknown) => value as CommandRecord;
function accept(record: CommandRecord, value: JsonObject) {
  assertEnvelope(record.connection, value, record.currency);
  if (value.requestId !== record.original.requestId || value.action !== record.original.action
    || !['UNCERTAIN', 'COMPLETED'].includes(String(value.status))) throw new Problem('strategy_receipt_identity_mismatch');
  if (value.status === 'COMPLETED') { const result = object(value.result, 'strategy_native_result');
    if (typeof result.strategy_id !== 'string' || typeof result.state !== 'string' || typeof result.native_version !== 'string') throw new Problem('strategy_native_result_invalid');
  }
  record.receipt = value; record.status = value.status as CommandRecord['status']; record.error = typeof value.error === 'string' ? value.error : null;
}
export const AccountStrategyCommand = defineTask<{ key: string }, { phase: 'send' | 'read' }, null>({
  name: 'qz.account-strategy-command', version: 1, initial: () => ({ phase: 'send' }),
  phases: {
    async send(task, runtime, ctx) {
      let captured!: CommandRecord;
      await runtime.commit(async tx => {
        const app = await tx.doc(Application); const record = commandRecord(app.strategyCommands![task.input.key]);
        if (record.original.action === 'START') assertBusinessAdmission(app);
        record.status = 'UNCERTAIN'; app.strategyCommands![task.input.key] = json(record); captured = json(record) as unknown as CommandRecord;
        return { status: 'running', checkpoint: { phase: 'read' } };
      }, ctx);
      try {
        const receipt = await downstreamRequest(captured.connection, 'strategy/commands', 'POST', {
          ...captured.original, clientId: captured.connection.clientId, accountId: captured.connection.accountId,
        }, runtime.signal);
        await runtime.commit(async tx => {
          const app = await tx.doc(Application); const record = commandRecord(app.strategyCommands![task.input.key]);
          accept(record, receipt); app.strategyCommands![task.input.key] = json(record);
          return { status: 'terminal', outcome: { status: 'completed', result: null } };
        }, ctx);
      } catch (error) {
        if (runtime.signal.aborted) throw error;
        await runtime.commit(async tx => {
          const app = await tx.doc(Application); const record = commandRecord(app.strategyCommands![task.input.key]);
          record.error = error instanceof Error ? error.message : 'strategy_outcome_uncertain'; app.strategyCommands![task.input.key] = json(record);
          return { status: 'running', checkpoint: { phase: 'read' } };
        }, ctx);
      }
    },
    async read(task, runtime, ctx) {
      const app = await runtime.snapshot(Application, ctx); const original = commandRecord(app!.strategyCommands![task.input.key]);
      try {
        const receipt = await downstreamRequest(original.connection, `strategy/commands/${encodeURIComponent(String(original.original.requestId))}`, 'GET', undefined, runtime.signal);
        await runtime.commit(async tx => {
          const app = await tx.doc(Application); const record = commandRecord(app.strategyCommands![task.input.key]);
          accept(record, receipt); app.strategyCommands![task.input.key] = json(record);
          return { status: 'terminal', outcome: { status: 'completed', result: null } };
        }, ctx);
      } catch (error) {
        if (runtime.signal.aborted) throw error;
        await runtime.commit(async tx => {
          const app = await tx.doc(Application); const record = commandRecord(app.strategyCommands![task.input.key]);
          record.error = error instanceof Error ? error.message : 'strategy_receipt_unavailable'; app.strategyCommands![task.input.key] = json(record);
          return { status: 'terminal', outcome: { status: 'failed', error: { message: record.error } } };
        }, ctx);
      }
    },
  },
  async abort(_task, runtime, ctx) {
    // Stopping a local waiter does not prove a remote strategy stopped.
    await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx);
  },
});

export function accountStrategyWorkbench(host: ResearchHost) {
  async function fact(id: string) {
    const app = await host.harness.snapshot(Application, context); const value = app?.observations[id];
    if (!value) throw new Problem('observation_not_found', 404); return observation(value);
  }
  async function status(id: string) {
    const binding = await fact(id); const result = await downstreamRequest(binding.connection, 'strategy', 'GET');
    assertEnvelope(binding.connection, result, binding.view.currency);
    const native = object(result.strategy);
    if (typeof native.strategy_id !== 'string' || typeof native.state !== 'string' || typeof native.native_version !== 'string') throw new Problem('native_strategy_status_invalid');
    return result;
  }
  async function command(id: string, value: unknown) {
    let input; try { input = accountStrategyCommand(value); } catch (error) { throw new Problem((error as Error).message, 422); }
    const binding = await fact(id);
    const key = await host.harness.commit(async tx => {
      const app = await tx.doc(Application); const key = `${accountKey(binding.connection, app.accountControls)}:${input.requestId}`;
      const old = app.strategyCommands?.[key];
      if (old) { if (canonical(commandRecord(old).original) !== canonical(json(input))) throw new Problem('request_id_conflict', 409); return key; }
      if (input.action === 'START') assertBusinessAdmission(app);
      const owner = await ensureCapitalWatch(tx, binding.connection, binding.view.currency);
      const taskId = await tx.createTask(AccountStrategyCommand, { key }, { conversationId: owner.conversationId, ownership: { kind: 'conversation' }, background: true });
      app.strategyCommands ??= {};
      app.strategyCommands[key] = json({ original: input, connection: binding.connection, currency: binding.view.currency,
        observationId: id, status: 'PENDING', receipt: null, error: null, taskId });
      return key;
    }, context);
    const app = await host.harness.snapshot(Application, context); return app!.strategyCommands![key]!;
  }
  async function receipt(id: string, requestId: string) {
    const binding = await fact(id); const app = (await host.harness.snapshot(Application, context))!;
    const key = `${accountKey(binding.connection, app.accountControls)}:${requestId}`; const stored = app.strategyCommands?.[key];
    if (!stored) throw new Problem('strategy_command_not_found', 404);
    const record = commandRecord(stored);
    if (record.status === 'UNCERTAIN') {
      const result = await downstreamRequest(record.connection, `strategy/commands/${encodeURIComponent(requestId)}`, 'GET');
      accept(record, result);
      await host.harness.commit(async tx => { const app = await tx.doc(Application); const current = commandRecord(app.strategyCommands![key]);
        if (current.status !== 'COMPLETED') { accept(current, result); app.strategyCommands![key] = json(current); }
      }, context);
    }
    return json(record);
  }
  // Credentials and internal task metadata are not part of the public receipt.
  const view = (value: JsonObject) => { const record = commandRecord(value); return { ...record.original, status: record.status, receipt: record.receipt, error: record.error }; };
  return { status, command: async (id: string, value: unknown) => view(await command(id, value)), receipt: async (id: string, requestId: string) => view(await receipt(id, requestId)) };
}
