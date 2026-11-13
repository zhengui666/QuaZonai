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
  for (const name of ['native-source-tools.yml', 'rust-regression.yml']) {
    const source = await readFile(new URL(`../../.github/workflows/${name}`, import.meta.url), 'utf8');
    for (const app of ['service', 'research']) {
      assert.match(source, new RegExp(`apps/${app}/package-lock\\.json`), `${name}: cache ${app} lock`);
      assert.match(source, new RegExp(`npm ci --prefix apps/${app}\\b`), `${name}: install ${app} dependencies`);
    }
  }
});

const results = ['success', 'failure', 'cancelled', 'skipped'];

test('pushes build revision-bound native artifacts without running science CI', async () => {
  const ci = await readFile(new URL('../../.github/workflows/ci.yml', import.meta.url), 'utf8');
  const artifact = ci.split('\n  native-artifacts:\n')[1]?.split('\n  science-core:\n')[0];
  const science = ci.split('\n  science-core:\n')[1];
  assert.match(artifact, /if: github\.event_name == 'push'/);
  assert.match(artifact, /native-build\.sh "\$PROFILE" "\$RUNNER_TEMP\/native-artifacts"/);
  assert.match(artifact, /git rev-parse HEAD > "\$RUNNER_TEMP\/native-artifacts\/tested-revision"/);
  assert.match(artifact, /name: native-\$\{\{ env\.TEST_REVISION \}\}/);
  assert.match(science, /if: github\.event_name == 'workflow_dispatch'/);
  assert.doesNotMatch(artifact, /check-science/);
});

test('CryptoHFTData conversion stays in the remote native image', async () => {
  const docker = await readFile(new URL('./Dockerfile', import.meta.url), 'utf8');
  const context = await readFile(new URL('./Dockerfile.dockerignore', import.meta.url), 'utf8');
  const application = docker.split('FROM scratch AS application-source\n')[1].split('\nFROM application-source AS nautilus-source')[0];
  const native = docker.split('FROM application-source AS nautilus-source\n')[1].split('\nFROM node AS execution-base')[0];
  assert.doesNotMatch(application, /runtimes\/data\/cryptohftdata\.py/);
  for (const name of ['acquire.py', 'providers.py', 'snapshot.py']) assert.ok(application.includes(`runtimes/data/${name}`));
  assert.ok(native.includes('runtimes/data/cryptohftdata.py'));
  assert.deepEqual([...docker.matchAll(/runtimes\/data\/([a-z_]+\.py)/g)].map(match => match[1]).sort(),
    ['acquire.py', 'cryptohftdata.py', 'providers.py', 'snapshot.py', 'source_plugins.py']);
  assert.ok(context.includes('!runtimes/data/*.py'));
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

test('science CI exercises original high-precision data and upstream strategies before archiving both formats', async () => {
  const ci = await readFile(new URL('../../.github/workflows/ci.yml', import.meta.url), 'utf8');
  const make = await readFile(new URL('../../Makefile', import.meta.url), 'utf8');
  assert.match(ci, /cargo test[^\n]*--features market-history,catalog-prepare,hyperliquid-offline --lib ticks::/);
  for (const target of ['native_strategy', 'native_instrument_updates']) {
    assert.ok(ci.includes(`--test ${target}`));
    await readFile(new URL(`../../apps/job/tests/${target}.rs`, import.meta.url));
  }
  assert.doesNotMatch(ci, /--test native_walk_forward/);
  assert.doesNotMatch(ci, /QZ_TICK_NATIVE_FIXTURE_DIR|tick-native\.test\.ts|--test instrument_versions/);
  assert.match(ci, /run: make check-science PROFILE="\$PROFILE"/);
  assert.match(ci, /run: make check-core/);
  assert.match(make, /-p job --features \$\(SCIENCE_FEATURES\) --lib --tests -- --test-threads=1/);
  assert.ok(make.includes('npm --prefix apps/service run test:native'));
  assert.ok(make.includes('npm --prefix apps/nautilus test'));
  for (const path of ['remote-native.test.ts', 'remote-admission.test.ts', 'native-scan-recovery.test.ts']) {
    assert.ok(make.includes(`apps/research/test/${path}`));
    await readFile(new URL(`../../apps/research/test/${path}`, import.meta.url));
  }
  assert.doesNotMatch(make, /QZ_PM_(?:EXPORT|NATIVE|OBSERVED|FORWARD)_FIXTURE/);
  const archive = ci.split('\n').find(line => line.includes('tar -czf') && line.includes('native.tar.gz'));
  for (const name of ['job source-tools catalog-prepare', 'job-high-precision catalog-prepare-high-precision market-history']) assert.ok(archive?.includes(name));
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
