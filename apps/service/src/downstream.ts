import { Agent, fetch } from 'undici';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { ConnectionConfig } from './config.ts';
import { canonical } from '../../research/src/documents.ts';
import { counter, decimal, nowNs, nsTime, object, Problem, text } from './facts.ts';
import type { ObservationFact } from './facts.ts';

export interface ObservationTaskInput {
  id: string; connection: ConnectionConfig; receiptId: string; target: JsonObject;
}
export function connectionBase(connection: ConnectionConfig): URL {
  const url = new URL(connection.url); url.hash = '';
  if (!url.pathname.endsWith('/')) url.pathname += '/';
  return url;
}
export async function downstream(input: ObservationTaskInput, operation: 'deliver' | 'receipt' | 'account', signal: AbortSignal): Promise<JsonObject> {
  const { connection } = input;
  const url = connectionBase(connection);
  url.pathname += operation === 'deliver' ? 'targets' : operation === 'receipt' ? `targets/${encodeURIComponent(input.receiptId)}` : 'account';
  const headers: Record<string, string> = { 'content-type': 'application/json', 'x-qz-client-id': connection.clientId };
  if (connection.tokenEnv) {
    const token = process.env[connection.tokenEnv]; if (!token) throw new Problem('downstream_credentials_missing');
    headers.authorization = `Bearer ${token}`;
  }
  if (operation === 'deliver' && BigInt(counter(input.target.valid_until_ns, 'expiry')) <= BigInt(nowNs())) throw new Problem('target_expired');
  const dispatcher = new Agent({ headersTimeout: 0, bodyTimeout: 0, connect: { timeout: 0 } });
  try {
    const response = await fetch(url, { method: operation === 'deliver' ? 'POST' : 'GET', headers,
      body: operation === 'deliver' ? JSON.stringify({ id: input.receiptId, clientId: connection.clientId, accountId: connection.accountId,
        expiresAt: nsTime(String(input.target.valid_until_ns)), target: input.target }) : undefined,
      signal, dispatcher, redirect: 'error',
    });
    if (!response.ok) { await response.body?.cancel(); throw new Problem(`downstream_${operation}_${response.status}`); }
    return object(await response.json(), `downstream_${operation}`);
  } finally { await dispatcher.close(); }
}

export function acceptReceipt(fact: ObservationFact, receipt: JsonObject): void {
  const original = fact.view.receipt!;
  if (receipt.id !== original.id || receipt.clientId !== fact.connection.clientId || receipt.accountId !== fact.connection.accountId || receipt.expiresAt !== original.expiresAt) throw new Problem('downstream_receipt_identity_mismatch');
  const status = text(receipt.status, 'receipt_status');
  if (!['PENDING', 'ACKNOWLEDGED', 'REJECTED', 'EXPIRED', 'REVOKED'].includes(status)) throw new Problem('downstream_receipt_status_unsupported');
  if (original.externalId !== null && typeof receipt.externalId === 'string' && original.externalId !== receipt.externalId) throw new Problem('downstream_receipt_external_id_conflict');
  original.status = status;
  original.externalId ??= typeof receipt.externalId === 'string' ? receipt.externalId : null;
  original.acknowledgedAt ??= typeof receipt.acknowledgedAt === 'string' ? receipt.acknowledgedAt : null;
  if (fact.view.status !== 'STOPPED') fact.view.status = status === 'ACKNOWLEDGED' ? 'OBSERVING' : status;
}
export function acceptSnapshot(fact: ObservationFact, snapshot: JsonObject): void {
  if (snapshot.clientId !== fact.connection.clientId || snapshot.accountId !== fact.connection.accountId || snapshot.currency !== fact.view.currency) throw new Problem('account_identity_mismatch');
  const sequence = BigInt(counter(snapshot.sequence, 'account_sequence'));
  const clock = BigInt(counter(snapshot.nativeClockNs, 'account_clock'));
  if (fact.snapshot) {
    const oldSequence = BigInt(counter(fact.snapshot.sequence, 'old_sequence'));
    const oldClock = BigInt(counter(fact.snapshot.nativeClockNs, 'old_clock'));
    if (sequence < oldSequence || clock < oldClock) throw new Problem('account_snapshot_out_of_order');
    if (sequence === oldSequence) { if (canonical(snapshot) !== canonical(fact.snapshot)) throw new Problem('account_snapshot_conflict'); return; }
  }
  const optionalMoney = (value: unknown, name: string) => value == null ? null : decimal(value, name);
  const positions = Array.isArray(snapshot.positions) ? snapshot.positions.map((item) => {
    const position = object(item, 'position');
    return { instrumentId: text(position.instrumentId, 'instrument'), quantity: decimal(position.quantity, 'quantity'),
      value: optionalMoney(position.value, 'position_value'), pnl: optionalMoney(position.pnl, 'position_pnl') };
  }) : [];
  fact.snapshot = snapshot;
  fact.view.equity = optionalMoney(snapshot.equity, 'equity'); fact.view.cash = optionalMoney(snapshot.cash, 'cash');
  fact.view.updatedAt = nsTime(clock.toString()); fact.view.positions = positions;
  fact.view.curve.push({ at: fact.view.updatedAt, value: fact.view.equity });
}
