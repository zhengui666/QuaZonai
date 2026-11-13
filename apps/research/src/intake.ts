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
        const upstreamBinding = input.data.native_strategy_backtest_request !== undefined || typeof input.data.catalog === 'string' && !['request', 'catalog_scan_request', 'sealed', 'nativeInput'].some(key => input.data[key] !== undefined);
        if (input.data.indicator_strategy_request !== undefined || input.data.indicator_strategy_batch_request !== undefined) throw new Error('custom_indicator_strategy_removed');
        await configure(tx, conversation.id, { model: input.model, thinkingLevel: task.input.thinkingLevel,
          instructions: upstreamBinding ? `${researchInstructions(input)}\nFor an owner-specified installed Rust backtest factory, use native_configured_configure with its name and unchanged config. Before results, declare every intended complete factory request in native_strategy_logic_review validationMethod.requests, including the purpose and independence assumptions of each case. Execute each case with native_configured_backtest, explicitly configure the next request, and request native_strategy_assessment_report after reading original native results. Missing implementation or data evidence must be disclosed, with null ratings when insufficient. This path can construct official multi-instrument BacktestRunConfig/BacktestNode; do not squeeze it into the single-instrument strategy request or invent QZ folds, scores or equity. The following single-strategy configuration instructions apply only to native_strategy_backtest. Start with native_strategy_brief to read the registered data binding and generated request schema without seeing results. If no strategy request is configured, inspect native_strategy_catalog and use native_strategy_configure to configure the official strategy with explicit parameters from the owner mandate. Ask only for missing substantive assumptions; do not ask the owner to handwrite JSON. Catalog file intervals are availability coverage, not a source-event window or completeness proof. For a complete study or overfitting assessment, declare complete original requests in validationMethod.requests, including each explicit window, strategy, venue and costs, and record each request purpose (training, held-out, or stress) in the declared protocol; then run native_strategy_logic_review before ANY backtest. Configure and execute each declared request separately with native_strategy_configure and native_strategy_backtest (do not finishResearch), read each original result, then call native_strategy_assessment_report in this same Pi conversation. QZ does not generate folds, grids or winners. The review must establish data separation and evidence sufficiency; historical results alone do not prove OOS. Do not stop after the historical result when validation was requested. For an ordinary single-backtest request, native_strategy_backtest with finishResearch=true remains available. Ask only for genuinely missing registered data/configuration or currency assumptions; do not create unsupported strategy logic or convert USDT/USDC implicitly. For an equity-return or annual-performance objective, predeclare metric_source=EQUITY_PERFORMANCE and explicit annualization_days=252 or 365 with the matching official native_key; explain the selected convention. Missing official equity returns remain unavailable, never silently substitute CANONICAL position returns. Market-data book snapshots are not every exchange event: inspect and disclose actual depth/cadence, omitted empty or malformed books, and carry/checkpoint semantics when present. Do not describe snapshot-based simulated fills as verified real high-frequency executions. Treat MARGIN starting balances as initial simulation collateral, not a position-notional cap. ORDER_BOOK_IMBALANCE max_trade_size limits each trade, not accumulated positions. venue.leverage=1 does not establish unlevered exposure because official margin requirements also depend on instrument.margin_init. Inspect original native account, position and portfolio snapshot evidence for recorded exposure and margin; missing rows are unknown, not zero, and ending positions do not prove peak exposure. Do not invent leverage ratios or impose unrequested position caps. State actual observed sample limits; official position-return Sharpe is not daily equity Sharpe, and short-window extrapolation is not established annual performance. A historical backtest is not independent OOS evidence or trading permission.` : researchInstructions(input),
          extensions: upstreamBinding ? [nativeStrategyBacktest, researchAssessment] : [nativeResearchExtension(), nativeStrategyBacktest, researchAssessment], tools: null,
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
