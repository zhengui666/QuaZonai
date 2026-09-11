import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import Ajv2020 from 'ajv/dist/2020';
import addFormats from 'ajv-formats';
import { bindingListError, budgetRelationError } from './authoring-constraints';
import { validateBaseCurrency } from './generated/responses.cjs';

const load = (path: string) => JSON.parse(readFileSync(new URL(path, import.meta.url), 'utf8'));
const cases: { tuple: Record<string, unknown>; schema_valid: boolean; admissible: boolean }[] = load('../../../tests/contracts/cost-tuples.json');
const brief = load('../../../tests/contracts/research-brief.json');
for (const name of ['api-v2', 'domain-v1']) {
  const document = load(`../../../contracts/generated/${name}.openapi.json`);
  const ajv = new Ajv2020({ strict: false }); addFormats(ajv);
  const valid = ajv.compile({ ...document.components.schemas.BudgetV1, components: document.components });
  describe(`native ${name} cost tuple alternatives`, () => {
    it.each(cases)('validates the complete tuple $tuple', item => {
      const budget = { ...brief.content.budget };
      for (const field of ['cost_enforcement', 'max_cost_decimal', 'cost_currency']) delete budget[field];
      expect(valid({ ...budget, ...item.tuple }), JSON.stringify(item.tuple)).toBe(item.schema_valid);
    });
    it('rejects unknown fields despite the composed schema', () => {
      expect(valid({ ...brief.content.budget, arbitrary_limit: 999 })).toBe(false);
    });
  });
}

describe('native currency membership and authoring relationships', () => {
  it('requires an actual non-null base currency, not just three letters', () => {
    for (const value of ['USD', 'EUR', 'CNY']) expect(validateBaseCurrency(value)).toBe(true);
    for (const value of ['AAA', 'ZZZ', 'usd', 'USD\n', null, '', 123]) expect(validateBaseCurrency(value)).toBe(false);
  });
  it('rejects the same dataset identity even with different roles or letter case', () => {
    const id = '01990000-0000-7000-8000-00000000abcd';
    const bindings = Object.freeze([{ dataset_revision_id: id, role: 'DISCOVERY' },
      { dataset_revision_id: id.toUpperCase(), role: 'SEALED', access_policy: 'EVALUATOR_ONLY' }]);
    expect(bindingListError(bindings)).toBeDefined();
    expect(bindings).toHaveLength(2);
    expect(bindingListError([{ dataset_revision_id: id }])).toBeUndefined();
    expect(bindingListError([])).toBeDefined();
    expect(bindingListError(Array.from({ length: 65 }, (_, n) => ({ dataset_revision_id: String(n) })))).toBeDefined();
  });
  it('checks both budget inequalities without mutating either side', () => {
    const value = { budget: { max_turns_per_mission: 1, max_repair_turns: 2, max_experiments: 1 },
      stop_rule: { stop_on_qualified_count: 2 } };
    const original = JSON.stringify(value);
    expect(budgetRelationError(value, 'turns')).toBeDefined();
    expect(budgetRelationError(value, 'experiments')).toBeDefined();
    expect(JSON.stringify(value)).toBe(original);
    value.budget.max_turns_per_mission = 2; value.budget.max_experiments = 2;
    expect(budgetRelationError(value, 'turns')).toBeUndefined();
    expect(budgetRelationError(value, 'experiments')).toBeUndefined();
    value.budget.max_repair_turns = 0;
    expect(budgetRelationError(value, 'turns')).toBeUndefined();
  });
});
