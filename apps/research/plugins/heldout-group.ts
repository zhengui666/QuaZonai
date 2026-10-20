import { Type } from '@earendil-works/pi-ai';
import { defineTool } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, TaskId } from '@earendil-works/pi-durable';
import { Study, canonical } from '../src/documents.ts';
import { object } from '../src/trials.ts';
import { FactorTask } from './native-factor-task.ts';
import { iterationRequest } from './native-wasm.ts';
import { checkExposure } from './native-exposure.ts';
import { catalogScope } from './native-catalog-scope.ts';
import { classifyExecutionError, executionDiagnostic, ExecutionFailure } from '../src/execution-errors.ts';

/** Assess preselected original studies together; never reopen an exposed window. */
export const finalizeGroup = defineTool({
  name: 'factor_finalize_group',
  description: 'Assess two or more original recorded Wasm selections together on one unseen held-out window. Include this conversation, select every member before any final assessment, then end research. No final tuning feedback.',
  parameters: Type.Object({ reason: Type.String(), selections: Type.Array(Type.Object({ researchId: Type.Integer(), trialId: Type.Integer() }), { minItems: 2 }) }),
  replay: 'safe', executionMode: 'sequential',
  async execute(args, api, context) {
    if (!args.selections.some((selection) => selection.researchId === api.conversationId)) throw new Error('caller_selection_required');
    if (new Set(args.selections.map((selection) => selection.researchId)).size !== args.selections.length) throw new Error('distinct_research_selections_required');
    const sources = await api.commit(async (tx) => {
      const members = [];
      for (const selection of args.selections) {
        const id = selection.researchId as ConversationId;
        if (!await tx.conversation(id)) throw new Error('research_not_found');
        const study = await tx.doc(Study, id);
        if (!study.input) throw new Error('research_input_missing');
        if (study.final && (study.final.taskId !== api.taskId || study.final.trialId !== selection.trialId)) throw new Error('research_already_final');
        const trial = study.trials.find((trial) => trial.taskId === selection.trialId);
        if (trial?.status !== 'COMPUTED') throw new Error('selected_trial_not_computed');
        const model = object(trial.proposal, 'proposal').model;
        if (typeof model !== 'string') throw new Error('selected_factor_evaluator_unavailable');
        const data = JSON.parse(JSON.stringify(object(study.input.data.sealed, 'heldout_data'))) as JsonObject;
        if (canonical(iterationRequest(data)) !== canonical(object(trial.result, 'trial_result').request!)) throw new Error('heldout_criteria_mismatch');
        if (members.length && canonical(data) !== canonical(members[0]!.data)) throw new Error('heldout_group_window_mismatch');
        study.final ??= { taskId: api.taskId, trialId: selection.trialId, comparedTrials: study.trials.map((trial) => trial.taskId!),
          selections: args.selections, reason: args.reason, status: 'STARTED', result: null, error: null };
        members.push({ id, model, data, settled: study.final.status !== 'STARTED' });
      }
      return members;
    }, context);
    if (sources.some((source) => !source.settled)) {
      try {
        const original = sources[0]!.data;
        const scoped = original.catalog_scan_request !== undefined ? await catalogScope(api, context, original) : original;
        await checkExposure(api, context, scoped, true);
        const children = await api.commit(async (tx) => {
          const result = [];
          for (const source of sources) {
            const final = (await tx.doc(Study, source.id)).final!;
            if (final.status !== 'STARTED') continue;
            final.nativeTaskId ??= await tx.createTask(FactorTask, { model: source.model, data: scoped }, { ownership: { kind: 'task', taskId: api.taskId } });
            result.push({ id: source.id, taskId: final.nativeTaskId as TaskId<JsonObject> });
          }
          return result;
        }, context);
        for (const child of children) {
          const settled = await api.waitForTask(child.taskId, context);
          if (settled.state.outcome.status !== 'completed') {
            const outcome = settled.state.outcome;
            const detail = 'error' in outcome ? outcome.error?.detail : undefined;
            throw new ExecutionFailure('error' in outcome && outcome.error ? outcome.error.message : outcome.status,
              detail && typeof detail === 'object' && !Array.isArray(detail) ? detail : { operation: 'evaluate-factor' });
          }
          const result = settled.state.outcome.result;
          await api.commit(async (tx) => { const final = (await tx.doc(Study, child.id)).final!; final.result = result; final.status = 'COMPUTED'; }, context);
        }
      } catch (error) {
        if (context.abortSignal?.aborted) throw error;
        await api.commit(async (tx) => {
          for (const source of sources) {
            const final = (await tx.doc(Study, source.id)).final!;
            if (final.status === 'STARTED') {
              final.status = 'FAILED'; final.error = executionDiagnostic(error, 'heldout_group_failed'); final.errorDetail = { ...classifyExecutionError(error) };
            }
          }
        }, context);
      }
    }
    return { content: [{ type: 'text', text: 'Joint research assessment ended. Original selections, one exposure and native outcomes are recorded for the owner; no final tuning feedback is returned.' }], control: { terminate: true } };
  },
});
