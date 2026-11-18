import type { Json } from './index.ts';

/** Original native table references. A page is not a complete evidence review. */
export interface NativeResultTableReference {
  name: string;
  path: string;
  rowCount: number;
  resultPath: string;
}
export interface NativeResultTablePage {
  taskId: number;
  kind: 'TABLES' | 'ROWS';
  path: string | null;
  offset: number;
  endExclusive: number;
  totalRows: number;
  nextOffset: number | null;
  maxPayloadBytes: number;
  scope: 'ORIGINAL_TASK_TABLES' | 'ORIGINAL_TASK_ROWS';
  rows: Json[];
}
