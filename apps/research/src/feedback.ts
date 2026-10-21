import type { JsonValue } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';

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

/** Only a model-facing projection. Original admitted facts are never rewritten. */
export function modelBriefInput(input: { data: JsonObject; policy: JsonObject }): { data: JsonObject; policy: JsonObject } {
  const { sealed: _sealed, ...data } = input.data;
  const strategy = record(input.policy.strategy);
  if (strategy?.kind === 'catalog-ensemble') {
    const { catalog_partitions: _partitions, ...visible } = data;
    return { data: { ...visible, feature_model: {
      abi: 'qz_set_feature_v2(i32,f64,i32,i64,i64)->();qz_predict_v2(i64,i64,i32,i32,i32)->f64',
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
