import { mkdir, open, readdir, readFile, rename } from 'node:fs/promises';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import type { AccountOptions } from './account-runtime.ts';
import { nativeAccountStrategyConfiguration } from '../../../packages/contracts/src/account-strategy.ts';

export interface AccountCreation { requestId: string; configuration: AccountOptions }
export function physicalAccount(account: AccountOptions): string {
  return JSON.stringify(account.mode === 'live' ? ['live', account.network ?? 'mainnet', account.accountAddress?.toLowerCase(), 'SPOT_CASH', 'USDC']
    : ['paper', account.network ?? 'mainnet', account.accountId, 'SPOT_CASH', 'USDC']);
}
function invalid(code: string): never { throw Object.assign(new Error(code), { statusCode: 422 }); }
export function accountCreation(value: unknown): AccountCreation {
  if (!value || typeof value !== 'object' || Array.isArray(value)) invalid('account_configuration_required');
  const row = value as Record<string, unknown>;
  if (Object.keys(row).some(key => !['requestId', 'configuration'].includes(key)) || typeof row.requestId !== 'string' || !row.requestId.trim()) invalid('account_creation_request_invalid');
  const config = row.configuration as AccountOptions;
  if (!config || typeof config !== 'object' || Array.isArray(config) || Object.keys(config).some(key => !['id','clientId','accountId','mode','nativeStrategy','instruments','accountAddress','network','executionProductTypes','startingBalance','cashBuffer','paperFeeRate','risk','paperExecution'].includes(key))) invalid('account_configuration_invalid');
  if (typeof config.id !== 'string' || !/^[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}$/.test(config.id) || typeof config.clientId !== 'string' || !config.clientId.trim() || typeof config.accountId !== 'string' || !config.accountId.trim()
    || !['paper','live'].includes(config.mode) || config.network !== undefined && !['mainnet','testnet'].includes(config.network)
    || !Array.isArray(config.instruments) || !config.instruments.length || !config.instruments.every(id => typeof id === 'string' && id.trim())) invalid('account_configuration_invalid');
  if (config.mode === 'live' && (typeof config.accountAddress !== 'string' || !/^0x[0-9a-fA-F]{40}$/.test(config.accountAddress))) invalid('hyperliquid_master_account_required');
  if (config.mode === 'paper' && (typeof config.startingBalance !== 'string' || !/^(?:[0-9]+)(?:\.[0-9]+)?$/.test(config.startingBalance) || !/[1-9]/.test(config.startingBalance) || typeof config.paperFeeRate !== 'string' || !/^[0-9]+(?:\.[0-9]+)?$/.test(config.paperFeeRate))) invalid('paper_balance_and_explicit_fee_required');
  nativeAccountStrategyConfiguration(config.nativeStrategy);
  if (!config.instruments.includes(config.nativeStrategy.instrument_id)) invalid('account_strategy_instrument_not_configured');
  return structuredClone(row) as unknown as AccountCreation;
}
/** Configuration lives alongside the original account receipts, never in a second account ledger. */
export async function loadAccountConfigurations(root: string): Promise<AccountCreation[]> {
  await mkdir(root, { recursive: true }); const result: AccountCreation[] = [];
  for (const entry of await readdir(root, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    let text: string;
    try { text = await readFile(join(root, entry.name, 'configuration.json'), 'utf8'); }
    catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') continue; throw error; }
    const saved = accountCreation(JSON.parse(text));
    if (saved.configuration.id !== entry.name) invalid('account_configuration_directory_mismatch');
    result.push(saved);
  }
  return result;
}
export function sameAccountCreation(a: AccountCreation, b: AccountCreation) { return isDeepStrictEqual(a, b); }
export async function saveAccountConfiguration(root: string, value: AccountCreation): Promise<void> {
  const directory = join(root, value.configuration.id); await mkdir(directory, { recursive: true });
  const temporary = join(directory, 'configuration.next.json');
  const file = await open(temporary, 'w', 0o600);
  try { await file.writeFile(JSON.stringify(value)); await file.sync(); } finally { await file.close(); }
  await rename(temporary, join(directory, 'configuration.json'));
  const dir = await open(directory, 'r'); try { await dir.sync(); } finally { await dir.close(); }
}
