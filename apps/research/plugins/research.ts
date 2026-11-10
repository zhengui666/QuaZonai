import { arr, int, obj, opt, str } from '@earendil-works/pi-schema';
import { defineDoc, defineExtension, defineTool, hook, section, ToolTask } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import { validateReviewPolicy } from '../../../packages/contracts/src/review-scoring.ts';
import { reviewTargetVersion } from '../src/review-identity.ts';
import { ensureTrialReview } from '../src/review-workflow.ts';
import { Study } from '../src/documents.ts';
import { object, studyInput } from '../src/trials.ts';
import { marketPage, modelBriefInput, modelFeedback } from '../src/feedback.ts';
import { LiteratureDoc } from '../src/literature-document.ts';
import { literatureInstructions, literatureTools } from '../src/literature-tools.ts';
import { CampaignResearchDoc } from '../src/pm-campaign.ts';
import { registerHypothesis } from '../src/pm-ledger.ts';
import { compareCampaign, freezeSelection } from '../src/pm-comparison.ts';
import { closeResearch } from '../src/closure.ts';
import { strategyIntakeTool } from '../src/strategy-intake.ts';
import { tickEconomicFeedback } from './tick-economic.ts';

const ExplicitFinish = defineDoc<{ intent: JsonObject | null }>({ kind: 'qz.research-explicit-finish', checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127, version: 1, scope: 'task', initial: () => ({ intent: null }) });
const detached = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

