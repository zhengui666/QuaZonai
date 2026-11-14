import {nativeStrategyControl,assertSameNativeStrategyControl} from '../../../packages/contracts/src/native-strategy-control.ts';
import {admitNativeControlTask,assertNativeControlTask} from './native-strategy-control-phases.ts';
import {acceptNativeStrategyControl} from './native-strategy-control-state.ts';
import {nativeStrategyControlPhases,type NativeControlInput,type NativeStrategyControlTransport} from './native-strategy-control-phases.ts';
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
  status: 'PENDING' | 'UNCERTAIN' | 'COMPLETED' | 'REJECTED'; receipt: JsonObject | null; error: string | null; taskId: number;
}
const context = BACKGROUND_CONTEXT;
function assertStoredNativeTask(record:unknown,task:{id:number;conversationId:number;owner?:number;input:unknown;kind:string},key:string){
  const input=object(task.input);
  if(task.kind!=='qz.account-strategy-command'||input.key!==key)throw new Problem('native_control_task_identity_mismatch');
  assertNativeControlTask(record,{...task,input:{key,original:nativeStrategyControl(input.original)}});
}

function throwIfHistoricalHttp(connection: ConnectionConfig) {
  if (!('kind' in connection) || connection.kind !== 'RUST_PROCESS') throw new Problem('native_http_control_retired_migrate_to_rust_process', 410);
}
const commandRecord = (value: unknown) => value as CommandRecord;
function accept(record: CommandRecord, value: JsonObject) {
  assertEnvelope(record.connection, value, record.currency);
  if (value.requestId !== record.original.requestId || value.action !== record.original.action
    || !['UNCERTAIN', 'COMPLETED', 'REJECTED'].includes(String(value.status))) throw new Problem('strategy_receipt_identity_mismatch');
  if (value.persisted === false && value.status !== 'UNCERTAIN') throw new Problem('strategy_unrecorded_result_invalid');
  if (value.status === 'COMPLETED' || value.persisted === false && value.result !== null) { const result = object(value.result, 'strategy_native_result');
    if (value.action === 'RECOVER') {
      if (result.recovery_only !== true || typeof result.native_version !== 'string' || typeof result.native_session_id !== 'string') throw new Problem('account_native_recovery_result_invalid');
    } else if (typeof result.strategy_id !== 'string' || typeof result.state !== 'string' || typeof result.native_version !== 'string') throw new Problem('strategy_native_result_invalid');
  }
  if (value.status === 'REJECTED') { const result = object(value.result, 'strategy_native_rejection');
    if (!['START', 'RECOVER'].includes(String(value.action)) || result.admission !== 'REJECTED' || result.started !== false || typeof result.reason !== 'string') throw new Problem('strategy_native_rejection_invalid');
  }
  record.receipt = value; record.status = value.status as CommandRecord['status']; record.error = typeof value.error === 'string' ? value.error : null;
}
/** One original Task definition. The optional native transport is installed only
 * by an owning host; absent transport means unsupported, never a fallback route.
 * Historical key-only tasks retain the existing phases and records unchanged. */
