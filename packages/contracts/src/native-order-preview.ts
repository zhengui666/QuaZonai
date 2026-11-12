/** Native construction only. This request cannot submit or enable protection.
 * BRACKET side is the entry side; standalone STOP_LOSS/TAKE_PROFIT side is the
 * exit side. Prices/quantities are exact decimal strings in instrument units.
 */
export type NativeOrderPreview =
  | { kind: 'STOP_LOSS' | 'TAKE_PROFIT'; side: 'BUY' | 'SELL'; quantity: string; trigger_price: string }
  | { kind: 'BRACKET'; side: 'BUY' | 'SELL'; quantity: string; entry_price: string; stop_loss: string; take_profit: string };

/** POST /accounts/<configured-id>/orders/preview on the existing native adapter. */
export interface NativeOrderPreviewRequest {
  clientId: string;
  accountId: string;
  instrument_id: string;
  order: NativeOrderPreview;
}

/** POST /api/v3/observations/<id>/orders/preview. No account selection or submission flags. */
export type ObservationOrderPreviewRequest = Pick<NativeOrderPreviewRequest, 'instrument_id' | 'order'>;

/** Optional top-level nativeBrackets on the existing account target request.
 * Only explicitly enabled, newly opened long positions receive a native bracket.
 * Existing protection must be canceled and confirmed settled before rebalancing.
 */
export interface NativeBracketProtection {
  enabled: true;
  stop_loss: string;
  take_profit: string;
}
export type NativeTargetBrackets = Record<string, NativeBracketProtection>;

export function validateNativeTargetBrackets(value: unknown): asserts value is NativeTargetBrackets {
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.keys(value).length === 0) throw new Error('native_brackets_invalid');
  for (const [instrument, fields] of Object.entries(value)) {
    if (!instrument || !fields || typeof fields !== 'object' || Array.isArray(fields)) throw new Error('native_brackets_invalid');
    const row = fields as Record<string, unknown>;
    if (row.enabled !== true || Object.keys(row).some(key => !['enabled', 'stop_loss', 'take_profit'].includes(key))) throw new Error('native_brackets_explicit_enable_required');
    for (const key of ['stop_loss', 'take_profit']) {
      const price = row[key];
      if (typeof price !== 'string' || !/^(?:0|[1-9]\d*)(?:\.\d+)?$/.test(price) || /^[0.]+$/.test(price)) throw new Error('native_brackets_positive_decimal_string_required');
    }
  }
}
