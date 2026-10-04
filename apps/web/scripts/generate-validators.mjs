// Native AJV emits once. The pinned compiler relocates its dependency graph;
// validation logic, schema registration and native function instances survive.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { transformWithEsbuild } from 'vite';
import { compileNative } from './validator-native.mjs';
import { partitionStandalone } from './validator-partition.mjs';
import { completeOutputs, publishOutputs, checkOutputs, marker } from './validator-output.mjs';

const defaultSource = new URL('../../../contracts/generated/api-v2.openapi.json', import.meta.url);
const defaultOutput = fileURLToPath(new URL('../src/generated/', import.meta.url));
const helpers = [
  ['cost-currency', 'nativeCostCurrency', 'validateCostCurrency'],
  ['base-currency', 'nativeBaseCurrency', 'validateBaseCurrency'],
  ['problem', 'nativeProblem', 'validateProblem'],
  ['decimal', 'nativeDecimal', 'validateDecimal'],
  ['cost-amount', 'nativeCostAmount', 'validateCostAmount'],
  ['catalog-key', 'nativeCatalogKey', 'validateNativeCatalogKey'],
];
const prefix = '@quazonai/web/response-contract/';
const moduleId = file => prefix + file.replace(/\.cjs$/, '');
function syntax(source) {
  const result = spawnSync(process.execPath, ['--input-type=module', '--check'], {
    input: source, encoding: 'utf8', env: {}, timeout: 15000, maxBuffer: 8192,
  });
  if (result.error || result.status !== 0) throw new Error('Native standalone validator generation produced invalid module syntax');
}
export async function generateOutputs(document) {
  const native = compileNative(document);
  syntax(native.standalone);
  const partition = partitionStandalone(native.standalone, native.provenance);
  const outputs = new Map(); const provenance = new Map(); const imports = new Map();
  async function compact(file, source, details = {}, ids = []) {
    const { code } = await transformWithEsbuild(source, file, {
      format: file.endsWith('.mjs') ? 'esm' : 'cjs', minify: true, treeShaking: true,
      sourcemap: false, target: ['es2022', 'safari16'],
    });
    outputs.set(file, marker + code); provenance.set(file, details); imports.set(file, ids);
  }
  for (const [file, code] of [...partition.modules].sort(([a], [b]) => a.localeCompare(b))) {
    await compact('response-contract/' + file, code, partition.provenance.get(file), [moduleId(file)]);
  }
  const allExports = new Map(partition.exports);
  for (const [alias, first] of Object.entries(native.aliases)) {
    const target = allExports.get(first);
    if (!target) throw new Error(`Unresolved native alias ${alias}`);
    allExports.set(alias, target);
  }
  const registry = structuredClone(native.registry);
  for (const entry of Object.values(registry)) for (const media of Object.values(entry.media)) if (media.validator) {
    const target = allExports.get(media.validator);
    media.module = moduleId(target.path); media.name = target.name;
  }
  await compact('response-contract/metadata.cjs', `
const registry = ${JSON.stringify(registry)};
function mediaType(value) { return typeof value === 'string' ? value.split(';', 1)[0].trim().toLowerCase() : ''; }
function responseEntry(path, method, status) { return registry[method.toUpperCase() + ' ' + path + ' ' + status]; }
exports.responseKind = function(path, method, status, contentType) {
  const entry = responseEntry(path, method, status);
  if (!entry) return undefined;
  return entry.empty ? 'empty' : entry.media[mediaType(contentType)]?.kind;
};
exports.responseDescriptor = function(path, method, status, contentType) {
  const entry = responseEntry(path, method, status);
  if (!entry) return undefined;
  return entry.empty ? { kind: 'empty' } : entry.media[mediaType(contentType || 'application/json')];
};`, { kind: 'response-media-registry' }, [prefix + 'metadata']);
  const helperDeclarations = [];
  for (const [entry, nativeName, publicName] of helpers) {
    const target = allExports.get(nativeName);
    const declaration = `export declare function ${publicName}(value: unknown): ${entry === 'problem' ? 'value is import("../api").components["schemas"]["Problem"]' : 'boolean'};\n`;
    outputs.set(`response-contract/${entry}.d.cts`, marker + declaration);
    helperDeclarations.push(declaration.replace('"../api"', '"./api"'));
    await compact(`response-contract/${entry}.cjs`, `const native = require(${JSON.stringify('./' + target.path)}).${target.name};\nexports.${publicName} = function(value) { return native(value); };\n`, { schema: target.schema, nativeName, publicName }, [prefix + entry]);
  }
  let facade = '"use strict";\n';
  for (const [name, target] of allExports) facade += `exports.${name} = require(${JSON.stringify('./response-contract/' + target.path)}).${target.name};\n`;
  facade += `const metadata = require('./response-contract/metadata.cjs');\nexports.responseKind = metadata.responseKind;\nexports.validateResponse = function(path, method, status, value, contentType) {\nconst descriptor = metadata.responseDescriptor(path, method, status, contentType);\nif (!descriptor) return false;\nif (descriptor.kind === 'empty') return value === undefined;\nreturn descriptor.kind === 'json' && exports[descriptor.validator](value);\n};\n`;
  for (const [entry, , publicName] of helpers) facade += `exports.${publicName} = require('./response-contract/${entry}.cjs').${publicName};\n`;
  await compact('responses.cjs', facade, { kind: 'eager-compatibility', exports: [...allExports].map(([name, target]) => ({ export: name, ...target })) }, ['@quazonai/web/response-contract']);
  const responseArgs = 'path: string, method: string, status: number, value: unknown, contentType?: string | null';
  const kindDeclaration = 'export declare function responseKind(path: string, method: string, status: number, contentType?: string | null): "json" | "binary" | "event-stream" | "empty" | undefined;\n';
  outputs.set('responses.d.cts', marker + `export declare function validateResponse(${responseArgs}): boolean;\n` + kindDeclaration + helperDeclarations.join(''));
  outputs.set('response-contract/metadata.d.cts', marker + kindDeclaration);
  // Loader implementation is first-party source, included deterministically in
  // the generated entrypoint. Import targets are finite generator literals.
  let lazy = fs.readFileSync(new URL('./response-loader.mjs', import.meta.url), 'utf8');
  lazy += `\nimport metadata from '${prefix}metadata';\nconst loaders = {\n`;
  const loadedModules = [...new Set([...allExports.values()].map(target => moduleId(target.path)))].sort();
  for (const id of loadedModules) lazy += `${JSON.stringify(id)}: () => import(${JSON.stringify(id)}),\n`;
  lazy += `};\nconst validate = createValidatorLoader(loaders);\nexport async function validateResponseResultAsync(path, method, status, value, contentType) {\nconst descriptor = metadata.responseDescriptor(path, method, status, contentType);\nif (!descriptor) return { valid: false, errors: null };\nif (descriptor.kind === 'empty') return { valid: value === undefined, errors: null };\nif (descriptor.kind !== 'json') return { valid: false, errors: null };\nreturn validate(descriptor, value);\n}\nexport async function validateResponseAsync(path, method, status, value, contentType) {\nreturn (await validateResponseResultAsync(path, method, status, value, contentType)).valid;\n}\n`;
  await compact('response-contract/lazy.mjs', lazy, { kind: 'lazy-dispatch', imports: loadedModules }, [prefix + 'lazy']);
  outputs.set('response-contract/lazy.d.mts', marker
    + 'export declare class ResponseValidatorLoadError extends Error { constructor(moduleId: string, cause?: unknown); readonly moduleId: string; }\n'
    + `export declare function validateResponseAsync(${responseArgs}): Promise<boolean>;\n`
    + `export declare function validateResponseResultAsync(${responseArgs}): Promise<{ valid: boolean; errors: import("ajv").ErrorObject[] | null }>;\n`);
  const optimizer = [...imports].filter(([file]) => file.startsWith('response-contract/') && file.endsWith('.cjs')).flatMap(([, ids]) => ids).sort();
  outputs.set('response-contract/optimizer.json', JSON.stringify({ generated: marker.trim(), include: optimizer }, null, 2) + '\n');
  return completeOutputs(outputs, provenance, imports);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2); let output = defaultOutput; let check = false; let requireTracked = false;
  while (args.length) {
    const argument = args.shift();
    if (argument === '--check') check = true;
    else if (argument === '--require-tracked') requireTracked = true;
    else if (argument === '--output-dir' && args[0]) output = path.resolve(args.shift());
    else throw new Error('Usage: generate-validators.mjs [--output-dir directory] [--check [--require-tracked]]');
  }
  if (requireTracked && !check) throw new Error('--require-tracked requires --check');
  const expected = await generateOutputs(JSON.parse(fs.readFileSync(defaultSource, 'utf8')));
  if (check) checkOutputs(output, expected, { requireTracked }); else publishOutputs(output, expected);
  console.log(`${check ? 'Verified' : 'Generated'} ${expected.size} response contract files`);
}
