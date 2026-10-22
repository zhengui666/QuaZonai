import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { renameSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import Fastify from 'fastify';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { ConnectionConfig } from '../src/config.ts';
import { json, nowNs, nsTime, object } from '../src/facts.ts';
import { pmArtifactDigest } from '../src/pm-evidence.ts';
import { compareDecimal, multiplyDecimal, pmCanonical } from '../../../packages/contracts/src/pm-validation.ts';

/** Explicit software PAPER model, registered before research. No real market
 * liquidity, broker certification, fill or live permission is asserted. The
 * source/native instrument/fees are original inputs; the fixed TEST limits
 * never depend on a selected portfolio, solver result or qualification. */
export async function paperReceiver(input: { source: JsonObject; directory: string; connection: ConnectionConfig; originalSnapshot: JsonObject }) {
  const { source, directory, connection } = input;
  const settings = object(source.settings);
  const metadata = object(JSON.parse(await readFile(join(String(source.catalog), 'qz-spot-cash-source.json'), 'utf8')));
  const definition = object(metadata.instrument_definition); const instrument = object(Object.values(definition)[0]);
  const instrumentId = String(instrument.id); const currency = String(settings.base_currency);
  const fees = (settings.fee_rates as JsonObject[]).find(row => row.instrument_id === instrumentId)!; assert.ok(fees);
  const schedule = object(object(object(settings.fee_model).parameters).schedule);
  const takerRules = (schedule.rules as JsonObject[]).filter(row => row.instrument_id === instrumentId && row.liquidity === 'TAKER');
  assert.equal(takerRules.length, 2); assert.ok(takerRules.every(row => row.rate === fees.taker));
  assert.equal(object(object(settings.fill_model).parameters).prob_slippage, '0');
  const registration: JsonObject = {
    schemaVersion: 1, kind: 'TEST_PAPER_EXECUTION_MODEL_REGISTERED_BEFORE_RESEARCH', registeredAtNs: nowNs(),
    environment: 'PAPER', method: 'ORIGINAL_NATIVE_SPOT_FEES_NO_SLIPPAGE_NO_IMPACT_FINITE_TEST_CAPACITY_V1',
    instrumentDefinition: definition, instrumentRulesRef: `sha256:${pmArtifactDigest(definition)}`,
    nativeFeeModel: settings.fee_model!, nativeFillModel: settings.fill_model!,
    costs: { commissionRate: fees.taker!, slippageRate: '0', impactRate: '0' },
    entry: { maximumBuyNotional: '100000', maximumSellNotional: '100000' }, exit: { maximumNotional: '100000' },
    liquidityNotional: '100000', validUntilNs: schedule.valid_until_ns!,
    originalSourceReceiptDigest: pmArtifactDigest(metadata),
    limitations: ['ARCHIVAL_SOFTWARE_CONTROLLED_CLOCK', 'FINITE_TEST_PAPER_CAPACITY_NOT_ESTIMATED_MARKET_LIQUIDITY',
      'PUBLIC_RATE_SCENARIO_NOT_VERIFIED_LIVE_APPLICABILITY', 'TARGET_ACKNOWLEDGEMENT_IS_NOT_A_FILL', 'NO_LIVE_AUTHORIZATION'],
  };
  const modelRef = `sha256:${pmArtifactDigest(registration)}`;
  await writeFile(join(directory, 'original-paper-execution-model.json'), JSON.stringify(registration), { flag: 'wx' });
  let snapshot: JsonObject | null = null; let allowReceipts = false;
  const receipts = new Map<string, JsonObject>(); const requests: JsonObject[] = [];
  let posts = 0, gets = 0; const remote = Fastify();
  const envelope = { clientId: connection.clientId, accountId: connection.accountId, mode: 'paper', currency };
  remote.get('/account', (_request, reply) => snapshot ? json(snapshot) : reply.code(503).send({ error: 'archival_account_not_currently_published' }));
  remote.get('/capabilities', (_request, reply) => snapshot ? { ...envelope,
    capitalExit: { version: 1, supported: false, readable: true },
    // Advertised only because POST below compares the actual whole source
    // identity synchronously with acceptance, not just controlRevision.
    automaticTargets: { version: 1, supported: true, revisionBound: true, snapshotBound: true },
  } : reply.code(503).send({ error: 'archival_capabilities_not_currently_published' }));
  remote.get('/capital-exits', (_request, reply) => snapshot ? { ...envelope, accountSourceId: snapshot.accountSourceId,
    observationId: snapshot.observationId, controlRevision: snapshot.controlRevision, reserved: '0', exits: [] }
    : reply.code(503).send({ error: 'archival_control_not_currently_published' }));
  remote.post('/targets', (request, reply) => {
    posts++; const body = object(request.body); const id = String(body.id); const old = receipts.get(id);
    if (old) return pmCanonical(requests.find(value => value.id === id)) === pmCanonical(body) ? json(old) : reply.code(409).send({ error: 'request_id_conflict' });
    if (!snapshot) return reply.code(503).send({ error: 'source_unavailable' });
    if (request.headers['x-qz-client-id'] !== connection.clientId || body.clientId !== connection.clientId || body.accountId !== connection.accountId)
      return reply.code(409).send({ error: 'account_identity_mismatch' });
    const allocation = object(body.allocation); const expected = { accountSourceId: snapshot.accountSourceId, observationId: snapshot.observationId,
      snapshotSequence: snapshot.sequence, nativeClockNs: snapshot.nativeClockNs, controlRevision: snapshot.controlRevision };
    if (Object.entries(expected).some(([key, value]) => allocation[key] !== value)) return reply.code(409).send({ error: 'account_snapshot_changed' });
    // Reservation is an exact decimal amount; 0 and 0.000... denote the same
    // source fact. Identity/sequence/clock fields above remain byte-exact.
    try { if (compareDecimal(String(allocation.reserved), '0') !== 0) return reply.code(409).send({ error: 'account_reservation_changed' }); }
    catch { return reply.code(409).send({ error: 'account_reservation_invalid' }); }
    const target = object(body.target); const deadline = BigInt(String(target.valid_until_ns));
    if (deadline <= BigInt(nowNs()) || body.expiresAt !== nsTime(String(target.valid_until_ns)) || allocation.validUntilNs !== target.valid_until_ns)
      return reply.code(409).send({ error: 'target_expired_or_deadline_mismatch' });
    // No asynchronous yield between the source check and original receipt
    // persistence. The external receiver survives local service close/reopen.
    const receipt: JsonObject = { ...json(body), status: 'ACKNOWLEDGED', externalId: `paper:${id}`, acknowledgedAt: nsTime(nowNs()) };
    const path = join(directory, 'paper-receiver-receipts.json');
    writeFileSync(`${path}.pending`, JSON.stringify([...receipts.values(), receipt]), { mode: 0o600 }); renameSync(`${path}.pending`, path);
    requests.push(json(body)); receipts.set(id, receipt);
    // Deliberate response loss AFTER actual target acceptance. QZ must retain
    // uncertainty and recover using GET with this original id, never POST twice.
    return reply.code(503).send({ error: 'controlled_response_lost_after_acceptance' });
  });
  remote.get<{ Params: { id: string } }>('/targets/:id', (request, reply) => {
    gets++; if (!allowReceipts) return reply.code(503).send({ error: 'controlled_receipt_temporarily_unavailable' });
    const receipt = receipts.get(request.params.id); return receipt ? json(receipt) : reply.code(404).send({ error: 'original_receipt_not_found' });
  });
  connection.url = await remote.listen({ host: '127.0.0.1', port: 0 });
  return { registration, modelRef, definition, connection, get posts() { return posts; }, get gets() { return gets; },
    get requests() { return requests.map(json); }, get receipts() { return [...receipts.values()].map(json); },
    revealReceipts() { allowReceipts = true; }, close: () => remote.close(),
    heartbeat(sequence: string) {
      assert.ok(snapshot); assert.ok(BigInt(sequence) > BigInt(String(snapshot.sequence)));
      const next = json(snapshot); next.sequence = sequence; object(next.nativeObservation).sequence = sequence;
      const proof = object(object(next.execution).pmEvidence); proof.sequence = sequence; proof.id = `paper-source:${modelRef}:${sequence}`;
      proof.availableAtNs = nowNs();
      // Same actual native frame and original deadlines. A transport heartbeat
      // cannot invent a new market event, inventory, cost model or validity.
      snapshot = next; return json(next);
    },
    publishCurrent(initial: JsonObject) {
      assert.equal(snapshot, null, 'This source publishes one original native frame; repeated GET never renews its evidence.');
      const publishedAtNs = nowNs(); const native = object(initial.snapshot);
      assert.ok(BigInt(String(initial.open_orders_available_ns)) <= BigInt(publishedAtNs));
      const availableResponses = (metadata.responses as JsonObject[]).filter(row => BigInt(String(row.received_ns)) <= BigInt(publishedAtNs));
      const response = availableResponses.at(-1)!; assert.ok(response);
      const bar = object((response.response as JsonObject[]).at(-1));
      const nativeBarEnd = ((BigInt(String(bar.T)) + 1n) * 1_000_000n).toString();
      const incoming = json(input.originalSnapshot); const wrapped = object(incoming.nativeObservation);
      incoming.observationId = native.event_id!; incoming.nativeClockNs = native.ts_event!; incoming.sequence = '2';
      wrapped.sequence = '2'; wrapped.observed_at_ns = native.ts_init!; wrapped.snapshot = native;
      incoming.openOrders = { schema_version: 1, account_id: initial.open_orders_account_id!, asof_ns: initial.open_orders_asof_ns!, available_ns: initial.open_orders_available_ns!, count: initial.open_orders_count! };
      incoming.execution = { currency, validUntilNs: registration.validUntilNs!, liquidity: [{ instrumentId, currency,
        eventNs: nativeBarEnd, availableNs: response.received_ns!, notional: registration.liquidityNotional!,
        lotNotional: multiplyDecimal(String(instrument.size_increment), String(bar.c)), minimumNotional: '0', takerFee: fees.taker! }],
        pmEvidence: { schemaVersion: 1, id: `paper-source:${modelRef}:2`, sourceKind: 'PAPER_EXECUTION_MODEL', sourceRevision: modelRef,
          accountSourceId: incoming.accountSourceId!, nativeAccountId: native.account_id!, observationId: incoming.observationId!, sequence: '2',
          controlRevision: incoming.controlRevision!, currency, asOfNs: native.ts_event!, availableAtNs: publishedAtNs,
          validUntilNs: registration.validUntilNs!, methodRef: modelRef, sourceEvidenceRef: modelRef,
          instruments: [{ instrumentId, rulesRef: registration.instrumentRulesRef!, costs: { ...object(registration.costs), evidenceRef: modelRef },
            entry: { ...object(registration.entry), evidenceRef: modelRef }, exit: { ...object(registration.exit), evidenceRef: modelRef } }] } };
      snapshot = incoming; return json(incoming);
    },
  };
}
