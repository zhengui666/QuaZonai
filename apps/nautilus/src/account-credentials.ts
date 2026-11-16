/** Public configuration contains a reference only, never a credential value. */
export function validateAccountCredentialReference(mode: string, reference: unknown): asserts reference is string | undefined {
  if (reference !== undefined && (mode !== 'live' || typeof reference !== 'string' || !/^QZ_HYPERLIQUID_[A-Z0-9_]+_PK$/.test(reference))) {
    throw Object.assign(new Error('account_credential_reference_invalid'), {statusCode:422});
  }
}
/** Resolve only when an explicitly started live runner needs its official environment. */
export function accountCredentialEnvironment(
  account: {mode:string;network?:string;credentialEnv?:string},
  environment: NodeJS.ProcessEnv,
): NodeJS.ProcessEnv {
  validateAccountCredentialReference(account.mode, account.credentialEnv);
  if (account.mode !== 'live') return {};
  const target = account.network === 'testnet' ? 'HYPERLIQUID_TESTNET_PK' : 'HYPERLIQUID_PK';
  const source = account.credentialEnv ?? target;
  const value = environment[source];
  if (account.credentialEnv !== undefined && !value) throw new Error('account_credential_reference_unavailable');
  // Never pass other accounts' keys, the source reference, or the opposite network key.
  return value ? {[target]:value} : {};
}
