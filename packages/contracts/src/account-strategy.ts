/** Configures compiled upstream strategies; no QuaZonai signal or indicator engine. */
export type NativeCompiledStrategy = { kind: 'REGISTERED'; name: string; config: unknown } | {
  kind: 'EMA_CROSS'; trade_size: string; fast_period: number; slow_period: number; base?: Record<string, unknown>;
} | {
  kind: 'ORDER_BOOK_IMBALANCE'; max_trade_size: string; trigger_min_size: string; trigger_imbalance_ratio: string;
  min_seconds_between_triggers: number; time_in_force: string; dry_run: boolean; base?: Record<string, unknown>;
};
export interface NativeAccountStrategyConfig { instrument_id: string; strategy: NativeCompiledStrategy }
export type AccountStrategyCommand = { requestId: string; action: 'STOP' } | { requestId: string; action: 'START' | 'RECOVER'; expectedAccountSourceId: string; expectedStrategyRevision: string };
export interface NativeAccountStrategyStatus {
  strategy_id: string | null; state: string; recovery_only?: boolean; native_version: string; instrument_id: string; config: NativeCompiledStrategy;
  restart_warmup?: 'OFFICIAL_STRATEGY_REINITIALIZES_INDICATORS' | 'UPSTREAM_STRATEGY_DEFINED'; default_start: 'PAUSED' | 'UNREGISTERED';
}
export function accountStrategyCommand(value: unknown): AccountStrategyCommand {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('strategy_command_required');
  const row = value as Record<string, unknown>;
  if (Object.keys(row).some(key => !['requestId', 'action', 'expectedAccountSourceId', 'expectedStrategyRevision'].includes(key)) || typeof row.requestId !== 'string' || !row.requestId.trim()
    || !['START', 'STOP', 'RECOVER'].includes(String(row.action))) throw new Error('strategy_command_invalid');
  if (row.action === 'START' || row.action === 'RECOVER') {
    if (typeof row.expectedAccountSourceId !== 'string' || !row.expectedAccountSourceId.trim() || typeof row.expectedStrategyRevision !== 'string' || !/^\d+$/.test(row.expectedStrategyRevision)) throw new Error('strategy_start_fence_required');
    return { requestId: row.requestId, action: row.action, expectedAccountSourceId: row.expectedAccountSourceId, expectedStrategyRevision: row.expectedStrategyRevision };
  }
  if (row.expectedAccountSourceId !== undefined || row.expectedStrategyRevision !== undefined) throw new Error('strategy_stop_fence_not_used');
  return { requestId: row.requestId, action: 'STOP' };
}

/** Preserve native configuration verbatim; this is not runner capability validation. */
export function nativeAccountStrategyConfiguration(value: unknown): NativeAccountStrategyConfig {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('account_strategy_config_required');
  const row = value as Record<string, unknown>;
  if (Object.keys(row).some(key => !['instrument_id', 'strategy'].includes(key)) || typeof row.instrument_id !== 'string' || !row.instrument_id.trim()) throw new Error('account_strategy_instrument_required');
  if (!row.strategy || typeof row.strategy !== 'object' || Array.isArray(row.strategy)) throw new Error('account_strategy_config_required');
  const strategy = row.strategy as Record<string, unknown>;
  if (!['REGISTERED', 'EMA_CROSS', 'ORDER_BOOK_IMBALANCE'].includes(String(strategy.kind))) throw new Error('account_strategy_kind_unsupported');
  if (strategy.kind === 'REGISTERED' && (typeof strategy.name !== 'string' || !strategy.name.trim() || !Object.hasOwn(strategy, 'config') || Object.keys(strategy).some(key => !['kind','name','config'].includes(key)))) throw new Error('account_strategy_registered_config_invalid');
  return structuredClone(value) as NativeAccountStrategyConfig;
}
