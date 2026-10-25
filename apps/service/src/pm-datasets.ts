import type * as Pm from '../../../packages/contracts/src/pm.ts';
import type { DatasetConfig } from './config.ts';
import { scienceDataWithoutBinding } from './config.ts';
import type { PmState } from './pm-facts.ts';
import { digest, fact, identity } from './pm-facts.ts';
import { counter, json, nowNs, object, Problem } from './facts.ts';
import type { SourceRevision } from './pm-source.ts';
import { relative, resolve, sep } from 'node:path';

/** Native decision_cutoff is the upper bound on usable availability, not the
 * event interval end. An empty future event interval is not itself leakage. */
export function assertPmDataAvailable(data: import('@earendil-works/pi-durable').JsonObject, availableCutoffNs: string) {
  const raw = object(data.catalog_ensemble_request ?? data.catalog_scan_request ?? data.request, 'native_dataset_request');
  const selection = raw.forecast ? object(object(raw.forecast).selection, 'native_selection') : raw;
  const nativeCutoff = counter(selection.decision_cutoff_ns, 'native_availability_cutoff');
  if (BigInt(nativeCutoff) > BigInt(availableCutoffNs)) throw new Problem('dataset_native_availability_after_decision_cutoff', 409);
  return nativeCutoff;
}

/** Registration freezes the server-owned source; HTTP cannot supply scientific rows. */
export function captureDataset(configured: DatasetConfig, state: PmState, atNs = nowNs(), source?: SourceRevision | null) {
  if (!configured.ready) throw new Problem('dataset_not_ready', 409);
  if (source && BigInt(source.availableAtNs) > BigInt(atNs)) throw new Problem('source_revision_not_available_asof', 409);
  // Worker configuration is frozen separately for each mandate. It must not
  // change a source revision's identity or pin later mandates to an old host.
  const data = scienceDataWithoutBinding(json(configured.data));
  const nativeCutoff = assertPmDataAvailable(data, atNs);
  const raw = object(data.catalog_ensemble_request ?? data.catalog_scan_request ?? data.request, 'native_dataset_request');
  const selection = raw.forecast ? object(object(raw.forecast).selection, 'native_selection') : raw;
  const startNs = counter(selection.event_start_ns, 'event_start');
  const endExclusiveNs = counter(selection.event_end_ns, 'event_end');
  if (BigInt(startNs) >= BigInt(endExclusiveNs)) throw new Problem('dataset_interval_invalid');
  const sourceRevision = source?.sourceRevision ?? digest({ data, strategy: configured.strategy ?? null, policy: configured.policy ?? null });
  const id = identity('dataset', [configured.id, sourceRevision]);
  if (state.datasets[id] && source && state.datasets[id]!.revision.contentRef !== source.contentDigest) throw new Problem('immutable_source_revision_conflict', 409);
  if (state.datasets[id]) return state.datasets[id]!;
  if (source) {
    const paths = (value: import('@earendil-works/pi-durable').JsonObject) => [value, value.sealed].flatMap(item => item && typeof item === 'object' && !Array.isArray(item)
      ? [item.catalog, item.catalog_partitions].filter((path): path is string => typeof path === 'string') : []);
    const currentPaths = paths(data);
    const overlaps = (left: string, right: string) => {
      const a = resolve(left); const b = resolve(right);
      const within = (parent: string, child: string) => { const path = relative(parent, child); return path === '' || path !== '..' && !path.startsWith(`..${sep}`) && !path.startsWith(sep); };
      return within(a, b) || within(b, a);
    };
    for (const previous of Object.values(state.datasets)) if (previous.revision.datasetId === configured.id && previous.revision.contentRef !== source.contentDigest
      && paths(previous.data).some(path => currentPaths.some(current => overlaps(path, current)))) throw new Problem('source_catalog_must_be_immutable', 409);
  }
  const prior = Object.values(state.datasets).filter(item => item.revision.datasetId === configured.id).at(-1);
  const revision: Pm.DatasetRevision = { ...fact(id, atNs), datasetId: configured.id, sourceId: configured.source,
    sourceRevision, parentRevisionId: prior?.revision.id ?? null, contentRef: source?.contentDigest ?? `configured-dataset:${sourceRevision}`,
    eventInterval: { startNs, endExclusiveNs }, availableThroughNs: nativeCutoff, admittedAtNs: atNs,
    sourceClockPrecision: 'UNKNOWN', universeSnapshotRef: identity('universe', selection.bar_types ?? raw.source ?? configured.source),
    instrumentRulesRef: identity('instrument-rules', [configured.source, sourceRevision]), schemaId: data.catalog_ensemble_request ? 'qz.catalog-ensemble/1' : data.catalog_scan_request ? 'qz.catalog-factor-scan/1' : 'qz.factor-evaluation/1',
    qualityEvidenceRefs: [], qualityStatus: 'INSUFFICIENT', missingCapabilities: ['receive_clock_point_in_time_not_verified', ...source ? [] : ['catalog_content_immutability_not_verified']],
    revisionKind: source?.revisionKind ?? (prior ? BigInt(startNs) >= BigInt(prior.revision.eventInterval.endExclusiveNs) ? 'APPEND' : 'CORRECTION' : 'APPEND') };
  state.datasets[id] = { revision, data, policy: json(configured.policy ?? {}), strategy: configured.strategy ? json(configured.strategy) : null,
    ...(source ? { sourceManifest: json(source.manifest) } : {}) };
  return state.datasets[id]!;
}

export function asOfDatasets(state: PmState, ids: string[], asOfNs: string) {
  return ids.map(id => {
    const registered = state.datasets[id]; if (!registered) throw new Problem('dataset_revision_not_found', 404);
    if (BigInt(registered.revision.admittedAtNs) > BigInt(asOfNs)) throw new Problem('dataset_not_available_asof', 409);
    if (BigInt(registered.revision.availableThroughNs) > BigInt(asOfNs)) throw new Problem('dataset_native_availability_after_decision_cutoff', 409);
    assertPmDataAvailable(registered.data, asOfNs);
    return registered;
  });
}
