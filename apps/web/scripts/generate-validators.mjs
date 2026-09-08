// Generate validators and response media dispatch from the actual Rust OpenAPI.
// No independent response DTOs, currency list, or permissive binary fallback.
import fs from 'node:fs';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';
import standaloneCode from 'ajv/dist/standalone/index.js';

const source = new URL('../../../contracts/generated/api-v2.openapi.json', import.meta.url);
const output = new URL('../src/generated/', import.meta.url);
const document = JSON.parse(fs.readFileSync(source, 'utf8'));
if (!document.paths || !document.components?.schemas) throw new Error('Native HTTP document is incomplete');
const root = 'urn:quazonai:http-contract:v2';
const ajv = new Ajv2020({ strict: false, allErrors: false, validateFormats: true,
  code: { source: true, esm: false, lines: true } });
addFormats(ajv);
ajv.addSchema(document, root);
const exported = {};
const registry = {};
const escapePointer = value => value.replaceAll('~', '~0').replaceAll('/', '~1');
function local(value) {
  const seen = new Set();
  while (value?.$ref) {
    if (!value.$ref.startsWith('#/') || seen.has(value.$ref)) throw new Error('Unsupported response reference');
    seen.add(value.$ref);
    value = value.$ref.slice(2).split('/').map(part => part.replaceAll('~1', '/').replaceAll('~0', '~'))
      .reduce((object, key) => object?.[key], document);
    if (!value) throw new Error('Unresolved native response reference');
  }
  return value;
}
function validator(name, schema) {
  const id = `${root}:${name}`;
  ajv.addSchema(schema, id);
  exported[name] = id;
  return name;
}
let sequence = 0;
for (const [path, item] of Object.entries(document.paths)) {
  for (const method of ['get', 'post', 'patch', 'put', 'delete', 'head', 'options']) {
    const operation = item[method];
    if (!operation) continue;
    for (const [status, responseValue] of Object.entries(operation.responses ?? {})) {
      if (!/^2[0-9]{2}$/.test(status)) continue;
      const response = local(responseValue);
      const key = `${method.toUpperCase()} ${path} ${status}`;
      const entry = { empty: false, media: {} };
      if (status === '204' || status === '205') {
        if (Object.keys(response.content ?? {}).length) throw new Error('Body declared for no-content response');
        entry.empty = true;
      } else {
        for (const [mime, content] of Object.entries(response.content ?? {})) {
          const media = mime.toLowerCase();
          if (media === 'application/json' || /^application\/[a-z0-9.+-]+\+json$/.test(media)) {
            if (!content.schema) throw new Error(`JSON success schema missing: ${key}`);
            // Keep local references attached to the same complete native document.
            const pointer = responseValue.$ref
              ? `${responseValue.$ref}/content/${escapePointer(mime)}/schema`
              : `#/paths/${escapePointer(path)}/${method}/responses/${status}/content/${escapePointer(mime)}/schema`;
            entry.media[media] = { kind: 'json', validator: validator(`response${sequence++}`, { $ref: root + pointer }) };
          } else if (media === 'text/event-stream') {
            entry.media[media] = { kind: 'event-stream' };
          } else if (local(content.schema)?.format === 'binary') {
            entry.media[media] = { kind: 'binary' };
          }
        }
      }
      registry[key] = entry;
    }
  }
}
if (!document.components.schemas.BudgetV1?.properties?.cost_currency) {
  throw new Error('Native budget currency schema missing');
}
validator('nativeCostCurrency', { $ref: `${root}#/components/schemas/BudgetV1/properties/cost_currency` });
const runtime = `
const responseRegistry = ${JSON.stringify(registry, null, 2)};
function mediaType(value) { return typeof value === 'string' ? value.split(';', 1)[0].trim().toLowerCase() : ''; }
function responseEntry(path, method, status) { return responseRegistry[method.toUpperCase() + ' ' + path + ' ' + status]; }
exports.responseKind = function(path, method, status, contentType) {
  const entry = responseEntry(path, method, status);
  if (!entry) return undefined;
  if (entry.empty) return 'empty';
  return entry.media[mediaType(contentType)]?.kind;
};
exports.validateResponse = function(path, method, status, value, contentType) {
  const entry = responseEntry(path, method, status);
  if (!entry) return false;
  if (entry.empty) return value === undefined;
  const media = entry.media[mediaType(contentType || 'application/json')];
  return media?.kind === 'json' && exports[media.validator](value);
};
exports.validateCostCurrency = function(value) { return exports.nativeCostCurrency(value); };
`;
fs.mkdirSync(output, { recursive: true });
fs.writeFileSync(new URL('responses.cjs', output), standaloneCode(ajv, exported) + runtime);
fs.writeFileSync(new URL('responses.d.cts', output),
  'export declare function validateResponse(path: string, method: string, status: number, value: unknown, contentType?: string | null): boolean;\n' +
  'export declare function responseKind(path: string, method: string, status: number, contentType?: string | null): "json" | "binary" | "event-stream" | "empty" | undefined;\n' +
  'export declare function validateCostCurrency(value: unknown): boolean;\n');
