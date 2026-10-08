import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';

const dockerfile = fs.readFileSync(new URL('./native-job.Dockerfile', import.meta.url), 'utf8');
const labels = new Map([...dockerfile.matchAll(/\b(io\.quazonai\.[\w.-]+)="([^"]*)"/g)]
  .map(([, name, value]) => [name, value]));

test('nullable resource quotas have a separate native image capability label', () => {
  assert.equal(labels.get('io.quazonai.native-job'), '1');
  assert.equal(labels.get('io.quazonai.optional-resource-quotas'), '1');
  assert.equal([...dockerfile.matchAll(/io\.quazonai\.optional-resource-quotas=/g)].length, 1);
});

test('optional quota label preserves the existing finite-job native stack contract', () => {
  const engine = fs.readFileSync(new URL('../../apps/runtime/src/engine.rs', import.meta.url), 'utf8');
  const nativeStack = engine.match(/pub const NATIVE_STACK: &str = "([^"]+)";/)?.[1];
  assert.ok(nativeStack, 'the runtime must declare its native stack contract');
  assert.equal(labels.get('io.quazonai.native-stack'), nativeStack);
  assert.ok(nativeStack.split(';').includes('optional-execution-budgets/1'));
  for (const optionalQuota of ['optional-resource-quotas', 'optional-cpu-rate', 'optional-memory-limit']) {
    assert.ok(!nativeStack.includes(optionalQuota), 'new optional quotas must not reject finite old images');
  }
});
