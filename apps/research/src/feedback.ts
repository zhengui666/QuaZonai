import type { JsonValue } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import { clipTickRegimeEvaluation, nativeResearchInput, TICK_FEATURE_ABI, TICK_FEATURE_KEYS, TICK_FLOW_FEATURE_KEYS, validateNativeTickFactorRequest } from '../../../packages/contracts/src/tick-input.ts';

function record(value: JsonValue | undefined): JsonObject | undefined {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value : undefined;
}

function scalars(value: JsonObject): JsonObject {
  return Object.fromEntries(Object.entries(value).filter(([, item]) => item === null || typeof item !== 'object'));
}

function partitions(value: JsonValue | undefined): JsonObject[] {
  return (Array.isArray(value) ? value : []).flatMap((item) => {
    const partition = record(item);
    if (!partition || !['TRAIN', 'VALIDATION'].includes(String(partition.partition))) return [];
    const metric = record(partition.metric);
    return [{ ...scalars(partition), metric: { counts: metric?.counts ?? null, pearson_ic: metric?.pearson_ic ?? null } }];
  });
}

/** Model projection only. The complete original result and native evidence stay unchanged. */
export function modelFeedback(result: JsonObject): JsonObject {
  if (result.portfolio === undefined && result.regime_partitions !== undefined) {
    const request = record(result.request);
    return { ...result,
      ...(request?.evaluation_regimes ? { request: clipTickRegimeEvaluation(validateNativeTickFactorRequest(request), String(request.validation_end_ns)) } : {}),
      partitions: partitions(result.partitions),
      regime_partitions: (Array.isArray(result.regime_partitions) ? result.regime_partitions : []).flatMap(item => {
        const partition = record(item);
        if (!partition || !['TRAIN', 'VALIDATION'].includes(String(partition.partition))) return [];
        return [{ ...scalars(partition), regimes: (Array.isArray(partition.regimes) ? partition.regimes : []).map(item => {
          const regime = record(item) ?? {};
          return { ...scalars(regime), metric: regime.metric ?? null, label_spans: regime.label_spans ?? null };
        }) }];
      }) };
  }
  if (result.portfolio === undefined) return result;
  const portfolio = record(result.portfolio);
  if (portfolio?.partition !== 'VALIDATION') throw new Error('validation_feedback_required');
  const crossMarket = portfolio.strategy_kind === 'cross-market-threshold';
  const provenance = record(portfolio.provenance);
  return {
    // Keep all original nullable account scalars before optional component diagnostics.
    portfolio: { ...scalars(portfolio), statistics: portfolio.statistics ?? [],
      ...(portfolio.strategy_kind === 'catalog-ensemble' ? { assumptions: portfolio.assumptions ?? [], consumed_fuel: portfolio.consumed_fuel ?? null, recorded_fee_totals: portfolio.recorded_fee_totals ?? null } : {}),
      summary: scalars(record(portfolio.summary) ?? {}), provenance: crossMarket && provenance
        ? { ...provenance, selection: modelSelection(provenance.selection) } : portfolio.provenance ?? null },
    schema_version: result.schema_version ?? null, request: crossMarket ? modelRequest(result.request) : result.request ?? null,
    evidenceTaskId: result.evidenceTaskId ?? null,
    partitions_component: result.partitions_component ?? null, partitions: partitions(result.partitions),
    components: (Array.isArray(result.components) ? result.components : []).map((item) => {
      const component = record(item) ?? {}; const summary = record(component.summary) ?? {};
      return { index: component.index ?? null, weight: component.weight ?? null,
        summary: { ...scalars(summary), native_versions: summary.native_versions ?? {}, partitions: partitions(summary.partitions) } };
    }),
  };
}

function modelSelection(value: JsonValue | undefined): JsonValue {
  const selection = record(value);
  if (!selection || !Array.isArray(selection.bar_types) || selection.bar_types.length <= 20) return value ?? null;
  const { bar_types, ...rest } = selection;
  return { ...rest, bar_types_count: bar_types.length, model_projection: 'MARKET_IDENTITIES_PAGED', market_page_tool: 'research_markets' };
}

function modelRequest(value: JsonValue | undefined): JsonValue {
  const request = record(value); const forecast = record(request?.forecast);
  return request && forecast ? { ...request, forecast: { ...forecast, selection: modelSelection(forecast.selection) } } : value ?? null;
}

const featureV2Semantics = {
  setter: 'qz_set_feature_v2(index, value, mask, event_ns, effective_available_ns): mask=0 means an observed finite value; mask=1 means NotYetAvailable, 2 Expired, 3 SourceMissing. For nonzero masks the numeric slot is canonical zero, not an observed zero. Never reject a value because its mask is zero.',
  prediction: 'qz_predict_v2(decision_ns, event_ns, target_index, ordinal, feature_count) must return a finite f64. NaN, infinity or a trap fails the counted trial; a nonfinite output is not a missing-prediction marker.',
};

