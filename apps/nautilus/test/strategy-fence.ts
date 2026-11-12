import type { IncomingMessage } from 'node:http';
import type { createCapitalAccount } from '../src/capital-account.ts';
type Account = Awaited<ReturnType<typeof createCapitalAccount>>;
const originals = new WeakMap<Account, Map<string, Record<string, unknown>>>();
/** Test client follows the public worker CAS contract and retains original bytes. */
export async function fencedBody(account: Account, method: string, path: string, body: unknown, request: (method: string) => IncomingMessage) {
  const row = body as Record<string, unknown>;
  if (method !== 'POST' || path !== '/strategy/commands' || row?.action !== 'START' || row.expectedStrategyRevision !== undefined) return body;
  let saved = originals.get(account); if (!saved) originals.set(account, saved = new Map());
  let fence = saved.get(String(row.requestId));
  if (!fence) {
    const response = (await account.handle('/strategy/fence', request('GET'), async () => null)).value as Record<string, unknown>;
    fence = { expectedAccountSourceId: response.accountSourceId, expectedStrategyRevision: response.strategyRevision };
    saved.set(String(row.requestId), fence);
  }
  Object.assign(row, fence); return row;
}
