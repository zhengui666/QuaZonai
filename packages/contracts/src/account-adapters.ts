/** Native adapter identities, not broker accounts or an alternative execution engine. */
export interface NativeAccountAdapterCapabilities { compiled: boolean; adapters?: string[] }
export const HYPERLIQUID_ACCOUNT_ADAPTER = 'HYPERLIQUID';

/** Older account runners shipped only the Hyperliquid factory. Missing metadata
 * retains that historical meaning; an explicit empty list means no adapters. */
export function nativeAccountAdapters(value: NativeAccountAdapterCapabilities): readonly string[] {
  return value.compiled ? value.adapters ?? [HYPERLIQUID_ACCOUNT_ADAPTER] : [];
}
export function validNativeAccountAdapters(value: NativeAccountAdapterCapabilities): boolean {
  return value.adapters === undefined || (Array.isArray(value.adapters)
    && value.adapters.every(id => typeof id === 'string' && /^[A-Z][A-Z0-9_]*$/.test(id))
    && new Set(value.adapters).size === value.adapters.length
    && (value.compiled || value.adapters.length === 0));
}
/** No default is persisted into original account configuration or receipts. */
export function accountAdapter(value: { adapter?: string }): string {
  return value.adapter === undefined ? HYPERLIQUID_ACCOUNT_ADAPTER : value.adapter;
}
