import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { JsonObject } from '@earendil-works/pi-durable';
import { captureAccount } from '../src/pm-account.ts';
import { accountKey } from '../src/capital-exit.ts';
import { counter, json, object } from '../src/facts.ts';
import { compareDecimal } from '../../../packages/contracts/src/pm-validation.ts';

const binary = process.env.QZ_JOB_BINARY;

export async function observedSpotFixture(directoryPath = process.env.QZ_PM_OBSERVED_FIXTURE_DIR) {
  assert.ok(directoryPath, 'QZ_PM_OBSERVED_FIXTURE_DIR or an explicit fixture directory is required.');
  const source = object(JSON.parse(await readFile(join(directoryPath, 'source.json'), 'utf8')));
  const original = object(source.initial_state);
  const native = object(original.snapshot);
  const simulation = object(source.simulation);
  const settings = object(simulation.settings);
  const sourceClock = counter(native.ts_event, 'original_snapshot_event');
  const nativePublished = counter(native.ts_init, 'original_snapshot_publication');
  const received = [counter(original.available_ns, 'source_available'), counter(original.open_orders_available_ns, 'orders_available')]
    .reduce((a, b) => BigInt(a) >= BigInt(b) ? a : b);
  const directory = await mkdtemp(join(tmpdir(), 'qz-pm-observed-native-'));
  const connection = { id: 'controlled-native-account', name: 'Controlled transport of actual native publication', mode: 'paper' as const,
    url: 'http://127.0.0.1:1', accountId: 'transport-alias', clientId: 'controlled-native-client' };
  // These transport identities and source receipt clocks are explicit protocol
  // fixtures. Native balances, event clocks and zero orders come from the real
  // fixture producer. This is not market/PIT or live account evidence.
  const application = { observations: { [connection.id]: json({ connection, taskId: 1,
    view: { currency: settings.base_currency }, snapshotReceivedAtNs: received,
    snapshot: { clientId: connection.clientId, accountId: connection.accountId, accountSourceId: 'original-native-run', observationId: native.event_id,
      controlRevision: '1', nativeClockNs: sourceClock, sequence: '1', currency: settings.base_currency, equity: null, cash: null, positions: [],
      nativeObservation: { schema_version: 1, binding: { schema_version: 1, project_id: source.nativeDatasetRevisionId,
        environment: 'PAPER', native_trader_id: 'CONTROLLED-001', native_session_id: 'CONTROLLED-SESSION', native_account_id: native.account_id, native_version: original.source_native_version },
        sequence: '1', dropped_events: '0', observed_at_ns: nativePublished, connection: 'CONNECTED', snapshot: native },
      openOrders: { schema_version: 1, account_id: original.open_orders_account_id, asof_ns: original.open_orders_asof_ns,
        available_ns: original.open_orders_available_ns, count: original.open_orders_count } } }) },
    accountControls: { [accountKey(connection)]: { receivedAtNs: received, commands: {}, status: { capability: 'SUPPORTED', reason: null,
      accountSourceId: 'original-native-run', observationId: native.event_id, controlRevision: '1', reserved: '0', exits: [] } } } };
  const capture = captureAccount(application, connection.id);
  const common = { capture, nativeDatasetRevisionId: String(source.nativeDatasetRevisionId),
    datasetRevision: { id: String(source.pmDatasetRevisionId), contentRef: String(source.contentRef) },
    catalog: String(source.catalog), binary: binary!, dataDir: directory };
  assert.equal(typeof source.catalog, 'string'); assert.ok(source.contentRef); assert.ok(source.pmDatasetRevisionId);
  assert.ok((native.balances as JsonObject[]).some(balance => object(balance.total).currency !== settings.base_currency
    && compareDecimal(String(object(balance.total).amount), '0') > 0), 'The original source contains actual noncash inventory.');
  return { source, original, native, simulation, settings, common, application, connection, cleanup: () => rm(directory, { recursive: true, force: true }) };
}
