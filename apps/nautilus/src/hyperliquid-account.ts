import { capitalDecimalUnits } from '../../../packages/contracts/src/capital-exit.ts';

export const DECIMAL_SCALE = 10n ** 18n;
export const units = capitalDecimalUnits;
export function decimal(value: bigint): string {
  const negative = value < 0n, absolute = negative ? -value : value;
  const text = absolute.toString().padStart(19, '0');
  return (negative ? '-' : '') + `${text.slice(0, -18)}.${text.slice(-18)}`.replace(/\.?0+$/, '');
}
export const minimum = (...values: bigint[]) => values.reduce((a, b) => a < b ? a : b);
export const maximum = (...values: bigint[]) => values.reduce((a, b) => a > b ? a : b);
export const multiply = (a: bigint, b: bigint) => a * b / DECIMAL_SCALE;
export const divide = (a: bigint, b: bigint) => { if (b <= 0n) throw new Error('capital_divisor_invalid'); return a * DECIMAL_SCALE / b; };
export const divideUp = (a: bigint, b: bigint) => { if (b <= 0n) throw new Error('capital_divisor_invalid'); return (a * DECIMAL_SCALE + b - 1n) / b; };
export const roundDown = (value: bigint, step: bigint) => { if (step <= 0n) throw new Error('capital_increment_invalid'); return value / step * step; };
export const roundUp = (value: bigint, step: bigint) => { if (step <= 0n) throw new Error('capital_increment_invalid'); return (value + step - 1n) / step * step; };

export interface HyperliquidAccountProof {
  account_address: string; asof: string; received_at: string; valid_until: string; abstraction: string;
  spot_total: string; spot_hold: string; spot_free: string; withdrawable: string | null;
  spot_taker_fee_rate: string | null;
  balances: { coin: string; total: string; hold: string }[];
  reason_codes: string[];
  original: { spot: unknown; clearinghouse: unknown; abstraction: unknown; fees: unknown };
}
export interface HyperliquidLedgerRow { time: number; hash: string; delta: Record<string, unknown> }
function canonical(value: unknown): string {
  if (Array.isArray(value)) return '[' + value.map(canonical).join(',') + ']';
  if (value && typeof value === 'object') return '{' + Object.keys(value).sort().map(key => JSON.stringify(key) + ':' + canonical((value as Record<string, unknown>)[key])).join(',') + '}';
  return JSON.stringify(value);
}
function record(value: unknown, code: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(code);
  return value as Record<string, unknown>;
}
function nonnegative(value: unknown, code: string): string { if (units(value, code) < 0n) throw new Error(code); return value as string; }

/** Official Info time-range pagination uses an inclusive last timestamp.
 * https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint#pagination
 * The general documented limit is 500 elements or distinct blocks; this ledger
 * endpoint has no documented larger override. A shorter page proves completion.
 */
export async function readHyperliquidLedger(startTime: number, endTime: number,
  readPage: (startTime: number, endTime: number) => Promise<unknown>): Promise<HyperliquidLedgerRow[]> {
  if (!Number.isSafeInteger(startTime) || !Number.isSafeInteger(endTime) || startTime < 0 || endTime < startTime) throw new Error('hyperliquid_ledger_time_range_invalid');
  const seen = new Map<string, { original: string; row: HyperliquidLedgerRow }>();
  let cursor = startTime, boundary = new Set<string>();
  while (true) {
    const value = await readPage(cursor, endTime);
    if (!Array.isArray(value)) throw new Error('hyperliquid_ledger_invalid');
    let previousTime = cursor;
    const page = value.map(item => {
      const row = record(item, 'hyperliquid_ledger_invalid');
      if (!Number.isSafeInteger(row.time) || typeof row.hash !== 'string' || !row.hash) throw new Error('hyperliquid_ledger_identity_invalid');
      const time = Number(row.time);
      // Do not sort an unproven page: a newest-first capped response could have
      // omitted older ledger events, including an unclaimed reverse transfer.
      if (time < previousTime || time > endTime) throw new Error('hyperliquid_ledger_order_or_range_invalid');
      previousTime = time;
      return { time, hash: row.hash, delta: record(row.delta, 'hyperliquid_ledger_delta_invalid') };
    });
    const identities = new Set(page.map(row => row.hash));
    if ([...boundary].some(hash => !identities.has(hash))) throw new Error('hyperliquid_ledger_overlap_missing');
    for (const row of page) {
      const original = canonical(row), prior = seen.get(row.hash);
      if (prior && prior.original !== original) throw new Error('hyperliquid_ledger_identity_conflict');
      if (!prior) seen.set(row.hash, { original, row });
    }
    if (page.length < 500) return [...seen.values()].map(value => value.row);
    const lastTime = page[page.length - 1]!.time;
    // A full page at one boundary millisecond cannot prove that all of that
    // millisecond was returned. Never advance by +1 and silently omit its tail.
    if (lastTime <= cursor) throw new Error('hyperliquid_ledger_timestamp_saturated');
    boundary = new Set(page.filter(row => row.time === lastTime).map(row => row.hash));
    cursor = lastTime;
  }
}

