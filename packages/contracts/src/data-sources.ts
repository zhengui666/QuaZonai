import type { JsonObject } from './index.ts';

export interface DataSourceField {
  key: string; label: string; type: 'text' | 'number' | 'boolean' | 'password'; required?: boolean; description?: string;
  /** Optional declared text choices; clients render the same contract as a selector. */
  choices?: { value: string; label: string }[];
}
export interface DataSourceView {
  name: string; version: string | null; enabled: boolean; state: 'loaded' | 'disabled' | 'error';
  fields: DataSourceField[]; config: JsonObject; datasetIds: string[]; lastLoadedAt: string | null; error: string | null;
}
export interface DataSourceUpdate { config: JsonObject; enabled: boolean }
