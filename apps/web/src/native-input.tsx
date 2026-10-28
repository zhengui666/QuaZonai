import type { DatasetView, JsonObject } from '../../../packages/contracts/src/index.ts';
import { Facts } from './ui';

export function labelHorizon(settings?: DatasetView['researchSettings']): string {
  if (settings?.horizonUnit === 'EVENT_TIME_NS' && settings.horizonNs !== undefined) {
    const ns = BigInt(settings.horizonNs);
    return `${ns % 1_000_000_000n === 0n ? `${ns / 1_000_000_000n} 秒` : `${ns} ns`}（事件时间；取首个达标成交组）`;
  }
  if (settings?.horizon === null || settings?.horizon === undefined) return '未记录';
  return `${settings.horizon} ${settings.horizonUnit === 'TRADE_COHORTS' ? '个后续成交组' : '次 BAR 观测'}`;
}

export function NativeInputFacts({ modality, settings, preparation, coverage }: { modality?: DatasetView['nativeModality']; settings?: DatasetView['researchSettings']; preparation?: DatasetView['preparation']; coverage?: JsonObject }) {
  const tick = (modality ?? settings?.modality) === 'TRADE_TICK';
  const flow = coverage?.representation === 'EVENT_TIME_COHORT_FLOW_CACHE_V1';
  const cache = flow || coverage?.representation === 'EVENT_TIME_COHORT_CACHE_V1';
  const items: Parameters<typeof Facts>[0]['items'] = [['输入类型', flow ? '派生成交组缓存（含主动净量）' : cache ? '派生成交组缓存' : tick ? '原生逐笔 TradeTick' : 'BAR']];
  if (flow) items.push(['特征', '8 项，含 signed_quantity（主动买量 − 主动卖量）']);
  if (settings?.sampling) items.push(['采样', settings.sampling === 'EVENT_TIME_COHORT_V1' ? '每品种完整同时间戳成交组' : settings.sampling]);
  if (settings) items.push(['标签预测跨度', labelHorizon(settings)]);
  if (settings?.sourceRevision) items.push(['来源修订', <code>{settings.sourceRevision}</code>]);
  if (tick) items.push(['可得时间', '事件时间代理（未验证）'], ['科学资格', '未获资格；仅供探索性分析']);
  else if (preparation) items.push(['科学资格', preparation.scientificQualification === 'UNVERIFIED' ? '未验证' : '尚未评定']);
  if (preparation) items.push(['原始档案', preparation.archiveLoaded ? '已加载' : '未加载'], [cache ? '研究输入' : '原生目录', preparation.nativePrepared ? '已准备' : '未准备']);
  if (coverage) {
    if (typeof coverage.trade_count === 'string') items.push(['原始成交数', coverage.trade_count]);
    if (typeof coverage.cohort_count === 'string') items.push(['成交组数', coverage.cohort_count]);
    if (Array.isArray(coverage.missing_paths)) items.push(['缺失路径', coverage.missing_paths.length ? coverage.missing_paths.filter(item => typeof item === 'string').join(' / ') : '所选路径无已知缺失']);
    if (typeof coverage.event_start_ns === 'string' && typeof coverage.event_end_ns === 'string') items.push(['精确事件范围（ns）', <span><code>{coverage.event_start_ns}</code> ≤ t &lt; <code>{coverage.event_end_ns}</code></span>]);
    if (typeof coverage.first_event_ns === 'string' && typeof coverage.last_event_ns === 'string') items.push(['实际观测首末（ns）', <span><code>{coverage.first_event_ns}</code> 至 <code>{coverage.last_event_ns}</code></span>]);
    const source = coverage.source_evidence;
    if (source && typeof source === 'object' && !Array.isArray(source)) {
      if (Array.isArray(source.selected_streams)) items.push(['所选数据流', source.selected_streams.filter(item => typeof item === 'string').join(' / ')]);
      if (Array.isArray(source.selected_path_prefixes)) items.push(['所选路径范围', source.selected_path_prefixes.filter(item => typeof item === 'string').join(' / ')]);
      if (Array.isArray(source.missing_partitions)) items.push(['缺失分区', source.missing_partitions.length ? source.missing_partitions.filter(item => typeof item === 'string').join(' / ') : '所选分区无已知缺失']);
    }
  }
  return <Facts items={items} />;
}
