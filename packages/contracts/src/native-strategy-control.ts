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
  action: 'START' | 'STOP';
  strategyId: string;
  binding: NativeSessionBinding;
}

const bindingKeys = ['sourceId', 'environmentId', 'buildId', 'nativeSessionId'] as const;
const commandKeys = ['schemaVersion', 'requestId', 'action', 'strategyId', 'binding'];
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
    || (command.action !== 'START' && command.action !== 'STOP')
    || bindingKeys.some(key => !identity(binding[key]))) throw new Error('native_strategy_control_invalid');
  return structuredClone(value) as NativeStrategyControl;
}

/** Used for both duplicate admission and reply matching against the ORIGINAL
 * persisted command. A valid native response is not authorization to change IDs.
 * A missing reply remains UNKNOWN in the existing Pi task; never retry here.
 */
export function assertSameNativeStrategyControl(original: NativeStrategyControl, received: unknown): void {
  const candidate = nativeStrategyControl(received);
  if (original.schemaVersion !== candidate.schemaVersion || original.requestId !== candidate.requestId
    || original.action !== candidate.action || original.strategyId !== candidate.strategyId
    || bindingKeys.some(key => original.binding[key] !== candidate.binding[key])) {
    throw new Error('native_strategy_control_identity_mismatch');
  }
}
