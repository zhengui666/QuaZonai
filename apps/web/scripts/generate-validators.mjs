// Web-authored generator. Ajv, not a second handwritten DTO implementation,
// compiles the Rust-owned OpenAPI schemas before deployment (no browser eval).
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';
import standaloneCode from 'ajv/dist/standalone/index.js';

const document = JSON.parse(await readFile(new URL('../../../contracts/generated/api-v2.openapi.json', import.meta.url), 'utf8'));
const root = 'https://contracts.quazonai.invalid/api-v2';
const ajv = new Ajv2020({ strict: false, allErrors: false, validateFormats: true, code: { source: true, esm: false, lines: true } });
addFormats(ajv);
ajv.addSchema(document, root);
const escape = value => value.replaceAll('~', '~0').replaceAll('/', '~1');
const exports = {};
const registry = {};
for (const [path, item] of Object.entries(document.paths)) {
  for (const [method, operation] of Object.entries(item)) {
    if (!['get', 'post', 'patch', 'put', 'delete'].includes(method)) continue;
    for (const [status, response] of Object.entries(operation.responses ?? {})) {
      if (!/^2\d\d$/.test(status)) continue;
      const key = `${method.toUpperCase()} ${path} ${status}`;
      if (status === '204') { registry[key] = null; continue; }
      if (!response.content?.['application/json']?.schema) continue;
      const name = `response${Object.keys(exports).length}`;
      const id = `${root}/response/${name}`;
      const pointer = `#/paths/${escape(path)}/${method}/responses/${status}/content/application~1json/schema`;
      ajv.addSchema({ $ref: root + pointer }, id);
      exports[name] = id;
      registry[key] = name;
    }
  }
}
if (!Object.keys(exports).length) throw new Error('OpenAPI has no JSON success responses');
const output = new URL('../src/generated/', import.meta.url);
await mkdir(output, { recursive: true });
const wrapper = `\nconst responses = ${JSON.stringify(registry, null, 2)};\nexports.validateResponse = function(path, method, status, value) {\n  const key = method + ' ' + path + ' ' + status;\n  if (!Object.prototype.hasOwnProperty.call(responses, key)) return false;\n  const name = responses[key];\n  return name === null ? value === undefined : exports[name](value);\n};\n`;
await writeFile(new URL('responses.cjs', output), '// Generated from Rust OpenAPI. Do not edit.\n' + standaloneCode(ajv, exports) + wrapper);
await writeFile(new URL('responses.d.cts', output), '// Generated from Rust OpenAPI. Do not edit.\nexport declare function validateResponse(path: string, method: string, status: number, value: unknown): boolean;\n');
console.log(`Generated ${Object.keys(exports).length} response validators from the committed Rust contract.`);
