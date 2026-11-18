import type { JsonObject } from './index.ts';

export interface DataSourceField {
  key: string; label: string; type: 'text' | 'number' | 'boolean' | 'password'; required?: boolean; description?: string;
  /** Optional declared text choices; clients render the same contract as a selector. */
  choices?: { value: string; label: string }[];
}
export interface DataSourceView {
  name: string; version: string | null; enabled: boolean; state: 'loaded' | 'disabled' | 'error';
  fields: DataSourceField[]; config: JsonObject; datasetIds: string[]; lastLoadedAt: string | null; error: string | null;
  /** CAS revision for the append-only registered dataset source. */
  revision?: number;
  acquisition?: { manifestPath: string; planId: string };
}
export interface DataSourceUpdate { config: JsonObject; enabled: boolean }
export interface DataSourceRefreshView {
  taskId: number | null;
  status: import('@earendil-works/pi-durable').TaskState<JsonObject, JsonObject>['status'] | 'idle';
  outcome?: import('@earendil-works/pi-durable').TaskOutcome<JsonObject>['status'];
  source: DataSourceView;
}

/** Existing catalog descriptors are registered, not executed or copied. */
export interface DatasetRegistryView { revision: number; datasetIds: string[]; availableDatasetIds: string[]; selectedDatasetIds: string[] | null }
export interface DatasetRegistrationRequest { requestId: string; expectedRevision: number; path: string }
export interface DatasetRegistrationReceipt { requestId: string; revision: number; datasetIds: string[] }
