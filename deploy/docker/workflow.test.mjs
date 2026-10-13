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
const results = ['success', 'failure', 'cancelled', 'skipped'];

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
