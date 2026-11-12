/** Configures compiled upstream strategies; no QuaZonai signal or indicator engine. */
export type NativeCompiledStrategy = {
  kind: 'EMA_CROSS'; trade_size: string; fast_period: number; slow_period: number; base?: Record<string, unknown>;
} | {
  kind: 'ORDER_BOOK_IMBALANCE'; max_trade_size: string; trigger_min_size: string; trigger_imbalance_ratio: string;
  min_seconds_between_triggers: number; time_in_force: string; dry_run: boolean; base?: Record<string, unknown>;
};
export interface NativeAccountStrategyConfig { instrument_id: string; strategy: NativeCompiledStrategy }
export type AccountStrategyCommand = { requestId: string; action: 'STOP' } | { requestId: string; action: 'START'; expectedAccountSourceId: string; expectedStrategyRevision: string };
export interface NativeAccountStrategyStatus {
  strategy_id: string; state: string; native_version: string; instrument_id: string; config: NativeCompiledStrategy;
  restart_warmup: 'OFFICIAL_STRATEGY_REINITIALIZES_INDICATORS'; default_start: 'PAUSED';
}
export function accountStrategyCommand(value: unknown): AccountStrategyCommand {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('strategy_command_required');
  const row = value as Record<string, unknown>;
  if (Object.keys(row).some(key => !['requestId', 'action', 'expectedAccountSourceId', 'expectedStrategyRevision'].includes(key)) || typeof row.requestId !== 'string' || !row.requestId.trim()
    || !['START', 'STOP'].includes(String(row.action))) throw new Error('strategy_command_invalid');
  if (row.action === 'START') {
    if (typeof row.expectedAccountSourceId !== 'string' || !row.expectedAccountSourceId.trim() || typeof row.expectedStrategyRevision !== 'string' || !/^\d+$/.test(row.expectedStrategyRevision)) throw new Error('strategy_start_fence_required');
    return { requestId: row.requestId, action: 'START', expectedAccountSourceId: row.expectedAccountSourceId, expectedStrategyRevision: row.expectedStrategyRevision };
  }
  if (row.expectedAccountSourceId !== undefined || row.expectedStrategyRevision !== undefined) throw new Error('strategy_stop_fence_not_used');
  return { requestId: row.requestId, action: 'STOP' };
}
