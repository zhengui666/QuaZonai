import { humanize } from '../../lib/format';

const success = new Set(['ACTIVE', 'SUCCEEDED', 'APPROVED', 'AVAILABLE', 'CLAIMED', 'DOWNSTREAM_ACCEPTED', 'FEEDBACK_RECEIVED', 'FEEDBACK_COMPLETE', 'HEALTHY', 'READY', 'CANDIDATE_READY', 'COMPLETE', 'VALID', 'ENABLED']);
const warning = new Set(['COOLING', 'WATCH', 'DEGRADING', 'WAITING_FOR_FEEDBACK', 'APPROVAL_PENDING', 'PENDING', 'PUBLISHING', 'FEEDBACK_PENDING', 'FEEDBACK_IN_PROGRESS', 'FEEDBACK_PARTIAL', 'STALE', 'INCOMPLETE']);
const danger = new Set(['FAILED', 'INVALIDATED', 'REJECTED', 'REVOKED', 'DOWNSTREAM_REJECTED', 'EXPIRED', 'BLOCKED', 'INVALID', 'UNREACHABLE', 'FEEDBACK_STALE', 'FEEDBACK_INCOMPLETE', 'FEEDBACK_INVALID', 'CONSUMER_UNREACHABLE']);
const info = new Set(['RUNNING', 'PLANNED', 'INTERRUPTED', 'CANCELLED', 'PAPER', 'LIVE']);

export function StateBadge({ state, label }: { state?: string | null; label?: string }) {
  const normalized = (state ?? 'UNKNOWN').toUpperCase();
  const tone = success.has(normalized) ? 'success' : warning.has(normalized) ? 'warning' : danger.has(normalized) ? 'danger' : info.has(normalized) ? 'info' : 'accent';
  return <span className="qz-status" data-tone={tone}>{label ?? humanize(state)}</span>;
}
