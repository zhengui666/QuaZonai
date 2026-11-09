import type { NativeJson } from '../../../packages/contracts/src/nautilus.ts';

/** Describes actual acceptance evidence, never a requested recovery mode. The
 * server advertises only a deployment-owned profile matching this host. */
export interface CheckpointProfile {
  profileId: string;
  verifiedCommands: string[];
  checkpointIntervalMs: number;
}
export interface CheckpointManifest {
  generation: number;
  createdAt: string;
  profileId: string;
  requestId: string;
  contentDigest: string;
}
export interface CheckpointInspection {
  phase: 'absent' | 'prepared' | 'running' | 'stopped' | 'completed' | 'failed' | 'unknown';
  completion?: { exitCode: number | null; signal: string | null; result?: NativeJson; error?: string };
  currentGeneration?: number;
}
/** Deliberately has no timer, task queue or retry policy. Official Durable tasks
 * own the order: prepare; checkpoint(0); commit generation; restore; start. */
export interface RecoveryDriver<Request = unknown> {
  profile(): Promise<CheckpointProfile | null>;
  prepare(request: Request): Promise<void>;
  inspect(requestId: string): Promise<CheckpointInspection>;
  checkpoint(requestId: string, generation: number): Promise<CheckpointManifest>;
  restore(requestId: string, generation: number): Promise<void>;
  start(requestId: string): Promise<void>;
  cancel(requestId: string): Promise<void>;
}
