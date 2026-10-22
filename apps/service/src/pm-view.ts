import type { PmMandateView } from '../../../packages/contracts/src/pm.ts';
import type { PmRecord, PmState } from './pm-facts.ts';
import { pmCapabilities } from './pm-registry.ts';

export function pmView(record: PmRecord, state: PmState): PmMandateView {
  return structuredClone({ mandate: record.mandate, activePolicy: record.policy,
    currentDecision: record.currentDecisionId ? record.decisions[record.currentDecisionId]! : null,
    lastCycle: record.latestCycleId ? record.cycles[record.latestCycleId]! : null,
    campaigns: Object.values(record.campaigns), strategies: Object.values(record.strategies),
    assessments: Object.values(record.assessments), capacityEstimates: Object.values(record.capacities), gaps: Object.values(record.gaps),
    history: Object.values(record.decisions), cycles: Object.values(record.cycles), datasetRevisions: [...new Set([...record.datasets, ...Object.values(record.campaigns).flatMap(item => item.datasetRevisionIds), ...Object.values(record.cycles).flatMap(item => item.datasetRevisionIds)])].map(id => state.datasets[id]!.revision),
    capabilities: pmCapabilities(), comparisons: Object.values(record.comparisons), ensembles: Object.values(record.ensembles),
    targetCommands: Object.values(record.commands), attributions: Object.values(record.attributions),
    outcomes: Object.values(record.outcomes), retirements: Object.values(record.retirements),
    researchTrials: Object.values(record.derivedTrials), budgetMovements: Object.values(record.budgetMovements), searchFamily: record.searchFamily });
}
