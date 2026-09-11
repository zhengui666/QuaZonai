import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import Ajv2020 from 'ajv/dist/2020';
import addFormats from 'ajv-formats';

type Document = {
  components: { schemas: Record<string, object> };
  paths: Record<string, Record<string, { parameters?: { name: string; in: string; schema: object }[] }>>;
};
const document = JSON.parse(readFileSync(new URL('../../../contracts/generated/api-v2.openapi.json', import.meta.url), 'utf8')) as Document;
function compile(schema: object) {
  const ajv = new Ajv2020({ strict: false, allErrors: true });
  addFormats(ajv);
  return ajv.compile({ ...schema, components: document.components });
}

describe('native-generated request constraints execute in the JavaScript validator', () => {
  it('validates every idempotency header including terminal newline and exact byte boundaries', () => {
    const headers = Object.values(document.paths).flatMap(item => Object.values(item)
      .flatMap(operation => operation.parameters ?? [])).filter(parameter => parameter.in === 'header' && parameter.name === 'Idempotency-Key');
    expect(headers.length).toBeGreaterThanOrEqual(10);
    for (const header of headers) {
      const valid = compile(header.schema);
      for (const value of ['x', 'a b', '!~', 'x'.repeat(200)]) expect(valid(value)).toBe(true);
      for (const value of ['', ' ', ' x', 'x ', 'x\tB', 'x\n', 'x\r\n', 'x'.repeat(201), '中文', '\u00a0', null, 1, ['x']]) {
        expect(valid(value), JSON.stringify(value)).toBe(false);
      }
      for (let byte = 0; byte <= 255; byte++) {
        expect(valid(String.fromCharCode(byte)), `single byte ${byte}`).toBe(byte >= 33 && byte <= 126);
        expect(valid(`a${String.fromCharCode(byte)}b`), `interior byte ${byte}`).toBe(byte >= 32 && byte <= 126);
      }
    }
  });
  it('rejects duplicate machine scopes while preserving valid nonempty unique requests', () => {
    const schema = document.components.schemas.CredentialIssue;
    if (!schema) throw new Error('Native CredentialIssue component missing');
    const valid = compile(schema);
    const request = { schema_version: 1, scope_codes: ['RUN_READ'], expires_at: '2030-09-08T00:00:00Z' };
    expect(valid(request)).toBe(true);
    expect(valid({ ...request, scope_codes: ['RUN_READ', 'RESEARCH_READ'] })).toBe(true);
    expect(valid({ ...request, scope_codes: [] })).toBe(false);
    expect(valid({ ...request, scope_codes: ['RUN_READ', 'RUN_READ'] })).toBe(false);
  });
});
