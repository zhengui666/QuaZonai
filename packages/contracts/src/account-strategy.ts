/** Configures compiled upstream strategies; no QuaZonai signal or indicator engine. */
export type NativeCompiledStrategy = {
  kind: 'EMA_CROSS'; trade_size: string; fast_period: number; slow_period: number; base?: Record<string, unknown>;
} | {
  kind: 'ORDER_BOOK_IMBALANCE'; max_trade_size: string; trigger_min_size: string; trigger_imbalance_ratio: string;
  min_seconds_between_triggers: number; time_in_force: string; dry_run: boolean; base?: Record<string, unknown>;
};
export interface NativeAccountStrategyConfig { instrument_id: string; strategy: NativeCompiledStrategy }
export interface AccountStrategyCommand { requestId: string; action: 'START' | 'STOP' }
export interface NativeAccountStrategyStatus {
  strategy_id: string; state: string; native_version: string; instrument_id: string; config: NativeCompiledStrategy;
  restart_warmup: 'OFFICIAL_STRATEGY_REINITIALIZES_INDICATORS'; default_start: 'PAUSED';
}
export function accountStrategyCommand(value: unknown): AccountStrategyCommand {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('strategy_command_required');
  const row = value as Record<string, unknown>;
  if (Object.keys(row).some(key => !['requestId', 'action'].includes(key)) || typeof row.requestId !== 'string' || !row.requestId.trim()
    || !['START', 'STOP'].includes(String(row.action))) throw new Error('strategy_command_invalid');
  return { requestId: row.requestId, action: row.action as 'START' | 'STOP' };
}
