import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { fact, identity } from './pm-facts.ts';
import { nowNs } from './facts.ts';

export const PM_CAPABILITY_ID = 'qz.native.pm/1';
export function pmCapabilities(): Pm.CapabilityDescriptor[] {
  // Legacy target replay and custom inventory valuation are no longer offered.
  // Historical capability/evidence facts remain readable in their original Docs.
  return [];
}

export function assessStage(strategy: Pm.StrategyVersion, stage: Pm.PipelineStage, evidence: Pm.EvidenceRef[], requirements: string[] = []): Pm.PipelineAssessment {
  const independent = evidence.filter(item => item.role === 'INDEPENDENT');
  const missing = [...requirements];
  if (['INDEPENDENT_EVALUATION', 'SHADOW', 'PAPER', 'TARGET_OPERATION', 'MONITORING'].includes(stage) && independent.length === 0) missing.push('independent_strategy_evidence_missing');
  if (stage === 'TARGET_OPERATION') missing.push('live_execution_capacity_requires_current_account_evidence');
  return { ...fact(identity('assessment', [strategy.id, stage, evidence.map(item => item.id), missing])), strategyVersionId: strategy.id, stage,
    outcome: missing.length ? 'NEEDS_EVIDENCE' : 'READY', evaluatedAtNs: nowNs(), requirementsVersion: 'qz.pipeline/1',
    evidenceRefs: evidence.map(item => item.id), reasonCodes: missing, nextPermittedActions: missing.length ? ['COLLECT_EVIDENCE'] : ['ADVANCE'] };
}
