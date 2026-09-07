// Generated schema constraints, exercised by the native ECMAScript engine.
// Rust validates the scalar corpus in wire.rs and authoring combinations in
// domain/tests/brief_budget_boundaries.rs. This is not a JSON Schema validator.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const load = (path) => JSON.parse(readFileSync(new URL(path, import.meta.url), 'utf8'));
const cases = load('./bigint-wire.json');
const positiveCases = cases.filter(({ schema }) => schema === 'Revision');
assert.ok(positiveCases.some(({ input, valid }) => input === '0' && !valid));
assert.ok(positiveCases.some(({ input, valid }) => input === '9223372036854775807' && valid));
assert.ok(positiveCases.some(({ input, valid }) => input === '9223372036854775808' && !valid));

let checked = 0;
function checkBigint(schema, name, corpus) {
  assert.ok(schema, `${name}: missing schema`);
  assert.equal(schema.type, 'string', name);
  assert.equal(schema.minLength, 1, name);
  assert.equal(schema.maxLength, 19, name);
  assert.equal(typeof schema.pattern, 'string', name);
  const pattern = new RegExp(schema.pattern);
  for (const { input, valid } of corpus) {
    const accepted = typeof input === 'string' && input.length >= schema.minLength
      && input.length <= schema.maxLength && pattern.test(input);
    assert.equal(accepted, valid, `${name}: ${JSON.stringify(input)}`);
    checked += 1;
  }
}

for (const document of ['domain-v1', 'api-v2']) {
  const schemas = load(`../../contracts/generated/${document}.openapi.json`).components.schemas;
  for (const name of ['DbCounter', 'Revision']) {
    const corpus = cases.filter(({ schema }) => schema === name);
    assert.ok(corpus.length > 0, `${name}: empty corpus`);
    checkBigint(schemas[name], `${document}/${name}`, corpus);
  }

  const budget = schemas.BudgetV1;
  for (const field of ['max_cpu_seconds', 'max_output_bytes']) {
    assert.ok(budget.required.includes(field), `${field}: required cap`);
    checkBigint(budget.properties[field], `${document}/BudgetV1.${field}`, positiveCases);
  }
  const optional = budget.properties.max_tokens.oneOf;
  assert.equal(optional.length, 2, 'max_tokens: exactly null or a positive decimal string');
  assert.equal(optional.filter((schema) => schema.type === 'null').length, 1);
  assert.equal(optional.filter((schema) => schema.type === 'string').length, 1);
  assert.ok(!budget.required.includes('max_tokens'), 'max_tokens: omission is allowed');
  checkBigint(optional.find((schema) => schema.type === 'string'),
    `${document}/BudgetV1.max_tokens`, positiveCases);

  for (const [field, minimum, maximum] of [
    ['max_experiments', 1, 4294967295],
    ['max_parallel_runs', 1, 65535],
    ['max_turns_per_mission', 1, 65535],
    ['max_repair_turns', 0, 65535],
    ['max_wall_seconds', 1, 4294967295],
    ['max_memory_mib', 1, 4294967295],
    ['max_cycles_per_day', 1, 65535],
    ['min_cycle_interval_seconds', 0, 4294967295],
  ]) {
    const schema = budget.properties[field];
    assert.equal(schema.type, 'integer', field);
    assert.equal(schema.minimum, minimum, `${document}/${field}: minimum`);
    assert.equal(schema.maximum, maximum, `${document}/${field}: maximum`);
  }

  const variants = schemas.BriefContentV1.oneOf;
  assert.equal(variants.length, 3);
  for (const kind of ['FIXED_BARS', 'FIXED_DURATION', 'VARIABLE_INTERVAL']) {
    const matching = variants.filter((schema) =>
      JSON.stringify(schema.properties.horizon_kind.enum) === JSON.stringify([kind]));
    assert.equal(matching.length, 1, `${kind}: exactly one variant`);
    const variant = matching[0];
    assert.equal(variant.additionalProperties, false);
    assert.ok(variant.required.includes('horizon_kind'));
    if (kind === 'VARIABLE_INTERVAL') {
      assert.equal(variant.properties.horizon_value.type, 'null');
      assert.ok(!variant.required.includes('horizon_value'));
    } else {
      assert.ok(variant.required.includes('horizon_value'));
      checkBigint(variant.properties.horizon_value,
        `${document}/BriefContentV1.${kind}.horizon_value`, positiveCases);
    }
  }
}
console.log(`Generated bigint schemas: ${checked} native-parser cases passed across domain and HTTP contracts.`);
