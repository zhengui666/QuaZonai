import { describe, expect, it } from 'vitest';
import { briefContent, initialBudget, initialStop } from './brief-fields';
import type { BriefContent } from './brief-fields';

function brief(): BriefContent {
  return {
    hypothesis: 'Optional memory quota regression', economic_rationale: 'Preserve the saved research budget',
    universe_version_id: '01990000-0000-7000-8000-000000000001',
    evaluation_policy_id: '01990000-0000-7000-8000-000000000002',
    execution_assumptions_id: '01990000-0000-7000-8000-000000000003',
    target_kind: 'SCORE', horizon_kind: 'FIXED_BARS', horizon_value: '1', base_currency: 'USD',
    budget: { ...initialBudget }, stop_rule: { ...initialStop },
  };
}

describe('optional Brief memory quota submission', () => {
  it('submits a new draft with an explicit null memory quota', () => {
    const result = briefContent(brief());
    expect(result.budget.max_memory_mib).toBeNull();
    expect(JSON.parse(JSON.stringify(result)).budget.max_memory_mib).toBeNull();
    expect(initialBudget.max_memory_mib).toBeNull();
  });

  it.each([1, 1024, 4096, 1048577, 4294967295])('preserves existing finite memory quota %s through save, reload and resubmission', max_memory_mib => {
    const original = brief();
    original.budget = Object.freeze({ ...original.budget, max_memory_mib });
    const saved = briefContent(original);
    const reloaded = JSON.parse(JSON.stringify(saved)) as BriefContent;
    expect(saved.budget.max_memory_mib).toBe(max_memory_mib);
    expect(reloaded.budget.max_memory_mib).toBe(max_memory_mib);
    expect(briefContent(reloaded).budget.max_memory_mib).toBe(max_memory_mib);
    expect(original.budget.max_memory_mib).toBe(max_memory_mib);
  });

  it('saves and reloads an explicit unset as null without changing the historical finite quota', () => {
    const original = brief();
    original.budget.max_memory_mib = 4096;
    const saved = briefContent(original);
    const edited = { ...saved, budget: { ...saved.budget, max_memory_mib: null } };
    const result = briefContent(edited);
    const reloaded = JSON.parse(JSON.stringify(result)) as BriefContent;
    expect(result.budget.max_memory_mib).toBeNull();
    expect(briefContent(reloaded).budget.max_memory_mib).toBeNull();
    expect(saved.budget.max_memory_mib).toBe(4096);
    expect(original.budget.max_memory_mib).toBe(4096);
  });

  it('normalizes an absent memory field to null without a numerical fallback', () => {
    const original = brief();
    delete original.budget.max_memory_mib;
    expect(briefContent(original).budget.max_memory_mib).toBeNull();
    expect(original.budget).not.toHaveProperty('max_memory_mib');
  });

  it.each([0, -1, 1.5, NaN, Infinity, 4294967296])('rejects explicit invalid memory quota %s rather than treating it as unset', max_memory_mib => {
    const original = brief();
    original.budget.max_memory_mib = max_memory_mib;
    expect(() => briefContent(original)).toThrow('内存限额');
    expect(original.budget.max_memory_mib).toBe(max_memory_mib);
  });
});