export function researchExtension(search?: Parameters<typeof literatureTools>[0]) {
  return defineExtension({
  name: 'qz.research',
  sections: [section('research', () => `Research factors or the owner's explicitly admitted strategy, not a mandatory downstream consumer. Read research_brief and follow its campaign binding.
If research_brief.campaign is null, this is standalone research: call factor_evaluate directly with hypothesis and model, omitting campaign-only hypothesisId and parentTrialIds. Do not call strategy_intake, research_hypothesis or research_compare, and do not invent a campaign, binding or identifier.
For a campaign with a binding, first call strategy_intake when binding.intakeRequired is true and preserve its actual archetype and exact owner statements. Read binding.ownerClarificationStatements and retain binding.ownerBoundaries; authenticated clarification adds meaning without resetting scientific history or data exposure. Explicit no-leverage can be normalized to maximumGrossNavRatio 1 and borrowingAllowed false; unknown wording requires clarification, and a future drawdown wish is never a forecast guarantee. Register research_hypothesis before each campaign hypothesis family; bind each campaign experiment to its registered hypothesisId and actual parentTrialIds. For campaign-bound ACCOUNT goals, after original factor probes call strategy_ensemble_fit on at least two COMPUTED source trials, then strategy_ensemble_evaluate for a separately counted joint validation replay. Use native fitted calibration and coefficients; do not handwrite ensemble weights. Read research_compare for the complete frozen campaign objective comparison; ties and unresolved preferences remain frontiers. Training fit success is not account validation.
For all research, model turns and recorded usage do not impose execution quotas. Follow nextAfterTrialId using afterTrialId for the trial ledger. For cross-market policy use research_markets(offset,limit) and nextOffset to read the complete original training-market universe and fee reserves; pages are response bounds, never market/trial caps. Use research_trial(trialId) to recover validation feedback instead of reevaluating. Propose falsifiable complete candidates and preserve failed/compared trials. Never tune against final results, look ahead, invent data or call computation qualification. Respect original criteria and stop rule. The running native host supplies successor generations when an unfinished response yields; a response boundary, tool-call count or finite invocation is not a scientific stop rule and requires no separate loop. Do not claim execution after host closure. Use research_finish only at an actual original stop condition. Use the registered final evaluator when requested; finalize alone without other reads/computations in that round. Target delivery remains optional.
${literatureInstructions}`)],
  // Native terminate is unanimous per round, not an immediate global stop.
  // Mark subsequent read/error results terminal after a recorded final; never
  // return final metrics to the model or permit more scientific computation.
  hooks: [hook(ToolTask, {
    async afterTool(_call, result, api, context) {
      if ((await api.snapshot(Study, api.conversationId, context))?.final) return { ...result, control: { ...result.control, terminate: true } };
      const campaign = await api.snapshot(CampaignResearchDoc, api.conversationId, context);
      if (campaign && campaign.state !== 'SEARCHING') return { ...result, control: { ...result.control, terminate: true } };
    },
  })],
  tools: [
    ...literatureTools(search),
    strategyIntakeTool,
    defineTool({
      name: 'research_hypothesis', description: 'Campaign-bound research only: register a falsifiable economic hypothesis before a campaign experiment. Link a parent for refinements and state the incremental information sought. This is intent, not scientific support; it does not consume a trial.',
      parameters: obj({ economicFamilyId: str(), parentHypothesisId: opt(str()), mechanism: str(), prediction: str(), falsification: str(), requiredDataCapabilities: arr(str()), incrementalQuestion: str() }), replay: 'safe', executionMode: 'sequential',
      async execute(args, api, context) { return { output: [{ type: 'text', text: JSON.stringify(await registerHypothesis(api, context, args)) }] }; },
    }),
    defineTool({
      name: 'research_compare', description: 'Campaign-bound research only: compare every registered campaign trial using exact registered selection metrics and common evidence scope. Preserve unavailable values, ties and multiobjective frontiers. Reads validation only and consumes no new trial.',
      parameters: obj({}), replay: 'safe', executionMode: 'sequential',
      async execute(_args, api, context) {
        const comparison = await api.commit(tx => compareCampaign(tx, api), context);
        if (!comparison) throw new Error('campaign_binding_required');
        return { output: [{ type: 'text', text: JSON.stringify(comparison) }] };
      },
    }),
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
          ...(trial.economic ? { economic: await tickEconomicFeedback(api, context, object(trial.economic, 'tick_economic')) } : {}),
        }) }] };
      },
    }),
    defineTool({
      name: 'research_finish', description: 'End research at an actual original stop condition without exposing an unseen sample. A response boundary, tool-call count or finite invocation is not a stop condition: the running native host supplies successor generations for unfinished research. Call alone in a final tool round. Record the actual stop reason and optional selected trial. Use a registered final evaluator when final held-out assessment is requested.',
      parameters: obj({ reason: str(), trialId: opt(int()) }),
      replay: 'safe', executionMode: 'sequential',
      async execute(args, api, context) {
        const current = await api.snapshot(CampaignResearchDoc, api.conversationId, context);
        if (current?.binding && args.trialId === undefined) {
          const compared = await api.commit(tx => compareCampaign(tx, api), context);
          const intakeStop = current.intake?.support === 'NEEDS_CLARIFICATION' ? 'strategy_intake_needs_clarification'
            : current.intake?.support === 'UNSUPPORTED' ? 'strategy_intake_unsupported' : null;
          if (!intakeStop && compared?.recommendationStatus === 'NEEDS_EVIDENCE' && !current.trials.some(trial => trial.countedAttempt)) throw new Error('research_stop_condition_not_met');
          await closeResearch(api, context, intakeStop ?? 'owner_selection_complete');
          return { output: [{ type: 'text', text: 'Research concluded with the registered deterministic comparison; independent results are owner-only.' }], control: { terminate: true } };
        }
        const finish = await api.commit(async (tx): Promise<JsonObject> => {
          const study = await tx.doc(Study, api.conversationId);
          const campaign = await tx.doc(CampaignResearchDoc, api.conversationId);
          if (study.final !== null) {
            if (study.final.taskId !== api.taskId) throw new Error('research_already_final');
            return { review: false };
          }
          if (study.input?.specification && args.trialId === undefined) throw new Error('research_stop_condition_not_met');
          const selected = study.trials.find((item) => item.taskId === args.trialId);
          if (args.trialId !== undefined && selected?.status !== 'COMPUTED') throw new Error('selected_trial_not_computed');
          if (args.trialId !== undefined && study.input?.data.sealed && campaign.binding?.independentConsumer !== 'PM_PORTFOLIO') throw new Error('registered_final_evaluator_required');
          const modern = args.trialId !== undefined && study.input !== null
            && (study.input.policy.reviewPolicy !== undefined || study.input.policy.reviewPolicies !== undefined);
          if (modern) {
            // Validate the exact selected candidate and authoritative comparison before closing search or reserving review work.
            if (study.trials.some(item => item.status === 'STARTED')) throw new Error('research_trials_unsettled');
            const bound = await tx.doc(ExplicitFinish, api.taskId);
            if (bound.intent) {
              if (bound.intent.trialId !== args.trialId || bound.intent.reason !== args.reason) throw new Error('research_finish_identity_conflict');
              return detached(bound.intent);
            }
            if (campaign.state !== 'SEARCHING') throw new Error('research_search_closed');
            const frozen = campaign.binding ? await freezeSelection(tx, api, args.trialId!) : null;
            const kind = Array.isArray(object(selected!.proposal, 'proposal').members) ? 'PORTFOLIO' : 'FACTOR';
            const policy = validateReviewPolicy(detached(study.input!.policy.reviewPolicy ?? object(study.input!.policy.reviewPolicies, 'review_policies')[kind]));
            if (policy.targetKind !== kind) throw new Error('review_policy_target_mismatch');
            const version = reviewTargetVersion(detached(study.input!) as JsonObject, detached(selected!) as JsonObject, detached(study.trials) as JsonObject[], policy);
            bound.intent = { review: true, trialId: args.trialId!, reason: args.reason, comparison: frozen, version };
            campaign.state = 'CLOSING'; campaign.closingReason = 'explicit_selection_review';
            return detached(bound.intent);
          }
          // Already-frozen legacy inputs retain their original finalization semantics.
          const frozen = campaign.binding && args.trialId !== undefined ? await freezeSelection(tx, api, args.trialId) : null;
          study.final = { taskId: api.taskId, trialId: args.trialId ?? null, comparedTrials: study.trials.map((item) => item.taskId!), reason: args.reason, status: 'STOPPED', result: null, ...(frozen ? { comparison: frozen } : {}) };
          if (campaign.binding) campaign.state = 'CONCLUDED';
          return { review: false };
        }, context);
        if (finish.review) {
          const trialId = finish.trialId as number;
          const report = await ensureTrialReview(api, context, trialId, { deferRefusalConclusion: true });
          await api.commit(async tx => {
            const study = await tx.doc(Study, api.conversationId);
            if (study.final) {
              if (study.final.taskId !== api.taskId) throw new Error('research_already_final');
              return;
            }
            const selected = study.trials.find(item => item.taskId === trialId);
            if (!study.input || selected?.status !== 'COMPUTED' || !report) throw new Error('research_finish_review_missing');
            const kind = Array.isArray(object(selected.proposal, 'proposal').members) ? 'PORTFOLIO' : 'FACTOR';
            const policy = validateReviewPolicy(detached(study.input.policy.reviewPolicy ?? object(study.input.policy.reviewPolicies, 'review_policies')[kind]));
            if (policy.targetKind !== kind) throw new Error('review_policy_target_mismatch');
            const currentVersion = reviewTargetVersion(detached(study.input) as JsonObject, detached(selected) as JsonObject, detached(study.trials) as JsonObject[], policy);
            if (currentVersion !== finish.version || report.target.version !== currentVersion
              || study.reviewSelection && (study.reviewSelection.trialId !== trialId || study.reviewSelection.version !== currentVersion)) throw new Error('research_finish_review_version_conflict');
            study.final = { taskId: api.taskId, trialId, comparedTrials: study.trials.map(item => item.taskId!), reason: finish.reason!, status: 'STOPPED', result: null,
              ...(finish.comparison ? { comparison: finish.comparison } : {}), reviewReportId: report.id, reviewDecision: report.decision };
            (await tx.doc(CampaignResearchDoc, api.conversationId)).state = 'CONCLUDED';
          }, context);
        }
        return { output: [{ type: 'text', text: 'Research stopped; original trials and selection are recorded.' }], control: { terminate: true } };
      },
    }),
  ],
  });
}

export default researchExtension();
