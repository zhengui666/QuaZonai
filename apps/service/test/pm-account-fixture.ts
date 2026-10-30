import type { JsonObject } from '@earendil-works/pi-durable';
import { captureAccount } from '../src/pm-account.ts';
import { accountKey } from '../src/capital-exit.ts';
import { json } from '../src/facts.ts';
import type { ObservationFact } from '../src/facts.ts';

// Original source-contract fixtures only. They test projection/receipt integrity,
// not a broker account, native simulation or financial performance.
export function accountSourceFixture() {
  const connection = { id: 'binding:original', name: 'Original source protocol', mode: 'paper' as const,
    url: 'http://127.0.0.1:1', clientId: 'transport-client', accountId: 'transport-account' };
  const money = (amount: string, currency: string) => ({ amount, currency });
  const balance = (amount: string, currency: string) => ({ total: money(amount, currency), free: money(amount, currency), locked: money('0', currency) });
  const native: JsonObject = { account_id: 'VENUE-001', account_type: 'CASH', base_currency: null,
    balances: [balance('900.000000000000000001', 'USDC'), balance('0.125000000000000001', 'BTC')], margins: [],
    unrealized_pnls: [], realized_pnls: [], total_equity: [money('900.000000000000000001', 'USDC'), money('0.125000000000000001', 'BTC')],
    base_currency_equity: null, is_stale: false, stale_instruments: [], stale_currencies: [], unpriced_instruments: [],
    event_id: '01900000-0000-7000-8000-000000000001', ts_event: '99', ts_init: '100' };
  const snapshot: JsonObject = { clientId: connection.clientId, accountId: connection.accountId, currency: 'USDC',
    accountSourceId: 'native-source', observationId: 'native-observation', controlRevision: '7', sequence: '9007199254740995', nativeClockNs: '99',
    equity: null, cash: null, positions: [],
    nativeObservation: { schema_version: 1, binding: { schema_version: 1, project_id: '01900000-0000-7000-8000-000000000002', environment: 'PAPER',
      native_trader_id: 'TRADER-001', native_session_id: 'SESSION-001', native_account_id: 'VENUE-001', native_version: '0.67.0' },
      sequence: '9007199254740995', dropped_events: '0', observed_at_ns: '101', connection: 'CONNECTED', snapshot: native },
    openOrders: { schema_version: 1, account_id: 'VENUE-001', asof_ns: '99', available_ns: '101', count: 0 } };
  const observation: ObservationFact = { input: { requestId: 'source-original', portfolioId: 'portfolio-original', mode: 'paper', connectionId: connection.id },
    connection, taskId: 1, target: {}, snapshot, snapshotReceivedAtNs: '102',
    view: { id: connection.id, name: connection.name, portfolioId: 'portfolio-original', mode: 'paper', status: 'OBSERVING', currency: 'USDC',
      equity: null, cash: null, positions: [], curve: [], updatedAt: null, error: null, receipt: null } };
  const application = { observations: { [connection.id]: json(observation) }, accountControls: { [accountKey(connection)]: {
    commands: {}, receivedAtNs: '101', status: { capability: 'SUPPORTED', reason: null, accountSourceId: 'native-source', observationId: 'native-observation', controlRevision: '7', reserved: '0', exits: [] },
  } } };
  return { connection, application, native, snapshot, observation,
    capture: () => captureAccount(application, connection.id, '110') };
}
