import { CheckIcon, XIcon } from '@phosphor-icons/react';
import { Button, Dialog, Select, TextArea } from '@radix-ui/themes';
import { useState } from 'react';
import { EvidencePanel } from '../components/approval/EvidencePanel';
import { EmptyState } from '../components/ui/EmptyState';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { PageHeader } from '../components/ui/PageHeader';
import { PageSkeleton } from '../components/ui/Skeleton';
import { StateBadge } from '../components/ui/StateBadge';
import { useI18n, type Locale } from '../i18n';
import { useApprovalDecision, useApprovals, useDownstreams } from '../lib/api/hooks';
import type { ApprovalSnapshot, DownstreamSystem } from '../lib/api/types';
import { formatCapitalAmount, formatDateTime, humanize } from '../lib/format';

const rejectionReasons = [
  'RESEARCH_EVIDENCE_INSUFFICIENT',
  'RISK_PROFILE_UNACCEPTABLE',
  'DRAWDOWN_TOO_HIGH',
  'TURNOVER_TOO_HIGH',
  'CAPACITY_TOO_LOW',
  'COMPLEXITY_TOO_HIGH',
  'INTERPRETABILITY_INSUFFICIENT',
  'MARKET_SCOPE_UNACCEPTABLE',
  'PAPER_EVIDENCE_INSUFFICIENT',
  'LIVE_READINESS_INSUFFICIENT',
  'NOT_ALIGNED_WITH_ORIGINAL_IDEA',
  'OTHER',
];

function compatible(approval: ApprovalSnapshot, systems: DownstreamSystem[]) {
  return systems.filter((system) => {
    if (!system.enabled) return false;
    if (system.preflight_state && !/READY|PASS|VALID/i.test(system.preflight_state)) return false;
    if (approval.purpose === 'PAPER' && system.environment_type !== 'PAPER') return false;
    if (approval.purpose === 'LIVE' && system.environment_type !== 'LIVE') return false;
    return true;
  });
}

export function formatDeployableCapital(locale: Locale, value?: number | string | null): string {
  return formatCapitalAmount(value, locale);
}

