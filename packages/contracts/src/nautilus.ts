/** A string is an explicit local executable for the independent worker and
 * scientific tests. Application configuration admits remote targets only. */
export type RemoteNativeTarget = { id: string; url: string; tokenEnv?: string; credentialRef?: string;
  /** Frozen recovery binding. Omitted only for original V1 executions. */
  protocolVersion?: 2; instanceId?: string; recoveryProfileId?: string };
/** Owner-configured instance. `tokenEnv` remains for older service files. */
export type RemoteNativeInstance = Omit<RemoteNativeTarget, 'credentialRef'> & { token?: string };
export type NativeTarget = string | RemoteNativeTarget;

export type NativeJson = null | boolean | number | string | NativeJson[] | { [key: string]: NativeJson };
export interface NativeCapabilities {
  schemaVersion: 1;
  implementationDigest: string;
  commands: string[];
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

/** A recovery profile is advertised only after the node's actual scientific
 * commands have passed internal checkpoint/restore acceptance. */
export interface NativeRecoveryProfile {
  profileId: string;
  verifiedCommands: string[];
  checkpointIntervalMs: number;
}
export interface NativeCapabilitiesV2 {
  schemaVersion: 2;
  instanceId: string;
  implementationDigest: string;
  commands: string[];
  recovery: NativeRecoveryProfile | null;
}
export interface NativeExecutionRequestV2 extends Omit<NativeExecutionRequest, 'schemaVersion'> {
  schemaVersion: 2;
  /** The original worker identity distinguishes absence from a replacement DB. */
  instanceId: string;
  recoveryProfileId: string;
}
export interface NativeExecutionRecovery {
  profileId: string;
  phase: string;
  generation: number | null;
  checkpointAt: string | null;
  blockedReason: string | null;
}
export type NativeExecutionReceiptV2 = {
  schemaVersion: 2;
  instanceId: string;
  requestId: string;
  taskId: number;
  recovery: NativeExecutionRecovery;
} & (
  { status: 'COMPLETED'; result: NativeJson } |
  { status: 'FAILED'; error: string } |
  { status: 'ACCEPTED' | 'RUNNING' | 'WAITING' | 'CANCELLED' }
);

/** Cancellation before execution admission is a separate durable fact. It has
 * no invented scientific Task or process-termination receipt. */
export interface NativeCancellationFenceV2 {
  schemaVersion: 2;
  instanceId: string;
  requestId: string;
  cancellation: 'FENCED_NOT_ADMITTED';
}

/** Both tick research operations use the configured precision companion and worker limits. */
export function isTickNativeCommand(command: string): boolean {
  return command === 'evaluate-tick-factor' || command === 'replay-tick-economic';
}
