import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { validatePmEntity } from '../../../packages/contracts/src/pm-validation.ts';
import { canonical } from '../../research/src/documents.ts';
import { counter, decimal, json, nowNs, object, Problem } from './facts.ts';
import { captureAccount } from './pm-account.ts';
import type { CapturedAccount } from './pm-account.ts';
import { gap, identity } from './pm-facts.ts';
import type { PmRecord } from './pm-facts.ts';

export const campaignAccountKey = (campaignId: string) => `campaign-account-start:${campaignId}`;

/** Reconstruct only the original captured fact, never a new source receipt. */
function originalAccount(record: Pick<PmRecord, 'evidence' | 'artifacts'>, value: unknown): CapturedAccount {
  const basis = validatePmEntity('AccountBasis', json(value));
  const evidence = record.evidence[basis.positionsEvidenceRef];
  if (!evidence || evidence.sourceKind !== 'ACCOUNT' || evidence.role !== 'OBSERVED' || evidence.visibility !== 'OWNER_ONLY') throw new Problem('campaign_account_original_evidence_missing', 409);
  const artifact = record.artifacts[evidence.objectKey];
  if (!artifact || evidence.sourceRevision !== identity('account-source', artifact)
    || evidence.objectKey !== evidence.sourceRevision || evidence.id !== identity('evidence', evidence.sourceRevision)
    || basis.outstandingCommandsRef !== evidence.objectKey) throw new Problem('campaign_account_original_evidence_mismatch', 409);
  const snapshot = object(artifact.snapshot); const control = object(artifact.capitalControl); const observation = object(artifact.observation);
  const availableAtNs = [counter(artifact.snapshotReceivedAtNs, 'snapshot_received'), counter(artifact.capitalControlReceivedAtNs, 'control_received'),
    ...(artifact.capitalCapabilities ? [counter(object(artifact.capitalCapabilities).receivedAtNs, 'capabilities_received')] : [])]
    .reduce((latest, item) => BigInt(item) > BigInt(latest) ? item : latest);
  const currency = String(snapshot.currency ?? basis.currency);
  const expected: Pm.AccountBasis = { accountBindingId: String(observation.id), accountSourceId: String(snapshot.accountSourceId ?? control.accountSourceId ?? observation.accountId),
    observationId: String(snapshot.observationId ?? control.observationId ?? observation.id), sequence: counter(snapshot.sequence, 'account_sequence'),
    observedAtNs: counter(snapshot.nativeClockNs, 'account_clock'), availableAtNs, controlRevision: String(control.controlRevision ?? ''), currency,
    equity: snapshot.equity === null || snapshot.equity === undefined ? null : { amount: decimal(snapshot.equity, 'account_equity'), currency },
    reserved: control.reserved === null ? null : { amount: decimal(control.reserved, 'account_reserved'), currency },
    positionsEvidenceRef: evidence.id, outstandingCommandsRef: evidence.objectKey };
  if (basis.valuation || canonical(json(basis)) !== canonical(json(expected)) || evidence.availableAtNs !== availableAtNs
    || BigInt(basis.observedAtNs) > BigInt(availableAtNs)) throw new Problem('campaign_account_original_basis_mismatch', 409);
  return json({ basis, evidence, artifact }) as unknown as CapturedAccount;
}

/** Each campaign admits one immutable historical starting state. Unknown
 * current control remains unknown in this state; no previous campaign fallback. */
export function freezeCampaignAccount(record: PmRecord, campaign: Pm.ResearchCampaign, capture: CapturedAccount) {
  if (record.mandate.capitalSource.kind !== 'ACCOUNT' || capture.basis.accountBindingId !== record.mandate.capitalSource.accountBindingId) throw new Problem('campaign_account_binding_mismatch', 409);
  const marker = json({ schemaVersion: 1, campaignId: campaign.id, datasetRevisionIds: campaign.datasetRevisionIds, basis: capture.basis, evidenceRef: capture.evidence.id });
  const key = campaignAccountKey(campaign.id); const previous = record.artifacts[key];
  if (previous && canonical(previous) !== canonical(marker)) throw new Problem('campaign_account_start_immutable', 409);
  const evidence = record.evidence[capture.evidence.id]; const artifact = record.artifacts[capture.evidence.objectKey];
  if (evidence && canonical(json(evidence)) !== canonical(json(capture.evidence)) || artifact && canonical(artifact) !== canonical(capture.artifact)) throw new Problem('campaign_account_evidence_conflict', 409);
  originalAccount({ evidence: { [capture.evidence.id]: capture.evidence }, artifacts: { [capture.evidence.objectKey]: capture.artifact } }, capture.basis);
  record.evidence[capture.evidence.id] = json(capture.evidence) as unknown as Pm.EvidenceRef;
  record.artifacts[capture.evidence.objectKey] = json(capture.artifact);
  record.artifacts[key] ??= marker;
}

/** Called only at creation, not polling. A failed admission is retained as a
 * gap and is never silently rebound to a later successful source receipt. */
export function admitCampaignAccount(record: PmRecord, campaign: Pm.ResearchCampaign, application: Parameters<typeof captureAccount>[0], atNs = nowNs()) {
  if (record.mandate.capitalSource.kind !== 'ACCOUNT') return;
  const failedKey = `campaign-account-start-unavailable:${campaign.id}`;
  if (record.artifacts[campaignAccountKey(campaign.id)] || record.artifacts[failedKey]) return;
  try { freezeCampaignAccount(record, campaign, captureAccount(application, record.mandate.capitalSource.accountBindingId, atNs)); }
  catch (error) {
    const reason = error instanceof Problem ? error.message : 'campaign_account_start_unavailable';
    record.artifacts[failedKey] = json({ campaignId: campaign.id, datasetRevisionIds: campaign.datasetRevisionIds, availableAtNs: atNs, reason });
    gap(record, 'campaign_account_start', reason, 'REFRESH_SOURCE', campaign.id);
  }
}

export function campaignAccount(record: PmRecord, campaign: Pm.ResearchCampaign): CapturedAccount {
  const key = campaignAccountKey(campaign.id); let marker = record.artifacts[key];
  if (!marker) {
    // Old records may recover only their own admitted historical cycle. A
    // different campaign, data revision or later current forward is not proof.
    const originals = Object.values(record.cycles).filter(cycle => record.artifacts[cycle.id]?.campaignId === campaign.id
      && record.artifacts[cycle.id]?.operatingForward !== true && cycle.accountBasis
      && canonical(json(cycle.datasetRevisionIds)) === canonical(json(campaign.datasetRevisionIds)))
      .map(cycle => originalAccount(record, cycle.accountBasis));
    if (!originals.length) throw new Problem('campaign_account_start_missing', 409);
    if (originals.some(item => canonical(json(item.basis)) !== canonical(json(originals[0]!.basis)))) throw new Problem('campaign_account_start_ambiguous', 409);
    freezeCampaignAccount(record, campaign, originals[0]!); marker = record.artifacts[key]!;
  }
  if (marker.schemaVersion !== 1 || marker.campaignId !== campaign.id
    || canonical(marker.datasetRevisionIds!) !== canonical(json(campaign.datasetRevisionIds))) throw new Problem('campaign_account_start_dataset_mismatch', 409);
  const captured = originalAccount(record, marker.basis);
  if (marker.evidenceRef !== captured.evidence.id || record.mandate.capitalSource.kind !== 'ACCOUNT'
    || captured.basis.accountBindingId !== record.mandate.capitalSource.accountBindingId) throw new Problem('campaign_account_binding_mismatch', 409);
  return captured;
}
