import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { importRustLauncher } from '../src/rust-launcher-import.ts';
const caps = { schemaVersion: 1, protocol: 'quazonai-native-process-v1', identity: { nautilusSource: 'registry+https://github.com/rust-lang/crates.io-index', nautilusVersion: '0.69.0', precision: 'standard' as const, jobVersion: 'test' }, research: { singleStrategyRegistration: false, configuredRunFactory: true }, account: { compiled: false, registration: false, command: 'account-runtime', requiresExplicitStart: true }, running: false };

test('explicit import copies immutable independent artifact, only invokes capabilities, and preserves prior output', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-import-test-'));
  try {
    const source = join(root, 'owner'); const marker = join(root, 'calls');
    const text = `#!/usr/bin/env node\nrequire('fs').appendFileSync(${JSON.stringify(marker)},JSON.stringify(process.argv.slice(2))+'\\n');console.log(${JSON.stringify(JSON.stringify(caps))});`;
    await writeFile(source, text, { mode: 0o600 });
    assert.equal(await readFile(source, 'utf8'), text);
    const result = await importRustLauncher({ sourceExecutable: source, directory: join(root, 'artifact'), expectedIdentity: caps.identity });
    assert.deepEqual(result.capabilities, caps); assert.equal(result.qualification, 'IMPORTED_NOT_CONNECTED');
    assert.equal(await readFile(marker, 'utf8'), '["capabilities"]\n');
    assert.equal(await readFile(source, 'utf8'), text);
    await writeFile(source, 'changed owner');
    assert.equal(await readFile(result.executable, 'utf8'), text);
    await assert.rejects(importRustLauncher({ sourceExecutable: source, directory: join(root, 'artifact') }), /EEXIST/);
    assert.equal(await readFile(result.executable, 'utf8'), text);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('capability and expected identity mismatch retain diagnostic output, never a usable result', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-import-invalid-'));
  try {
    const source = join(root, 'owner');
    for (const [index, value] of [ { ...caps, running: true }, { ...caps, identity: {} }, { ...caps, identity: { ...caps.identity, nautilusRevision: 123 } }, { ...caps, account: { ...caps.account, registration: true } } ].entries()) {
      await writeFile(source, `#!/usr/bin/env node\nconsole.log(${JSON.stringify(JSON.stringify(value))});`);
      const directory = join(root, `invalid${index}`);
      await assert.rejects(importRustLauncher({ sourceExecutable: source, directory }), /capabilities_invalid/);
      assert.deepEqual(JSON.parse(await readFile(join(directory, 'capabilities.stdout'), 'utf8')), value);
    }
    await writeFile(source, `#!/usr/bin/env node\nconsole.log(${JSON.stringify(JSON.stringify(caps))});`);
    await assert.rejects(importRustLauncher({ sourceExecutable: source, directory: join(root, 'identity'), expectedIdentity: { ...caps.identity, precision: 'high' } }), /identity_mismatch/);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('actual loader failure is visible and interrupted inspection remains unknown', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-import-fail-'));
  try {
    const source = join(root, 'owner');
    await writeFile(source, '#!/nonexistent/qz-test-loader\n');
    await assert.rejects(importRustLauncher({ sourceExecutable: source, directory: join(root, 'loader') }), /ENOENT/);
    assert.match(await readFile(join(root, 'loader/capabilities.stderr'), 'utf8'), /ENOENT/);
    await writeFile(source, '#!/usr/bin/env node\nconsole.error("inspection started");setInterval(()=>{},1000);');
    const control = new AbortController();
    const pending = importRustLauncher({ sourceExecutable: source, directory: join(root, 'interrupted'), signal: control.signal });
    const rejected = assert.rejects(pending, /interrupted_outcome_unknown/);
    setTimeout(() => control.abort(), 150); await rejected;
    assert.match(await readFile(join(root, 'interrupted/capabilities.stderr'), 'utf8'), /aborted/);
  } finally { await rm(root, { recursive: true, force: true }); }
});

// A capabilities invocation is trusted code, but need not inherit service secrets.
test('inspection passes only process lookup and isolated temporary directory, not service credentials', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-import-env-'));
  const old = process.env.QZ_IMPORT_TEST_SECRET; process.env.QZ_IMPORT_TEST_SECRET = 'must-not-inherit';
  try {
    const source = join(root, 'owner');
    await writeFile(source, `#!/usr/bin/env node\nif(process.env.QZ_IMPORT_TEST_SECRET || process.env.HOME || process.env.NODE_OPTIONS)process.exit(39);if(process.env.TMPDIR!==process.cwd())process.exit(40);console.log(${JSON.stringify(JSON.stringify(caps))});`);
    const result = await importRustLauncher({ sourceExecutable: source, directory: join(root, 'artifact') });
    assert.equal(result.qualification, 'IMPORTED_NOT_CONNECTED');
  } finally {
    if (old === undefined) delete process.env.QZ_IMPORT_TEST_SECRET; else process.env.QZ_IMPORT_TEST_SECRET = old;
    await rm(root, { recursive: true, force: true });
  }
});
