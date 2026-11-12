import { int, obj, opt, str } from '@earendil-works/pi-schema';
import { defineExtension, defineTool, hook, section, ToolTask } from '@earendil-works/pi-durable';
import type { Extension } from '@earendil-works/pi-durable';
import { Study } from '../src/documents.ts';
import { object, studyInput } from '../src/trials.ts';
import { marketPage, modelBriefInput, modelFeedback } from '../src/feedback.ts';
import { LiteratureDoc } from '../src/literature-document.ts';
import { literatureInstructions, literatureTools } from '../src/literature-tools.ts';
import { CampaignResearchDoc } from '../src/pm-campaign.ts';

const instructions = `Research the owner's question using the registered brief and literature. Use native_strategy_backtest for an explicitly bound, unchanged upstream Nautilus strategy, and native_strategy_walk_forward for separate out-of-sample validation. Do not translate custom signals into non-equivalent upstream strategies. Preserve registered data, original fee models and execution receipts. Never read sealed data, invent performance, approval or trading permission. Ask for genuinely missing inputs; an ordinary discussion does not need an executable binding.
${literatureInstructions}`;
export function researchExtension(search?: Parameters<typeof literatureTools>[0]) {
  return defineExtension({ name: 'qz.research', sections: [section('research', () => instructions)],
    hooks: [hook(ToolTask, { async afterTool(_call, result, api, context) {
      if ((await api.snapshot(Study, api.conversationId, context))?.final) return { ...result, control: { ...result.control, terminate: true } };
    } })], tools: [
    ...literatureTools(search),
    defineTool({
      name: 'research_brief', description: 'Original objective, available research data and fixed policy with a compact trial ledger page. Follow nextAfterTrialId with afterTrialId for complete disclosure. research_trial reads original validation feedback without computing. No final held-out values.',
      parameters: obj({ afterTrialId: opt(int({ minimum: 1 })), limit: opt(int({ minimum: 1, maximum: 20 })) }), replay: 'safe',
      async execute(args, api, context) {
        const input = await studyInput(api, context);
        const { data, policy } = modelBriefInput(input);
        const study = await api.snapshot(Study, api.conversationId, context);
        const campaign = await api.snapshot(CampaignResearchDoc, api.conversationId, context);
        const literature = await api.snapshot(LiteratureDoc, api.conversationId, context);
        const trials = study?.trials ?? [];
        const available = trials.filter((trial) => (trial.taskId as number) > (args.afterTrialId ?? 0)).sort((a, b) => (a.taskId as number) - (b.taskId as number));
        const page = available.slice(0, args.limit ?? 10);
        return { output: [{ type: 'text', text: JSON.stringify({
          trials: page.map((trial) => { const proposal = object(trial.proposal, 'trial_proposal');
            const scope = proposal.allocations === 'ALL_EQUAL' ? proposal.strategy : null;
            const selected = scope && typeof scope === 'object' && !Array.isArray(scope) ? scope.instrumentIds : proposal.allocations;
            return {
            taskId: trial.taskId, status: trial.status, error: trial.error ?? null, hypothesis: proposal.hypothesis ?? null,
            instrumentId: proposal.instrumentId ?? null, marketCount: Array.isArray(selected) ? selected.length : null,
            memberCount: Array.isArray(proposal.members) ? proposal.members.length : null,
          }; }), totalTrials: trials.length, nextAfterTrialId: available.length > page.length ? page.at(-1)!.taskId : null,
          goal: input.goal, specification: input.specification ?? null, data, policy, stopped: study?.final !== null,
          campaign: campaign?.binding ? { binding: campaign.binding, state: campaign.state, intake: campaign.intake, hypotheses: campaign.hypotheses,
            trials: campaign.trials, budgetMovements: campaign.budgetMovements, resourceMeasurements: campaign.resourceMeasurements,
            orchestration: campaign.orchestration, latestComparison: campaign.comparisons.at(-1) ?? null } : null,
          literature: { searchCount: literature?.searches.length ?? 0, noteCount: literature?.notes.length ?? 0,
            publicQueries: (input.publicLiteratureQueries ?? []).map((query, queryIndex) => ({ queryIndex, query })),
            latestSearchId: literature?.searches.at(-1)?.id ?? null, latestStatus: literature?.searches.at(-1)?.status ?? null, readTool: 'research_literature' },
        }) }] };
      },
    }),
    defineTool({
      name: 'research_markets', description: 'Read a page of the original admitted cross-market universe in its original training order, with original training-only metadata and fee sizing reserves. Follow nextOffset to read all markets. No computation, attempt consumption, final-period performance, inferred event independence or external data.',
      parameters: obj({ offset: opt(int({ minimum: 0 })), limit: opt(int({ minimum: 1, maximum: 50 })) }), replay: 'safe',
      async execute(args, api, context) {
        const input = await studyInput(api, context);
        return { output: [{ type: 'text', text: JSON.stringify(marketPage(input, args.offset ?? 0, args.limit ?? 20)) }] };
      },
    }),
    defineTool({
      name: 'research_trial', description: 'Read one original recorded trial in this conversation: status, failure and validation feedback only. Does not compute, consume a trial, read raw evidence or expose final held-out results. Use it to recover feedback, never resubmit merely to read a result.',
      parameters: obj({ trialId: int({ minimum: 1 }) }), replay: 'safe',
      async execute(args, api, context) {
        const study = await api.snapshot(Study, api.conversationId, context);
        const trial = study?.trials.find((item) => item.taskId === args.trialId);
        if (!trial) throw new Error('research_trial_not_found');
        const proposal = object(trial.proposal, 'trial_proposal');
        return { output: [{ type: 'text', text: JSON.stringify({ trialId: trial.taskId, status: trial.status, error: trial.error ?? null,
          hypothesis: proposal.hypothesis ?? null, instrumentId: proposal.instrumentId ?? null,
          result: trial.result === null || trial.result === undefined ? null : modelFeedback(object(trial.result, 'trial_result')),
        }) }] };
      },
    }),
    defineTool({ name: 'research_finish', description: 'Record a discussion conclusion without a score, scientific qualification or trading permission. Native execution tools record their own actual results.',
      parameters: obj({ reason: str() }), replay: 'safe', executionMode: 'sequential',
      async execute(args, api, context) {
        await api.commit(async tx => {
          const study = await tx.doc(Study, api.conversationId);
          if (study.final && study.final.taskId !== api.taskId) throw new Error('research_already_final');
          study.final = { taskId: api.taskId, trialId: null, comparedTrials: [], reason: args.reason, status: 'STOPPED', result: null };
        }, context);
        return { output: [{ type: 'text', text: 'Research discussion concluded. No score or trading authorization was produced.' }], control: { terminate: true } };
      },
    }),
  ] });
}
const research = researchExtension();
export function nativeResearchExtension(source: Extension = research) {
  return defineExtension({ name: 'qz.research-native', tools: source.tools, hooks: source.hooks, sections: source.sections });
}
export default research;
