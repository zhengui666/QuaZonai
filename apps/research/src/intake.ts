import nativeWalkForward from '../plugins/native-walk-forward.ts';
import researchAssessment from '../plugins/research-assessment.ts';
import nativeStrategyBacktest from '../plugins/native-strategy-backtest.ts';
import { configure, defineTask, ProviderDoc } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, SubmissionId } from '@earendil-works/pi-durable';
import type { ModelThinkingLevel } from '@earendil-works/pi-ai';
import { Requests, Study } from './documents.ts';
import type { ResearchInput } from './documents.ts';
import { bindReviewSourceEvidence } from './review-source.ts';
import type { CapturedReviewSource } from './review-source.ts';
import { researchInstructions } from '../../../packages/contracts/src/research.ts';
import { ModelTiers } from './models.ts';
import { nativeResearchExtension } from '../plugins/research.ts';

export type ResearchAdmission = { conversationId: ConversationId; submissionId: SubmissionId };
type IntakeInput = { requestId: string; input: ResearchInput; thinkingLevel: ModelThinkingLevel; toolPolicy?: 'NAUTILUS_ONLY'; sourceEvidence?: JsonObject };
type IntakeCheckpoint = { phase: 'create' } | { phase: 'submit'; conversationId: ConversationId };

// Creation writes tables; submit(tx, draft) must precede table writes. Two
// official phases make that boundary durable without a startup repair scan.
export const ResearchIntakeTask = defineTask<IntakeInput, IntakeCheckpoint, ResearchAdmission>({
  name: 'qz.research.intake', version: 1, initial: () => ({ phase: 'create' }),
  phases: {
    async create(task, runtime, context) {
      await runtime.commit(async tx => {
        const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
        const input = task.input.input;
        const upstreamBinding = input.data.native_strategy_backtest_request !== undefined;
        if (input.data.indicator_strategy_request !== undefined || input.data.indicator_strategy_batch_request !== undefined) throw new Error('custom_indicator_strategy_removed');
        await configure(tx, conversation.id, { model: input.model, thinkingLevel: task.input.thinkingLevel,
          instructions: upstreamBinding ? `${researchInstructions(input)}\nStart with native_strategy_brief to read the registered official strategy and data binding without seeing results. For a complete study or overfitting assessment, plan the ordered official strategy candidates and validationMethod, then run native_strategy_logic_review before ANY backtest. Continue with native_strategy_backtest (do not finishResearch), native_strategy_walk_forward with the declared method, native_strategy_sensitivity with the declared neighbours/costs, and native_strategy_assessment_report in this same Pi conversation. Do not stop after the historical result when validation was requested. For an ordinary single-backtest request, native_strategy_backtest with finishResearch=true remains available. Ask only for genuinely missing registered data/configuration or currency assumptions; do not create unsupported strategy logic or convert USDT/USDC implicitly. State actual observed sample limits; official position-return Sharpe is not daily equity Sharpe, and short-window extrapolation is not established annual performance. A historical backtest is not independent OOS evidence or trading permission.` : researchInstructions(input),
          extensions: upstreamBinding ? [nativeStrategyBacktest, nativeWalkForward, researchAssessment] : [nativeResearchExtension(), nativeStrategyBacktest, nativeWalkForward, researchAssessment], tools: null,
        });
        if (input.serviceTier) (await tx.doc(ModelTiers)).tiers[(await tx.doc(ProviderDoc, conversation.id)).sessionId] = input.serviceTier;
        (await tx.doc(Study, conversation.id)).toolPolicy = 'NAUTILUS_ONLY';
        (await tx.doc(Study, conversation.id)).input = JSON.parse(JSON.stringify(input));
        if (task.input.sourceEvidence) await bindReviewSourceEvidence(tx, conversation.id, input.data, task.input.sourceEvidence as unknown as CapturedReviewSource);
        (await tx.doc(Requests)).requests[`request:${task.input.requestId}`]!.conversationId = conversation.id;
        return { status: 'running', checkpoint: { phase: 'submit', conversationId: conversation.id } };
      }, context);
    },
    async submit(task, runtime, context) {
      const conversationId = task.state.checkpoint.conversationId;
      const handle = await runtime.conversation(conversationId, context);
      if (!handle) throw new Error('research_conversation_missing');
      await runtime.commit(async tx => {
        const study = await tx.doc(Study, conversationId);
        if (study.final || study.deletedAt) throw new Error('research_already_final');
        const submissionId = await handle.submit(tx, { type: 'input', content: researchInstructions(task.input.input), requestId: task.input.requestId });
        (await tx.doc(Requests)).requests[`request:${task.input.requestId}`]!.submissionId = submissionId;
        return { status: 'terminal', outcome: { status: 'completed', result: { conversationId, submissionId } } };
      }, context);
    },
  },
  abort: (_task, runtime, context) => runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context),
});
