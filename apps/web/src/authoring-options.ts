// Presentation of known field constraints, never an authorization decision.
import type { Schema } from './api';

type ProjectFacts = Pick<Schema['ProjectView'], 'id' | 'state' | 'current_brief_id'>;
type BriefFacts = Pick<Schema['BriefView'], 'id' | 'project_id' | 'state' | 'frozen_at'>;

export function projectStateOptions(project: ProjectFacts, brief?: BriefFacts) {
  const options: { value: Schema['ProjectState']; label: string }[] = [
    { value: 'DRAFT', label: '草稿' }, { value: 'ACTIVE', label: '启用' },
    { value: 'PAUSED', label: '暂停' }, { value: 'ARCHIVED', label: '归档' },
  ];
  if (project.state === 'ARCHIVED') return options.filter(item => item.value === 'ARCHIVED');
  const frozen = brief !== undefined && brief.id === project.current_brief_id
    && brief.project_id === project.id && brief.state === 'FROZEN' && brief.frozen_at !== null;
  return options.filter(item => item.value !== 'ACTIVE' || frozen);
}

export function bindingAccessOptions(role: unknown): { value: Schema['DataAccess']; label: string }[] {
  if (role === 'SEALED') return [
    { value: 'METADATA_ONLY', label: 'METADATA_ONLY' }, { value: 'EVALUATOR_ONLY', label: 'EVALUATOR_ONLY' },
  ];
  if (role === 'DISCOVERY' || role === 'VALIDATION' || role === 'FORWARD') return [
    { value: 'METADATA_ONLY', label: 'METADATA_ONLY' }, { value: 'RESEARCH_READ', label: 'RESEARCH_READ' },
  ];
  return [];
}

export const costOptions = [
  { value: 'UNAVAILABLE', label: '没有可用费用度量' },
  { value: 'ESTIMATED', label: '估算值（不等于实际账单）' },
] satisfies { value: Schema['CostEnforcement']; label: string }[];
