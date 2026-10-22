import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { v7 as uuidv7 } from 'uuid';
import type { Money } from '../../../packages/contracts/src/pm.ts';
import { compareDecimal, subtractDecimal } from '../../../packages/contracts/src/pm-validation.ts';
import { executeNative } from '../../research/plugins/native-portfolio.ts';
import { compileWasm, iterationRequest, normalizedRequest } from '../../research/plugins/native-wasm.ts';
import { jsonChunks } from '../../research/src/native-json.ts';
import { canonical } from '../../research/src/documents.ts';
import { object } from './facts.ts';
import { pmAllocationMethod } from './pm-method.ts';
import type { CapturedAccount } from './pm-account.ts';
import { compareObservedSpotAccount, valueObservedSpotAccount } from './pm-account-native.ts';
import type { ObservedSpotLeg, ObservedSpotValuationResult } from './pm-account-native.ts';

export interface PmScienceInput {
  source: JsonObject; capital: Money; currency: string; catalog: string;
  dataDir: string; binary: string; maximumTrials: number; cycleId: string; allocationMethod: JsonObject;
}
export interface PmScienceCheckpoint {
  read(key: string): Promise<JsonObject | undefined>;
  write(key: string, value: JsonObject): Promise<void>;
}
export interface PmSciencePoint {
  capital: string; status: 'COMPLETED' | 'FAILED'; simulationRequest: JsonObject | null;
  simulation: JsonObject | null; allocation: JsonObject | null; reason: string | null;
}
export interface PmScienceResult {
  points: PmSciencePoint[]; cash: { simulationRequest: JsonObject; simulation: JsonObject };
  consumedTrials: number; limitations: string[]; evidence: JsonObject;
}
export interface PmIndependentInput extends Omit<PmScienceInput, 'maximumTrials'> {
  selectedCapital: string; heldoutData: JsonObject;
  capture?: CapturedAccount; datasetRevision?: { id: string; contentRef: string };
}
export interface PmIndependentResult {
  point: PmSciencePoint;
  baseline: { kind: 'CASH' | 'HOLD'; status: 'COMPLETED' | 'FAILED'; simulationRequest: JsonObject; simulation: JsonObject | null; reason: string | null };
  consumedTrials: number; evidence: JsonObject; limitations: string[];
}
export interface PmAccountScienceInput extends Omit<PmScienceInput, 'capital'> {
  capture: CapturedAccount; datasetRevision: { id: string; contentRef: string };
}
export interface PmAccountSciencePoint extends PmSciencePoint {
  hold: ObservedSpotLeg & { simulationRequest: JsonObject };
  comparison: JsonObject | null;
}
export interface PmAccountScienceResult {
  capital: Money; points: PmAccountSciencePoint[]; valuation: ObservedSpotValuationResult;
  consumedTrials: number; limitations: string[]; evidence: JsonObject;
}

async function implementationIdentity(binary: string): Promise<string> {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(binary)) hash.update(chunk);
  return `sha256:${hash.digest('hex')}`;
}

/** Compare actual native rows with the digest taken while the original fit
 * read those rows. Paths, matching metadata and a fresh digest taken only at
 * independent evaluation cannot establish that the original history survived.
 */
async function verifyOriginalPrefix(source: JsonObject, catalog: string, binary: string, dataDir: string, context: Context): Promise<JsonObject> {
  const expected = object(source.catalogPrefix, 'original_fit_catalog_prefix_evidence');
  const selection = object(object(object(source.request).forecast).selection, 'original_fit_prefix_selection');
  if (expected.method !== 'NAUTILUS_SELECTED_PREFIX_SHA256_V1' || typeof expected.content_sha256 !== 'string'
    || !/^[0-9a-f]{64}$/.test(expected.content_sha256) || canonical(expected.selection!) !== canonical(selection)) throw new Error('pm_original_catalog_prefix_evidence_invalid');
  const root = join(dataDir, 'scratch'); await mkdir(root, { recursive: true });
  const directory = await mkdtemp(join(root, 'pm-source-prefix-'));
  try {
    const actual = await executeNative(binary, 'catalog-prefix-digest', selection, ['--catalog', catalog], directory, context);
    if (canonical(actual) !== canonical(expected)) throw new Error('pm_original_catalog_prefix_changed');
    return actual;
  } finally { await rm(directory, { recursive: true, force: true }); }
}

