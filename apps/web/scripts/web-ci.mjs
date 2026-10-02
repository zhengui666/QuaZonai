// Web CI partition and execution accounting. Discovery always uses the full
// current configs; passing an empty/partial shard can never stand in for a suite.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const partitions = {
  'synthetic-a': ['settings-autosave.spec.ts'],
  'synthetic-b': ['chatgpt-auth.spec.ts', 'console-loading.spec.ts', 'data-dialog-recovery.spec.ts', 'data-inputs.spec.ts'],
};
const authConfig = 'playwright.auth.config.ts';
const validatorConfig = 'playwright.validators.config.ts';
const web = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const cli = resolve(web, 'node_modules/@playwright/test/cli.js');
const key = (project, file, titles) => JSON.stringify([project, file, titles]);
export const validatorSkips = [
  'a failed validator download cannot activate a partially precached application',
  'a delayed static validator delays installation, then every lazy validator is cached',
].map((title) => key('normal-dev-prebundle', 'validator-loading.spec.ts', ['production PWA validator precache', title]));

export function inventory(report) {
  assert.deepEqual(report.errors, [], 'Playwright discovery/execution reported errors');
  assert.ok(Array.isArray(report.suites), 'Missing Playwright suites');
  const entries = new Map();
  function visit(suite, parents = []) {
    // The root suite is the filename, not a describe() title.
    const titles = suite.column === 0 && suite.line === 0 ? parents : [...parents, suite.title];
    for (const spec of suite.specs ?? []) for (const test of spec.tests) {
      const id = key(test.projectName, spec.file, [...titles, spec.title]);
      assert.ok(!entries.has(id), `Duplicate browser instance: ${id}`);
      entries.set(id, test);
    }
    for (const child of suite.suites ?? []) visit(child, titles);
  }
  for (const suite of report.suites) visit(suite);
  assert.ok(entries.size > 0, 'Empty browser inventory');
  return entries;
}

export function reconcilePartition(complete, groups) {
  const all = inventory(complete);
  const selected = new Set();
  for (const group of groups) for (const id of inventory(group).keys()) {
    assert.ok(all.has(id), `Unknown selected browser instance: ${id}`);
    assert.ok(!selected.has(id), `Overlapping browser groups: ${id}`);
    selected.add(id);
  }
  assert.deepEqual([...selected].sort(), [...all.keys()].sort(), 'Browser partition omits current tests');
  return all.size;
}

export function reconcileExecution(discovery, execution, allowedSkips = []) {
  const expected = inventory(discovery);
  const actual = inventory(execution);
  assert.deepEqual([...actual.keys()].sort(), [...expected.keys()].sort(), 'Executed browser inventory differs from discovery');
  const skipped = new Set(allowedSkips);
  assert.equal(skipped.size, allowedSkips.length, 'Duplicate allowed skips');
  for (const id of skipped) assert.ok(expected.has(id), `Intentional skip missing from discovery: ${id}`);
  const results = [];
  for (const [id, test] of actual) {
    assert.equal(test.results.length, 1, `Missing or retried browser instance: ${id}`);
    const [result] = test.results;
    assert.equal(result.retry, 0, `Retried browser instance: ${id}`);
    const status = skipped.has(id) ? 'skipped' : 'passed';
    assert.equal(result.status, status, `Browser instance did not ${status}: ${id}`);
    assert.equal(test.expectedStatus, status, `Unexpected browser expectation: ${id}`);
    assert.equal(test.status, status === 'passed' ? 'expected' : 'skipped', `Unexpected browser outcome: ${id}`);
    results.push({ id: JSON.parse(id), status, duration_ms: result.duration });
  }
  return results;
}

export function requireJobs(needs) {
  const required = ['source', 'synthetic-a', 'synthetic-b', 'native'];
  assert.deepEqual(Object.keys(needs).sort(), [...required].sort(), 'Missing/unexpected Web job');
  for (const job of required) assert.equal(needs[job].result, 'success', `Required Web job ${job} did not succeed`);
}

function playwright(args, options = {}) {
  const result = spawnSync(process.execPath, [cli, 'test', ...args], {
    cwd: web, env: { ...process.env, PLAYWRIGHT_JSON_OUTPUT_NAME: '', PLAYWRIGHT_JSON_OUTPUT_DIR: '', PLAYWRIGHT_JSON_OUTPUT_FILE: '' },
    encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, ...options,
  });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, 'Playwright command failed');
  return result.stdout;
}

function discover(config, files = []) {
  return JSON.parse(playwright(['--config', config, ...files, '--list', '--reporter=json']));
}

function execute(config, files, discovery, skips = []) {
  const temp = mkdtempSync(join(tmpdir(), 'quazonai-web-ci-'));
  try {
    const output = join(temp, 'execution.json');
    playwright(['--config', config, ...files, '--reporter=list,json'], {
      stdio: 'inherit', env: { ...process.env, PLAYWRIGHT_JSON_OUTPUT_FILE: output },
    });
    const results = reconcileExecution(discovery, JSON.parse(readFileSync(output, 'utf8')), skips);
    console.log(`Reconciled ${results.length} ${config} instances (${skips.length} existing intentional skips)`);
    // Only source identities/status/timings enter the hosted log, never raw
    // Playwright errors, HTTP data, fixture state or authentication material.
    console.log(JSON.stringify({ suite: config, browser_instances: results }));
  } finally { rmSync(temp, { recursive: true, force: true }); }
}

function main(command) {
  if (command === 'gate') return requireJobs(JSON.parse(process.env.WEB_JOB_RESULTS));
  assert.ok(command === 'inventory' || command in partitions, 'Unknown Web CI command');
  const complete = discover(authConfig);
  const groups = Object.fromEntries(Object.entries(partitions).map(([name, files]) => [name, discover(authConfig, files)]));
  console.log(`Reconciled ${reconcilePartition(complete, Object.values(groups))} synthetic browser instances`);
  if (command !== 'inventory') execute(authConfig, partitions[command], groups[command]);
  if (command === 'inventory' || command === 'synthetic-b') {
    const validators = discover(validatorConfig);
    const found = inventory(validators);
    for (const id of validatorSkips) assert.ok(found.has(id), `Intentional validator skip disappeared: ${id}`);
    console.log(`Discovered ${found.size} validator instances; ${validatorSkips.length} explicit dev PWA skips`);
    if (command === 'synthetic-b') execute(validatorConfig, [], validators, validatorSkips);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main(process.argv[2]);
