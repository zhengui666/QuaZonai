// Generate validators and response media dispatch from the actual Rust OpenAPI.
// No independent response DTOs, currency list, or permissive binary fallback.
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';
import standaloneCode from 'ajv/dist/standalone/index.js';

const source = new URL('../../../contracts/generated/api-v2.openapi.json', import.meta.url);
const args = process.argv.slice(2);
if (args.length !== 0 && (args.length !== 2 || args[0] !== '--output-dir' || !args[1])) {
  throw new Error('Usage: generate-validators.mjs [--output-dir directory]');
}
// Verification can compare native outputs in a new private directory without
// changing checked-in artifacts. The schema source remains fixed in this repo.
const output = args.length === 2
  ? pathToFileURL(path.resolve(args[1]) + path.sep)
  : new URL('../src/generated/', import.meta.url);
const document = JSON.parse(fs.readFileSync(source, 'utf8'));
if (!document.paths || !document.components?.schemas) throw new Error('Native HTTP document is incomplete');
const root = 'urn:quazonai:http-contract:v2';
// Reuse generated functions for native component references instead of
// repeating Problem and nested Brief validators in every operation's bundle.
const ajv = new Ajv2020({ strict: false, allErrors: false, validateFormats: true, inlineRefs: false,
  code: { source: true, esm: false, lines: true } });
addFormats(ajv);
ajv.addSchema(document, root);
const exported = {};
const aliases = {};
const nativeExports = new Map();
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
  // Export the native cached schema function directly. Registering a new
  // wrapper schema for every operation recompiles equivalent response roots.
  // The original document remains the reference-resolution authority.
  if (typeof schema.$ref !== 'string' || Object.keys(schema).length !== 1) {
    throw new Error('Expected one exact native schema reference');
  }
  const native = ajv.getSchema(schema.$ref);
  if (!native) throw new Error('Native validator reference could not be compiled');
  const first = nativeExports.get(native);
  if (first !== undefined) {
    // A repeated standalone export can repeat its function declaration in Ajv
    // 8.17.1. Export each native function once, then share its exact identity.
    aliases[name] = first;
  } else {
    nativeExports.set(native, name);
    exported[name] = schema.$ref;
  }
  return name;
}
let sequence = 0;
for (const [path, item] of Object.entries(document.paths)) {
  for (const method of ['get', 'post', 'patch', 'put', 'delete', 'head', 'options']) {
    const operation = item[method];
    if (!operation) continue;
    for (const [status, responseValue] of Object.entries(operation.responses ?? {})) {
      if (!/^[2-5][0-9]{2}$/.test(status)) continue;
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
            if (!content.schema) throw new Error(`JSON response schema missing: ${key}`);
            const responsePointer = responseValue.$ref
              ? `${responseValue.$ref}/content/${escapePointer(mime)}/schema`
              : `#/paths/${escapePointer(path)}/${method}/responses/${status}/content/${escapePointer(mime)}/schema`;
            // Only a pure $ref may reuse the component root. A sibling keyword
            // (including a constraint) must retain the exact response schema.
            const pureReference = typeof content.schema.$ref === 'string'
              && Object.keys(content.schema).length === 1;
            const pointer = pureReference ? content.schema.$ref : responsePointer;
            if (!pointer.startsWith('#/')) throw new Error('Nonlocal native response schema');
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
validator('nativeCostCurrency', { $ref: `${root}#/components/schemas/BudgetV1/allOf/0/properties/cost_currency` });
validator('nativeBaseCurrency', { $ref: `${root}#/components/schemas/BriefContentV1/oneOf/0/properties/base_currency` });
validator('nativeProblem', { $ref: `${root}#/components/schemas/Problem` });
validator('nativeDecimal', { $ref: `${root}#/components/schemas/DecimalValue` });
validator('nativeCostAmount', { $ref: `${root}#/components/schemas/BudgetV1/allOf/1/oneOf/1/properties/max_cost_decimal` });
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
exports.validateBaseCurrency = function(value) { return exports.nativeBaseCurrency(value); };
exports.validateProblem = function(value) { return exports.nativeProblem(value); };
exports.validateDecimal = function(value) { return exports.nativeDecimal(value); };
exports.validateCostAmount = function(value) { return exports.nativeCostAmount(value); };
`;
const aliasCode = Object.entries(aliases)
  .map(([name, first]) => `exports[${JSON.stringify(name)}] = exports[${JSON.stringify(first)}];`)
  .join('\n');
const generated = '// Generated from Rust OpenAPI. Do not edit.\n'
  + standaloneCode(ajv, exported) + '\n' + aliasCode + '\n' + runtime;
// CJS parsing alone permits duplicate function declarations that fail when
// Vite/Vitest loads the same code as a module. Check module syntax, not execute it.
const syntax = spawnSync(process.execPath, ['--input-type=module', '--check'], {
  input: generated, encoding: 'utf8', env: {}, timeout: 15000, maxBuffer: 8192,
});
if (syntax.error || syntax.status !== 0) {
  throw new Error('Native standalone validator generation produced invalid module syntax');
}
fs.mkdirSync(output, { recursive: true });
fs.writeFileSync(new URL('responses.cjs', output), generated);
fs.writeFileSync(new URL('responses.d.cts', output),
  '// Generated from Rust OpenAPI. Do not edit.\n' +
  'export declare function validateResponse(path: string, method: string, status: number, value: unknown, contentType?: string | null): boolean;\n' +
  'export declare function responseKind(path: string, method: string, status: number, contentType?: string | null): "json" | "binary" | "event-stream" | "empty" | undefined;\n' +
  'export declare function validateCostCurrency(value: unknown): boolean;\n' +
  'export declare function validateBaseCurrency(value: unknown): boolean;\n' +
  'export declare function validateDecimal(value: unknown): boolean;\n' +
  'export declare function validateCostAmount(value: unknown): boolean;\n' +
  'export declare function validateProblem(value: unknown): value is import("./api").components["schemas"]["Problem"];\n');
