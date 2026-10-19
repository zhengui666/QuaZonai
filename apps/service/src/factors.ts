import type { Cursor } from '@earendil-works/pi-durable';
import type { FactorView, ResearchSpecification } from '../../../packages/contracts/src/index.ts';
import { researchCriterion } from '../../../packages/contracts/src/research.ts';
import type { ResearchHost } from '../../research/src/host.ts';
import { strategyMembers } from '../../research/src/strategy.ts';
import { metrics } from './projections.ts';
import { object, Problem } from './facts.ts';
import type { ResearchAdmissions } from './facts.ts';

export const factorId = (researchId: number, trialId: number, memberIndex: number) => `r${researchId}.t${trialId}.m${memberIndex}`;
export function factorReference(id: string) {
  const match = /^r(\d+)\.t(\d+)\.m(\d+)$/.exec(id);
  if (!match || match.slice(1).some(value => !Number.isSafeInteger(Number(value)))) throw new Problem('factor_id_invalid');
  return { researchId: Number(match[1]), trialId: Number(match[2]), memberIndex: Number(match[3]) };
}
export function factorViews(id: number, study: { input: import('../../research/src/documents.ts').ResearchInput | null; trials: import('@earendil-works/pi-durable').JsonObject[]; final: import('@earendil-works/pi-durable').JsonObject | null }, admissions?: ResearchAdmissions, requestId?: string): FactorView[] {
  const input = study.input; if (!input) return [];
  const admission = requestId ? admissions?.[`request:${requestId}`] : undefined;
  const dataset = admission?.dataset;
  const strategy = input.policy.strategy ? object(input.policy.strategy) : null;
  return study.trials.flatMap(trial => {
    const proposal = object(trial.proposal); let members;
    try { members = strategyMembers(proposal); } catch { return []; }
    return members.map((member, memberIndex): FactorView => {
      const selected = study.final?.trialId === trial.taskId;
      const independentlyComputed = selected && study.final?.status === 'COMPUTED' && members.length === 1;
      const result = independentlyComputed ? study.final?.result : trial.result;
      const raw = result ? object(result) : null;
      const components = Array.isArray(raw?.components) ? raw.components : [];
      const diagnostic = components.length ? object(object(components[memberIndex]).summary) : raw;
      const measured = metrics(diagnostic ?? undefined);
      const criterion = researchCriterion(input.specification, independentlyComputed ? 'COMPUTED' : String(trial.status), metrics(raw ?? undefined));
      const heldout = diagnostic && Array.isArray(diagnostic.partitions) ? diagnostic.partitions.map(item => object(item)).find(item => item.partition === 'HELDOUT') : null;
      const included = heldout ? object(object(heldout.metric).counts).included : null;
      const evidence = independentlyComputed ? strategy?.evidenceMode === 'HISTORICAL_REASSESSMENT' ? 'historical' : 'independent' : members.length > 1 ? 'component_only' : trial.status === 'COMPUTED' ? 'validation_only' : 'unavailable';
      const reason = !independentlyComputed ? evidence === 'component_only' ? 'component_not_independently_assessed' : 'independent_evidence_missing'
        : typeof included !== 'string' || !/^\d+$/.test(included) || BigInt(included) === 0n ? 'heldout_observations_missing'
          : criterion && !['computed', 'met'].includes(criterion.status) ? criterion.status === 'insufficient' ? 'research_criterion_insufficient' : 'research_criterion_not_met' : !strategy ? 'strategy_policy_missing' : null;
      return { id: factorId(id, Number(trial.taskId), memberIndex), researchId: id, trialId: Number(trial.taskId), memberIndex,
        name: input.specification?.name ?? input.goal, hypothesis: String(proposal.hypothesis ?? ''), modelSource: member.model,
        datasetId: typeof dataset?.id === 'string' ? dataset.id : null, currency: typeof dataset?.currency === 'string' ? dataset.currency : typeof (strategy?.simulationSettings as Record<string, unknown> | undefined)?.base_currency === 'string' ? String((strategy!.simulationSettings as Record<string, unknown>).base_currency) : null,
        market: Array.isArray(proposal.allocations) ? proposal.allocations.map(item => String(object(item).instrumentId)) : [proposal.instrumentId ?? strategy?.instrumentId].filter((item): item is string => typeof item === 'string'),
        status: String(trial.status), ...(input.specification?.capital ? { capacityAssumptions: input.specification.capital } : {}), evidence, metrics: measured, criterion: criterion ?? null, combinable: reason === null, reason };
    });
  });
}
export async function listFactors(host: ResearchHost, admissions?: ResearchAdmissions) {
  const factors: FactorView[] = []; let cursor: Cursor | undefined;
  do {
    const page = await host.list(cursor);
    for (const row of page.items) {
      const state = await host.state(row.id);
      if (state.research) factors.push(...factorViews(row.id, state.research, admissions, state.originalSubmission?.requestId));
    }
    cursor = page.next;
  } while (cursor);
  return factors;
}
export async function readFactor(host: ResearchHost, id: string, admissions?: ResearchAdmissions) {
  const reference = factorReference(id);
  const state = await host.state(reference.researchId); const study = state.research;
  const view = study && factorViews(reference.researchId, study, admissions, state.originalSubmission?.requestId).find(item => item.id === id);
  if (!view || !study?.input) throw new Problem('factor_not_found', 404);
  return { view, study, specification: study.input.specification as ResearchSpecification | undefined };
}
