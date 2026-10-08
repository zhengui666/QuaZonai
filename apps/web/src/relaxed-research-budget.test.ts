import { describe, expect, it } from 'vitest';
import { applyRelaxedResearchBudget, initialBudget, relaxedResearchBudget } from './brief-fields';

describe('explicit relaxed execution preset', () => {
  it('preserves a stricter scientific trial budget and does not mutate its input', () => {
    const original = { ...relaxedResearchBudget, max_experiments: 3, max_repair_turns: 0 };
    const result = applyRelaxedResearchBudget(original);
    expect(result.max_experiments).toBe(3);
    expect(result.max_repair_turns).toBeNull();
    expect(original.max_repair_turns).toBe(0);
  });
  it('uses actual absent time/CPU/memory/token/output limits and an absent parallel ceiling for a new draft', () => {
    const preset = applyRelaxedResearchBudget();
    expect(preset.max_wall_seconds).toBeNull();
    expect(preset.max_tokens).toBeNull();
    expect(preset.max_cpu_seconds).toBeNull();
    expect(preset.max_memory_mib).toBeNull();
    expect(initialBudget.max_memory_mib).toBeNull();
    expect(relaxedResearchBudget.max_memory_mib).toBeNull();
    expect(preset.max_parallel_runs).toBeNull();
    expect(preset.max_output_bytes).toBeNull();
    expect(preset.max_turns_per_mission).toBeNull();
    expect(preset.max_repair_turns).toBeNull();
    expect(preset.max_cycles_per_day).toBeNull();
  });
  it('clears execution caps only when explicitly applied and preserves cost/scientific choices', () => {
    const original = { ...relaxedResearchBudget, max_experiments: 64, max_turns_per_mission: 100,
      max_repair_turns: 9, max_parallel_runs: 2, max_cpu_seconds: '72000', max_memory_mib: 4096,
      max_output_bytes: '134217728', max_wall_seconds: 120, max_tokens: '8000' };
    const result = applyRelaxedResearchBudget(original);
    expect(result.max_experiments).toBe(64);
    expect(result.max_turns_per_mission).toBeNull();
    expect(result.max_repair_turns).toBeNull();
    expect(result.max_cycles_per_day).toBeNull();
    expect(result.max_parallel_runs).toBeNull();
    expect(original.max_parallel_runs).toBe(2);
    expect(result.max_cpu_seconds).toBeNull();
    expect(result.max_memory_mib).toBe(4096);
    expect(original.max_memory_mib).toBe(4096);
    expect(result.max_output_bytes).toBeNull();
    expect(result.cost_enforcement).toBe(original.cost_enforcement);
    expect(result.max_wall_seconds).toBeNull();
    expect(result.max_tokens).toBeNull();
    expect(original.max_wall_seconds).toBe(120);
    expect(original.max_tokens).toBe('8000');
  });
});
