import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { inventory, partitions, reconcilePartition, reconcileExecution, requireJobs, validatorSkips } from './web-ci.mjs';

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

test('CLI discovers and executes the complete independent capital inventory only in synthetic-b', () => {
  const temp = mkdtempSync(join(tmpdir(), 'quazonai-web-capital-ci-test-'));
  try {
    mkdirSync(join(temp, 'scripts'));
    mkdirSync(join(temp, 'node_modules/@playwright/test'), { recursive: true });
    cpSync(new URL('./web-ci.mjs', import.meta.url), join(temp, 'scripts/web-ci.mjs'));
    const auth = report(Object.values(partitions).flat());
    for (const spec of auth.suites[0].specs) spec.file = spec.title;
    const validators = report(validatorSkips.map((id) => JSON.parse(id)[2].at(-1)));
    validators.suites[0].suites = [{ title: 'production PWA validator precache', line: 1, column: 1, specs: validators.suites[0].specs }];
    delete validators.suites[0].specs;
    for (const spec of validators.suites[0].suites[0].specs) {
      spec.file = 'validator-loading.spec.ts';
      spec.tests[0].projectName = 'normal-dev-prebundle';
      spec.tests[0].expectedStatus = 'skipped';
    }
    // A fifth case stands for future additions; no count or title allowlist.
    const capital = report(['one', 'two', 'three', 'four', 'new case']);
    for (const spec of capital.suites[0].specs) spec.file = 'capital-exits.spec.ts';
    const fixture = {
      'playwright.auth.config.ts': auth,
      'playwright.validators.config.ts': validators,
      'playwright.capital-exits.config.ts': capital,
    };
    writeFileSync(join(temp, 'node_modules/@playwright/test/cli.js'), `
      const { appendFileSync, readFileSync, writeFileSync } = require('node:fs');
      const args = process.argv.slice(2);
      const config = args[args.indexOf('--config') + 1];
      const list = args.includes('--list');
      const files = args.filter(arg => arg.endsWith('.spec.ts'));
      const fixture = JSON.parse(readFileSync('fixture.json', 'utf8'));
      const value = fixture[config];
      appendFileSync('calls.jsonl', JSON.stringify({ config, list, files }) + '\\n');
      if (files.length) value.suites[0].specs = value.suites[0].specs.filter(spec => files.includes(spec.file));
      if (!list) {
        function execute(suite) {
          for (const spec of suite.specs ?? []) for (const test of spec.tests) {
            test.results = [{ status: test.expectedStatus, retry: 0, duration: 1 }];
            test.status = test.expectedStatus === 'skipped' ? 'skipped' : 'expected';
          }
          for (const child of suite.suites ?? []) execute(child);
        }
        value.suites.forEach(execute);
        if (config === 'playwright.capital-exits.config.ts' && fixture.execution) Object.assign(value, fixture.execution);
      }
      if (process.env.PLAYWRIGHT_JSON_OUTPUT_FILE) writeFileSync(process.env.PLAYWRIGHT_JSON_OUTPUT_FILE, JSON.stringify(value));
      else console.log(JSON.stringify(value));
    `);
    function run(command, value = fixture) {
      writeFileSync(join(temp, 'fixture.json'), JSON.stringify(value));
      writeFileSync(join(temp, 'calls.jsonl'), '');
      const result = spawnSync(process.execPath, [join(temp, 'scripts/web-ci.mjs'), command], { encoding: 'utf8' });
      const calls = readFileSync(join(temp, 'calls.jsonl'), 'utf8').trim().split('\n').map(line => JSON.parse(line));
      return { ...result, calls };
    }
    const config = 'playwright.capital-exits.config.ts';
    const discovered = run('inventory');
    assert.equal(discovered.status, 0, discovered.stderr);
    assert.match(discovered.stdout, /Discovered 5 capital-exit instances/);
    assert.deepEqual(discovered.calls.filter(call => call.config === config), [{ config, list: true, files: [] }]);
    const executed = run('synthetic-b');
    assert.equal(executed.status, 0, executed.stderr);
    assert.deepEqual(executed.calls.filter(call => call.config === config), [
      { config, list: true, files: [] }, { config, list: false, files: [] },
    ]);
    const summary = executed.stdout.split('\n').filter(line => line.startsWith('{')).map(line => JSON.parse(line)).find(row => row.suite === config);
    assert.equal(summary.browser_instances.length, 5);
    assert.ok(summary.browser_instances.every(row => row.status === 'passed'));
    const other = run('synthetic-a');
    assert.equal(other.status, 0, other.stderr);
    assert.equal(other.calls.filter(call => call.config === config).length, 0);
    const empty = run('inventory', { ...fixture, [config]: report([]) });
    assert.notEqual(empty.status, 0);
    assert.match(empty.stderr, /Empty browser inventory/);

    const passed = structuredClone(capital);
    for (const spec of passed.suites[0].specs) Object.assign(spec.tests[0], {
      status: 'expected', results: [{ status: 'passed', retry: 0, duration: 1 }],
    });
    const cases = [
      [value => value.suites[0].specs.pop(), /differs/],
      [value => { value.suites[0].specs = []; }, /Empty/],
      [value => value.suites[0].specs.push(value.suites[0].specs[0]), /Duplicate/],
      [value => { value.suites[0].specs[0].tests[0].results = []; }, /Missing or retried/],
      [value => value.suites[0].specs[0].tests[0].results.push({ status: 'passed', retry: 1 }), /Missing or retried/],
      [value => { value.suites[0].specs[0].tests[0].results[0].retry = 1; }, /Retried/],
      ...['failed', 'timedOut', 'interrupted', 'skipped'].map(status => [value => {
        value.suites[0].specs[0].tests[0].results[0].status = status;
      }, /did not passed/]),
    ];
    for (const [mutate, error] of cases) {
      const execution = structuredClone(passed);
      mutate(execution);
      const failed = run('synthetic-b', { ...fixture, execution });
      assert.notEqual(failed.status, 0);
      assert.match(failed.stderr, error);
    }
  } finally { rmSync(temp, { recursive: true, force: true }); }
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
