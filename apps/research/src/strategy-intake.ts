import { Type } from '@earendil-works/pi-ai';
import { defineTool } from '@earendil-works/pi-durable';
import { Study } from './documents.ts';
import { CampaignResearchDoc, fact, nowNs } from './pm-campaign.ts';
import { canonicalOwnerBoundary, explicitOwnerBoundaryProposals, normalizeOwnerBoundaryProposal, OWNER_BOUNDARY_NORMALIZATION_VERSION } from '../../../packages/contracts/src/pm-intent.ts';
import type { OwnerBoundaryProposal } from '../../../packages/contracts/src/pm-intent.ts';
import type { OwnerBoundary } from '../../../packages/contracts/src/pm.ts';

export const strategyIntakeTool = defineTool({
  name: 'strategy_intake', description: 'Campaign-bound research only: interpret the ORIGINAL owner goal into an economic strategy proposal before experiments. Preserve the actual archetype; never translate event-driven/relative-value/microstructure into predictive alpha merely because an adapter is missing. Quote exact owner goal or authenticated clarification substrings for owner boundaries; no invented numerical limits. For an explicit no-leverage statement use LEVERAGE normalizedValue {maximumGrossNavRatio:"1",borrowingAllowed:false}; the versioned normalizer accepts exact supported statements and otherwise preserves the constraint for owner clarification. Previously accepted boundaries remain effective even when omitted by the model. Never turn a future maximum-drawdown wish into a return guarantee. Unresolved intent and unsupported capabilities are explicit outcomes.',
  parameters: Type.Object({ name: Type.String(), archetype: Type.Union(['PREDICTIVE_ALPHA', 'RELATIVE_VALUE', 'EVENT_DRIVEN', 'MICROSTRUCTURE', 'PORTFOLIO_OVERLAY', 'FACTOR_RESEARCH'].map(value => Type.Literal(value))),
    economicMechanism: Type.String(), falsificationConditions: Type.Array(Type.String()), requiredCapabilities: Type.Array(Type.String()), intendedHoldingHorizonRef: Type.String(), knownFailureModes: Type.Array(Type.String()),
    ambiguities: Type.Array(Type.String()), ownerBoundaries: Type.Array(Type.Object({ kind: Type.Union(['LOSS', 'LEVERAGE', 'MARKET_SCOPE', 'LIQUIDITY_NEED', 'CAPITAL_COMMITMENT', 'DATA_USE'].map(value => Type.Literal(value))), sourceStatement: Type.String(), normalizedValue: Type.Optional(Type.Object({ maximumGrossNavRatio: Type.String(), borrowingAllowed: Type.Boolean() })) })) }),
  replay: 'safe', executionMode: 'sequential',
  async execute(args, api, context) {
    const result = await api.commit(async tx => {
      const study = await tx.doc(Study, api.conversationId); const campaign = await tx.doc(CampaignResearchDoc, api.conversationId);
      if (!study.input || !campaign.binding) throw new Error('campaign_binding_required');
      if (campaign.intake) {
        if (campaign.intake.taskId !== api.taskId) throw new Error('strategy_intake_already_frozen');
        return JSON.parse(JSON.stringify(campaign.intake));
      }
      if (campaign.trials.length || campaign.state !== 'SEARCHING') throw new Error('strategy_intake_before_experiments_required');
      const at = nowNs(); const id = `${campaign.binding.campaignId}:strategy`;
      const strategy = { ...fact(id, at), name: args.name, archetype: args.archetype as import('../../../packages/contracts/src/pm.ts').StrategyArchetype,
        economicMechanism: args.economicMechanism, falsificationConditions: args.falsificationConditions, opportunityUniverseRef: campaign.binding.scopes[0]!.allowedUniverseRef,
        requiredCapabilities: args.requiredCapabilities, intendedHoldingHorizonRef: args.intendedHoldingHorizonRef, knownFailureModes: args.knownFailureModes };
      const ownerBoundaries = (campaign.binding.ownerBoundaries ?? []).map(boundary => canonicalOwnerBoundary(JSON.parse(JSON.stringify(boundary))));
      const ownerBoundaryStatements = ownerBoundaries.map(boundary => ({ kind: boundary.kind, sourceStatement: boundary.sourceStatement }));
      const sourceStatements = [study.input.goal, ...(campaign.binding.ownerClarificationStatements ?? [])];
      const proposed: OwnerBoundaryProposal[] = args.ownerBoundaries.map(proposal => ({ ...proposal, kind: proposal.kind as OwnerBoundary['kind'] }));
      for (const found of explicitOwnerBoundaryProposals(sourceStatements)) if (!proposed.some(item => item.kind === found.kind && item.sourceStatement === found.sourceStatement)) proposed.push(found);
      let unresolvedBoundary = false;
      for (const [index, proposal] of proposed.entries()) {
        const boundary = normalizeOwnerBoundaryProposal({ ...proposal, kind: proposal.kind as OwnerBoundary['kind'] }, { id: `${id}:boundary:${index}`, effectiveFromNs: at,
          sourceStatements, acceptedBoundaries: ownerBoundaries });
        if (!ownerBoundaryStatements.some(item => item.kind === proposal.kind && item.sourceStatement === proposal.sourceStatement)) ownerBoundaryStatements.push({ kind: proposal.kind as OwnerBoundary['kind'], sourceStatement: proposal.sourceStatement });
        if (boundary && !ownerBoundaries.some(item => item.id === boundary.id)) ownerBoundaries.push(boundary);
        if (!boundary) unresolvedBoundary = true;
      }
      const unsupportedBoundary = ownerBoundaries.some(boundary => boundary.kind !== 'LEVERAGE');
      const supported = ['PREDICTIVE_ALPHA', 'FACTOR_RESEARCH', 'PORTFOLIO_OVERLAY'].includes(args.archetype);
      const accountConsumerNotRequested = campaign.binding.independentConsumer === 'PM_PORTFOLIO' && args.archetype === 'FACTOR_RESEARCH';
      const result = { normalizationVersion: OWNER_BOUNDARY_NORMALIZATION_VERSION, strategy, ownerBoundaries, ownerBoundaryStatements, sourceGoal: study.input.goal,
        support: !supported || unsupportedBoundary ? 'UNSUPPORTED' as const : args.ambiguities.length || accountConsumerNotRequested || unresolvedBoundary ? 'NEEDS_CLARIFICATION' as const : 'CONDITIONAL' as const,
        missingCapabilities: [...(supported ? [] : [`strategy_archetype:${args.archetype}`]), ...(unresolvedBoundary ? ['owner_boundary_normalization_required'] : []), ...(unsupportedBoundary ? ['owner_boundary_execution_unavailable'] : []), ...(accountConsumerNotRequested ? ['pm_account_consumer_not_requested'] : [])], ambiguities: args.ambiguities, taskId: api.taskId as number };
      campaign.intake = result;
      return result;
    }, context);
    return { content: [{ type: 'text', text: JSON.stringify(result) }] };
  },
});
