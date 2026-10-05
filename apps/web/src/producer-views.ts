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
