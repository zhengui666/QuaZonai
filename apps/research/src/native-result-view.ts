import { isNativeTable, nativeRowCount } from './native-table.ts';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { JsonValue } from '@earendil-works/chord';
import { object } from './trials.ts';

// Match the official tool output allowance explicitly; compact JSON must never
// depend on Harness truncating an oversized response into invalid JSON.
export const nativeToolMaxBytes = 50 * 1024;
export const nativeToolOutputLimits = { maxBytes: nativeToolMaxBytes, maxLines: 2000 };
export function nativeToolText(view: JsonObject, taskId: number): string {
  const text = JSON.stringify(view);
  return Buffer.byteLength(text) <= nativeToolMaxBytes ? text : JSON.stringify({ taskId,
    projection: 'SUMMARY_NOT_INCLUDED_USE_PAGED_TASK_VIEW', reader: 'native_assessment_read_exposure',
    summaryPath: '$.summary', rowsReviewed: false, note: 'Even the compact summary exceeds one response; read the original Task summary in pages. Original evidence is retained.' });
}

/** A model-facing view only; the official Task retains the result and original row-page references. */
export function nativeResultTables(result: JsonObject, taskId: number, resultPath = '$'): JsonObject {
  const report = result.canonical_result === null && Array.isArray(result.native_results) ? {} : object(result.canonical_result, 'native_report');
  const tables: JsonObject = {};
  for (const [name, rows] of Object.entries(report)) if (Array.isArray(rows) || isNativeTable(rows)) tables[name] = { path: `${resultPath}.canonical_result.${name}`, rowCount: nativeRowCount(rows) };
  const equity = result.equity_performance;
  if (equity && typeof equity === 'object' && !Array.isArray(equity) && Array.isArray(equity.returns)) tables.equity_returns = { path: `${resultPath}.equity_performance.returns`, rowCount: equity.returns.length };
  return { source: 'UPSTREAM_CANONICAL_REPORT_REFERENCE', taskId, resultRoot: 'task.state.outcome.result', resultPath, tables,
    rowsReviewed: false, scope: 'REFERENCE_ONLY_ROWS_NOT_REVIEWED_NO_PEAK_EXPOSURE_OR_MARGIN_CLAIM' };
}
export function nativeResultView(result: JsonObject, taskId: number, resultPath = '$'): JsonObject {
  if (Array.isArray(result.folds)) return { ...result, taskId, projection: 'COMPACT_NATIVE_RESULT_ORIGINAL_TASK_RETAINED',
    folds: result.folds.map(value => {
      const fold = object(value, 'native_fold'); const path = `${resultPath}.folds[${fold.index}]`;
      return { ...fold, trials: (fold.trials as JsonValue[]).map(value => { const trial = object(value, 'native_trial');
        return { ...trial, result: trial.result === null ? null : nativeResultView(object(trial.result, 'native_result'), taskId, `${path}.trials[${trial.candidate_index}].result`) };
      }), test_result: fold.test_result === null ? null : nativeResultView(object(fold.test_result, 'native_result'), taskId, `${path}.test_result`) };
    }) };
  const reference = nativeResultTables(result, taskId, resultPath);
  const tables = reference.tables as JsonObject;
  const rowReference = (name: string): JsonObject => ({ source: 'ORIGINAL_TASK_ROWS_NOT_INCLUDED', taskId, ...object(tables[name], 'native_table'), rowsReviewed: false });
  const report = result.canonical_result === null && Array.isArray(result.native_results) ? {} : object(result.canonical_result, 'native_report');
  const equity = result.equity_performance ? object(result.equity_performance, 'native_equity') : null;
  return { ...result, taskId, projection: 'COMPACT_NATIVE_RESULT_ORIGINAL_TASK_RETAINED', evidenceTables: reference,
    canonical_result: result.canonical_result === null ? null : Object.fromEntries(Object.entries(report).map(([key, value]) => [key, Array.isArray(value) || isNativeTable(value) ? rowReference(key) : value])),
    ...(equity ? { equity_performance: { ...equity, returns: rowReference('equity_returns') } } : {}) };
}
