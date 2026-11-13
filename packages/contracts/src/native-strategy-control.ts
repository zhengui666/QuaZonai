/** Internal candidate; no production route accepts this contract yet.
 * One explicit command for an existing official StrategyId. This is neither a
 * native configuration schema nor a durable task/receipt ledger.
 */
export interface NativeSessionBinding {
  sourceId: string;
  environmentId: string;
  buildId: string;
  nativeSessionId: string;
}

export interface NativeStrategyControl {
  schemaVersion: 1;
  requestId: string;
  action: 'START' | 'STOP' | 'CANCEL';
  order?: { traderId: string; accountId: string; instrumentId: string; clientOrderId: string };
  strategyId: string;
  binding: NativeSessionBinding;
}

const bindingKeys = ['sourceId', 'environmentId', 'buildId', 'nativeSessionId'] as const;
const commandKeys = ['schemaVersion', 'requestId', 'action', 'strategyId', 'binding', 'order'];
const orderKeys = ['traderId', 'accountId', 'instrumentId', 'clientOrderId'] as const;
function row(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('native_strategy_control_invalid');
  return value as Record<string, unknown>;
}
function identity(value: unknown): value is string {
  return typeof value === 'string' && value.trim().length > 0;
}

/** Validate without normalizing original IDs or introducing a currency scope. */
export function nativeStrategyControl(value: unknown): NativeStrategyControl {
  const command = row(value); const binding = row(command.binding);
  if (Object.keys(command).some(key => !commandKeys.includes(key))
    || Object.keys(binding).some(key => !bindingKeys.includes(key as typeof bindingKeys[number]))
    || command.schemaVersion !== 1 || !identity(command.requestId) || !identity(command.strategyId)
    || (command.action !== 'START' && command.action !== 'STOP' && command.action !== 'CANCEL')
    || bindingKeys.some(key => !identity(binding[key]))) throw new Error('native_strategy_control_invalid');
  let order: NativeStrategyControl['order'];
  if (command.action === 'CANCEL') {
    const value = row(command.order);
    if (Object.keys(value).some(key => !orderKeys.includes(key as typeof orderKeys[number]))
      || orderKeys.some(key => !identity(value[key]))) throw new Error('native_strategy_control_invalid');
    order = { traderId: value.traderId as string, accountId: value.accountId as string,
      instrumentId: value.instrumentId as string, clientOrderId: value.clientOrderId as string };
  } else if ('order' in command) throw new Error('native_strategy_control_invalid');
  // Pi documents can be proxy objects. Copy the validated primitive fields,
  // not the proxy itself; preserve exact string values without normalization.
  return { schemaVersion: 1, requestId: command.requestId as string, action: command.action as NativeStrategyControl['action'],
    ...(order ? { order } : {}),
    strategyId: command.strategyId as string, binding: {
      sourceId: binding.sourceId as string, environmentId: binding.environmentId as string,
      buildId: binding.buildId as string, nativeSessionId: binding.nativeSessionId as string,
    } };
}

/** Used for both duplicate admission and reply matching against the ORIGINAL
 * persisted command. A valid native response is not authorization to change IDs.
 * A missing reply remains UNKNOWN in the existing Pi task; never retry here.
 */
export function assertSameNativeStrategyControl(original: NativeStrategyControl, received: unknown): void {
  const candidate = nativeStrategyControl(received);
  if (original.schemaVersion !== candidate.schemaVersion || original.requestId !== candidate.requestId
    || original.action !== candidate.action || original.strategyId !== candidate.strategyId
    || bindingKeys.some(key => original.binding[key] !== candidate.binding[key])
    || orderKeys.some(key => original.order?.[key] !== candidate.order?.[key])) {
    throw new Error('native_strategy_control_identity_mismatch');
  }
}
