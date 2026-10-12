import { ApiFailure } from './api';
import type { Schema } from './api';

// Projections for presentation only. The native payload remains available in full;
// plugin documents and native status strings are not a second wire schema.
export type NativeObject = Record<string, unknown>;
export type ResearchSubmission = Schema['DurableResearchStartV1'];
export type ResearchModel = Schema['DurableModelViewV1'];
export type NativeConversation = NativeObject & { id: number };
export type ResearchPage = { items: NativeConversation[]; cursor?: NativeObject; raw: unknown };
export type ResearchView = {
  conversation: NativeConversation; entries: unknown[]; docs: NativeObject;
  research: NativeObject & { trials: NativeObject[] }; raw: unknown;
};

export function object(value: unknown): NativeObject | undefined {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value as NativeObject : undefined;
}
export function nativeId(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}
function incompatible(label: string): never {
  throw new ApiFailure('HTTP_CONTRACT_ERROR', `${label}的原生响应结构不兼容；请查看 Host 配置。`);
}
export function modelsOf(value: unknown): ResearchModel[] {
  if (!Array.isArray(value)) return incompatible('模型列表');
  return value.map(item => {
    const model = object(item);
    if (!model || typeof model.provider !== 'string' || !model.provider || typeof model.modelId !== 'string' || !model.modelId
      || typeof model.name !== 'string' || typeof model.reasoning !== 'boolean') return incompatible('模型列表');
    return { provider: model.provider, modelId: model.modelId, name: model.name, reasoning: model.reasoning };
  });
}
export function conversationsOf(value: unknown): ResearchPage {
  const page = object(value);
  if (!page || !Array.isArray(page.items)) return incompatible('研究列表');
  const items = page.items.map(item => {
    const conversation = object(item);
    if (!conversation || !nativeId(conversation.id)) return incompatible('会话编号');
    return conversation as NativeConversation;
  });
  if (page.cursor !== undefined && !object(page.cursor)) return incompatible('研究分页游标');
  return { items, cursor: object(page.cursor), raw: value };
}
export function viewOf(value: unknown, id: number): ResearchView {
  const view = object(value); const conversation = object(view?.conversation);
  const research = object(view?.research); const docs = object(view?.docs);
  if (!view || !conversation || !nativeId(conversation.id) || conversation.id !== id || !Array.isArray(view.entries)
    || !research || !Array.isArray(research.trials) || !docs) return incompatible('研究详情');
  const trials = research.trials.map(trial => object(trial) ?? incompatible('试验记录'));
  return { conversation: conversation as NativeConversation, entries: view.entries, docs,
    research: { ...research, trials }, raw: value };
}
export function receiptOf(value: unknown): Pick<Schema['DurableSubmissionReceiptV1'], 'conversationId' | 'submissionId'> {
  const receipt = object(value);
  if (!receipt || !nativeId(receipt.conversationId) || !nativeId(receipt.submissionId)) return incompatible('提交回执');
  return { conversationId: receipt.conversationId, submissionId: receipt.submissionId };
}
export function tasksOf(value: unknown): NativeObject & { tasks: NativeObject[]; submissions: unknown[] } {
  const snapshot = object(value);
  if (!snapshot || !Array.isArray(snapshot.tasks) || !Array.isArray(snapshot.submissions)) return incompatible('任务快照');
  const tasks = snapshot.tasks.map(task => {
    const row = object(task);
    if (!row || !object(row.record) || !object(row.state)) return incompatible('任务快照');
    return row;
  });
  return { ...snapshot, tasks, submissions: snapshot.submissions };
}
export function usageOf(value: unknown): { models: NativeObject; tools: NativeObject; raw: unknown } {
  const usage = object(value); const models = object(usage?.models); const tools = object(usage?.tools);
  if (!models || !tools) return incompatible('用量');
  return { models, tools, raw: value };
}

export function nativeText(value: unknown): string {
  if (value === undefined) return '未报告';
  if (value === null) return '未报告（null）';
  if (typeof value === 'string') return value;
  return JSON.stringify(value) ?? '未报告';
}
export function hypothesisOf(proposal: unknown): string | undefined {
  const hypothesis = object(proposal)?.hypothesis;
  return typeof hypothesis === 'string' ? hypothesis : undefined;
}
export function jsonObjectInput(text: string, label: string): ResearchSubmission['input']['data'] {
  let value: unknown;
  try {
    value = JSON.parse(text, (_key, item: unknown) => {
      if (typeof item === 'number' && !Number.isFinite(item)) throw new Error('non-finite number');
      return item;
    });
  } catch { throw new Error(`${label}必须是有效的 JSON 对象。`); }
  if (!object(value)) throw new Error(`${label}必须是 JSON 对象，不能是数组、null 或标量。`);
  return value as ResearchSubmission['input']['data'];
}
export function policyInput(text: string): ResearchSubmission['input']['policy'] {
  const policy = jsonObjectInput(text, '研究政策');
  if (policy.max_trials !== undefined && (typeof policy.max_trials !== 'number' || !Number.isSafeInteger(policy.max_trials) || policy.max_trials < 0)) {
    throw new Error('max_trials 必须是非负的安全整数。');
  }
  return policy;
}
/** A single form intent. A retry always sends this captured body, not current fields. */
export class ResearchIntent {
  readonly body: ResearchSubmission;
  constructor(input: ResearchSubmission['input'], requestId: string = crypto.randomUUID()) {
    this.body = { requestId, input: structuredClone(input) };
  }
}
