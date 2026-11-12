import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import type { ConversationId, TaskId } from '@earendil-works/pi-durable';
import type { ResearchHost } from '../../research/src/host.ts';
import type { ResearchInput } from '../../research/src/documents.ts';
import { Study, canonical } from '../../research/src/documents.ts';
import { ResearchConfiguration, registeredNativeStrategyRequest } from '../../research/src/research-configuration.ts';
import { normalizedNativeStrategyRequest } from '../../research/plugins/native-strategy-backtest.ts';
import { nativeTargetForData } from '../../research/src/native-execution.ts';
import type { Json, JsonObject, ResearchAssessmentView } from '../../../packages/contracts/src/index.ts';
import { object, Problem } from './facts.ts';

const evidenceKinds = new Set(['qz.research.assessment', 'qz.native-strategy.backtest']);
function references(value: unknown, found = new Set<number>()): Set<number> {
  if (!value || typeof value !== 'object') return found;
  for (const [key, child] of Object.entries(value)) {
    if ((key === 'taskId' || key === 'toolTaskId' || key.endsWith('TaskId')) && Number.isSafeInteger(child)) found.add(child as number);
    if (key.endsWith('TaskIds') && Array.isArray(child)) for (const id of child) if (Number.isSafeInteger(id)) found.add(id);
    if (typeof child === 'object') references(child, found);
  }
  return found;
}
/** Read original official Tasks. No parallel report store or model-text reconstruction. */
export async function readResearchAssessments(host: ResearchHost, id: number, input: ResearchInput): Promise<ResearchAssessmentView[]> {
  let request: JsonObject | null = null; let target: Json = null;
  try { request = normalizedNativeStrategyRequest(registeredNativeStrategyRequest(input)); target = nativeTargetForData(input.data) as Json; }
  catch { /* Unconfigured current input does not erase historical evidence. */ }
  return host.harness.commit(async tx => {
    const configuration = await tx.doc(ResearchConfiguration, id as ConversationId);
    const rows: ResearchAssessmentView[] = []; let cursor;
    do {
      const page = await tx.scanTasks({ conversationId: id as ConversationId, kind: 'qz.research.assessment', order: 'descending' }, 100, cursor);
      for (const task of page.items) {
        const invocation = object(task.input, 'assessment_input');
        if (invocation.mode !== 'LOGIC' && invocation.mode !== 'SYNTHESIS') continue;
        const evidence = object(invocation.evidence, 'assessment_evidence');
        let current = false;
        try {
          current = request !== null && canonical(normalizedNativeStrategyRequest(object(invocation.request, 'assessment_request'))) === canonical(request)
            && canonical(evidence.executionSource ?? null) === canonical({ catalog: input.data.catalog ?? null, target })
            && (invocation.configurationTaskId ?? null) === (configuration.configuredByTaskId ?? null);
        } catch { /* Malformed historical bindings cannot be labelled current. */ }
        const outcome = task.state.status === 'terminal' ? task.state.outcome : null;
        const result = outcome?.status === 'completed' ? object(outcome.result, 'assessment_result') : null;
        const links: ResearchAssessmentView['evidenceLinks'] = [];
        for (const taskId of references(result?.evidence ?? evidence)) {
          const linked = await tx.task(taskId as TaskId);
          if (linked?.conversationId === id && evidenceKinds.has(linked.kind)) links.push({ taskId, href: `/api/v3/research/${id}/evidence/${taskId}` });
        }
        rows.push({ taskId: task.id, mode: invocation.mode, status: outcome?.status ?? task.state.status,
          configuration: current ? 'CURRENT' : 'HISTORICAL', result,
          error: outcome?.status === 'failed' ? 'research_assessment_failed' : null,
          evidenceLinks: [{ taskId: task.id, href: `/api/v3/research/${id}/evidence/${task.id}` }, ...links.filter(link => link.taskId !== task.id)] });
      }
      cursor = page.next;
    } while (cursor);
    return rows;
  }, context);
}
/** Owner-readable evidence links stay scoped to this research and public research Task kinds. */
export async function researchTaskEvidence(host: ResearchHost, id: number, taskId: number) {
  if (!Number.isSafeInteger(id) || id < 1 || !Number.isSafeInteger(taskId) || taskId < 1) throw new Problem('research_evidence_not_found', 404);
  return host.harness.commit(async tx => {
    const task = await tx.task(taskId as TaskId);
    if (!task || task.conversationId !== id || !evidenceKinds.has(task.kind)) throw new Problem('research_evidence_not_found', 404);
    const study = await tx.doc(Study, id as ConversationId);
    if (!study.input || study.deletedAt) throw new Problem('research_evidence_not_found', 404);
    return { taskId, kind: task.kind, input: task.input, state: task.state };
  }, context);
}
