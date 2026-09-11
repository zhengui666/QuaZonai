import { describe, expect, it } from 'vitest';
import { bindingAccessOptions, costOptions, projectStateOptions } from './authoring-options';
import type { Schema } from './api';

const project = { id: 'project', state: 'DRAFT' as const, current_brief_id: 'brief' };
const frozen = { id: 'brief', project_id: 'project', state: 'FROZEN' as const, frozen_at: '2026-09-08T00:00:00Z' };
describe('field choices based on actual resource facts', () => {
  it('never advertises unsupported exact-cost enforcement', () => {
    expect(costOptions.map(option => option.value)).toEqual(['UNAVAILABLE', 'ESTIMATED']);
  });
  it('requires the exact frozen Brief, not merely a non-null pointer', () => {
    const available = (brief?: Parameters<typeof projectStateOptions>[1]) => projectStateOptions(project, brief).map(option => option.value);
    expect(available()).not.toContain('ACTIVE');
    expect(available(frozen)).toContain('ACTIVE');
    for (const brief of [
      { ...frozen, id: 'another' }, { ...frozen, project_id: 'another' },
      { ...frozen, state: 'DRAFT' as const, frozen_at: null }, { ...frozen, frozen_at: null },
    ]) expect(available(brief)).not.toContain('ACTIVE');
    expect(projectStateOptions({ ...project, current_brief_id: null }, frozen).map(option => option.value)).not.toContain('ACTIVE');
  });
  it('never offers an exit from ARCHIVED even when its Brief is frozen', () => {
    expect(projectStateOptions({ ...project, state: 'ARCHIVED' }, frozen).map(option => option.value)).toEqual(['ARCHIVED']);
  });
  it.each(['DRAFT', 'ACTIVE', 'PAUSED'] as const)('preserves non-activation choices for %s without assuming authorization', state => {
    expect(projectStateOptions({ ...project, state }).map(option => option.value)).toEqual(['DRAFT', 'PAUSED', 'ARCHIVED']);
  });
});

describe('partition access choices never raise authority', () => {
  it.each(['DISCOVERY', 'VALIDATION', 'SEALED', 'FORWARD'] as const)('matches the existing partition rule for %s', role => {
    const actual = bindingAccessOptions(role).map(option => option.value);
    for (const access of ['METADATA_ONLY', 'RESEARCH_READ', 'EVALUATOR_ONLY'] satisfies Schema['DataAccess'][]) {
      const valid = !(role === 'SEALED' && access === 'RESEARCH_READ') && !(role !== 'SEALED' && access === 'EVALUATOR_ONLY');
      expect(actual.includes(access)).toBe(valid);
    }
  });
  it('offers no permission for an unknown or missing role', () => {
    for (const role of [undefined, null, '', 'UNKNOWN']) expect(bindingAccessOptions(role)).toEqual([]);
  });
});
