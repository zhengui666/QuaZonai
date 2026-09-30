import test from 'node:test';
import assert from 'node:assert/strict';
import { createValidatorLoader, ResponseValidatorLoadError } from './response-loader.mjs';
const descriptor = { module: 'schema-a', name: 'validate' };
function validator(value) { validator.errors = value ? null : [{ instancePath: '/value', params: { allowed: [1, 2] } }]; return !!value; }
test('concurrent calls share one load and snapshot full mutable errors in the validation continuation', async () => {
  let calls = 0; let release; const gate = new Promise(resolve => { release = resolve; });
  const validate = createValidatorLoader({ 'schema-a': async () => { calls++; await gate; return { default: { validate: validator } }; } });
  const invalid = validate(descriptor, false); const valid = validate(descriptor, true); release();
  const [a, b] = await Promise.all([invalid, valid]); assert.equal(calls, 1); assert.equal(a.valid, false); assert.equal(b.valid, true);
  assert.deepEqual(a.errors, [{ instancePath: '/value', params: { allowed: [1, 2] } }]); assert.equal(b.errors, null); assert.equal(validator.errors, null);
  validator(false); validator.errors[0].params.allowed.push(3); assert.deepEqual(a.errors[0].params.allowed, [1, 2]);
});
test('transport failure rejects distinctly, clears only rejected promise, and later explicit calls share a retry', async () => {
  let calls = 0;
  const validate = createValidatorLoader({ 'schema-a': async () => { calls++; if (calls === 1) throw new TypeError('download failed'); return { default: { validate: validator } }; } });
  const result = await Promise.allSettled([validate(descriptor, true), validate(descriptor, true)]);
  assert.equal(calls, 1); for (const entry of result) { assert.equal(entry.status, 'rejected'); assert.ok(entry.reason instanceof ResponseValidatorLoadError); assert.equal(entry.reason.moduleId, descriptor.module); }
  assert.deepEqual(await Promise.all([validate(descriptor, true), validate(descriptor, false)]).then(values => values.map(value => value.valid)), [true, false]); assert.equal(calls, 2);
});
test('persistent module-evaluation failure stays failed on explicit retry without cache busting', async () => {
  let calls = 0; const failure = new SyntaxError('bad evaluated module');
  const validate = createValidatorLoader({ 'schema-a': async () => { calls++; throw failure; } });
  for (let attempt = 0; attempt < 2; attempt++) await assert.rejects(validate(descriptor, true), error => error instanceof ResponseValidatorLoadError && error.cause === failure);
  assert.equal(calls, 2);
});
test('unknown modules, missing default and missing native binding never pass validation', async () => {
  await assert.rejects(createValidatorLoader({})(descriptor, true), ResponseValidatorLoadError);
  for (const namespace of [{ validate: validator }, { default: {} }]) await assert.rejects(createValidatorLoader({ 'schema-a': () => namespace })(descriptor, true), ResponseValidatorLoadError);
});
