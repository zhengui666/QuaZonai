import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import { ModelCredentials } from '../src/credentials.ts';

test('private credential store serializes refresh/delete, rolls back and survives restart', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-credential-test-')); const directory = join(root, 'auth');
  let store = await ModelCredentials.open(directory);
  try {
    assert.equal((await stat(directory)).mode & 0o777, 0o700);
    assert.equal((await stat(join(directory, 'credentials.sqlite'))).mode & 0o777, 0o600);
    assert.equal(await store.read('test'), undefined);
    await store.modify('test', async () => ({ type: 'api_key', key: 'synthetic-test-key' }));
    const order: number[] = [];
    await Promise.all([1, 2, 3].map(index => store.modify('test', async previous => {
      order.push(index); await new Promise(resolve => setImmediate(resolve));
      assert.equal(previous?.type, 'api_key');
      return { type: 'api_key', key: `${previous?.type === 'api_key' ? previous.key : ''}-${index}` };
    })));
    assert.deepEqual(order, [1, 2, 3]);
    await assert.rejects(store.modify('test', async () => { throw new Error('controlled rollback'); }), /controlled rollback/);
    assert.deepEqual(await store.list(), [{ providerId: 'test', type: 'api_key' }]);
    await store.close(); store = await ModelCredentials.open(directory);
    assert.deepEqual(await store.read('test'), { type: 'api_key', key: 'synthetic-test-key-1-2-3' });
    await Promise.all([store.modify('test', async () => ({ type: 'api_key', key: 'replacement' })), store.delete('test')]);
    assert.equal(await store.read('test'), undefined);
    assert.deepEqual(await store.list(), []);
  } finally { await store.close(); await rm(root, { recursive: true, force: true }); }
});

test('cancelled official login cannot persist a queued credential; started refresh still commits', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-credential-abort-')); const store = await ModelCredentials.open(root);
  let release!: () => void; const gate = new Promise<void>(resolve => { release = resolve; });
  let enter!: () => void; const entered = new Promise<void>(resolve => { enter = resolve; });
  try {
    const blocker = store.modify('other-provider', async () => { enter(); await gate; return undefined; }); await entered;
    const faux = fauxProvider({ provider: 'cancelled-login' });
    const models = createModels({ credentials: store });
    models.setProvider({ ...faux.provider, auth: { apiKey: { name: 'Synthetic login', async login() { return { type: 'api_key', key: 'synthetic-queued-only' }; }, async resolve() { return undefined; } } } });
    const controller = new AbortController();
    const login = models.login(faux.provider.id, 'api_key', { signal: controller.signal, async prompt() { return ''; }, notify() {} });
    const rejected = assert.rejects(login);
    await new Promise(resolve => setImmediate(resolve)); controller.abort(); await rejected;
    release(); await blocker;
    assert.equal(await store.read(faux.provider.id), undefined);
    const refreshController = new AbortController();
    const refresh = store.modify('started-refresh', async () => { refreshController.abort(); return { type: 'api_key', key: 'synthetic-started-only' }; }, { signal: refreshController.signal });
    await refresh;
    assert.deepEqual(await store.read('started-refresh'), { type: 'api_key', key: 'synthetic-started-only' });
  } finally { release(); await store.close(); await rm(root, { recursive: true, force: true }); }
});
