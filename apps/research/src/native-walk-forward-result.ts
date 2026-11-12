import { executionVersions } from './native-result-version.ts';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { JsonValue } from '@earendil-works/chord';
import { canonical } from './documents.ts';
import { object } from './trials.ts';
import { nativeStrategyResult, normalizedNativeStrategyRequest } from '../plugins/native-strategy-backtest.ts';
export const walkForwardScope = 'INDEPENDENT_COLD_START_TEST_WINDOW_NATIVE_STATISTICS';
function clock(value: JsonValue | undefined): bigint {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)$/.test(value) || BigInt(value) > 9223372036854775807n) throw new Error('walk_forward_clock_invalid');
  return BigInt(value);
}
export function normalizedWalkForwardObjective(value: JsonValue | undefined): JsonObject {
  const result = { ...object(value, 'walk_forward_objective') }; result.currency ??= null; result.metric_source ??= 'CANONICAL'; result.annualization_days ??= null;
  if (!['CANONICAL', 'EQUITY_PERFORMANCE'].includes(String(result.metric_source)) || (result.metric_source === 'CANONICAL' ? result.annualization_days !== null : ![252, 365].includes(Number(result.annualization_days)) || typeof result.annualization_days !== 'number')) throw new Error('walk_forward_metric_source_invalid');
  return result;
}
function options(value: JsonValue | undefined, fields: string[]) {
  const result = { ...object(value, 'walk_forward_options') }; for (const key of fields) result[key] ??= null; return result;
}
function window(base: JsonObject, start: JsonValue | undefined, end: JsonValue | undefined) {
  clock(start); clock(end);
  return { ...base, available_start_ns: start!, available_end_ns: end! };
}
/** Wire and provenance checks only; native code owns split generation, metrics and strategy execution. */
export function nativeWalkForwardResult(value: JsonValue, request: JsonObject): JsonObject {
  const result = object(value, 'walk_forward_result'); const echo = object(result.request, 'walk_forward_request');
  const candidates = request.candidates;
  if (request.schema_version !== 1 || echo.schema_version !== 1 || !Array.isArray(candidates) || !candidates.length || !Array.isArray(echo.candidates) || echo.candidates.length !== candidates.length
    || !candidates.every((candidate, i) => canonical(normalizedNativeStrategyRequest(object(candidate, 'walk_forward_candidate'))) === canonical(normalizedNativeStrategyRequest(object((echo.candidates as JsonValue[])[i], 'walk_forward_candidate'))))
    || canonical(options(request.split, ['test_size', 'max_train_size'])) !== canonical(options(echo.split, ['test_size', 'max_train_size']))
    || canonical(normalizedWalkForwardObjective(request.objective)) !== canonical(normalizedWalkForwardObjective(echo.objective))) throw new Error('walk_forward_request_mismatch');
  const split = object(request.split, 'walk_forward_split');
  if (result.schema_version !== 1 || result.performance_scope !== walkForwardScope || result.splitter_version !== 'solow-cv 0.7.3 TimeSeriesSplit'
    || typeof result.native_version !== 'string' || !executionVersions.has(result.native_version) || !Array.isArray(result.folds) || result.folds.length !== split.n_splits
    || !Array.isArray(result.notes) || !result.notes.every(note => typeof note === 'string')) throw new Error('walk_forward_result_invalid');
  let previousTestEnd: bigint | null = null;
  for (const [index, value] of result.folds.entries()) {
    const fold = object(value, 'walk_forward_fold');
    const base = object(candidates[0], 'walk_forward_candidate');
    if (clock(fold.train_start_ns) < clock(base.available_start_ns ?? base.event_start_ns) || clock(fold.test_end_ns) > clock(base.available_end_ns)) throw new Error('walk_forward_outside_registered_scope');
    if (fold.index !== index || clock(fold.train_start_ns) > clock(fold.train_end_ns) || clock(fold.train_end_ns) >= clock(fold.test_start_ns)
      || clock(fold.test_start_ns) > clock(fold.test_end_ns) || previousTestEnd !== null && clock(fold.test_start_ns) <= previousTestEnd
      || !Array.isArray(fold.trials) || fold.trials.length !== candidates.length) throw new Error('walk_forward_fold_invalid');
    previousTestEnd = clock(fold.test_end_ns);
    for (const [candidateIndex, value] of fold.trials.entries()) {
      const trial = object(value, 'walk_forward_trial');
      if (trial.candidate_index !== candidateIndex || trial.metric_value !== null && (typeof trial.metric_value !== 'number' || !Number.isFinite(trial.metric_value))
        || trial.error !== null && typeof trial.error !== 'string') throw new Error('walk_forward_trial_invalid');
      if (trial.result !== null) {
        const native = nativeStrategyResult(trial.result!, window(object(candidates[candidateIndex], 'walk_forward_candidate'), fold.train_start_ns, fold.train_end_ns));
        if (native.native_version !== result.native_version) throw new Error('walk_forward_version_mismatch');
      } else if (trial.metric_value !== null || typeof trial.error !== 'string') throw new Error('walk_forward_trial_evidence_missing');
    }
    const selected = fold.selected_candidate_index;
    if (selected !== null && (typeof selected !== 'number' || !Number.isInteger(selected) || selected < 0 || selected >= candidates.length)) throw new Error('walk_forward_selected_candidate_invalid');
    if (fold.status === 'COMPLETED' || fold.status === 'TEST_STOPPED') {
      if (typeof selected !== 'number' || (fold.status === 'COMPLETED' ? fold.reason !== null : fold.reason !== 'WALK_FORWARD_TEST_WINDOW_STOPPED') || fold.test_result === null) throw new Error('walk_forward_test_evidence_missing');
      const trial = object(fold.trials[selected], 'walk_forward_selected_trial');
      if (typeof trial.metric_value !== 'number') throw new Error('walk_forward_selected_metric_missing');
      const native = nativeStrategyResult(fold.test_result!, window(object(candidates[selected], 'walk_forward_candidate'), fold.test_start_ns, fold.test_end_ns));
      if (native.native_version !== result.native_version) throw new Error('walk_forward_version_mismatch');
      const outcome = object(object(native.canonical_result, 'walk_forward_canonical').run, 'walk_forward_run').outcome;
      if (outcome !== (fold.status === 'COMPLETED' ? 'completed' : 'stopped')) throw new Error('walk_forward_test_outcome_mismatch');
    } else if (!['TEST_FAILED', 'NOT_EVALUATED'].includes(String(fold.status)) || fold.test_result !== null || typeof fold.reason !== 'string') throw new Error('walk_forward_status_invalid');
  }
  return result;
}
