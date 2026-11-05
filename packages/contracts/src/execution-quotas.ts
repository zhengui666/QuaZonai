// Compare scientific inputs without retired execution allowances. This is never
// used to rewrite frozen requests, identity digests, ledgers or native receipts.
const retiredQuotaFields = new Set(['max_trials', 'maxTrials', 'max_tokens', 'maxTokens', 'maximumScientificCalls',
  'maximumTrials', 'maximumModelTurns', 'max_turns', 'max_model_turns', 'max_output_tokens', 'max_cost',
  'nativeComputeSeconds', 'modelSpend', 'dataBytes', 'total_fuel', 'maximum_rows', 'maximum_source_rows']);

export function withoutExecutionQuotas<T>(value: T): T {
  if (Array.isArray(value)) {
    let copy: unknown[] | undefined;
    for (let index = 0; index < value.length; index++) {
      const child = withoutExecutionQuotas(value[index]);
      if (child !== value[index]) { copy ??= [...value]; copy[index] = child; }
    }
    return (copy ?? value) as T;
  }
  if (value && typeof value === 'object') {
    const originalRecord = value as Record<string, unknown>;
    let copy: Record<string, unknown> | undefined;
    for (const [key, original] of Object.entries(originalRecord)) {
      if (retiredQuotaFields.has(key)) { copy ??= { ...originalRecord }; delete copy[key]; continue; }
      const child = withoutExecutionQuotas(original);
      if (child !== original) { copy ??= { ...originalRecord }; copy[key] = child; }
    }
    return (copy ?? value) as T;
  }
  return value;
}
