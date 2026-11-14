/** A string is an explicit local executable for the independent worker and
 * scientific tests. Application configuration admits remote targets only. */
export type RemoteNativeTarget = { id: string; url: string; tokenEnv?: string; credentialRef?: string };
/** Owner-configured instance. `tokenEnv` remains for older service files. */
export type RemoteNativeInstance = { id: string; url: string; token?: string; tokenEnv?: string };
export type ProcessNativeTarget = {
  kind: 'RUST_PROCESS'; id: string; binary: string;
  /** Exact result of the explicitly inspected user-built runner, not a claimed live state. */
  capabilities: { schemaVersion: 1; protocol: 'quazonai-native-process-v1'; identity: NativeJson;
    research: { singleStrategyRegistration: boolean; configuredRunFactory: boolean };
    account: { compiled: boolean; registration: boolean; command: 'account-runtime'; requiresExplicitStart: true; compatibilityDeclaration?: 'TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1' | null; compatibilityVerified?: false }; running: false };
}
export type NativeExecutionTarget = RemoteNativeTarget | ProcessNativeTarget;
export type NativeTarget = string | NativeExecutionTarget;

export type NativeJson = null | boolean | number | string | NativeJson[] | { [key: string]: NativeJson };
export interface NativeCapabilities {
  schemaVersion: 1;
  implementationDigest: string;
  commands: string[];
  buildIdentity?: NativeJson;
}
export interface NativeExecutionRequest {
  schemaVersion: 1;
  requestId: string;
  /** Fresh for each scientific invocation; receipt replay retains this value. */
  invocationId: string;
  implementationDigest: string;
  command: string;
  input: NativeJson;
  args: string[];
  /** Base64 artifacts; paths in args use @file:name or @objects:prefix. */
  files: Record<string, string>;
}
export type NativeExecutionReceipt = {
  schemaVersion: 1;
  requestId: string;
} & (
  { status: 'COMPLETED'; result: NativeJson } |
  { status: 'FAILED' | 'UNKNOWN'; error: string } |
  { status: 'RUNNING' | 'CANCELLED'; progress?: { snapshots: NativeJson[] } }
);

/** Authoritative admission refusal, before any execution receipt or process.
 * This is not a terminal execution receipt. Generic HTTP errors carry no such proof. */
export interface NativeExecutionRejection {
  schemaVersion: 1;
  requestId: string;
  admission: 'REJECTED';
  error: 'nautilus_tick_capacity_exhausted' | 'nautilus_tick_resource_limits_unavailable';
}

/** Tick replay uses the configured precision companion and worker limits. */
export function isTickNativeCommand(command: string): boolean {
  return command === 'native-strategy-backtest' || command === 'native-configured-backtest';
}

/** Official LiveRiskEngine limits; omitted fields retain upstream defaults. */
export interface NativeAccountRiskConfig {
  max_order_submit_rate?: string;
  max_order_modify_rate?: string;
  max_notional_per_order?: Record<string, string>;
}
/** Official Sandbox scenario, rejected for live accounts. Seeds and nanoseconds are exact strings. */
export interface NativePaperExecutionConfig {
  prob_fill_on_limit: number; prob_slippage: number; random_seed: string;
  base_latency_ns: string; insert_latency_ns: string; update_latency_ns: string; cancel_latency_ns: string;
  queue_position?: boolean;
}
/** POST /accounts/<id>/sizing/fixed-risk. Inputs are hypothetical, not observed balances. */
export interface NativeFixedRiskSizingRequest {
  clientId: string; accountId: string;
  sizing: {
    instrument_id: string; entry: string; stop_loss: string; equity: string; currency: string;
    /** Fraction of scenario equity, greater than zero and at most one. */
    risk: string; commission_rate: string; exchange_rate: string; unit_batch_size: string; hard_limit?: string | null;
  };
}
export interface NativeFixedRiskSizingResponse {
  instrument_id: string; quantity: string; model: 'NAUTILUS_FIXED_RISK';
  preview_only: true; input_source: 'REQUEST_SCENARIO'; account_balance_verified: false;
}

/** POST /api/v3/observations/<id>/sizing/fixed-risk. The service binds account identity. */
export type ObservationFixedRiskSizingRequest = Pick<NativeFixedRiskSizingRequest, 'sizing'>;

/** Closed native sizing payload. In particular, transport command fields are never scenario fields. */
export function nativeFixedRiskSizing(value: unknown): NativeFixedRiskSizingRequest['sizing'] {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('native_sizing_object_required');
  const input = value as Record<string, unknown>;
  const required = ['instrument_id', 'entry', 'stop_loss', 'equity', 'currency', 'risk', 'commission_rate', 'exchange_rate', 'unit_batch_size'] as const;
  if (Object.keys(input).some(key => !required.includes(key as typeof required[number]) && key !== 'hard_limit')) throw new Error('native_sizing_unknown_field');
  if (required.some(key => typeof input[key] !== 'string' || input[key] === '')
    || input.hard_limit != null && (typeof input.hard_limit !== 'string' || input.hard_limit === '')) throw new Error('native_sizing_string_required');
  return Object.fromEntries([...required.map(key => [key, input[key]]),
    ...(input.hard_limit == null ? [] : [['hard_limit', input.hard_limit]])]) as NativeFixedRiskSizingRequest['sizing'];
}

/** Owner's existing Rust environment. Saving this source never builds or connects. */
export interface NativeRustEnvironment {
  kind: 'RUST_PROCESS'; id: string; cargoBinary: string; manifestPath: string; packageName: string;
  features?: string[]; backtestFactory?: string; accountFactory?: string;
  /** Explicit trusted-code compatibility declaration, not node verification or isolation. */
  accountCompatibility?: 'TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1';
}

/** Read-only selected capability facts retained in the original account binding.
 * No executable/source paths, credentials or current-session certification. */
export interface SavedAccountRunnerReport {
 source:'SAVED_ACCOUNT_BINDING';
 nautilusVersion:string|null;
 nautilusRevision:string|null;
 precision:'standard'|'high'|null;
 compatibilityDeclaration:'TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1'|null;
 compatibilityVerified:false|null;
}
