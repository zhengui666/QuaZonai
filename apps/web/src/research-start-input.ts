import type { ModelRef } from '../../../packages/contracts/src/index.ts';
import { publicLiteratureTopics } from './literature-input.ts';

/** Only transport user intent; do not reconstruct retired scoring or fit specifications. */
export function nativeResearchInput(values: Record<string, string>) {
  const publicQueries = publicLiteratureTopics(values.publicLiteratureTopics);
  const source = values.dataSource === 'catalog' ? { catalog: values.catalog } : { datasetId: values.datasetId };
  return { goal: values.goal, ...(publicQueries ? { publicLiteratureQueries: publicQueries } : {}), model: JSON.parse(values.model!) as ModelRef, ...source, ...(values.thinkingLevel ? { thinkingLevel: values.thinkingLevel } : {}), ...(values.serviceTier ? { serviceTier: values.serviceTier } : {}) };
}
