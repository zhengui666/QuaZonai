import type { ResearchInput } from './documents.ts';
import { publicLiteratureQueries, researchSpecification } from '../../../packages/contracts/src/research.ts';
import { researchCampaignBinding } from './pm-campaign.ts';
import { compareDecimal } from '../../../packages/contracts/src/pm-validation.ts';

export function record(value: unknown, name: string): Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${name}_required`);
  return value as Record<string, unknown>;
}

export function validateNumbers(value: unknown): void {
  if (typeof value === 'number' && (!Number.isFinite(value) || (Number.isInteger(value) && !Number.isSafeInteger(value)))) throw new Error('invalid_json_number');
  if (value !== null && typeof value === 'object') for (const child of Object.values(value)) validateNumbers(child);
}

export function researchInput(value: unknown): ResearchInput {
  const input = record(value, 'research_input');
  validateNumbers(input);
  const model = record(input.model, 'model');
  if (typeof input.goal !== 'string' || !input.goal.trim()) throw new Error('research_goal_required');
  if (typeof model.provider !== 'string' || !model.provider || typeof model.modelId !== 'string' || !model.modelId) throw new Error('model_required');
  if (input.thinkingLevel !== undefined && !['off', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max'].includes(String(input.thinkingLevel))) throw new Error('invalid_thinking_level');
  if (input.serviceTier !== undefined && input.serviceTier !== 'default' && input.serviceTier !== 'priority') throw new Error('invalid_service_tier');
  publicLiteratureQueries(input.publicLiteratureQueries);
  if (input.specification !== undefined) {
    const specification = researchSpecification(input.specification);
    if (input.goal !== specification.question) throw new Error('research_question_mismatch');
  }
  const data = record(input.data, 'data');
  const policy = record(input.policy, 'policy');
  if (input.specification !== undefined && policy.max_trials === undefined) throw new Error('research_trial_budget_required');
  if (policy.max_trials !== undefined && (!Number.isSafeInteger(policy.max_trials) || (policy.max_trials as number) < 0)) throw new Error('invalid_trial_budget');
  if (input.campaign !== undefined) {
    const campaign = researchCampaignBinding(input.campaign);
    if (policy.max_trials !== campaign.budget.maximumTrials) throw new Error('campaign_trial_budget_mismatch');
    const raw = data.catalog_ensemble_request ?? data.catalog_scan_request ?? data.request;
    if (raw !== undefined) {
      const request = record(raw, 'native_request');
      const scope = campaign.scopes[0]!;
      const trainEnd = data.catalog_ensemble_request !== undefined ? data.train_end_ns : request.train_end_ns;
      const validationEnd = data.catalog_ensemble_request !== undefined ? data.validation_end_ns : request.validation_end_ns;
      if (scope.interval.startNs !== trainEnd || scope.interval.endExclusiveNs !== validationEnd) throw new Error('campaign_native_interval_mismatch');
      const strategy = policy.strategy === undefined ? null : record(policy.strategy, 'strategy_policy');
      const settings = data.catalog_ensemble_request !== undefined ? request.settings : strategy?.simulationSettings;
      if (scope.capital !== null && settings !== undefined) {
        const original = record(settings, 'simulation_settings');
        if (scope.capital.currency !== original.base_currency || typeof original.starting_capital !== 'string'
          || compareDecimal(scope.capital.amount, original.starting_capital) !== 0) throw new Error('campaign_native_capital_mismatch');
      }
    }
  }
  return structuredClone(input) as ResearchInput;
}
