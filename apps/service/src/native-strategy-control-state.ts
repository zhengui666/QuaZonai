/** Internal helpers for the EXISTING Pi account-command task/doc.
 * No route, transport, session discovery, new task type or separate ledger.
 * The future caller must commit its read-phase checkpoint in the SAME transaction
 * as prepare, before sending. These helpers do not authorize a native session.
 */
import type { JsonObject, Tx } from '@earendil-works/pi-durable';
import { nativeStrategyControl, assertSameNativeStrategyControl, type NativeStrategyControl } from '../../../packages/contracts/src/native-strategy-control.ts';
import { Application, object, json, Problem } from './facts.ts';

const kind = 'NATIVE_STRATEGY_CONTROL_V1';
function retained(value: unknown) {
  const record = object(value, 'native_control_record_missing');
  if (record.kind !== kind || (record.status !== 'PENDING' && record.status !== 'UNCERTAIN' && record.status !== 'COMPLETED')) throw new Problem('native_control_record_invalid');
  const original = nativeStrategyControl(record.original);
  return { record, original };
}

export async function prepareNativeStrategyControl(tx: Tx, key: string, expected: NativeStrategyControl): Promise<{ send: boolean; original: NativeStrategyControl }> {
  const app = await tx.doc(Application);
  const { record, original } = retained(app.strategyCommands?.[key]);
  assertSameNativeStrategyControl(original, expected);
  if (record.status !== 'PENDING') return { send: false, original };
  // Keep task, owner, source, original command and historical fields untouched.
  record.status = 'UNCERTAIN'; record.error = null;
  app.strategyCommands![key] = json(record);
  return { send: true, original };
}

export async function acceptNativeStrategyControl(tx: Tx, key: string, value: JsonObject): Promise<void> {
  const app = await tx.doc(Application);
  const { record, original } = retained(app.strategyCommands?.[key]);
  assertSameNativeStrategyControl(original, value.command);
  if (record.status === 'PENDING') throw new Problem('native_control_reply_before_dispatch');
  if (value.status !== 'COMPLETED' && value.status !== 'UNKNOWN') throw new Problem('native_control_reply_invalid');
  // Queue dispatch returns no durable cancellation receipt. Until an official
  // event reader is installed, CANCEL may only retain UNKNOWN, never completion.
  if (value.status === 'COMPLETED' && original.action === 'CANCEL') throw new Problem('native_cancel_observation_unavailable');
  if (value.status === 'COMPLETED') {
    const result = object(value.result, 'native_control_result_required');
    if (value.persisted !== true || result.strategyId !== original.strategyId || result.success !== true) throw new Problem('native_control_unrecorded_or_wrong_result');
  }
  if (record.status === 'COMPLETED') return; // A late UNKNOWN cannot regress an accepted receipt.
  record.status = value.status === 'COMPLETED' ? 'COMPLETED' : 'UNCERTAIN';
  record.receipt = value;
  record.error = typeof value.error === 'string' ? value.error : null;
  app.strategyCommands![key] = json(record);
}
