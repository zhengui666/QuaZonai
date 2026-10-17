import type { JsonValue } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';

function record(value: JsonValue | undefined): JsonObject | undefined {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value : undefined;
}

function scalars(value: JsonObject): JsonObject {
  return Object.fromEntries(Object.entries(value).filter(([, item]) => item === null || typeof item !== 'object'));
}

function partitions(value: JsonValue | undefined): JsonObject[] {
  return (Array.isArray(value) ? value : []).flatMap((item) => {
    const partition = record(item);
    if (!partition || !['TRAIN', 'VALIDATION'].includes(String(partition.partition))) return [];
    const metric = record(partition.metric);
    return [{ ...scalars(partition), metric: { counts: metric?.counts ?? null, pearson_ic: metric?.pearson_ic ?? null } }];
  });
}

/** Model projection only. The complete original result and native evidence stay unchanged. */
export function modelFeedback(result: JsonObject): JsonObject {
  if (result.portfolio === undefined) return result;
  const portfolio = record(result.portfolio);
  if (portfolio?.partition !== 'VALIDATION') throw new Error('validation_feedback_required');
  return {
    // Keep all original nullable account scalars before optional component diagnostics.
    portfolio: { ...scalars(portfolio), statistics: portfolio.statistics ?? [],
      summary: scalars(record(portfolio.summary) ?? {}), provenance: portfolio.provenance ?? null },
    schema_version: result.schema_version ?? null, request: result.request ?? null,
    evidenceTaskId: result.evidenceTaskId ?? null,
    partitions_component: result.partitions_component ?? null, partitions: partitions(result.partitions),
    components: (Array.isArray(result.components) ? result.components : []).map((item) => {
      const component = record(item) ?? {}; const summary = record(component.summary) ?? {};
      return { index: component.index ?? null, weight: component.weight ?? null,
        summary: { ...scalars(summary), native_versions: summary.native_versions ?? {}, partitions: partitions(summary.partitions) } };
    }),
  };
}