/** Only a model-facing projection. Original admitted facts are never rewritten. */
export function modelBriefInput(input: { data: JsonObject; policy: JsonObject }): { data: JsonObject; policy: JsonObject } {
  const { sealed: _sealed, ...data } = input.data;
  const tagged = nativeResearchInput(data);
  const flow = tagged?.modality === 'TRADE_TICK' && tagged.request.input.representation === 'EVENT_TIME_COHORT_FLOW_CACHE_V1';
  const { evaluation_regimes: _regimes, ...tickRequest } = tagged?.modality === 'TRADE_TICK' ? tagged.request : {};
  if (tagged?.modality === 'TRADE_TICK') return { data: { ...data,
    ...(tagged.request.evaluation_regimes ? { nativeInput: { modality: 'TRADE_TICK', request: tickRequest } } : {}),
    feature_model: {
    abi: TICK_FEATURE_ABI, ...featureV2Semantics, features: [...(flow ? TICK_FLOW_FEATURE_KEYS : TICK_FEATURE_KEYS)],
    ...(flow ? { schema: 'qz.tick-cohort-flow-features.v1' } : {}),
    definitions: ['Quantity-weighted price over exactly the current complete instrument/event-time cohort; previous complete cohort price.',
      'Fast/slow native EMA advance once per complete cohort; periods count cohorts.',
      'Sum of original cohort quantities, original trade count and elapsed event nanoseconds since the preceding cohort.',
      ...(flow ? ['Signed quantity is the exact current-cohort BUYER quantity minus SELLER quantity before f64 conversion. Balanced zero is observed; unknown aggressors are rejected, never filled with zero.'] : [])],
    missing: 'The native tick evaluator skips prediction until a previous cohort exists and both EMAs are initialized; its eligible cohort features are observed values with mask=0. Missing values are not observed zero.',
    scope: 'One fresh score-model instance per instrument. Current and earlier eligible cohorts only; no future labels enter the model.',
    evidence: 'EVENT_TIME_PROXY_UNVERIFIED: exploratory computation, not PIT qualification, independent source validation or execution evidence.',
  } }, policy: input.policy };
  const strategy = record(input.policy.strategy);
  if (strategy?.kind === 'catalog-ensemble') {
    const { catalog_partitions: _partitions, ...visible } = data;
    return { data: { ...visible, feature_model: {
      abi: TICK_FEATURE_ABI, ...featureV2Semantics,
      features: ['completed_bar_close', 'previous_observed_bar_return', 'source_taker_quantity_imbalance', 'source_taker_cash_imbalance',
        'source_trade_count', 'source_fee_per_cash', 'source_within_minute_price_response'],
      definitions: ['Native completed BAR close; close/previous observed market BAR close minus 1 (first BAR missing).',
        'Signed source taker BUY-minus-SELL quantity/total quantity and cash/total cash; inconsistent direction pairs are missing.',
        'Original linked trade count; sum source fee/sum source cash; last original trade price/first original trade price minus 1 within this minute.'],
      target_index: 'Stable u32 assigned on each market first causal arrival in (available_ns,instrument_id) order; ordinal is the observed BAR index within that market.',
      missing: 'Use v2 missing masks and original feature event/availability clocks; unavailable values are not observations of zero.',
      scope: 'One stateful model instance per member over the complete causal cross-market stream; no fixed market choices.',
    }, assumptions: ['Source receive clocks and PIT are unverified; modeled_lag_ns is a conditional assumption.',
      'Recorded fill flow and completed BARs are available; historical order books are not reconstructed.',
      'One long-only margin 1x pUSD proxy account; conditional simulation is not physical venue cash or scientific qualification.'] }, policy: input.policy };
  }
  if (strategy?.kind !== 'cross-market-threshold') return { data, policy: input.policy };
  const { instrumentIds, simulationSettings, ...mapping } = strategy;
  const { fee_rates, ...settings } = record(simulationSettings) ?? {};
  const { trainingMarkets: _training, ...marketData } = data;
  return { data: { ...marketData, request: modelRequest(data.request) }, policy: { ...input.policy, strategy: { ...mapping,
    totalMarkets: Array.isArray(instrumentIds) ? instrumentIds.length : null, market_page_tool: 'research_markets',
    simulationSettings: { ...settings, original_fee_reserve_count: Array.isArray(fee_rates) ? fee_rates.length : null } } } };
}

/** Read only the admitted universe/training metadata, never final-period outcomes. */
export function marketPage(input: { data: JsonObject; policy: JsonObject }, offset: number, limit: number): JsonObject {
  const strategy = record(input.policy.strategy);
  if (strategy?.kind !== 'cross-market-threshold' || !Array.isArray(strategy.instrumentIds)) throw new Error('research_markets_not_configured');
  const ids = strategy.instrumentIds;
  const training = new Map((Array.isArray(input.data.trainingMarkets) ? input.data.trainingMarkets : []).map((row) => {
    const item = record(row) ?? {}; return [item.instrumentId, item] as const;
  }));
  const rates = record(strategy.simulationSettings)?.fee_rates;
  const reserve = new Map((Array.isArray(rates) ? rates : []).map((row) => { const item = record(row) ?? {}; return [item.instrument_id, item] as const; }));
  const markets = ids.slice(offset, offset + limit).map((instrumentId, index) => {
    const metadata = training.get(instrumentId); const rate = reserve.get(instrumentId);
    return { rank: offset + index + 1, instrumentId, training: metadata
      ? { observedCalendarDays: metadata.observedCalendarDays ?? null, barRows: metadata.barRows ?? null } : null,
      feeReserve: rate ? { maker: rate.maker ?? null, taker: rate.taker ?? null, purpose: 'ORIGINAL_TRAINING_SIZING_RESERVE_NOT_COMMISSION' } : null };
  });
  return { markets, totalMarkets: ids.length, nextOffset: offset + markets.length < ids.length ? offset + markets.length : null };
}
