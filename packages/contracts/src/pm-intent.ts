import type { Json } from './index.ts';
import type { OwnerBoundary, PmCommand } from './pm.ts';
import { compareDecimal, pmCanonical, validatePmEntity } from './pm-validation.ts';

/** Owner-authenticated clarification adds meaning; it never deletes earlier
 * constraints, resets research resources or grants reuse of exposed evidence. */
export interface PmIntentClarification extends PmCommand {
  statement: string;
  boundaries: { kind: OwnerBoundary['kind']; sourceStatement: string; value: Json }[];
  resolvedAmbiguities: string[];
}
export interface OwnerBoundaryProposal {
  kind: OwnerBoundary['kind']; sourceStatement: string; normalizedValue?: Json;
}
export interface OwnerBoundaryNormalizationContext {
  id: string; effectiveFromNs: string; sourceStatements: readonly string[];
  acceptedBoundaries?: readonly OwnerBoundary[];
}
/** Canonical domain validator, shared by the model adapter and authenticated
 * owner command. Shape validity alone does not prove execution capability. */
export function canonicalOwnerBoundary(value: unknown): OwnerBoundary {
  return validatePmEntity('OwnerBoundary', value);
}
export const OWNER_BOUNDARY_NORMALIZATION_VERSION = 'qz.owner-boundary.exact-no-leverage/2' as const;
const noLeverageStatements = new Set(['不要杠杆', '不使用杠杆', '不要使用杠杆', '不得使用杠杆', '无杠杆',
  'no leverage', 'do not use leverage', 'without leverage', 'unlevered']);
function exactNoLeverage(statement: string): boolean {
  return noLeverageStatements.has(statement.trim().replace(/[.!。！,，;；]+$/u, '').trim().toLowerCase());
}
/** Discover only complete affirmative clauses from the same finite language
 * contract. A quoted substring inside a larger negation/question is not a
 * discovered constraint. The exact original clause remains the source quote. */
export function explicitOwnerBoundaryProposals(sourceStatements: readonly string[]): OwnerBoundaryProposal[] {
  const found: OwnerBoundaryProposal[] = [];
  for (const source of sourceStatements) for (const match of source.matchAll(/[^.!?。！？;；,，\n]+[.!?。！？;；,，]?/gu)) {
    const sourceStatement = match[0].trim();
    if (exactNoLeverage(sourceStatement) && !found.some(item => item.sourceStatement === sourceStatement)) found.push({ kind: 'LEVERAGE', sourceStatement });
  }
  return found;
}
/** Only a deliberately small, exact language contract is auto-normalized.
 * Other wording remains an owner clarification, not an inferred financial cap.
 * A future-loss aspiration is never converted into a forecast guarantee. */
export function normalizeOwnerBoundaryProposal(proposal: OwnerBoundaryProposal, context: OwnerBoundaryNormalizationContext): OwnerBoundary | null {
  if (!proposal.sourceStatement.trim() || !context.sourceStatements.some(source => source.includes(proposal.sourceStatement))) throw new Error('owner_boundary_source_not_in_original_goal');
  const accepted = context.acceptedBoundaries?.find(boundary => boundary.kind === proposal.kind && boundary.sourceStatement === proposal.sourceStatement);
  if (accepted) {
    if (proposal.normalizedValue !== undefined && pmCanonical(proposal.normalizedValue) !== pmCanonical(accepted.value)) throw new Error('owner_boundary_accepted_value_conflict');
    return canonicalOwnerBoundary(accepted);
  }
  if (proposal.kind !== 'LEVERAGE' || !exactNoLeverage(proposal.sourceStatement)) return null;
  const value = { maximumGrossNavRatio: '1', borrowingAllowed: false };
  if (proposal.normalizedValue !== undefined && pmCanonical(proposal.normalizedValue) !== pmCanonical(value)) throw new Error('owner_boundary_normalization_conflict');
  return canonicalOwnerBoundary({ id: context.id, kind: 'LEVERAGE', value, sourceStatement: proposal.sourceStatement, effectiveFromNs: context.effectiveFromNs });
}
/** Existing effective constraints survive clarification. A leverage refinement
 * can tighten its cap or remove borrowing, never enlarge either permission. */
export function ownerBoundaryRefines(previous: OwnerBoundary, next: OwnerBoundary): boolean {
  canonicalOwnerBoundary(previous); canonicalOwnerBoundary(next);
  if (previous.kind !== next.kind || previous.sourceStatement !== next.sourceStatement) return false;
  if (previous.kind !== 'LEVERAGE') return pmCanonical(previous.value) === pmCanonical(next.value);
  const old = previous.value as { maximumGrossNavRatio: string; borrowingAllowed: boolean };
  const value = next.value as { maximumGrossNavRatio: string; borrowingAllowed: boolean };
  return compareDecimal(value.maximumGrossNavRatio, old.maximumGrossNavRatio) <= 0 && (old.borrowingAllowed || !value.borrowingAllowed);
}
