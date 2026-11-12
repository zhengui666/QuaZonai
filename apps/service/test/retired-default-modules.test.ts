import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { pathToFileURL } from 'node:url';
import { loadConfig } from '../src/config.ts';

test('old Docker module defaults retire without changing existing login, configuration or data files', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-modules-upgrade-'));
  try {
    const data = join(root, 'home/zzy/.local/share/quazonai/data');
    await mkdir(join(data, 'research'), { recursive: true });
    const path = join(data, 'service.json');
    const custom = join(data, 'owner/factor-wasm.ts');
    await mkdir(join(data, 'owner'));
    // A custom module with the retired basename is not an official default and must not disappear.
    await writeFile(custom, 'import "/opt/quazonai/apps/research/plugins/heldout-wasm.ts";\n');
    const value = { dataDir: data, passwordHash: 'test-existing-password-hash',
      defaultModel: { provider: 'test-provider', modelId: 'test-model' },
      research: { database: './research/research.sqlite', providerModules: ['./owner/provider.ts'], modules: [
        '/opt/quazonai/apps/research/plugins/research.ts', '/opt/quazonai/apps/research/plugins/factor-wasm.ts',
        '/opt/quazonai/apps/research/plugins/heldout-wasm.ts', custom,
      ] }, datasets: [], connections: [], extensions: [], secureCookies: true };
    const originals = new Map([
      [path, JSON.stringify(value)], [join(data, 'research/research.sqlite'), 'original database bytes'],
      [join(data, 'research/provider.env'), 'TEST_PROVIDER_TOKEN=private-test-value'],
      [join(data, 'owner/session.json'), '{"token":"existing-test-session"}'],
    ]);
    for (const [file, bytes] of originals) await writeFile(file, bytes);
    const result = await loadConfig(path);
    assert.deepEqual(result.research.modules, ['/opt/quazonai/apps/research/plugins/research.ts', custom]);
    assert.equal(result.research.database, join(data, 'research/research.sqlite'));
    assert.deepEqual(result.defaultModel, value.defaultModel);
    assert.equal(result.passwordHash, value.passwordHash);
    assert.equal(result.secureCookies, true);
    assert.deepEqual(result.research.providerModules, [join(data, 'owner/provider.ts')]);
    for (const [file, bytes] of originals) assert.equal(await readFile(file, 'utf8'), bytes);
    await assert.rejects(import(pathToFileURL(custom).href), /ERR_MODULE_NOT_FOUND|Cannot find module/);
  } finally { await rm(root, { recursive: true, force: true }); }
});