function ApprovalCard({ approval, systems }: { approval: ApprovalSnapshot; systems: DownstreamSystem[] }) {
  const { locale, t } = useI18n();
  const options = compatible(approval, systems);
  const [downstream, setDownstream] = useState(approval.downstream_system_id ?? options[0]?.id ?? '');
  const [reason, setReason] = useState('');
  const [note, setNote] = useState('');
  const decision = useApprovalDecision(approval.id);
  const pending = approval.state === 'PENDING';
  const expiry = approval.valid_until ?? approval.expires_at;
  const mutationError = decision.approve.error ?? decision.reject.error;

  return (
    <article className="qz-approval">
      <div className="qz-approval-header">
        <div>
          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}><StateBadge state={approval.purpose} /><StateBadge state={approval.state} /></div>
          <h2 className="qz-approval-title" style={{ marginTop: 10 }}><bdi dir="auto">{approval.candidate?.mandate_name ?? t('approval.portfolio')}</bdi> · {t('common.candidate')} <bdi dir="ltr">{approval.candidate_id.slice(0, 8)}</bdi></h2>
          <p className="qz-approval-rationale" dir="auto">{approval.recommendation_rationale ?? t('approval.noRationale')}</p>
        </div>
        <div className="qz-section-meta qz-number" style={{ textAlign: 'right' }}>{t('approval.validUntil')}<br />{formatDateTime(expiry)}</div>
      </div>
      <div className="qz-approval-grid">
        <div><div className="qz-label" style={{ marginBottom: 7 }}>{t('approval.level2')}</div><EvidencePanel approval={approval} /></div>
        <div className="qz-panel qz-panel-pad qz-form-grid">
          <div><div className="qz-label">{t('approval.capitalContext')}</div><div className="qz-list-title qz-number" style={{ marginTop: 5 }}><bdi dir="ltr">{approval.capital_context?.base_currency ?? '—'} {formatDeployableCapital(locale, approval.capital_context?.deployable_capital)}</bdi></div><div className="qz-list-subtitle">{t('approval.observedDate', { date: formatDateTime(approval.capital_context?.observed_at) })}</div></div>
          <div><div className="qz-label">{t('approval.humanReport')}</div><div className="qz-list-subtitle" dir="auto" style={{ whiteSpace: 'normal', lineHeight: 1.55 }}>{typeof approval.human_report === 'string' ? <bdi dir="auto">{approval.human_report}</bdi> : approval.human_report ? <bdi dir="ltr">{JSON.stringify(approval.human_report)}</bdi> : t('approval.noReport')}</div></div>
        </div>
      </div>
      <div className="qz-approval-actions">
        <div className="qz-field" style={{ minWidth: 260 }}><span className="qz-label">{t('approval.compatibleDownstream')}</span><Select.Root value={downstream} onValueChange={setDownstream} disabled={!pending}><Select.Trigger placeholder={options.length ? t('approval.selectDownstream') : t('approval.noDownstream')} /><Select.Content>{options.map((option) => <Select.Item value={option.id} key={option.id}><bdi dir="auto">{option.name}</bdi> · {humanize(option.environment_type)}</Select.Item>)}</Select.Content></Select.Root></div>
        <div style={{ display: 'flex', gap: 8 }}>
          <Dialog.Root>
            <Dialog.Trigger><Button variant="soft" color="red" disabled={!pending}><XIcon size={14} />{t('approval.reject')}</Button></Dialog.Trigger>
            <Dialog.Content maxWidth="480px">
              <Dialog.Title>{t('approval.rejectTitle')}</Dialog.Title>
              <Dialog.Description size="2">{t('approval.rejectDesc')}</Dialog.Description>
              <div className="qz-form-grid" style={{ marginTop: 16 }}>
                <label className="qz-field"><span className="qz-label">{t('approval.reasonCode')}</span><Select.Root value={reason} onValueChange={setReason}><Select.Trigger placeholder={t('approval.selectReason')} /><Select.Content>{rejectionReasons.map((item) => <Select.Item key={item} value={item}>{humanize(item)}</Select.Item>)}</Select.Content></Select.Root></label>
                <label className="qz-field"><span className="qz-label">{t('approval.optionalNote')}</span><TextArea dir="auto" value={note} onChange={(event) => setNote(event.target.value)} /></label>
                <Button color="red" disabled={!reason || decision.reject.isPending} onClick={() => decision.reject.mutate({ reason_code: reason, note: note || undefined })}>{t('approval.confirmRejection')}</Button>
              </div>
            </Dialog.Content>
          </Dialog.Root>
          <Button color="green" disabled={!pending || !downstream || decision.approve.isPending} onClick={() => decision.approve.mutate(downstream)}><CheckIcon size={14} />{decision.approve.isPending ? t('common.approving') : t('approval.approve')}</Button>
        </div>
      </div>
      {mutationError ? <div style={{ marginTop: 12 }}><ErrorPanel error={mutationError} /></div> : null}
    </article>
  );
}

export function ApprovalInboxPage() {
  const approvals = useApprovals();
  const downstreams = useDownstreams();
  if (approvals.isLoading || downstreams.isLoading) return <PageSkeleton />;
  if (approvals.error || downstreams.error) return <ErrorPanel error={approvals.error ?? downstreams.error} />;
  const sorted = [...(approvals.data ?? [])].sort((a, b) => Number(b.state === 'PENDING') - Number(a.state === 'PENDING'));
  return <><PageHeader title="Candidate Approval" description="Each decision contains one immutable system recommendation. Paper and Live are separate approvals; this page never edits weights, Alpha qualifications, Mandates, or evidence." />{sorted.length ? <div className="qz-list" style={{ gap: 16 }}>{sorted.map((approval) => <ApprovalCard key={approval.id} approval={approval} systems={downstreams.data ?? []} />)}</div> : <EmptyState title="No candidate decisions" description="The system only creates an Approval when promotion gates, evidence maturity, material improvement and downstream compatibility pass." />}</>;
}