// Exact monetary fractions rounded toward zero: a numerical representation may
// reduce a deployment cap, but may never enlarge the owner's capital allowance.
function fraction(numerator: string, denominator: string): string {
  const units = (value: string) => {
    if (!/^\d+(?:\.\d{1,18})?$/.test(value)) throw new Error('pm_capital_decimal_invalid');
    const [whole, decimal = ''] = value.split('.'); return BigInt(whole! + decimal.padEnd(18, '0'));
  };
  const bottom = units(denominator); if (!bottom) throw new Error('pm_capital_basis_zero');
  const scaled = units(numerator) * 10n ** 18n / bottom;
  const whole = (scaled / 10n ** 18n).toString();
  const tail = (scaled % 10n ** 18n).toString().padStart(18, '0').replace(/0+$/, '');
  return tail ? `${whole}.${tail}` : whole;
}
function counter(value: unknown, label: string): bigint {
  if (typeof value !== 'string' || !/^\d+$/.test(value)) throw new Error(`pm_${label}_clock_invalid`);
  return BigInt(value);
}
function wholeSeconds(ns: bigint): number {
  if (ns % 1_000_000_000n !== 0n) throw new Error('pm_subsecond_native_study_ttl_unsupported');
  const seconds = ns / 1_000_000_000n;
  if (seconds < 1n || seconds > 4_294_967_295n) throw new Error('pm_schedule_seconds_unsupported');
  return Number(seconds);
}
function scheduleClocks(anchor: bigint, start: bigint, end: bigint, ttl: bigint): bigint[] {
  if (ttl <= 0n || end <= start) throw new Error('pm_schedule_interval_invalid');
  const index = start > anchor ? (start - anchor + ttl - 1n) / ttl : 0n;
  const clocks: bigint[] = [];
  for (let at = anchor + index * ttl; at < end; at += ttl) {
    if (clocks.length >= 100_000) throw new Error('pm_schedule_resource_limit');
    clocks.push(at);
  }
  return clocks;
}

/** A native allocation and one fresh, causal, shared-account study at EVERY K.
 * All candidates retain the owner's whole initial NAV; K/NAV caps the exposure
 * fraction and profits are reinvested at that fraction. The remainder stays as
 * cash. K is not an absolute lifetime notional cap. No scaled fill path is used.
 * This historical selection experiment does not authorize a current target.
 */
export async function computePmScience(input: PmScienceInput, context: Context, checkpoint?: PmScienceCheckpoint): Promise<PmScienceResult> {
  const result = await runPmScience(input, context, checkpoint);
  if (!('cash' in result)) throw new Error('pm_selection_result_kind_mismatch');
  return result;
}

/** Original observed spot balances are valued without changing source equity.
 * At each capital point two actual native runs start from those same balances:
 * HOLD submits no orders; REBALANCE recomputes against the evolving account.
 * Both runs consume the shared scientific allowance, including failed legs.
 */
export async function computePmAccountScience(input: PmAccountScienceInput, context: Context, checkpoint?: PmScienceCheckpoint): Promise<PmAccountScienceResult> {
  const result = await runPmAccountScience(input, context, checkpoint);
  if (!('valuation' in result)) throw new Error('pm_account_result_kind_mismatch');
  return result;
}

async function runPmAccountScience(input: PmAccountScienceInput, context: Context, checkpoint?: PmScienceCheckpoint,
  independent?: { source: JsonObject; selectedCapital: string; heldoutData: JsonObject }): Promise<PmAccountScienceResult | PmIndependentResult> {
  if (!Number.isSafeInteger(input.maximumTrials) || input.maximumTrials < 2 || input.maximumTrials > 64) throw new Error('pm_account_science_budget_invalid');
  if (input.currency !== input.capture.basis.currency) throw new Error('pm_account_science_currency_mismatch');
  const sourceRequest = object(input.source.request, 'original_factor_request');
  const prefix = object(object(sourceRequest.forecast).selection, 'original_source_selection');
  const asof = counter(input.capture.basis.observedAtNs, 'account_snapshot');
  if (asof < counter(sourceRequest.train_end_ns, 'train_end') || asof >= counter(sourceRequest.validation_end_ns, 'validation_end')
    || asof <= counter(prefix.event_start_ns, 'source_start')) throw new Error('pm_account_state_outside_registered_validation');
  const settings = object(object(input.source.simulationRequest).settings, 'original_execution_settings');
  if (settings.multi_currency_spot_cash == null) throw new Error('pm_account_explicit_spot_execution_policy_required');
  const nativeImplementation = await implementationIdentity(input.binary);
  const fingerprint = createHash('sha256').update(canonical({ source: input.source, capture: input.capture as unknown as JsonObject,
    datasetRevision: input.datasetRevision, method: input.allocationMethod, cycleId: input.cycleId, nativeImplementation,
    independent: independent ?? null })).digest('hex');
  async function retain(key: string, compute: () => Promise<JsonObject>): Promise<JsonObject> {
    const saved = await checkpoint?.read(`account:${key}`);
    if (saved) {
      if (saved.fingerprint !== fingerprint) throw new Error('pm_account_science_checkpoint_basis_mismatch');
      return object(saved.value);
    }
    const value = await compute(); await checkpoint?.write(`account:${key}`, { fingerprint, value }); return value;
  }
  // A UUID here identifies this real bridge artifact. It is explicitly NOT a
  // claim that the PM content-addressed revision was a native source UUID.
  const bridge = await retain('dataset-bridge', async () => ({ id: uuidv7(), kind: 'PM_DATASET_REVISION_BRIDGE',
    pmDatasetRevisionId: input.datasetRevision.id, contentRef: input.datasetRevision.contentRef, sourceSelection: prefix }));
  const common = { capture: input.capture, nativeDatasetRevisionId: String(bridge.id), datasetRevision: input.datasetRevision,
    catalog: input.catalog, binary: input.binary, dataDir: input.dataDir };
  const valued = await retain('valuation', async () => {
    const value = await valueObservedSpotAccount({ ...common, settings,
      selection: { ...prefix, event_end_ns: asof.toString(), decision_cutoff_ns: asof.toString() } }, context);
    return value as unknown as JsonObject;
  });
  const valuation = valued as unknown as ObservedSpotValuationResult;
  if (compareDecimal(valuation.reportValue.amount, '0') <= 0) throw new Error('pm_account_derived_nav_not_positive');
  const result = await runPmScience({ ...input, source: independent?.source ?? input.source, capital: valuation.reportValue }, context, checkpoint, independent,
    { capture: input.capture, datasetRevision: input.datasetRevision, bridge, valuation });
  if (!('valuation' in result) && !('point' in result)) throw new Error('pm_account_result_kind_mismatch');
  return result;
}

