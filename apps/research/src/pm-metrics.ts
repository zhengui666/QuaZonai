import type { JsonObject } from '@earendil-works/pi-durable';
import type { MetricObservation } from '../../../packages/contracts/src/pm.ts';
import { projectNativeValidationMetrics } from '../../../packages/contracts/src/decision.ts';
import type { ResearchCampaignBinding } from './pm-campaign.ts';

/** One shared exact metric adapter; independent values never enter selection. */
export function selectionObservations(result: JsonObject, binding: ResearchCampaignBinding, evidenceRef: string, asOfNs: string): MetricObservation[] {
  binding = JSON.parse(JSON.stringify(binding)) as ResearchCampaignBinding;
  const scope = binding.scopes[0]!;
  if (scope.role !== 'SELECTION' && scope.role !== 'INNER_VALIDATION') throw new Error('campaign_selection_scope_required');
  return projectNativeValidationMetrics(result, binding.definitions, { role: scope.role, interval: scope.interval,
    asOfNs, availableAtNs: asOfNs, evidenceRefs: [evidenceRef], methodRef: binding.policy.id,
    calendarId: scope.calendarId, currency: scope.capital?.currency ?? null });
}
