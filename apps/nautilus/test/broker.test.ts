import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { createCheckpointBroker, BrokerRecoveryDriver } from '../src/broker.ts';
import type { RecoveryDriver } from '../src/checkpoint-driver.ts';

test('Unix broker exposes only fixed driver operations and retains original IDs and generations', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-broker-')); const calls: unknown[] = [];
  const driver: RecoveryDriver = { profile: async () => null, prepare: async input => { calls.push(['prepare', input]); },
    inspect: async id => { calls.push(['inspect', id]); return { phase: 'stopped' }; },
    checkpoint: async (requestId, generation) => ({ requestId, generation, profileId: 'p', createdAt: 'now', contentDigest: 'c' }),
    restore: async (id, generation) => { calls.push(['restore', id, generation]); },
    start: async id => { calls.push(['start', id]); }, cancel: async id => { calls.push(['cancel', id]); } };
  const broker = await createCheckpointBroker({ socketPath: join(root, 'broker.sock'), ownerDirectory: root, driver });
  try {
    const client = new BrokerRecoveryDriver(join(root, 'broker.sock')); const id = 'd'.repeat(64);
    assert.equal(await client.profile(), null); await client.prepare({ requestId: id });
    assert.equal((await client.inspect(id)).phase, 'stopped'); assert.equal((await client.checkpoint(id, 4)).generation, 4);
    await client.restore(id, 4); await client.start(id); await client.cancel(id);
    assert.deepEqual(calls, [['prepare', { requestId: id }], ['inspect', id], ['restore', id, 4], ['start', id], ['cancel', id]]);
    await assert.rejects(client.restore('../host-path', 0), /checkpoint_request_id_invalid/);
    await assert.rejects(client.checkpoint(id, -1), /checkpoint_generation_invalid/);
    await assert.rejects(createCheckpointBroker({ socketPath: join(root, 'second.sock'), ownerDirectory: root, driver }), /locked/);
  } finally { await broker.close(); await rm(root, { recursive: true, force: true }); }
});
