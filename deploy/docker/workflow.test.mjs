import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const workflow = await readFile(new URL('../../.github/workflows/release-version.yml', import.meta.url), 'utf8');
function condition(job) {
  const section = workflow.split(`\n  ${job}:\n`)[1]?.split(/\n  [a-z]+:\n/)[0];
  const value = section?.match(/^    if: \$\{\{ (.+) \}\}$/m)?.[1];
  assert.ok(value, `${job} must explicitly handle its deliberately skipped formal-regression ancestor`);
  assert.match(value, /!cancelled\(\)/, `${job} must override implicit success() without running after cancellation`);
  return Function('needs', 'cancelled', 'inputs', `return (${value});`);
}
test('native source integration workflows install and cache both exporter dependency trees', async () => {
  for (const name of ['polymarket-history.yml', 'rust-regression.yml']) {
    const source = await readFile(new URL(`../../.github/workflows/${name}`, import.meta.url), 'utf8');
    for (const app of ['service', 'research']) {
      assert.match(source, new RegExp(`apps/${app}/package-lock\\.json`), `${name}: cache ${app} lock`);
      assert.match(source, new RegExp(`npm ci --prefix apps/${app}\\b`), `${name}: install ${app} dependencies`);
    }
  }
});

const results = ['success', 'failure', 'cancelled', 'skipped'];

test('application carries the historical provider profile and existing downloader', async () => {
  const docker = await readFile(new URL('./Dockerfile', import.meta.url), 'utf8');
  const context = await readFile(new URL('./Dockerfile.dockerignore', import.meta.url), 'utf8');
  const application = docker.split('FROM scratch AS application-source\n')[1].split('\nFROM application-source AS nautilus-source')[0];
  for (const file of ['hub.py', 'sources/hyperliquid.json']) {
    assert.ok(application.includes(`runtimes/data/${file} /opt/quazonai/runtimes/data/${file}`));
  }
  assert.ok(context.includes('!runtimes/data/sources/\n!runtimes/data/sources/hyperliquid.json'));
});

test('only the remote native image installs the tick OS limiter and native payload', async () => {
  const docker = await readFile(new URL('./Dockerfile', import.meta.url), 'utf8');
  const native = docker.split('FROM execution-base AS nautilus\n')[1]?.split('\nFROM execution-base AS application')[0];
  const application = docker.split('FROM execution-base AS application\n')[1];
  assert.ok(native && application);
  assert.match(native, /apt-get install[^\n]*\butil-linux\b/);
  assert.match(native, /test -x \/usr\/bin\/prlimit/);
  assert.match(native, /install -m755 \/native\/job/);
  for (const name of ['job-high-precision', 'catalog-prepare-high-precision', 'market-history']) assert.ok(native.includes(`/native/${name} `));
  assert.match(native, /QZ_TICK_JOB_BINARY=\/opt\/quazonai\/bin\/job-high-precision/);
  assert.doesNotMatch(application, /\/native\/job|\butil-linux\b|\bprlimit\b|QZ_TICK_JOB_BINARY/);
});

test('science CI exports a new controlled high-precision fixture and runs connected native ticks before archiving both formats', async () => {
  const ci = await readFile(new URL('../../.github/workflows/ci.yml', import.meta.url), 'utf8');
  assert.ok(ci.includes('test ! -e "$tick_fixture"'));
  assert.match(ci, /QZ_TICK_NATIVE_FIXTURE_DIR="\$tick_fixture" cargo test[^\n]*--features market-history,catalog-prepare,hyperliquid-offline --lib ticks::/);
  assert.ok(ci.includes('test -s "$tick_fixture/prepared/catalog-metadata.json"'));
  assert.match(ci, /QZ_JOB_BINARY="\$RUNNER_TEMP\/native-artifacts\/job" QZ_TICK_JOB_BINARY="\$RUNNER_TEMP\/native-artifacts\/job-high-precision" QZ_TICK_NATIVE_FIXTURE_DIR="\$tick_fixture" node --experimental-strip-types --test apps\/service\/test\/tick-native\.test\.ts/);
  const archive = ci.split('\n').find(line => line.includes('tar -czf') && line.includes('native.tar.gz'));
  for (const name of ['job source-tools catalog-prepare browser_catalog_fixture', 'job-high-precision catalog-prepare-high-precision market-history']) assert.ok(archive?.includes(name));
});

test('formal releases still require full regression and cancelled builds never proceed', () => {
  const evaluate = condition('build');
  for (const regression of results) for (const branch of ['main', 'dev']) for (const cancelled of [false, true]) {
    assert.equal(evaluate({ regression: { result: regression } }, () => cancelled, { branch }),
      !cancelled && (branch === 'dev' || regression === 'success'), `${branch}/${regression}/cancelled=${cancelled}`);
  }
});

test('dev assets survive a skipped formal ancestor only when build and every CLI succeed', () => {
  const evaluate = condition('assets');
  for (const build of results) for (const cli of results) for (const cancelled of [false, true]) {
    const needs = { build: { result: build }, cli: { result: cli }, regression: { result: 'skipped' } };
    assert.equal(evaluate(needs, () => cancelled), !cancelled && build === 'success' && cli === 'success', `${build}/${cli}/cancelled=${cancelled}`);
  }
});

test('publish survives a skipped formal ancestor only after complete assets and never after cancellation', () => {
  const evaluate = condition('publish');
  for (const assets of results) for (const cancelled of [false, true]) {
    const needs = { assets: { result: assets }, regression: { result: 'skipped' } };
    assert.equal(evaluate(needs, () => cancelled), !cancelled && assets === 'success', `${assets}/cancelled=${cancelled}`);
  }
});
