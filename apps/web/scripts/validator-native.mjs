// Generate validators and response media dispatch from the actual Rust OpenAPI.
// No independent response DTOs, currency list, or permissive binary fallback.
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';
import standaloneCode from 'ajv/dist/standalone/index.js';

import ts from 'typescript';

export function compileNative(document) {
if (!document.paths || !document.components?.schemas) throw new Error('Native HTTP document is incomplete');
const pointerByObject = new WeakMap();
const provenance = new Map();
function index(value, pointer) {
  if (!value || typeof value !== 'object') return;
  if (pointerByObject.has(value)) throw new Error('Ambiguous native schema object identity');
  pointerByObject.set(value, pointer);
  for (const [key, child] of Object.entries(value)) index(child, pointer + '/' + key.replaceAll('~', '~0').replaceAll('/', '~1'));
}
index(document, '#');
function observe(code, schemaEnv) {
  const pointer = pointerByObject.get(schemaEnv.schema);
  if (pointer) {
    const ast = ts.createSourceFile('native-process.js', code, ts.ScriptTarget.ES2022, true, ts.ScriptKind.JS);
    if (ast.parseDiagnostics.length) throw new Error('Invalid native process syntax');
    const functions = [];
    function visit(node) {
      if (ts.isReturnStatement(node) && node.expression && ts.isFunctionExpression(node.expression) && node.expression.name) {
        functions.push(node.expression.name.text);
      }
      ts.forEachChild(node, visit);
    }
    visit(ast);
    if (functions.length !== 1) throw new Error('Unrecognized native code.process function shape');
    const name = functions[0];
    if (provenance.has(name) && provenance.get(name) !== pointer) throw new Error('Conflicting native function provenance');
    provenance.set(name, pointer);
  }
  // Public observation hook; return the exact upstream source unchanged.
  return code;
}
const root = 'urn:quazonai:http-contract:v2';
// Reuse generated functions for native component references instead of
// repeating Problem and nested Brief validators in every operation's bundle.
const ajv = new Ajv2020({ strict: false, allErrors: false, validateFormats: true, inlineRefs: false,
  code: { source: true, esm: false, lines: true, process: observe } });
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
validator('nativeCatalogKey', { $ref: `${root}#/components/schemas/DataSourceCreate/properties/native_catalog_ref` });
return { standalone: standaloneCode(ajv, exported), aliases, registry, exported, provenance };
}
