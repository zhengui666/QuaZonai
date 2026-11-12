import type { ConnectionConfig } from '../src/config.ts';
import { Application, json } from '../src/facts.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';
import { setup, context } from './support.ts';

// Only historical protocol input. No new admission, delivery or financial result.
export async function retainTarget(state: Awaited<ReturnType<typeof setup>>, connection: ConnectionConfig, portfolioId: string, status: 'ACKNOWLEDGED' | 'UNCERTAIN' = 'UNCERTAIN') {
  const fact = accountSourceFixture().observation;
  const id = `retained-${connection.id}`;
  fact.connection = structuredClone(connection);
  fact.input = { requestId: id, portfolioId, mode: connection.mode, connectionId: connection.id };
  fact.taskId = 0; fact.snapshot = null; fact.target = {};
  Object.assign(fact.view, { id, name: connection.name, portfolioId, mode: connection.mode, currency: 'USD', status,
    receipt: { id: `receipt-${id}`, status, expiresAt: '2030-01-01T00:00:00.000000000Z', externalId: null, acknowledgedAt: null } });
  await state.service.research.harness.commit(async tx => { (await tx.doc(Application)).observations[id] = json(fact); }, context);
  return { id: fact.view.receipt!.id, clientId: connection.clientId, accountId: connection.accountId, expiresAt: fact.view.receipt!.expiresAt, observationId: id };
}
