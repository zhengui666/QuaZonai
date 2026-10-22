/** Owner-authenticated service maintenance. No investment qualification is
 * inferred: QUIESCED only means original work/receipts satisfy the drain gate. */
export type MaintenanceMode = 'RUNNING' | 'DRAINING' | 'QUIESCED';
export interface MaintenanceEnter { requestId: string; reason?: string }
export interface MaintenanceResume { requestId: string; epoch: string }
export interface MaintenanceReceipt { requestId: string; action: 'enter' | 'resume'; epoch: string }
export interface MaintenanceTaskBlocker { taskId: number; kind: string; status: string; reason: string }
export interface MaintenanceSubmissionBlocker { submissionId: number; status: string }
export interface MaintenanceExternalBlocker { kind: string; id: string; reason: string }
export interface MaintenanceStatus {
  state: MaintenanceMode;
  /** Opaque maintenance identity, distinct from official numeric task IDs. */
  epoch: string | null;
  barrierTaskId: number | null;
  enteredAtNs: string | null;
  quiescedAtNs: string | null;
  resumedAtNs: string | null;
  parkedTaskIds: number[];
  blockingTasks: MaintenanceTaskBlocker[];
  blockingSubmissions: MaintenanceSubmissionBlocker[];
  blockingExternal: MaintenanceExternalBlocker[];
  activeBusinessRequests: number;
}
/** Replayed commands keep the ORIGINAL receipt and report CURRENT status. */
export interface MaintenanceCommandResult { receipt: MaintenanceReceipt; status: MaintenanceStatus }
