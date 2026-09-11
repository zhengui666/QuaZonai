// Test the exact scalar contract emitted by utoipa, not a second numeric parser.
import fs from 'node:fs';
import assert from 'node:assert/strict';
const cases = JSON.parse(fs.readFileSync(new URL('./fraction-wire.json', import.meta.url)));
for (const file of ['domain-v1.openapi.json', 'api-v2.openapi.json']) {
  const document = JSON.parse(fs.readFileSync(new URL(`../../contracts/generated/${file}`, import.meta.url)));
  for (const component of ['EvaluationPolicyCreate', 'EvaluationPolicyView']) {
    const field = document.components.schemas[component].properties.maximum_missing_fraction;
    assert.equal(field.allOf.length, 2);
    for (const {input, valid} of cases) {
      const actual = typeof input === 'string' && field.allOf.every(part => part.type === 'string'
        && (part.minLength === undefined || input.length >= part.minLength)
        && (part.maxLength === undefined || input.length <= part.maxLength)
        && new RegExp(part.pattern).test(input));
      assert.equal(actual, valid, `${file}/${component}: ${JSON.stringify(input)}`);
    }
  }
}
console.log(`${cases.length} exact fraction wire cases passed for both documents and DTOs`);
