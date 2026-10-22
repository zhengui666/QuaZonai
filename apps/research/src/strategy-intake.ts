import { Type } from '@earendil-works/pi-ai';
import { defineTool } from '@earendil-works/pi-durable';
import { Study } from './documents.ts';
import { CampaignResearchDoc, fact, nowNs } from './pm-campaign.ts';

export const strategyIntakeTool = defineTool({
  name: 'strategy_intake', description: 'Interpret the ORIGINAL owner goal into an economic strategy proposal before experiments. Preserve the actual archetype; never translate event-driven/relative-value/microstructure into predictive alpha merely because an adapter is missing. Quote exact owner goal substrings for owner boundaries; no invented numerical limits. Unresolved intent and unsupported capabilities are explicit outcomes.',
  parameters: Type.Object({ name: Type.String(), archetype: Type.Union(['PREDICTIVE_ALPHA', 'RELATIVE_VALUE', 'EVENT_DRIVEN', 'MICROSTRUCTURE', 'PORTFOLIO_OVERLAY', 'FACTOR_RESEARCH'].map(value => Type.Literal(value))),
    economicMechanism: Type.String(), falsificationConditions: Type.Array(Type.String()), requiredCapabilities: Type.Array(Type.String()), intendedHoldingHorizonRef: Type.String(), knownFailureModes: Type.Array(Type.String()),
    ambiguities: Type.Array(Type.String()), ownerBoundaries: Type.Array(Type.Object({ kind: Type.Union(['LOSS', 'LEVERAGE', 'MARKET_SCOPE', 'LIQUIDITY_NEED', 'CAPITAL_COMMITMENT', 'DATA_USE'].map(value => Type.Literal(value))), sourceStatement: Type.String() })) }),
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
      for (const boundary of args.ownerBoundaries) if (!boundary.sourceStatement.trim() || !study.input.goal.includes(boundary.sourceStatement)) throw new Error('owner_boundary_source_not_in_original_goal');
      const at = nowNs(); const id = `${campaign.binding.campaignId}:strategy`;
      const strategy = { ...fact(id, at), name: args.name, archetype: args.archetype as import('../../../packages/contracts/src/pm.ts').StrategyArchetype,
        economicMechanism: args.economicMechanism, falsificationConditions: args.falsificationConditions, opportunityUniverseRef: campaign.binding.scopes[0]!.allowedUniverseRef,
        requiredCapabilities: args.requiredCapabilities, intendedHoldingHorizonRef: args.intendedHoldingHorizonRef, knownFailureModes: args.knownFailureModes };
      const supported = ['PREDICTIVE_ALPHA', 'FACTOR_RESEARCH', 'PORTFOLIO_OVERLAY'].includes(args.archetype);
      const accountConsumerNotRequested = campaign.binding.independentConsumer === 'PM_PORTFOLIO' && args.archetype === 'FACTOR_RESEARCH';
      const result = { strategy, ownerBoundaries: [], ownerBoundaryStatements: args.ownerBoundaries.map(boundary => ({ kind: boundary.kind as import('../../../packages/contracts/src/pm.ts').OwnerBoundary['kind'], sourceStatement: boundary.sourceStatement })), sourceGoal: study.input.goal,
        support: args.ambiguities.length || accountConsumerNotRequested ? 'NEEDS_CLARIFICATION' as const : supported ? 'CONDITIONAL' as const : 'UNSUPPORTED' as const,
        missingCapabilities: [...(supported ? [] : [`strategy_archetype:${args.archetype}`]), ...(args.ownerBoundaries.length ? ['owner_boundary_normalization_required'] : []), ...(accountConsumerNotRequested ? ['pm_account_consumer_not_requested'] : [])], ambiguities: args.ambiguities, taskId: api.taskId as number };
      campaign.intake = result;
      return result;
    }, context);
    return { content: [{ type: 'text', text: JSON.stringify(result) }] };
  },
});
