import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { inventory, reconcilePartition, reconcileExecution, requireJobs } from './web-ci.mjs';

function report(titles, execution = false) {
  return { errors: [], suites: [{ title: 'sample.spec.ts', file: 'sample.spec.ts', line: 0, column: 0,
    specs: titles.map((title) => ({ title, file: 'sample.spec.ts', tests: [{ projectName: 'browser',
      expectedStatus: 'passed', status: execution ? 'expected' : 'skipped',
      results: execution ? [{ status: 'passed', retry: 0, duration: 1 }] : [] }] })) }] };
}

test('partition exactly covers all current discovered instances', () => {
  assert.equal(reconcilePartition(report(['one', 'two']), [report(['one']), report(['two'])]), 2);
  assert.throws(() => reconcilePartition(report(['one', 'two']), [report(['one'])]), /omits/);
  assert.throws(() => reconcilePartition(report(['one']), [report(['one']), report(['one'])]), /Overlapping/);
  assert.throws(() => reconcilePartition(report(['one']), [report(['extra'])]), /Unknown/);
  assert.throws(() => reconcilePartition(report(['one']), [report([])]), /Empty/);
  assert.throws(() => inventory(report(['one', 'one'])), /Duplicate/);
});

test('project and full describe title distinguish instances', () => {
  const value = report(['one']);
  value.suites[0].suites = [{ title: 'nested', line: 12, column: 1, specs: value.suites[0].specs }];
  value.suites[0].specs[0].tests.push({ ...value.suites[0].specs[0].tests[0], projectName: 'other' });
  assert.equal(inventory(value).size, 4);
});

test('execution requires every instance exactly once and passing', () => {
  assert.equal(reconcileExecution(report(['one']), report(['one'], true)).length, 1);
  assert.throws(() => reconcileExecution(report(['one', 'two']), report(['one'], true)), /differs/);
  assert.throws(() => reconcileExecution(report(['one']), report(['one'])), /Missing or retried/);
  for (const status of ['failed', 'timedOut', 'interrupted', 'skipped']) {
    const value = report(['one'], true);
    value.suites[0].specs[0].tests[0].results[0].status = status;
    assert.throws(() => reconcileExecution(report(['one']), value), /did not passed/);
  }
  const retry = report(['one'], true);
  retry.suites[0].specs[0].tests[0].results[0].retry = 1;
  assert.throws(() => reconcileExecution(report(['one']), retry), /Retried/);
});

test('only named intentional skips are accepted, and they must exist', () => {
  const value = report(['one'], true);
  const row = value.suites[0].specs[0].tests[0];
  row.expectedStatus = row.status = row.results[0].status = 'skipped';
  const id = [...inventory(value).keys()][0];
  assert.equal(reconcileExecution(report(['one']), value, [id])[0].status, 'skipped');
  assert.throws(() => reconcileExecution(report(['one']), value), /did not passed/);
  assert.throws(() => reconcileExecution(report(['one']), report(['one'], true), [id]), /did not skipped/);
  assert.throws(() => reconcileExecution(report(['one']), value, ['absent']), /missing from discovery/);
  assert.throws(() => reconcileExecution(report(['one']), value, [id, id]), /Duplicate/);
});

test('required-name aggregate fails closed on every non-success and missing job', () => {
  const all = Object.fromEntries(['source', 'synthetic-a', 'synthetic-b', 'native'].map((job) => [job, { result: 'success' }]));
  requireJobs(all);
  for (const job of Object.keys(all)) for (const result of ['failure', 'cancelled', 'skipped', undefined]) {
    assert.throws(() => requireJobs({ ...all, [job]: { result } }), /did not succeed/);
  }
  const { native, ...partial } = all;
  assert.throws(() => requireJobs(partial), /Missing/);
  assert.throws(() => requireJobs({ ...all, extra: { result: 'success' } }), /unexpected/);
});

test('installed Playwright discovery and JSON results reconcile without launching a browser', () => {
  const temp = mkdtempSync(join(tmpdir(), 'quazonai-web-reporter-test-'));
  try {
    const api = new URL('../node_modules/@playwright/test/index.mjs', import.meta.url).href;
    const cli = fileURLToPath(new URL('../node_modules/@playwright/test/cli.js', import.meta.url));
    writeFileSync(join(temp, 'playwright.config.mjs'), `export default { testDir: ${JSON.stringify(temp)}, workers: 1, retries: 0, outputDir: ${JSON.stringify(join(temp, 'results'))} };`);
    writeFileSync(join(temp, 'reporter.spec.mjs'), `import { test, expect } from ${JSON.stringify(api)};\ntest.describe('nested', () => { test('passes', () => expect(1).toBe(1)); test('intentional skip', () => test.skip(true, 'fixture')); });`);
    function run(list) {
      const result = spawnSync(process.execPath, [cli, 'test', '--config', join(temp, 'playwright.config.mjs'), '--reporter=json', ...(list ? ['--list'] : [])], {
        encoding: 'utf8', env: { ...process.env, PLAYWRIGHT_JSON_OUTPUT_FILE: '', PLAYWRIGHT_JSON_OUTPUT_DIR: '', PLAYWRIGHT_JSON_OUTPUT_NAME: '' },
      });
      assert.equal(result.status, 0, result.stderr);
      return JSON.parse(result.stdout);
    }
    const discovered = run(true);
    const skip = [...inventory(discovered).keys()].find((id) => id.includes('intentional skip'));
    const result = reconcileExecution(discovered, run(false), [skip]);
    assert.equal(result.filter((row) => row.status === 'passed').length, 1);
    assert.equal(result.filter((row) => row.status === 'skipped').length, 1);
  } finally { rmSync(temp, { recursive: true, force: true }); }
});
