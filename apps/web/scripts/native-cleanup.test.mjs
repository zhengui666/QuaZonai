// Real PostgreSQL DDL + lost acknowledgement/SIGTERM; not a mocked database.
// Run only with the same explicit disposable loopback admin as native-browser.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const input = process.env.QUAZONAI_WEB_TEST_ADMIN_URL;
if (!input) throw new Error('Native cleanup regression requires the disposable PostgreSQL administrator');
const admin = new URL(input);
if (!['postgres:', 'postgresql:'].includes(admin.protocol) || !['127.0.0.1', 'localhost', '[::1]'].includes(admin.hostname)
  || !admin.port || admin.pathname !== '/postgres' || admin.search || admin.hash || !admin.username || !admin.password) {
  throw new Error('Native cleanup regression requires an explicit loopback PostgreSQL /postgres URL');
}
const psql = process.env.QUAZONAI_WEB_TEST_PSQL ?? 'psql';
const env = Object.fromEntries(['PATH', 'HOME', 'USER', 'LOGNAME', 'TMPDIR', 'LANG', 'LC_ALL', 'LD_LIBRARY_PATH']
  .filter(key => process.env[key] !== undefined).map(key => [key, process.env[key]]));
const pg = { ...env, PGHOST: admin.hostname.replace(/^\[|\]$/g, ''), PGPORT: admin.port,
  PGUSER: decodeURIComponent(admin.username), PGPASSWORD: decodeURIComponent(admin.password), PGDATABASE: 'postgres',
  PGCONNECT_TIMEOUT: '5', PGSSLMODE: 'disable', PGOPTIONS: '-c statement_timeout=15000 -c lock_timeout=5000' };
function sql(text) {
  const result = spawnSync(psql, ['--no-psqlrc', '--set', 'ON_ERROR_STOP=1', '--tuples-only', '--no-align', '--quiet'],
    { env: pg, input: text, encoding: 'utf8', timeout: 30000, maxBuffer: 1024 * 1024 });
  assert.equal(result.status, 0, 'Native PostgreSQL query must complete, not be assumed successful');
  return result.stdout.trim();
}
for (const fault of ['ROLE', 'DATABASE']) for (const signal of [false, true]) {
  test(`committed CREATE ${fault} is cleaned after ${signal ? 'SIGTERM' : 'lost ACK'}`, { timeout: 60000 }, async () => {
    const directory = await fs.mkdtemp(join(tmpdir(), 'quazonai-native-cleanup-'));
    const report = join(directory, 'report'); const eventsFile = join(directory, 'events.jsonl');
    const wrapper = join(directory, 'psql-wrapper.mjs');
    let created = [];
    try {
      await fs.copyFile(new URL('../tests/psql-ack-fixture.mjs', import.meta.url), wrapper);
      await fs.chmod(wrapper, 0o700);
      await fs.writeFile(join(directory, 'settings.json'), JSON.stringify({ psql, events: eventsFile, fault, signal }), { mode: 0o600 });
      await fs.writeFile(eventsFile, '', { mode: 0o600 });
      const result = spawnSync(process.execPath, [fileURLToPath(new URL('./native-browser.mjs', import.meta.url))], {
        env: { ...env, QUAZONAI_WEB_TEST_ADMIN_URL: input, QUAZONAI_WEB_TEST_PSQL: wrapper, QUAZONAI_WEB_TEST_REPORT_DIR: report },
        encoding: 'utf8', timeout: 45000, maxBuffer: 1024 * 1024,
      });
      created = (await fs.readFile(eventsFile, 'utf8')).trim().split('\n').filter(Boolean).map(line => JSON.parse(line));
      assert.ok(created.some(entry => entry.kind === fault && entry.committed), 'The injected failure must occur AFTER real DDL committed');
      assert.ok(result.status === 1 || result.status === 143, 'The harness must report failure, never false acceptance');
      const outcome = JSON.parse(await fs.readFile(join(report, 'result.json'), 'utf8'));
      assert.equal(outcome.status, 'FAILED');
      for (const entry of created) {
        assert.match(entry.name, /^web_(?:app|e2e)_[0-9a-f]{24}$/);
        const table = entry.kind === 'ROLE' ? 'pg_roles' : 'pg_database';
        const column = entry.kind === 'ROLE' ? 'rolname' : 'datname';
        assert.equal(sql(`SELECT count(*) FROM ${table} WHERE ${column}='${entry.name}';`), '0', 'No test-owned native resource may leak');
        const cleanup = entry.kind === 'ROLE' ? 'drop-owned-role' : 'drop-owned-database';
        assert.ok(outcome.stages.some(stage => stage.name === cleanup && stage.exit_code === 0), 'Cleanup must actually execute');
      }
    } finally {
      // A failed regression must not itself leak its generated, confirmed objects.
      let failure;
      for (const entry of [...created].reverse()) {
        if (!/^web_(?:app|e2e)_[0-9a-f]{24}$/.test(entry.name)) continue;
        try { sql(entry.kind === 'DATABASE' ? `DROP DATABASE IF EXISTS "${entry.name}" WITH (FORCE);` : `DROP ROLE IF EXISTS "${entry.name}";`); }
        catch (error) { failure ??= error; }
      }
      await fs.rm(directory, { recursive: true, force: true });
      if (failure) throw failure;
    }
  });
}
