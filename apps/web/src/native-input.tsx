import { useTranslation } from 'react-i18next';
import { tr } from './i18n';
import type { DatasetView, JsonObject } from '../../../packages/contracts/src/index.ts';
import { Facts } from './ui';

export function labelHorizon(settings?: DatasetView['researchSettings']): string {
  if ((settings?.horizonUnit === 'EVENT_TIME_NS' || settings?.horizonUnit === 'RECEIVED_TIME_NS') && settings.horizonNs !== undefined) {
    const ns = BigInt(settings.horizonNs);
    const duration = ns % 1_000_000_000n === 0n ? tr('{{seconds}} seconds', { seconds: String(ns / 1_000_000_000n) }) : `${ns} ns`;
    return tr('{{duration}} ({{clock}} time; first qualifying trade group)', { duration, clock: settings.horizonUnit === 'RECEIVED_TIME_NS' ? tr('Receipt') : tr('Event') });
  }
  if (settings?.horizon === null || settings?.horizon === undefined) return tr("Not recorded");
  return `${settings.horizon} ${settings.horizonUnit === 'TRADE_COHORTS' ? tr("subsequent trade groups") : tr("BAR observations")}`;
}

export function NativeInputFacts({ modality, settings, preparation, coverage }: { modality?: DatasetView['nativeModality']; settings?: DatasetView['researchSettings']; preparation?: DatasetView['preparation']; coverage?: JsonObject }) {
  useTranslation('core');
  const tick = (modality ?? settings?.modality) === 'TRADE_TICK';
  const received = (settings?.availability ?? coverage?.availability) === 'RECEIVED_TIME_OBSERVED';
  const flow = coverage?.representation === 'EVENT_TIME_COHORT_FLOW_CACHE_V1' || coverage?.representation === 'RECEIVED_TIME_COHORT_FLOW_CACHE_V1';
  const cache = flow || coverage?.representation === 'EVENT_TIME_COHORT_CACHE_V1' || coverage?.representation === 'RECEIVED_TIME_COHORT_CACHE_V1';
  const nativeTypes = Array.isArray(coverage?.native_data_types) ? coverage.native_data_types.filter(value => typeof value === 'string').join(' / ') : '';
  const items: Parameters<typeof Facts>[0]['items'] = [[tr("Input type"), flow ? tr("Derived trade-group cache with signed volume") : cache ? tr("Derived trade-group cache") : tick ? tr("Native TradeTick records") : (modality ?? settings?.modality) === 'BAR' ? 'BAR' : nativeTypes || tr("Not specified")]];
  if (flow) items.push([tr("Features"), tr("8 features, including signed_quantity (aggressive buy volume minus aggressive sell volume)")]);
  if (settings?.sampling) items.push([tr("Sampling"), settings.sampling === 'EVENT_TIME_COHORT_V1' ? tr("Complete trade groups per instrument and event timestamp") : settings.sampling === 'RECEIVED_TIME_COHORT_V1' ? tr("Trade groups per instrument and receipt timestamp") : settings.sampling]);
  if (settings) items.push([tr("Label horizon"), labelHorizon(settings)]);
  if (settings?.sourceRevision) items.push([tr("Source revision"), <code>{settings.sourceRevision}</code>]);
  if (tick) items.push([tr("Availability time"), received ? tr("Observed source receipt time (clock not verified)") : tr("Event-time proxy (unverified)")], [tr("Research qualification"), tr("Not qualified; exploratory analysis only")]);
  else if (preparation) items.push([tr("Research qualification"), preparation.scientificQualification === 'UNVERIFIED' ? tr("Unverified") : tr("Not yet assessed")]);
  if (preparation) items.push([tr("Source archive"), preparation.archiveLoaded ? tr("Loaded") : tr("Not loaded")], [cache ? tr("Research input") : tr("Native catalog"), preparation.nativePrepared ? tr("Prepared") : tr("Not prepared")]);
  if (coverage) {
    if (typeof coverage.trade_count === 'string') items.push([tr("Source trades"), coverage.trade_count]);
    if (typeof coverage.cohort_count === 'string') items.push([tr("Trade groups"), coverage.cohort_count]);
    if (Array.isArray(coverage.missing_paths)) items.push([tr("Missing paths"), coverage.missing_paths.length ? coverage.missing_paths.filter(item => typeof item === 'string').join(' / ') : tr("No known gaps in the selected paths")]);
    if (typeof coverage.event_start_ns === 'string' && typeof coverage.event_end_ns === 'string') items.push([tr('Exact {{clock}} window (ns)', { clock: received ? tr('Receipt') : tr('Event') }), <span><code>{coverage.event_start_ns}</code> ≤ t &lt; <code>{coverage.event_end_ns}</code></span>]);
    if (typeof coverage.first_event_ns === 'string' && typeof coverage.last_event_ns === 'string') items.push([tr("Observed first and last timestamps (ns)"), <span><code>{coverage.first_event_ns}</code>  {tr("to")} <code>{coverage.last_event_ns}</code></span>]);
    const source = coverage.source_evidence;
    if (source && typeof source === 'object' && !Array.isArray(source)) {
      if (Array.isArray(source.selected_streams)) items.push([tr("Selected streams"), source.selected_streams.filter(item => typeof item === 'string').join(' / ')]);
      if (Array.isArray(source.selected_path_prefixes)) items.push([tr("Selected path scope"), source.selected_path_prefixes.filter(item => typeof item === 'string').join(' / ')]);
      if (Array.isArray(source.missing_partitions)) items.push([tr("Missing partitions"), source.missing_partitions.length ? source.missing_partitions.filter(item => typeof item === 'string').join(' / ') : tr("No known gaps in the selected partitions")]);
    }
  }
  return <Facts items={items} />;
}
