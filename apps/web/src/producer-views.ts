import type { Schema } from './api';

// These are shape discriminants of validated API envelopes, not eligibility checks.
export function isForecastAlphaVersion(value: Schema['AlphaVersionEnvelopeV2']): value is Schema['AlphaVersionView'] {
  return 'signal_kind' in value;
}

export function isForecastMandate(value: Schema['MandateViewEnvelopeV2']): value is Schema['MandateViewV1'] {
  return 'objective' in value.content;
}

export function isForecastCandidate(value: Schema['PortfolioCandidateListEnvelopeV2']): value is Schema['CandidateViewV1'] {
  return 'execution_status' in value;
}

export function isForecastCandidateDetail(value: Schema['PortfolioCandidateEnvelopeV2']): value is Schema['CandidateDetailV1'] {
  return 'header' in value;
}

export function isForecastRelease(value: Schema['ReleaseViewEnvelopeV2']): value is Schema['ReleaseViewV1'] {
  return 'evaluation_id' in value;
}

// Package versions describe immutable records, not approval or native readiness.
export function targetReleaseProtocol(value: Schema['ReleaseViewEnvelopeV2']): string {
  if (value.package_schema_version !== '2') return `V${value.package_schema_version} · 历史不可交付`;
  return isForecastRelease(value) ? 'V2 · Forecast 评估' : 'V2 · 原生目标决策';
}

export function activeTargetVersions(versions: readonly string[]): boolean {
  return versions.length === 1 && versions[0] === '2';
}

// Never coerce historical ["1"] or ["1", "2"] into a new capability claim.
// An explicit selection is required before a settings write can be serialized.
export function targetDownstreamConfiguration(value: Schema['DownstreamConfigurationV1']): Schema['DownstreamUpdate']['configuration'] {
  if (!activeTargetVersions(value.accepted_package_versions)) throw new Error('保存前请明确选择目标包 V2；历史 V1 不能用于新交付。');
  return { ...value, accepted_package_versions: ['2'] };
}

export function downstreamTargetStatus(value: Schema['DownstreamConfigurationV1']): string {
  if (!value.accepted_package_versions.includes('2')) return '历史 V1 配置，不可交付';
  if (!activeTargetVersions(value.accepted_package_versions)) return '含历史 V1 的配置，修改时须明确仅选 V2';
  return value.enabled ? 'V2 配置已启用，仍须原生就绪探测' : '已停用';
}