/** Called only after the official owner-side task reserves the original sealed
 * exposure. The selected capital, method and factor coefficients are frozen;
 * this path has no capital planner, new fitting or candidate ranking.
 */
export async function evaluatePmIndependent(input: PmIndependentInput, context: Context, checkpoint?: PmScienceCheckpoint): Promise<PmIndependentResult> {
  const selectedData = object(input.source.data, 'original_fit_source_data');
  if (typeof selectedData.catalog !== 'string' || typeof input.heldoutData.catalog !== 'string') throw new Error('pm_original_catalog_binding_missing');
  const request = object(input.source.request, 'selected_factor_request');
  if (canonical(iterationRequest(input.heldoutData, true)) !== canonical(request)) throw new Error('pm_independent_training_contract_mismatch');
  const heldoutRequest = normalizedRequest(input.heldoutData);
  const forecastRequest = object(heldoutRequest.forecast, 'independent_forecast_request');
  const selected = object(forecastRequest.selection, 'independent_selection');
  const validationEnd = counter(request.validation_end_ns, 'validation_end'); const end = counter(selected.event_end_ns, 'heldout_end');
  if (end <= validationEnd || typeof input.heldoutData.catalog !== 'string') throw new Error('pm_independent_partition_unavailable');
  if (!Array.isArray(input.source.members)) throw new Error('pm_independent_original_members_missing');
  const original = object(input.source.simulationRequest); const policy = object(input.source.policy);
  const ttl = counter(policy.targetTtlNs, 'target_ttl'); wholeSeconds(ttl);
  const root = join(input.dataDir, 'scratch'); await mkdir(root, { recursive: true });
  const directory = await mkdtemp(join(root, 'pm-independent-schedule-'));
  try {
    const scheduleKey = 'independent-schedule';
    const scheduleFingerprint = createHash('sha256').update(canonical({ source: input.source, heldoutData: input.heldoutData,
      method: input.allocationMethod, capital: { ...input.capital }, selectedCapital: input.selectedCapital })).digest('hex');
    const previous = await checkpoint?.read(scheduleKey); let schedule: JsonObject;
    if (previous) {
      if (previous.fingerprint !== scheduleFingerprint) throw new Error('pm_independent_checkpoint_basis_mismatch');
      schedule = object(previous.value);
    } else {
      if (!Array.isArray(original.target_points) || !original.target_points.length) throw new Error('pm_selection_schedule_anchor_missing');
      const anchor = counter(object(original.target_points[0]).asof_ns, 'selection_anchor');
      const cutoffClocks = scheduleClocks(anchor, validationEnd, end, ttl);
      const targetPoints = cutoffClocks.map(at => ({ schema_version: 1, asof_ns: at.toString(), valid_until_ns: (at + ttl).toString(), targets: [], cash_weight: '1' }));
      schedule = { anchorNs: anchor.toString(), ttlNs: ttl.toString(), cutoffNs: cutoffClocks.map(String), targetPoints,
        rule: input.allocationMethod.clockRule! };
      await checkpoint?.write(scheduleKey, { fingerprint: scheduleFingerprint, value: schedule });
    }
    const source: JsonObject = { ...input.source, simulationRequest: { ...original,
      selection: { ...selected, event_start_ns: validationEnd.toString() }, target_points: schedule.targetPoints! } };
    if (!!input.capture !== !!input.datasetRevision) throw new Error('pm_independent_original_account_binding_missing');
    const independent = { source, selectedCapital: input.selectedCapital, heldoutData: input.heldoutData };
    const result = input.capture && input.datasetRevision
      ? await runPmAccountScience({ ...input, catalog: input.heldoutData.catalog, maximumTrials: 2, capture: input.capture, datasetRevision: input.datasetRevision }, context, checkpoint, independent)
      : await runPmScience({ ...input, source, catalog: input.heldoutData.catalog, maximumTrials: 2 }, context, checkpoint, independent);
    if (!('point' in result)) throw new Error('pm_independent_result_kind_mismatch');
    result.evidence.schedule = schedule;
    return result;
  } finally { await rm(directory, { recursive: true, force: true }); }
}