/** Official Hyperliquid Info endpoints only: no signer, exchange action or transfer method. */
export class HyperliquidAccountReader {
  private url: string;
  private address: string;
  constructor(address: string, network: 'mainnet' | 'testnet' = 'mainnet') {
    if (!/^0x[0-9a-fA-F]{40}$/.test(address)) throw new Error('hyperliquid_master_account_required');
    this.address = address;
    this.url = network === 'testnet' ? 'https://api.hyperliquid-testnet.xyz/info' : 'https://api.hyperliquid.xyz/info';
  }
  private async info(type: string, extra: Record<string, unknown> = {}): Promise<unknown> {
    const response = await fetch(this.url, { method: 'POST', redirect: 'error', headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ type, user: this.address, ...extra }), signal: AbortSignal.timeout(10_000) });
    if (!response.ok) throw new Error(`hyperliquid_info_${response.status}`);
    return response.json();
  }
  async account(): Promise<HyperliquidAccountProof> {
    const requestStartedAt = new Date().toISOString();
    const [spotValue, clearingValue, abstractionValue, fees] = await Promise.all([
      this.info('spotClearinghouseState'), this.info('clearinghouseState'), this.info('userAbstraction'), this.info('userFees'),
    ]);
    return parseHyperliquidAccount(this.address, spotValue, clearingValue, abstractionValue, fees, requestStartedAt);
  }
  async ledger(startTime: number): Promise<HyperliquidLedgerRow[]> {
    const endTime = Date.now();
    return readHyperliquidLedger(startTime, endTime,
      (cursor, end) => this.info('userNonFundingLedgerUpdates', { startTime: cursor, endTime: end }));
  }
}

export function parseHyperliquidAccount(address: string, spotValue: unknown, clearingValue: unknown, abstractionValue: unknown, feesValue: unknown, requestStartedAt = new Date().toISOString()): HyperliquidAccountProof {
  const spot = record(spotValue, 'hyperliquid_spot_state_invalid'), clearing = record(clearingValue, 'hyperliquid_clearinghouse_state_invalid');
  if (!Array.isArray(spot.balances) || !Array.isArray(clearing.assetPositions) || typeof abstractionValue !== 'string') throw new Error('hyperliquid_account_state_incomplete');
  const balances = spot.balances.map(item => {
    const row = record(item, 'hyperliquid_spot_balance_invalid');
    if (typeof row.coin !== 'string') throw new Error('hyperliquid_spot_currency_invalid');
    return { coin: row.coin, total: nonnegative(row.total, 'hyperliquid_spot_total_invalid'), hold: nonnegative(row.hold, 'hyperliquid_spot_hold_invalid') };
  });
  if (new Set(balances.map(row => row.coin)).size !== balances.length) throw new Error('hyperliquid_spot_balance_duplicate');
  const cash = balances.find(row => row.coin === 'USDC');
  // A successfully read complete spot balance list without USDC is observed zero.
  const total = cash?.total ?? '0', hold = cash?.hold ?? '0';
  if (units(hold) > units(total)) throw new Error('hyperliquid_spot_hold_exceeds_total');
  const reasons: string[] = [];
  if (!['disabled', 'default'].includes(abstractionValue)) reasons.push('hyperliquid_account_abstraction_unsupported');
  for (const value of clearing.assetPositions) {
    const position = record(record(value, 'hyperliquid_position_invalid').position, 'hyperliquid_position_invalid');
    if (units(position.szi, 'hyperliquid_perp_size') !== 0n) reasons.push('hyperliquid_perpetual_position_outside_spot_scope');
  }
  const margin = record(clearing.crossMarginSummary ?? clearing.marginSummary, 'hyperliquid_margin_summary_required');
  if (units(margin.totalMarginUsed, 'hyperliquid_margin_used') !== 0n) reasons.push('hyperliquid_margin_responsibility_outside_spot_scope');
  if (!Number.isFinite(Date.parse(requestStartedAt))) throw new Error('hyperliquid_request_time_invalid');
  const asof = requestStartedAt, receivedAt = new Date().toISOString();
  const fees = record(feesValue, 'hyperliquid_user_fees_invalid');
  const feeRate = fees.userSpotCrossRate == null ? null : nonnegative(fees.userSpotCrossRate, 'hyperliquid_spot_taker_fee_invalid');
  return { account_address: address.toLowerCase(), asof, received_at: receivedAt, valid_until: new Date(Date.parse(asof) + 10_000).toISOString(), abstraction: abstractionValue,
    spot_total: total, spot_hold: hold, spot_free: decimal(units(total) - units(hold)),
    withdrawable: clearing.withdrawable == null ? null : nonnegative(clearing.withdrawable, 'hyperliquid_withdrawable_invalid'),
    spot_taker_fee_rate: feeRate, balances, reason_codes: [...new Set(reasons)], original: { spot: spotValue, clearinghouse: clearingValue, abstraction: abstractionValue, fees: feesValue } };
}

export function ledgerTransfer(row: HyperliquidLedgerRow): { direction: 'TO_WITHDRAWAL' | 'TO_SPOT'; amount: bigint } {
  if (row.delta.type !== 'accountClassTransfer' || typeof row.delta.toPerp !== 'boolean') throw new Error('capital_external_class_transfer_required');
  const amount = units(row.delta.usdc, 'capital_ledger_usdc');
  if (amount <= 0n) throw new Error('capital_ledger_amount_invalid');
  return { direction: row.delta.toPerp ? 'TO_WITHDRAWAL' : 'TO_SPOT', amount };
}

export function ledgerAmount(row: HyperliquidLedgerRow, kind: 'TRANSFER' | 'WITHDRAWAL'): bigint {
  if (kind === 'TRANSFER') {
    if (row.delta.type !== 'accountClassTransfer' || row.delta.toPerp !== true) throw new Error('capital_external_spot_to_perp_transfer_required');
  } else if (row.delta.type !== 'withdraw') throw new Error('capital_external_withdrawal_required');
  const value = units(row.delta.usdc, 'capital_ledger_usdc'); if (value <= 0n) throw new Error('capital_ledger_amount_invalid');
  return value;
}
