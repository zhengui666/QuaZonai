/** Only wired adapters are selectable. Omission retains the historical Hyperliquid runner. */
export type AccountAdapter = 'HYPERLIQUID';
/** Build-time capabilities, not proof of connection, authorization or trading readiness. */
export type AccountAdapterCapability = {
  id: string; compiled: boolean; modes: string[]; accountTypes: string[];
  unsupported?: string[]; reason?: string;
}
export function accountAdapterCapabilities(value: unknown): AccountAdapterCapability[] | undefined {
  if (value === undefined) return undefined;
  const invalid = (): never => { throw new Error('native_account_adapter_capabilities_invalid'); };
  if (!Array.isArray(value)) return invalid();
  const seen = new Set<string>();
  return value.map(row => {
    if (!row || typeof row !== 'object' || typeof row.id !== 'string' || !row.id || seen.has(row.id)
      || typeof row.compiled !== 'boolean' || !Array.isArray(row.modes) || !row.modes.every((v: unknown) => typeof v === 'string')
      || !Array.isArray(row.accountTypes) || !row.accountTypes.every((v: unknown) => typeof v === 'string')
      || row.reason !== undefined && typeof row.reason !== 'string'
      || row.unsupported !== undefined && (!Array.isArray(row.unsupported) || !row.unsupported.every((v: unknown) => typeof v === 'string'))) return invalid();
    seen.add(row.id);
    return {id:row.id, compiled:row.compiled, modes:[...row.modes], accountTypes:[...row.accountTypes],
      ...(row.reason !== undefined ? {reason:row.reason} : {}), ...(row.unsupported !== undefined ? {unsupported:[...row.unsupported]} : {})};
  });
}