async function runPmScience(input: PmScienceInput, context: Context, checkpoint?: PmScienceCheckpoint,
  independent?: { selectedCapital: string; heldoutData: JsonObject },
  account?: { capture: CapturedAccount; datasetRevision: { id: string; contentRef: string }; bridge: JsonObject; valuation: ObservedSpotValuationResult }): Promise<PmScienceResult | PmIndependentResult | PmAccountScienceResult> {
  if (!Number.isSafeInteger(input.maximumTrials) || input.maximumTrials < (independent ? 1 : 2) || input.maximumTrials > 65) throw new Error('pm_science_budget_invalid');
  if (input.currency !== input.capital.currency || compareDecimal(input.capital.amount, '0') <= 0) throw new Error('pm_science_capital_invalid');
  if (canonical(input.allocationMethod) !== canonical(pmAllocationMethod())) throw new Error('pm_unregistered_allocation_method');
  const source = input.source; const fit = object(source.fit, 'frozen_fit');
  const verifiedPrefix = await verifyOriginalPrefix(source, input.catalog, input.binary, input.dataDir, context);
  const fitRequest = object(source.fitRequest, 'original_fit_request');
  const original = object(source.simulationRequest, 'original_simulation_request');
  const request = object(source.request, 'original_factor_request');
  const forecast = object(request.forecast, 'original_forecast_request');
  const fullSelection = independent ? object(object(normalizedRequest(independent.heldoutData).forecast).selection) : object(forecast.selection, 'causal_source_selection');
  const selection = object(original.selection, 'selection_partition');
  const settings: JsonObject = { ...object(original.settings, 'native_execution_settings'), starting_capital: input.capital.amount };
  if (settings.base_currency !== input.currency) throw new Error('pm_science_currency_mismatch');
  if (settings.multi_currency_spot_cash != null && !account) throw new Error('pm_joint_spot_cash_study_not_supported');
  if (account && settings.multi_currency_spot_cash == null) throw new Error('pm_account_explicit_spot_execution_policy_required');
  if (!Array.isArray(source.members) || !Array.isArray(fit.members) || !Array.isArray(fitRequest.members) || source.members.length < 2) throw new Error('pm_original_fitted_members_missing');
  if (fitRequest.fit_end_ns !== fit.fit_end_ns || fit.fit_end_ns !== request.train_end_ns) throw new Error('pm_frozen_training_cutoff_mismatch');
  const sourceMembers = source.members.map(value => object(value, 'factor_model'));
  const fittedMembers = fit.members.map(value => object(value, 'fitted_factor'));
  const reports = fitRequest.members.map(value => object(value, 'original_report_member'));
  if (fittedMembers.length !== sourceMembers.length || reports.length !== sourceMembers.length) throw new Error('pm_frozen_member_alignment');
  const points = Array.isArray(original.target_points) ? original.target_points.map(value => object(value, 'original_target_point')) : [];
  if (points.length < 2) throw new Error('pm_joint_schedule_insufficient');
  const originalClocks = points.map(point => counter(point.asof_ns, 'decision'));
  if (originalClocks.some((clock, index) => index > 0 && clock <= originalClocks[index - 1]!)) throw new Error('pm_joint_schedule_order');
  const end = counter(selection.event_end_ns, 'evaluation_end');
  if (!independent && (counter(selection.event_start_ns, 'evaluation_start') < counter(request.train_end_ns, 'train_end')
    || end > counter(request.validation_end_ns, 'validation_end')
    || counter(selection.decision_cutoff_ns, 'evaluation_cutoff') > counter(request.validation_end_ns, 'validation_end'))) throw new Error('pm_selection_outside_registered_validation');
  if (independent && (counter(selection.event_start_ns, 'independent_start') < counter(request.validation_end_ns, 'validation_end')
    || end > counter(fullSelection.event_end_ns, 'independent_end'))) throw new Error('pm_independent_scope_mismatch');
  if (originalClocks[0]! <= counter(fit.fit_end_ns, 'fit_end') || originalClocks.at(-1)! >= end) throw new Error('pm_joint_schedule_training_overlap');
  const originalPolicy = object(source.policy, 'registered_policy');
  const targetTtl = counter(originalPolicy.targetTtlNs, 'target_ttl');
  if (points.some((point, index) => counter(point.valid_until_ns, 'target_expiry') - originalClocks[index]! !== targetTtl)) throw new Error('pm_original_target_ttl_policy_mismatch');
  const start = account && counter(account.capture.basis.availableAtNs, 'account_availability') > originalClocks[0]!
    ? counter(account.capture.basis.availableAtNs, 'account_availability') : originalClocks[0]!;
  const clocks = scheduleClocks(originalClocks[0]!, start, end, targetTtl);
  if (clocks.length < 2) throw new Error('pm_joint_schedule_insufficient');
  // This maximum is a causal input-availability limit derived from the original
  // target protocol, not a claimed exchange/session calendar or liquidity limit.
  const scheduleSeconds = wholeSeconds(targetTtl);
  const scratchRoot = join(input.dataDir, 'scratch'); await mkdir(scratchRoot, { recursive: true });
  const directory = await mkdtemp(join(scratchRoot, 'pm-science-'));
  let objects: JsonObject = {}; let bindings: JsonObject[] = [];
  const nativeImplementation = await implementationIdentity(input.binary);
  const fingerprint = createHash('sha256').update(canonical({ source: input.source, capital: { ...input.capital },
    method: input.allocationMethod, maximumTrials: input.maximumTrials, cycleId: input.cycleId, catalog: input.catalog,
    nativeImplementation, verifiedPrefix, account: account ? { sourceEvidenceRef: account.capture.evidence.id, bridge: account.bridge, valuation: account.valuation as unknown as JsonObject } : null })).digest('hex');
  async function retained(key: string, compute: () => Promise<JsonObject>): Promise<JsonObject> {
    const saved = await checkpoint?.read(key);
    if (saved) {
      if (saved.fingerprint !== fingerprint) throw new Error('pm_science_checkpoint_basis_mismatch');
      return object(saved.value, 'retained_native_result');
    }
    const value = await compute(); await checkpoint?.write(key, { fingerprint, value }); return value;
  }
  const put = async (payload: JsonObject, bytes?: Uint8Array): Promise<string> => {
    const id = uuidv7(); objects[id] = payload;
    await writeFile(join(directory, id), bytes ?? jsonChunks(payload)); return id;
  };
  try {
    const prepared = await retained('preparation', async () => {
    const frozenMembers: JsonObject[] = [];
    for (const [index, member] of sourceMembers.entries()) {
      const fitted = fittedMembers[index]!; const report = reports[index]!;
      if (typeof member.model !== 'string' || member.factorVersionId !== fitted.factor_version_id || member.factorVersionId !== report.factor_version_id
        || fitted.source_evidence_ref !== report.source_evidence_ref || canonical(object(report.report).request!) !== canonical(request)) throw new Error('pm_factor_report_model_binding_mismatch');
      const reportId = await put(object(report.report));
      frozenMembers.push({ factor_version_id: fitted.factor_version_id!, report_artifact_id: reportId, report: report.report! });
    }
    const freezeRequest = { schema_version: 1, fit, members: frozenMembers };
    const frozen = await executeNative(input.binary, 'freeze-ensemble-calibrations', freezeRequest, [], directory, context);
    if (!Array.isArray(frozen.members) || frozen.members.length !== sourceMembers.length) throw new Error('pm_frozen_calibration_result_mismatch');
    const nativeMembers: JsonObject[] = [];
    for (const [index, member] of sourceMembers.entries()) {
      const frozenMember = object(frozen.members[index]); const fitted = fittedMembers[index]!;
      if (frozenMember.factor_version_id !== member.factorVersionId) throw new Error('pm_frozen_calibration_identity_mismatch');
      const modelBytes = compileWasm(String(member.model), 'pm-factor.wat', 'study-portfolio');
      const modelId = await put({ mediaType: 'application/wasm', encoding: 'base64', data: Buffer.from(modelBytes).toString('base64'), sourceTrialId: member.sourceTrialId! }, modelBytes);
      const calibrationId = await put(object(frozenMember.calibration, 'frozen_calibration'));
      const alphaId = await put({ factorVersionId: member.factorVersionId!, sourceTrialId: member.sourceTrialId! });
      const versionId = await put({ factorVersionId: member.factorVersionId!, modelArtifactId: modelId, calibrationArtifactId: calibrationId });
      nativeMembers.push({ alpha_id: alphaId, alpha_version_id: versionId, model_artifact_id: modelId, calibration_artifact_id: calibrationId,
        target_kind: 'SCORE', ensemble_weight: fitted.weight!, parameters: object(forecast.parameters) });
      bindings.push({ factorVersionId: member.factorVersionId!, sourceTrialId: member.sourceTrialId!, nativeAlphaId: alphaId, nativeAlphaVersionId: versionId,
        reportArtifactId: frozenMembers[index]!.report_artifact_id!, modelArtifactId: modelId, calibrationArtifactId: calibrationId });
    }
    const costId = await put(settings);
    const universeId = await put({ barTypes: fullSelection.bar_types!, sourceRequest: request });
    const policyId = await put({ kind: independent ? 'INDEPENDENT_FROZEN_CONFIGURATION' : 'HISTORICAL_SELECTION_ONLY', cycleId: input.cycleId, originalPolicy,
      allocationMethod: input.allocationMethod, fitCutoffNs: fit.fit_end_ns!, schedule: { anchorNs: originalClocks[0]!.toString(), ttlNs: targetTtl.toString(), cutoffNs: clocks.map(String) } });
    const assumptionsId = await put({ simulationSettingsId: costId, sourceSelection: selection });
    return { objects, bindings, nativeMembers, frozenMembers, freezeRequest, frozen, costId, universeId, policyId, assumptionsId };
    });
    objects = object(prepared.objects); bindings = (prepared.bindings as JsonObject[]);
    const nativeMembers = prepared.nativeMembers as JsonObject[];
    const { freezeRequest, frozen, costId, universeId, policyId, assumptionsId } = prepared;
    // The official task owns these payloads. A recovered process materializes
    // exactly the same UUID bindings, rather than inventing new report refs.
    for (const [id, value] of Object.entries(objects)) {
      const payload = object(value);
      await writeFile(join(directory, id), payload.mediaType === 'application/wasm' && payload.encoding === 'base64'
        ? Buffer.from(String(payload.data), 'base64') : jsonChunks(payload));
    }
    if (!Array.isArray(settings.fee_rates) || !settings.fee_rates.length) throw new Error('pm_original_cost_schedule_missing');
    const assets = settings.fee_rates.map(value => { const fee = object(value); return { instrument_id: fee.instrument_id!, currency: input.currency,
      current_weight: '0', transaction_cost_rate: fee.taker!, available_notional: null, groups: [] }; });
    const maximumPoints = account ? Math.floor(input.maximumTrials / 2) : input.maximumTrials - 1;
    const planned = independent ? { generation: { planned_capitals: [independent.selectedCapital] }, termination: 'FROZEN_SELECTION_NO_PLANNING' }
      : await retained('capital-plan', () => executeNative(input.binary, 'capital-curve', { schema_version: 1,
      simulation: { ...original, settings }, capital_basis: { currency: input.currency, equity: input.capital.amount, reserved: '0', owner_liquidity_reserve: '0' },
      // Planning only: this admission clock does not install an execution deadline.
      budget: { maximum_simulations: maximumPoints, maximum_elapsed_ms: '1' } }, ['--catalog', input.catalog, '--plan-only',
        ...(account ? ['--dataset-revision-id', String(account.bridge.id)] : [])], directory, context));
    if ((!independent && planned.termination !== 'PLANNED_ONLY') || !Array.isArray(object(planned.generation).planned_capitals)) throw new Error('pm_capital_planner_result_invalid');
    const capitalPoints = object(planned.generation).planned_capitals as string[];
    if (!capitalPoints.length || capitalPoints.length > (independent ? 1 : maximumPoints)) throw new Error('pm_capital_planner_budget_invalid');
    const resultPoints: PmSciencePoint[] = []; const studies: JsonObject[] = [];
    const accountPoints: PmAccountSciencePoint[] = [];
    let consumedTrials = 0;
    for (const amount of capitalPoints) {
      if (typeof amount !== 'string' || compareDecimal(amount, '0') <= 0 || compareDecimal(amount, input.capital.amount) > 0) throw new Error('pm_planned_capital_invalid');
      const exposure = fraction(amount, input.capital.amount);
      const studyRequest: JsonObject = { schema_version: 1, settlements: original.settlements ?? [],
        source_selection: { ...fullSelection, event_end_ns: end.toString(), decision_cutoff_ns: end.toString() },
        evaluation_start_ns: clocks[0]!.toString(), manual_cutoffs_ns: clocks.map(String), calendar: null,
        research_available_through_ns: fit.fit_end_ns!, execution_settings: settings, rolling_liquidity: null, assets, members: nativeMembers,
        mandate: { objective: input.allocationMethod.objective!, risk_measure: input.allocationMethod.riskMeasure!, base_currency: input.currency, capital_assumption: input.capital.amount,
          universe_version_id: universeId,
          covariance_estimator: { schema_version: 1, adapter_kind: 'SAMPLE_COVARIANCE', upstream_class: 'ndarray_stats::CorrelationExt::cov', upstream_version: '0.7.0', parameters: { ddof: 1 } },
          alpha_ensemble: { schema_version: 1, adapter_kind: 'FIXED_WEIGHTED_FORECAST', upstream_class: 'ndarray::ArrayBase::dot', upstream_version: '0.17.1', parameters: {} },
          optimizer: input.allocationMethod.optimizer!,
          constraints: { schema_version: 1, long_only: true, min_cash_weight: subtractDecimal('1', exposure), max_cash_weight: '1', min_asset_weight: '0', max_asset_weight: exposure,
            max_gross_exposure: exposure, min_net_exposure: '0', max_net_exposure: exposure, max_turnover_per_rebalance: '2',
            max_participation: null, max_ex_ante_risk: null, group_bounds: [], asset_overrides: [], transaction_costs_ref: costId, liquidity_ref: null },
          rebalance_schedule: { schema_version: 1, kind: 'MANUAL', interval_seconds: null, calendar_ref: null, timezone: 'UTC', session_offset_seconds: null,
            max_input_age_seconds: scheduleSeconds, target_ttl_seconds: scheduleSeconds },
          required_evaluation_policy_id: policyId, execution_assumptions_id: assumptionsId, exposure_tolerance: settings.exposure_tolerance! } };
      consumedTrials += account ? 2 : 1;
      const receipt = await retained(`capital:${amount}`, async (): Promise<JsonObject> => {
      try {
        if (account) {
          const paired = await compareObservedSpotAccount({ capture: account.capture, datasetRevision: account.datasetRevision,
            nativeDatasetRevisionId: String(account.bridge.id), catalog: input.catalog, binary: input.binary, dataDir: input.dataDir,
            operation: { kind: 'STUDY', study: studyRequest, objectsDirectory: directory } }, context);
          return { capital: amount, request: studyRequest, paired: paired as unknown as JsonObject };
        }
        const study = await executeNative(input.binary, 'study-portfolio', studyRequest, ['--catalog', input.catalog, '--objects', directory], directory, context);
        return { capital: amount, request: studyRequest, result: study };
      } catch (error) {
        if (context.abortSignal?.aborted) throw error;
        const reason = error instanceof Error ? error.message : String(error);
        return { capital: amount, request: studyRequest, error: reason };
      }
      });
      studies.push(receipt);
      const paired = account && receipt.paired != null ? object(receipt.paired) : null;
      const rebalance = paired ? object(paired.rebalance) : null;
      const study = rebalance ? rebalance.result == null ? null : object(rebalance.result) : receipt.result == null ? null : object(receipt.result);
      const simulationRequest = study?.simulation_request == null ? null : object(study.simulation_request);
      const simulation = study?.simulation == null ? null : object(study.simulation);
      const completed = (!account || rebalance?.status === 'COMPLETED') && !!simulationRequest && !!simulation;
      const point: PmSciencePoint = { capital: amount, status: completed ? 'COMPLETED' : 'FAILED', simulationRequest, simulation,
        allocation: study, reason: completed ? null : String(rebalance?.reason ?? receipt.error ?? 'native_study_no_feasible_target_path') };
      resultPoints.push(point);
      if (account) {
        const hold = paired ? object(paired.hold) : null;
        accountPoints.push({ ...point, comparison: paired, hold: {
          status: hold?.status === 'COMPLETED' ? 'COMPLETED' : 'FAILED', result: hold?.result == null ? null : object(hold.result),
          reason: hold?.status === 'COMPLETED' ? null : String(hold?.reason ?? receipt.error ?? 'native_hold_replay_unavailable'),
          simulationRequest: { schema_version: 1, selection: { ...object(studyRequest.source_selection), event_start_ns: clocks[0]!.toString() },
            settings, target_points: [], settlements: studyRequest.settlements! },
        } });
      }
    }
    if (account && independent) {
      const point = accountPoints[0]!;
      return { point, baseline: { kind: 'HOLD', status: point.hold.status, simulationRequest: point.hold.simulationRequest,
        simulation: point.hold.result, reason: point.hold.reason }, consumedTrials,
        limitations: ['FIXED_OBSERVED_BALANCES_AS_COUNTERFACTUAL_INDEPENDENT_START_NOT_ACTUAL_FUTURE_INVENTORY',
          'FROZEN_ORIGINAL_NAV_EXPOSURE_FRACTION_NO_NEW_CAPITAL_SEARCH', 'INDEPENDENT_REPLAY_NOT_LIVE_EXECUTION_QUALIFICATION'],
        evidence: { schemaVersion: 1, cycleId: input.cycleId, role: 'INDEPENDENT', nativeImplementation, catalogPrefix: verifiedPrefix,
          accountEvidenceRef: account.capture.evidence.id, datasetBridge: account.bridge, valuation: account.valuation as unknown as JsonObject,
          bindings, objects, freezeRequest: freezeRequest!, frozen: frozen!, studies, method: input.allocationMethod,
          selectedCapital: independent.selectedCapital, consumedTrials } };
    }
    if (account) return { capital: input.capital, points: accountPoints, valuation: account.valuation, consumedTrials,
      limitations: ['OBSERVED_BALANCES_HELD_CONSTANT_WITHOUT_ORDERS_IN_HOLD_BASELINE',
        'HISTORICAL_REPLAY_NOT_REALIZED_ACCOUNT_PNL_OR_FORECAST', 'PIT_DERIVED_INVENTORY_NAV_DOES_NOT_REPLACE_REPORTED_EQUITY',
        'EACH_HOLD_AND_REBALANCE_RUN_CONSUMES_ONE_SCIENTIFIC_ATTEMPT',
        'CAPITAL_POINT_IS_INITIAL_NAV_EXPOSURE_FRACTION_WITH_PROFIT_REINVESTMENT', 'NO_MARKET_IMPACT_OR_EXIT_CAPACITY_BOUND',
        'LOCKED_BALANCES_DEBT_OPEN_ORDERS_AND_MARGIN_ACCOUNTS_UNSUPPORTED'],
      evidence: { schemaVersion: 1, cycleId: input.cycleId, role: 'SELECTION', nativeImplementation, catalogPrefix: verifiedPrefix,
        accountEvidenceRef: account.capture.evidence.id, datasetBridge: account.bridge, valuation: account.valuation as unknown as JsonObject,
        bindings, objects, freezeRequest: freezeRequest!, frozen: frozen!, planned, studies, method: input.allocationMethod, consumedTrials } };
    // The baseline uses exactly the same selection, initial NAV, execution
    // settings and decision clocks. A receipt is never treated as a return.
    const cashRequest: JsonObject = { ...original, selection: { ...selection, event_start_ns: clocks[0]!.toString() }, settings,
      target_points: clocks.map(clock => ({ schema_version: 1, asof_ns: clock.toString(), valid_until_ns: (clock + targetTtl).toString(), cash_weight: '1',
        targets: assets.map(asset => ({ instrument_id: asset.instrument_id, currency: asset.currency, weight: '0' })) })) };
    consumedTrials++;
    const cashReceipt = await retained('cash', async (): Promise<JsonObject> => {
      try { return { simulation: await executeNative(input.binary, 'simulate', cashRequest, ['--catalog', input.catalog], directory, context) }; }
      catch (error) { if (context.abortSignal?.aborted) throw error; return { error: error instanceof Error ? error.message : String(error) }; }
    });
    const cashSimulation = cashReceipt.simulation == null ? null : object(cashReceipt.simulation);
    if (independent) return { point: resultPoints[0]!, baseline: { kind: 'CASH', status: cashSimulation ? 'COMPLETED' : 'FAILED', simulationRequest: cashRequest,
      simulation: cashSimulation, reason: cashSimulation ? null : String(cashReceipt.error ?? 'native_cash_baseline_failed') }, consumedTrials,
      limitations: ['INDEPENDENT_EVALUATION_NOT_PROOF_OF_FUTURE_PROFIT', 'NO_LIVE_CAPACITY_OR_TARGET_AUTHORIZATION'],
      evidence: { schemaVersion: 1, cycleId: input.cycleId, role: 'INDEPENDENT', nativeImplementation, catalogPrefix: verifiedPrefix, bindings, objects, freezeRequest: freezeRequest!, frozen: frozen!,
        studies, method: input.allocationMethod, selectedCapital: independent.selectedCapital, cashRequest, cashReceipt, consumedTrials } };
    if (!cashSimulation) throw new Error(String(cashReceipt.error ?? 'native_cash_baseline_failed'));
    return { points: resultPoints, cash: { simulationRequest: cashRequest, simulation: cashSimulation }, consumedTrials,
      limitations: ['HISTORICAL_SELECTION_NOT_INDEPENDENT_EVALUATION', 'FRESH_CASH_ACCOUNT_NOT_AN_EXISTING_ACCOUNT_HOLD_COMPARISON',
        'CAPITAL_POINT_IS_INITIAL_NAV_ALLOCATION_WITH_PROFIT_REINVESTMENT_NOT_ABSOLUTE_NOTIONAL_CAP',
        'OWNER_NAV_RETAINED_AT_EVERY_DEPLOYMENT_CAP', 'REGISTERED_MEAN_VARIANCE_METHOD_RISK_AVERSION_ONE_NOT_INFERRED_OWNER_PREFERENCE',
        'NO_PARTICIPATION_OR_MARKET_IMPACT_CAPACITY_BOUND', 'NO_FEASIBILITY_INTERPOLATION_BETWEEN_TESTED_POINTS', 'RUNTIME_FUEL_AND_FAILED_EVENT_CONSUMPTION_NOT_FULLY_METERED'],
      evidence: { schemaVersion: 1, cycleId: input.cycleId, nativeImplementation, catalogPrefix: verifiedPrefix, bindings, objects, freezeRequest, frozen, planned, studies,
        method: input.allocationMethod, cashRequest, cashSimulation, consumedTrials } };
  } finally {
    try {
      // A source changed between child runs invalidates the whole comparison,
      // including a resumed computation whose earlier receipts were retained.
      if (!context.abortSignal?.aborted) await verifyOriginalPrefix(source, input.catalog, input.binary, input.dataDir, context);
    } finally { await rm(directory, { recursive: true, force: true }); }
  }
}
