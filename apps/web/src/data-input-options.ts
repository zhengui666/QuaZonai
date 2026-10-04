import { ApiFailure, isCounter } from './api';
import type { Schema } from './api';

export type WorkbenchPurpose = Extract<Schema['InputPurpose'], 'DISCOVERY' | 'VALIDATION'>;
export type DatasetSelection = { dataset: Schema['DatasetView']; source: Schema['DataSourceView'] };
export const validationDatasetLimit = 255;

/** UTC microseconds, without passing a fractional instant through JS Date. */
export function utcMicros(value: string): bigint | undefined {
  const match = /^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2})(?:\.(\d{1,6}))?Z$/.exec(value);
  if (!match) return undefined;
  const milliseconds = Date.parse(`${match[1]}Z`);
  if (!Number.isFinite(milliseconds) || new Date(milliseconds).toISOString() !== `${match[1]}.000Z`) return undefined;
  return BigInt(milliseconds) * 1000n + BigInt((match[2] ?? '').padEnd(6, '0'));
}

export function datasetIssue(selection: DatasetSelection, purpose: WorkbenchPurpose, runtimeId: string): string | undefined {
  const { dataset, source } = selection;
  if (dataset.source_id !== source.id || source.runtime_id !== runtimeId) return '数据源不属于所选 Runtime';
  if (dataset.partition !== purpose) return '分区与冻结用途不一致';
  if (!dataset.native_metadata_artifact_id) return '缺少原生登记证据';
  if (!dataset.source_enabled || !source.enabled || !dataset.runtime_enabled) return '数据源或 Runtime 已停用';
  if (dataset.license_state !== 'ACTIVE') return `读取时许可状态：${dataset.license_state}`;
  return undefined;
}

export function createFrozenRequest(projectId: string, purpose: WorkbenchPurpose, cutoff: string,
  runtimeId: string, selected: DatasetSelection[]): Schema['InputSetCreate'] {
  if (!projectId || !runtimeId) throw new Error('请明确选择项目和 Runtime');
  if (purpose !== 'DISCOVERY' && purpose !== 'VALIDATION') throw new Error('此工作台仅创建 DISCOVERY / VALIDATION 输入');
  const instant = utcMicros(cutoff);
  if (instant === undefined) throw new Error('请填写精确 UTC 时间，最多六位小数，例如 2026-01-01T00:00:00.000001Z');
  if (selected.length < 1 || selected.length > validationDatasetLimit) throw new Error('请选择 1–255 个已登记数据版本');
  if (new Set(selected.map(item => item.dataset.id)).size !== selected.length) throw new Error('不能重复选择同一数据版本');
  for (const item of selected) {
    const issue = datasetIssue(item, purpose, runtimeId);
    if (issue) throw new Error(issue);
    const available = utcMicros(item.dataset.available_through);
    if (available === undefined || available > instant) throw new Error('截止时间不得早于所选版本的 available_through');
  }
  return { schema_version: 1, project_id: projectId, purpose, decision_cutoff: cutoff,
    items: selected.map(({ dataset }) => ({ kind: 'DATASET', dataset_revision_id: dataset.id, role: purpose })) };
}

export function validationInputIssue(input: Schema['InputSetView'], projectId: string): string | undefined {
  if (input.header.project_id !== projectId) return '输入不属于当前项目';
  if (!['DISCOVERY', 'VALIDATION'].includes(input.header.purpose)) return '仅 DISCOVERY / VALIDATION 可请求独立数据验证';
  if (!input.items.length || input.items.length > validationDatasetLimit) return '独立数据验证需要 1–255 个数据版本';
  if (input.items.some(({ item }) => item.kind !== 'DATASET' || item.role !== input.header.purpose)) return '此工作台仅验证与用途一致的纯数据输入';
  return undefined;
}

export type ValidationLimits = { cpu_seconds: string; wall_seconds: number; memory_mib: number; output_bytes: string };
export function validationRequest(input: Schema['InputSetView'], projectId: string,
  runtime: Schema['RuntimeView'], selections: DatasetSelection[], limits: ValidationLimits): Schema['DataValidateRequest'] {
  const issue = validationInputIssue(input, projectId);
  if (issue) throw new Error(issue);
  if (!runtime.configuration.enabled || !runtime.configuration.allowed_capabilities.includes('DATA_VALIDATE') || !isCounter(runtime.revision, true)) throw new Error('所选 Runtime 不允许 DATA_VALIDATE');
  if (selections.length !== input.items.length) throw new Error('请重新载入全部原始数据绑定');
  for (const { item } of input.items) {
    const selection = selections.find(({ dataset }) => item.kind === 'DATASET' && dataset.id === item.dataset_revision_id);
    if (!selection) throw new Error('原始数据绑定缺失');
    const problem = datasetIssue(selection, input.header.purpose as WorkbenchPurpose, runtime.id);
    if (problem) throw new Error(problem);
  }
  if (!isCounter(limits.cpu_seconds, true) || !isCounter(limits.output_bytes, true)
    || BigInt(limits.output_bytes) > 67108864n
    || !Number.isInteger(limits.wall_seconds) || limits.wall_seconds < 1 || limits.wall_seconds > 86400
    || !Number.isInteger(limits.memory_mib) || limits.memory_mib < 1 || limits.memory_mib > 1048576) throw new Error('资源限额超出允许范围');
  return { schema_version: 1, project_id: projectId, input_set_id: input.header.id,
    runtime_id: runtime.id, expected_runtime_revision: runtime.revision,
    limits: { schema_version: 1, experiments: 0, ...limits } };
}

export function runtimeValidationIssue(runtime: Schema['RuntimeView'], readiness: Schema['RuntimeReadinessV1'], now: number): string | undefined {
  const observation = readiness.latest_observation;
  if (runtime.id !== readiness.runtime_id || runtime.revision !== readiness.integration_revision
    || observation?.runtime_id !== runtime.id || observation.integration_revision !== runtime.revision) return 'Runtime 版本或观测已改变，请重新载入并确认';
  if (!runtime.configuration.enabled || readiness.state !== 'AVAILABLE'
    || !readiness.available_job_kinds.includes('DATA_VALIDATE') || observation.outcome.status !== 'AVAILABLE') return 'Runtime 尚未具备 DATA_VALIDATE 能力，请到集成完成原生探测';
  if (!Number.isFinite(Date.parse(observation.valid_until)) || Date.parse(observation.valid_until) <= now) return 'Runtime 探测已过期，请到集成重新探测';
  return undefined;
}

// A lost/malformed acknowledgement or server failure is not proof of rejection.
export function unknownOutcome(error: unknown): boolean {
  return !(error instanceof ApiFailure) || error.status === 0 || error.status >= 500 || error.code === 'HTTP_CONTRACT_ERROR';
}
export function requiresReload(error: unknown): boolean {
  return error instanceof ApiFailure && (error.code === 'REVISION_CONFLICT' || error.code === 'CAPABILITY_UNAVAILABLE'
    || error.problem?.field_errors.some(field => /revision|runtime|input|dataset/.test(field.field)) === true);
}

export function validationArtifacts(items: Schema['ArtifactView'][], project: string, run: string, attempt: string | null | undefined): Schema['ArtifactView'][] {
  return items.filter(item => !!attempt && item.project_id === project && item.producer_run_id === run && item.producer_attempt_id === attempt
    && item.created_by === 'RUNTIME' && item.access_class === 'RESEARCH');
}
