import type { ResearchInput } from './documents.ts';

export function record(value: unknown, name: string): Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${name}_required`);
  return value as Record<string, unknown>;
}

function validateNumbers(value: unknown): void {
  if (typeof value === 'number' && (!Number.isFinite(value) || (Number.isInteger(value) && !Number.isSafeInteger(value)))) throw new Error('invalid_json_number');
  if (value !== null && typeof value === 'object') for (const child of Object.values(value)) validateNumbers(child);
}

export function researchInput(value: unknown): ResearchInput {
  const input = record(value, 'research_input');
  validateNumbers(input);
  const model = record(input.model, 'model');
  if (typeof input.goal !== 'string' || !input.goal.trim()) throw new Error('research_goal_required');
  if (typeof model.provider !== 'string' || !model.provider || typeof model.modelId !== 'string' || !model.modelId) throw new Error('model_required');
  record(input.data, 'data');
  const policy = record(input.policy, 'policy');
  if (policy.max_trials !== undefined && (!Number.isSafeInteger(policy.max_trials) || (policy.max_trials as number) < 0)) throw new Error('invalid_trial_budget');
  return structuredClone(input) as ResearchInput;
}
