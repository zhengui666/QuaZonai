// Generated schema constraints, exercised by the native ECMAScript engine.
// Rust validates this exact corpus against DbCounter/Revision in wire.rs.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const load = (path) => JSON.parse(readFileSync(new URL(path, import.meta.url), 'utf8'));
const schemas = load('../../contracts/generated/domain-v1.openapi.json').components.schemas;
const cases = load('./bigint-wire.json');
for (const { schema: name, input, valid } of cases) {
  const schema = schemas[name];
  assert.equal(schema.type, 'string');
  assert.equal(schema.minLength, 1);
  assert.equal(schema.maxLength, 19);
  const accepted = typeof input === 'string' && input.length >= schema.minLength
    && input.length <= schema.maxLength && new RegExp(schema.pattern).test(input);
  assert.equal(accepted, valid, `${name}: ${JSON.stringify(input)}`);
}
console.log(`Generated bigint schemas: ${cases.length} native-parser cases passed.`);
