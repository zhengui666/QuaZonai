import type { Context } from '@earendil-works/chord';
import { AgentDoc, AssistantEntry, ToolResultEntry } from '@earendil-works/pi-durable';
import type { ConversationId, Cursor, EntryId, Harness, ToolTaskInput } from '@earendil-works/pi-durable';

/** Read native ownership and retained answers; never infer children from task counts. */
export async function readUltraDelegations(harness: Harness, parentConversationId: ConversationId, context: Context) {
  const rows = [];
  let cursor: Cursor | undefined;
  do {
    const page = await harness.commit(tx => tx.scanTasks({ conversationId: parentConversationId, kind: 'pi.tool' }, 100, cursor), context);
    for (const task of page.items) {
      const input = task.input as ToolTaskInput;
      const call = input.kind === 'nested' ? input.call : await harness.commit(async tx => {
        const entry = await tx.entry(input.assistant);
        return entry?.model?.filter(message => message.role === 'assistant').flatMap(message => message.content).find(part => part.type === 'toolCall' && part.id === input.callId);
      }, context);
      if (!call || call.type !== 'toolCall' || call.name !== 'ultra_delegate') continue;
      const child = await harness.commit(async tx => (await tx.scanConversations({ ownerTaskId: task.id }, 1)).items[0], context);
      const agent = child ? await harness.snapshot(AgentDoc, child.id, context) : undefined;
      const submission = child ? await harness.commit(tx => tx.submissionByRequest(child.id, `ultra-delegate:${task.id}`), context) : undefined;
      let answer: string | null = null;
      if (child && submission?.status === 'done' && submission.type === 'input') {
        answer = await harness.commit(async tx => {
          const entry = await tx.entry(submission.answer);
          if (!entry || entry.conversationId !== child.id || !AssistantEntry.is(entry)) return null;
          return (entry.model ?? []).filter(message => message.role === 'assistant').flatMap(message => message.content).filter(part => part.type === 'text').map(part => part.text).join('');
        }, context);
      }
      let error: string | null = submission?.status === 'unanswered' ? submission.reason : null;
      let toolFailed = false;
      if (task.state.status === 'terminal' && task.state.outcome.status === 'completed') {
        const result = task.state.outcome.result as { entryId?: EntryId } | null;
        if (result?.entryId !== undefined) {
          const diagnostic = await harness.commit(async tx => {
            const entry = await tx.entry(result.entryId!);
            if (!entry || !ToolResultEntry.is(entry)) return null;
            return { error: entry.data.diagnostics.filter(item => item.severity === 'error').map(item => item.message).join('; ') || null, failed: entry.model?.some(message => message.role === 'toolResult' && message.isError) ?? false };
          }, context);
          error ??= diagnostic?.error ?? null; toolFailed = diagnostic?.failed ?? false;
        }
      } else if (task.state.status === 'terminal' && 'error' in task.state.outcome) error ??= task.state.outcome.error?.message ?? null;
      rows.push({ taskId: task.id, conversationId: child?.id ?? null, submissionId: submission?.id ?? null,
        status: task.state.status === 'terminal' ? task.state.outcome.status === 'completed' ? submission?.status ?? (toolFailed ? 'failed' : 'completed') : task.state.outcome.status : task.state.status,
        error, model: agent?.model ?? null, effectiveLevel: agent?.thinkingLevel ?? null, answer });
    }
    cursor = page.next;
  } while (cursor);
  return rows;
}
