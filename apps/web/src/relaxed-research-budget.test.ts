import { describe, expect, it } from 'vitest';
import { applyRelaxedResearchBudget, relaxedResearchBudget } from './brief-fields';

describe('explicit relaxed execution preset', () => {
  it('uses actual absent time/CPU/token/output limits and finite memory for a new draft', () => {
    const preset = applyRelaxedResearchBudget();
    expect(preset.max_wall_seconds).toBeNull();
    expect(preset.max_tokens).toBeNull();
    expect(preset.max_cpu_seconds).toBeNull();
    expect(preset.max_memory_mib).toBe(1024);
    expect(preset.max_output_bytes).toBeNull();
  });
  it('never lowers existing larger allowances or silently clears cost/host settings', () => {
    const original = { ...relaxedResearchBudget, max_experiments: 64, max_turns_per_mission: 100,
      max_repair_turns: 9, max_parallel_runs: 2, max_cpu_seconds: '72000', max_memory_mib: 4096,
      max_output_bytes: '134217728', max_wall_seconds: 120, max_tokens: '8000' };
    const result = applyRelaxedResearchBudget(original);
    expect(result.max_experiments).toBe(64);
    expect(result.max_turns_per_mission).toBe(100);
    expect(result.max_repair_turns).toBe(9);
    expect(result.max_parallel_runs).toBe(2);
    expect(result.max_cpu_seconds).toBeNull();
    expect(result.max_memory_mib).toBe(4096);
    expect(result.max_output_bytes).toBeNull();
    expect(result.cost_enforcement).toBe(original.cost_enforcement);
    expect(result.max_wall_seconds).toBeNull();
    expect(result.max_tokens).toBeNull();
    expect(original.max_wall_seconds).toBe(120);
    expect(original.max_tokens).toBe('8000');
  });
});
