import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { v7 as uuidv7 } from 'uuid';
import { executeNative } from '../../research/plugins/native-portfolio.ts';
import { jsonChunks } from '../../research/src/native-json.ts';
import { canonical } from '../../research/src/documents.ts';
import { json, object, Problem } from './facts.ts';
import { toNativeObservedSpotStart } from './pm-account-native.ts';
import type { CapturedAccount } from './pm-account.ts';
import { implementationIdentity, verifyOriginalPrefix } from './pm-science.ts';
import type { PmScienceCheckpoint } from './pm-science.ts';
import { pmArtifactDigest } from './pm-evidence.ts';

export interface PmForwardNativeInput {
  seed: JsonObject;
  capture: CapturedAccount;
  datasetRevision: { id: string; contentRef: string };
  selection: JsonObject;
  catalog: string;
  binary: string;
  dataDir: string;
  basisRevision: string;
}

/** Exactly one inference/solve of the original selected method. No trial,
 * fit, parameter sweep, historical return projection or synthetic account. */
export async function buildPmForward(input: PmForwardNativeInput, context: Context, checkpoint: PmScienceCheckpoint): Promise<JsonObject> {
  const source = object(input.seed.source, 'frozen_forward_source');
  const study = object(input.seed.study, 'frozen_forward_study');
  const funding = object(object(input.seed.allocationMethod, 'frozen_forward_allocation_method').cashFunding, 'frozen_forward_cash_funding');
  if (funding.applicableDomain !== 'OBSERVED_SPOT_CASH_LONG_ONLY_MAX_UTILITY_VARIANCE'
    || funding.nativeConstraint !== 'PRESERVE_MIN_CASH_AFTER_LINEAR_TURNOVER_V1'
    || object(object(study.mandate).constraints).cash_funding !== funding.nativeConstraint) throw new Problem('forward_original_cash_funding_method_missing_or_changed');
  const originalObjects = object(input.seed.objects, 'frozen_forward_objects');
  const cutoff = String(input.selection.decision_cutoff_ns);
  if (!/^\d+$/.test(cutoff)) throw new Problem('forward_cutoff_invalid');
  const initialState = toNativeObservedSpotStart(input.capture, { selectionStartNs: cutoff });
  const nativeImplementation = await implementationIdentity(input.binary);
  if (input.seed.nativeImplementation !== nativeImplementation) throw new Problem('forward_native_implementation_requires_method_requalification');
  const prefix = await verifyOriginalPrefix(source, input.catalog, input.binary, input.dataDir, context);
  const fingerprint = pmArtifactDigest({ seed: input.seed, capture: input.capture as unknown as JsonObject,
    datasetRevision: input.datasetRevision, selection: input.selection, basisRevision: input.basisRevision, nativeImplementation, prefix });
  const bridgeSaved = await checkpoint.read('forward-bridge');
  if (bridgeSaved && bridgeSaved.fingerprint !== fingerprint) throw new Problem('forward_checkpoint_basis_mismatch');
  const bridge = bridgeSaved ? object(bridgeSaved.value) : { schema_version: 1, id: uuidv7(), kind: 'PM_CURRENT_DATASET_REVISION_BRIDGE',
    datasetRevision: input.datasetRevision, basisRevision: input.basisRevision, nativeImplementation, selection: input.selection };
  if (!bridgeSaved) await checkpoint.write('forward-bridge', { fingerprint, value: bridge });
  const request: JsonObject = { schema_version: 1, dataset_revision_id: bridge.id!, initial_state: initialState,
    selection: input.selection, mandate: object(study.mandate), execution_settings: object(study.execution_settings),
    rolling_liquidity: study.rolling_liquidity ?? null,
    assets: (study.assets as JsonObject[]).map(asset => ({ ...asset, current_weight: '0' })), members: study.members! };
  const root = join(input.dataDir, 'scratch'); await mkdir(root, { recursive: true });
  const directory = await mkdtemp(join(root, 'pm-current-'));
  try {
    const saved = await checkpoint.read('forward-build');
    if (saved && saved.fingerprint !== fingerprint) throw new Problem('forward_checkpoint_basis_mismatch');
    let receipt: JsonObject;
    if (saved) receipt = object(saved.value);
    else {
      for (const [id, value] of Object.entries(originalObjects)) {
        if (!/^[0-9a-f-]{36}$/i.test(id)) throw new Problem('forward_native_object_identity_invalid');
        const payload = object(value);
        await writeFile(join(directory, id), payload.mediaType === 'application/wasm' && payload.encoding === 'base64'
          ? Buffer.from(String(payload.data), 'base64') : jsonChunks(payload));
      }
      const started = process.hrtime.bigint();
      const result = await executeNative(input.binary, 'build-observed-spot', request,
        ['--catalog', input.catalog, '--objects', directory], directory, context);
      receipt = { request, result, resources: { operation: 'build-observed-spot', recordedCompletedInvocations: 1,
        elapsedNs: (process.hrtime.bigint() - started).toString(), classification: 'FROZEN_METHOD_OPERATIONAL_COMPUTE',
        measurementScope: 'RECORDED_COMPLETED_INVOCATION', uncheckpointedPriorPhysicalAttempts: 'UNKNOWN',
        logicalSolves: 1, searchTrials: 0, nativeImplementation } };
      await checkpoint.write('forward-build', { fingerprint, value: receipt });
    }
    const result = object(receipt.result, 'forward_native_result');
    if (result.schema_version !== 1 || result.kind !== 'OBSERVED_SPOT_FORWARD_BUILD'
      || result.basis !== 'FORWARD_MODEL_ESTIMATE_FROM_OBSERVED_SPOT_BALANCES'
      || canonical(result.request!) !== canonical(request)) throw new Problem('forward_native_result_basis_mismatch');
    const valuation = object(result.valuation, 'forward_valuation');
    if (valuation.asof_ns !== cutoff || valuation.report_currency !== input.capture.basis.currency
      || BigInt(String(valuation.available_ns)) > BigInt(cutoff)) throw new Problem('forward_valuation_clock_or_currency_mismatch');
    await verifyOriginalPrefix(source, input.catalog, input.binary, input.dataDir, context);
    return { ...receipt, bridge, catalogPrefix: prefix, fingerprint, sourceEvidenceRef: input.capture.evidence.id,
      originalSeedEvidenceRef: input.seed.selectionEvidenceRef!, executionKind: 'FORWARD_MODEL_ESTIMATE',
      limitations: ['CURRENT_WEIGHTS_NOT_INDEPENDENTLY_REPLAYED', 'MODELED_RETURN_IS_NOT_OBSERVED_DAILY_RETURN',
        'FIXED_REGISTERED_OPTIMIZER_IS_NOT_OWNER_UTILITY', 'NO_REAL_ORDERS'] };
  } finally { await rm(directory, { recursive: true, force: true }); }
}