export function createAccountStrategyCommand(nativeTransport?: NativeStrategyControlTransport) {
 const native = nativeTransport ? nativeStrategyControlPhases(nativeTransport) : null;
 return defineTask<NativeControlInput, { phase: 'send' | 'read' }, null>({
  name: 'qz.account-strategy-command', version: 1, initial: () => ({ phase: 'send' }),
  phases: {
    async send(task, runtime, ctx) {
      if (task.input.original !== undefined) {
        if (!native) throw new Problem('native_control_transport_unsupported');
        return native.send(task, runtime, ctx);
      }
      let captured!: CommandRecord;
      await runtime.commit(async tx => {
        const app = await tx.doc(Application); const record = commandRecord(app.strategyCommands![task.input.key]);
        if (object(record).kind === 'NATIVE_STRATEGY_CONTROL_V1') throw new Problem('native_control_task_identity_mismatch');
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
      if (task.input.original !== undefined) {
        if (!native) throw new Problem('native_control_transport_unsupported');
        return native.read(task, runtime, ctx);
      }
      const app = await runtime.snapshot(Application, ctx); const original = commandRecord(app!.strategyCommands![task.input.key]);
      if (object(original).kind === 'NATIVE_STRATEGY_CONTROL_V1') throw new Problem('native_control_task_identity_mismatch');
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
}
export const AccountStrategyCommand = createAccountStrategyCommand();

export function accountStrategyWorkbench(host: ResearchHost,configuredBinding?:(id:string)=>Promise<{connection:ConnectionConfig;view:{currency:string}}>, nativeTransport?:NativeStrategyControlTransport, commandTask=AccountStrategyCommand) {
  async function fact(id: string) {
    if(configuredBinding)return configuredBinding(id);
    const app = await host.harness.snapshot(Application, context); const value = app?.observations[id];
    if (!value) throw new Problem('observation_not_found', 404); return observation(value);
  }
  async function fence(id: string) {
    const binding = await fact(id); const result = await downstreamRequest(binding.connection, 'strategy/fence', 'GET');
    assertEnvelope(binding.connection, result, binding.view.currency);
    if (typeof result.accountSourceId !== 'string' || typeof result.strategyRevision !== 'string' || !/^\d+$/.test(result.strategyRevision)) throw new Problem('strategy_fence_invalid');
    return result;
  }
  async function status(id: string) {
    const binding = await fact(id); const result = await downstreamRequest(binding.connection, 'strategy', 'GET');
    assertEnvelope(binding.connection, result, binding.view.currency);
    if (typeof result.accountSourceId !== 'string' || typeof result.strategyRevision !== 'string' || !/^\d+$/.test(result.strategyRevision)) throw new Problem('strategy_fence_invalid');
    const native = object(result.strategy);
    const unregistered = native.strategy_id === null && native.state === 'UNREGISTERED' && native.recovery_only === true;
    if ((native.recovery_only === true && !unregistered) || (!unregistered && typeof native.strategy_id !== 'string') || typeof native.state !== 'string' || typeof native.native_version !== 'string') throw new Problem('native_strategy_status_invalid');
    return result;
  }
  async function orders(id:string,offsetValue:unknown){
    if(!configuredBinding)throw new Problem('native_control_transport_unsupported',409);
    const offset=offsetValue===undefined?0:Number(offsetValue);
    if(!Number.isSafeInteger(offset)||offset<0||typeof offsetValue==='object')throw new Problem('native_order_offset_invalid',422);
    const binding=await fact(id);throwIfHistoricalHttp(binding.connection);
    const result=await downstreamRequest(binding.connection,`strategy/orders/${offset}`,'GET');
    assertEnvelope(binding.connection,result,binding.view.currency);return result;
  }
  async function legacySnapshot(id:string,value:unknown) {
    if(!configuredBinding)throw new Problem('native_control_transport_unsupported',409);
    const binding=await fact(id);throwIfHistoricalHttp(binding.connection);
    const body=object(value);
    const result=await downstreamRequest(binding.connection,'strategy/legacy-snapshot','POST',{
      mode:body.mode,expectedNativeSessionId:body.expectedNativeSessionId,acknowledgeFullHistoryCost:body.acknowledgeFullHistoryCost,
      clientId:binding.connection.clientId,accountId:binding.connection.accountId});
    assertEnvelope(binding.connection,result,binding.view.currency);return result;
  }
  async function spotObservation(id: string) {
    const binding = await fact(id);
    throwIfHistoricalHttp(binding.connection);
    const result = await downstreamRequest(binding.connection, 'spot-balance-observation', 'GET');
    assertEnvelope(binding.connection, result, binding.view.currency);
    return result; // Separate original observation; never merge into Portfolio sequence.
  }
  async function cancel(id:string,value:unknown){
    if(!configuredBinding||!nativeTransport)throw new Problem('native_control_transport_unsupported',409);
    let original; try { original=nativeStrategyControl(value); } catch(error) { throw new Problem((error as Error).message,422); }
    if(original.action!=='CANCEL')throw new Problem('native_cancel_action_invalid',422);
    const binding=await fact(id);throwIfHistoricalHttp(binding.connection);
    if(binding.connection.mode!=='paper'||binding.connection.accountId!==original.order!.accountId)throw new Problem('native_cancel_account_binding_mismatch',409);
    const snapshot=(await host.harness.snapshot(Application,context))!;
    const key=`${accountKey(binding.connection,snapshot.accountControls)}:${original.requestId}`;
    const existing=snapshot.strategyCommands?.[key];
    if(existing){
      const task=await host.harness.commit(tx=>tx.task(existing.taskId as never),context);
      if(!task)throw new Problem('native_control_task_identity_mismatch');
      assertStoredNativeTask(existing,task,key);
      try { assertSameNativeStrategyControl(nativeStrategyControl(existing.original),original); } catch { throw new Problem('request_id_conflict',409); }
      return existing;
    }
    const current=await downstreamRequest(binding.connection,'strategy/fence','GET');
    if(!current.ordinaryVenueCancel)throw new Problem('native_cancel_original_session_unavailable',409);
    const currentBinding=object(current.ordinaryVenueCancel,'native_cancel_original_session_unavailable');
    if(Object.entries(original.binding).some(([key,value])=>currentBinding[key]!==value))throw new Problem('native_cancel_original_session_unavailable',409);
    await host.harness.commit(async tx=>{
      const app=await tx.doc(Application);
      const owner=await ensureCapitalWatch(tx,binding.connection,binding.view.currency);
      await admitNativeControlTask(tx,commandTask,{key,original},{conversationId:owner.conversationId,ownership:{kind:'conversation'},background:true});
    },context);
    return (await host.harness.snapshot(Application,context))!.strategyCommands![key]!;
  }
  async function command(id: string, value: unknown) {
    if(object(value).action==='CANCEL')return cancel(id,value);
    let input; try { input = accountStrategyCommand(value); } catch (error) { throw new Problem((error as Error).message, 422); }
    const binding = await fact(id);
    const snapshot = (await host.harness.snapshot(Application, context))!;
    const old = snapshot.strategyCommands?.[`${accountKey(binding.connection, snapshot.accountControls)}:${input.requestId}`];
    if (old) {
      if (canonical(commandRecord(old).original) !== canonical(json(input))) throw new Problem('request_id_conflict', 409);
      return old;
    }
    // A new historical HTTP command is refused before creating durable intent.
    // Existing request IDs above remain readable without inventing a new UNKNOWN.
    throwIfHistoricalHttp(binding.connection);
    const key = await host.harness.commit(async tx => {
      const app = await tx.doc(Application); const key = `${accountKey(binding.connection, app.accountControls)}:${input.requestId}`;
      const old = app.strategyCommands?.[key];
      if (old) { if (canonical(commandRecord(old).original) !== canonical(json(input))) throw new Problem('request_id_conflict', 409); return key; }
      if (input.action === 'START') assertBusinessAdmission(app);
      const owner = await ensureCapitalWatch(tx, binding.connection, binding.view.currency);
      const taskId = await tx.createTask(commandTask, { key }, { conversationId: owner.conversationId, ownership: { kind: 'conversation' }, background: true });
      app.strategyCommands ??= {};
      app.strategyCommands[key] = json({ original: input, connection: binding.connection, currency: binding.view.currency,
        observationId: id, status: 'PENDING', receipt: null, error: null, taskId });
      return key;
    }, context);
    const app = await host.harness.snapshot(Application, context); return app!.strategyCommands![key]!;
  }
  async function history(id:string,offsetValue:unknown){
    const offset=offsetValue===undefined?0:Number(offsetValue);
    if(!Number.isSafeInteger(offset)||offset<0)throw new Problem('native_order_offset_invalid',422);
    const binding=await fact(id);const app=(await host.harness.snapshot(Application,context))!;
    const prefix=`${accountKey(binding.connection,app.accountControls)}:`;
    // Read original Pi facts only. No transport, BOOT, or cancellation attempt.
    const items=[];let matched=0,more=false;
    const commands=app.strategyCommands??{};
    // Do not materialize every historical value/entry tuple before paging.
    // Pi still owns the full document; offset scans and key enumeration are not bounded.
    for(const key in commands){
      if(!Object.hasOwn(commands,key)||!key.startsWith(prefix))continue;
      const value=commands[key]!;
      if(object(value.original).action!=='CANCEL')continue;
      if(matched++<offset)continue;
      if(items.length===50){more=true;break;}
      items.push(view(value));
    }
    return {items,nextOffset:more?offset+50:null};
  }
  async function receipt(id: string, requestId: string) {
    const binding = await fact(id); const app = (await host.harness.snapshot(Application, context))!;
    const key = `${accountKey(binding.connection, app.accountControls)}:${requestId}`; const stored = app.strategyCommands?.[key];
    if (!stored) throw new Problem('strategy_command_not_found', 404);
    const record = commandRecord(stored);
    if(object(stored).kind==='NATIVE_STRATEGY_CONTROL_V1'){
      if(!nativeTransport)return stored;
      const original=nativeStrategyControl(stored.original);
      const value=await nativeTransport.read(original,new AbortController().signal);
      await host.harness.commit(async tx=>{
        const app=await tx.doc(Application);const task=await tx.task(stored.taskId as never);
        if(!task)throw new Problem('native_control_task_identity_mismatch');
        assertStoredNativeTask(app.strategyCommands![key],task,key);
        await acceptNativeStrategyControl(tx,key,value);
      },context);
      return (await host.harness.snapshot(Application,context))!.strategyCommands![key]!;
    }
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
  async function verifyConfiguration(id: string, requestId: string) {
    const binding = await fact(id);
    const result = await downstreamRequest(binding.connection, `strategy/config/commands/${encodeURIComponent(requestId)}/verify`, 'POST', { clientId: binding.connection.clientId, accountId: binding.connection.accountId });
    assertEnvelope(binding.connection, result, binding.view.currency);
    if (result.requestId !== requestId || result.action !== 'CONFIGURE') throw new Problem('account_configuration_receipt_mismatch');
    return result;
  }
  async function configuration(id: string, value?: unknown) {
    const binding = await fact(id);
    const body = value === undefined ? undefined : object(value);
    const result = await downstreamRequest(binding.connection, 'strategy/config', body ? 'POST' : 'GET', body ? { ...body, clientId: binding.connection.clientId, accountId: binding.connection.accountId } : undefined);
    assertEnvelope(binding.connection, result, binding.view.currency);
    return result;
  }
  return { status, fence, orders, history, legacySnapshot, spotObservation, configuration, verifyConfiguration, command: async (id: string, value: unknown) => view(await command(id, value)), receipt: async (id: string, requestId: string) => view(await receipt(id, requestId)) };
}
