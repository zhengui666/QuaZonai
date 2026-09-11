// Native JavaScript RegExp validates the same shared corpus as Rust BigDecimal.
// No second decimal parser, generated schema rewrite, or npm dependency.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const load = (path) => JSON.parse(readFileSync(new URL(path, import.meta.url), 'utf8'));
const schema = load('../../contracts/generated/domain-v1.openapi.json').components.schemas.DecimalValue;
const cases = load('./decimal-wire.json');
assert.equal(schema.type, 'string');
assert.equal(schema.minLength, 1);
assert.equal(schema.maxLength, 64);
assert.equal(typeof schema.pattern, 'string');
const pattern = new RegExp(schema.pattern);
for (const { input, valid } of cases) {
  const accepted = typeof input === 'string' && input.length >= schema.minLength
    && input.length <= schema.maxLength && pattern.test(input);
  assert.equal(accepted, valid, `Generated decimal schema disagrees for ${JSON.stringify(input)}`);
}
console.log(`Generated decimal schema: ${cases.length} shared native-parser cases passed.`);
