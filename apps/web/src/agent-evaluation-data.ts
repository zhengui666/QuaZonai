import type { Schema } from './api';
export type AgentReport = Schema['AgentEvaluationReportV1'];
export type AgentCase = Schema['AgentEvaluationCaseV1'];
export function caseCounts(cases: AgentCase[]) {
  const counts = { PASS: 0, FAIL: 0, BLOCKED: 0, UNRUN: 0 };
  for (const item of cases) counts[item.status]++;
  return counts;
}
export function measured(value: string | null | undefined, unit: string): string {
  return value === null || value === undefined ? '未知（未测量 / 未报告）' : `${value} ${unit}`;
}
export function observedIdentity(item: AgentCase): string {
  return item.observed ? `${item.observed.settings.model} / ${item.observed.settings.reasoning_effort}` : '未知（未观察到）';
}
