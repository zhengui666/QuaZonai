// Final production adapter acceptance, independently replayed from the original
// native schema and standalone program. No legacy/prototype comparison is used.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { performance } from 'node:perf_hooks';
import ts from 'typescript';
import { generateOutputs } from './generate-validators.mjs';
import { compileNative } from './validator-native.mjs';
import { publishOutputs, checkOutputs } from './validator-output.mjs';
const require = createRequire(import.meta.url);
const document = JSON.parse(fs.readFileSync(new URL('../../../contracts/generated/api-v2.openapi.json', import.meta.url), 'utf8'));
const id = '01990000-0000-7000-8000-000000000001';
const problem = { type: 'urn:quazonai:problem:validation-error', title: 'Validation failed', status: 422,
  code: 'VALIDATION_ERROR', detail: 'Check the request.', request_id: id, retryable: false, current_revision: null, field_errors: [], safe_next_actions: [] };
const corpus = [undefined, null, false, true, -1, 0, 1, 1.25, NaN, Infinity, -Infinity, 1n, 9223372036854775807n,
  '', id, '0', '+000.0100', '1\n', [], {}, { schema_version: 1 }, { schema_version: 1, items: [], next_cursor: null },
  { schema_version: 1, items: [], next_cursor: id }, { schema_version: 1, items: [], next_cursor: 'bad' },
  { schema_version: 1, replayed: false, resource: {} }, problem, { ...problem, extra: true }, { ...problem, status: '422' },
  { ...problem, request_id: 'bad' }, 'USD', 'CNY', 'USDT', 'usd', 'XXX', '1e-8', '9223372036854775808',
  '12345678901234567890.123456789012345678', '0.1234567890123456789', 'a/b', '../catalog', 'a\\b', '2026-09-30T00:00:00Z',
  { value: 1n }, [1n], { schema_version: 1, setup_required: false },
  { schema_version: 1, authenticated_at: '2026-09-30T00:00:00Z', expires_at: '2026-10-01T00:00:00Z' }];

let outputs;
test('browser application imports use selective validators instead of the eager compatibility facade', () => {
  const root = fileURLToPath(new URL('../src/', import.meta.url));
  const violations = [];
  const walk = directory => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      if (entry.name === 'generated') continue;
      const file = path.join(directory, entry.name);
      if (entry.isDirectory()) { walk(file); continue; }
      if (!/\.tsx?$/.test(entry.name) || /\.test\.tsx?$/.test(entry.name)) continue;
      const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
      const visit = node => {
        let specifier;
        if (ts.isImportDeclaration(node) && !node.importClause?.isTypeOnly) specifier = node.moduleSpecifier;
        if (ts.isExportDeclaration(node) && !node.isTypeOnly) specifier = node.moduleSpecifier;
        if (ts.isCallExpression(node) && (node.expression.kind === ts.SyntaxKind.ImportKeyword
          || (ts.isIdentifier(node.expression) && node.expression.text === 'require'))) specifier = node.arguments[0];
        if (specifier && ts.isStringLiteralLike(specifier) && specifier.text === '@quazonai/web/response-contract') {
          violations.push(path.relative(root, file) + ':' + (source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1));
        }
        ts.forEachChild(node, visit);
      };
      visit(source);
    }
  };
  walk(root);
  assert.deepEqual(violations, [], 'Browser imports must use a selective validator or the lazy response loader');
});

test('fresh complete repeated generation is byte-identical, schema-local, and checkable', { timeout: 120_000 }, async () => {
  const started = performance.now(); outputs = await generateOutputs(document); const firstMs = performance.now() - started;
  const second = await generateOutputs(structuredClone(document)); assert.deepEqual([...second], [...outputs]);
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'qz-complete-'));
  try {
    publishOutputs(root, outputs); checkOutputs(root, second);
    const before = new Map([...outputs.keys()].map(file => [file, fs.statSync(path.join(root, file)).mtimeMs]));
    publishOutputs(root, second);
    for (const [file, mtime] of before) assert.equal(fs.statSync(path.join(root, file)).mtimeMs, mtime);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
  const added = structuredClone(document);
  added.paths = { '/__validator_fixture': { get: { responses: { 200: { content: { 'application/json': { schema: { type: 'string', minLength: 7 } } } } } } }, ...added.paths };
  const addition = await generateOutputs(added);
  const modules = [...outputs].filter(([file]) => file.startsWith('response-contract/modules/'));
  for (const [file, code] of modules) assert.equal(addition.get(file), code, file);
  assert.equal([...addition.keys()].filter(file => file.startsWith('response-contract/modules/')).length, modules.length + 1);
  const edited = structuredClone(document); edited.components.schemas.DecimalValue.maxLength = 127;
  const edit = await generateOutputs(edited);
  assert.deepEqual([...edit.keys()].sort(), [...outputs.keys()].sort());
  assert.equal(modules.filter(([file, code]) => edit.get(file) !== code).length, 1);
  console.log(JSON.stringify({ generatedFiles: outputs.size, modules: modules.length, moduleBytes: modules.reduce((sum, [, code]) => sum + Buffer.byteLength(code), 0), fullGenerationMs: Math.round(firstMs), localizedModulesChanged: 1 }));
});

test('all native results/errors, exported aliases, CJS/ESM/selective identities and lazy decisions match', { timeout: 120_000 }, async () => {
  outputs ??= await generateOutputs(document);
  const { standalone, aliases, registry, exported } = compileNative(document);
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'qz-parity-'));
  try {
    publishOutputs(root, outputs);
    fs.symlinkSync(new URL('../node_modules', import.meta.url), path.join(root, 'node_modules'));
    fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ name: '@quazonai/web', type: 'module', exports: { './response-contract/modules/*': './response-contract/modules/*.cjs', './response-contract/metadata': './response-contract/metadata.cjs' } }));
    fs.writeFileSync(path.join(root, 'original.cjs'), standalone + '\n' + Object.entries(aliases).map(([name, original]) => `exports.${name}=exports.${original};`).join('\n'));
    const original = require(path.join(root, 'original.cjs')); const generated = require(path.join(root, 'responses.cjs'));
    const esm = await import(pathToFileURL(path.join(root, 'responses.cjs')));
    let comparisons = 0;
    for (const name of Object.keys(original)) {
      assert.equal(typeof generated[name], 'function'); assert.equal(esm.default[name], generated[name]); assert.equal(esm[name], generated[name]);
      for (const value of corpus) { assert.equal(generated[name](value), original[name](value), name); assert.deepEqual(generated[name].errors, original[name].errors, name); comparisons++; }
    }
    assert.equal(Object.keys(generated).length, Object.keys(original).length + 8);
    for (const [name, first] of Object.entries(aliases)) assert.equal(generated[name], generated[first]);
    for (const [entry, name] of [['decimal', 'validateDecimal'], ['problem', 'validateProblem'], ['catalog-key', 'validateNativeCatalogKey'], ['cost-amount', 'validateCostAmount'], ['cost-currency', 'validateCostCurrency'], ['base-currency', 'validateBaseCurrency']]) {
      const file = path.join(root, 'response-contract', entry + '.cjs'); const cjs = require(file); const esm = await import(pathToFileURL(file));
      assert.equal(cjs[name], generated[name]); assert.equal(esm[name], generated[name]); assert.equal(esm.default[name], generated[name]);
    }
    const manifest = JSON.parse(outputs.get('response-contract/manifest.json'));
    const functions = manifest.files.flatMap(file => file.provenance.bindings ?? []).filter(binding => binding.kind === 'function');
    const ast = ts.createSourceFile('native.js', standalone, 99, true, ts.ScriptKind.JS);
    assert.equal(functions.length, ast.statements.filter(ts.isFunctionDeclaration).length);
    const lazy = await import(pathToFileURL(path.join(root, 'response-contract/lazy.mjs')));
    let routeComparisons = 0;
    for (const [key, entry] of Object.entries(registry)) {
      const [method, pathname, status] = key.split(' ');
      const medias = [...Object.keys(entry.media), 'application/json; charset=utf-8', 'APPLICATION/PROBLEM+JSON', 'unknown/media', '', null];
      for (const media of medias) for (const value of [undefined, null, {}, corpus[21], problem, 1n]) {
        const actual = await lazy.validateResponseResultAsync(pathname, method, Number(status), value, media);
        assert.equal(actual.valid, generated.validateResponse(pathname, method, Number(status), value, media)); routeComparisons++;
      }
    }
    assert.equal(await lazy.validateResponseAsync('/unknown', 'GET', 200, {}), false);
    const sameSchema = new Map(); for (const [name, reference] of Object.entries(exported)) { const group = sameSchema.get(reference) ?? []; group.push(name); sameSchema.set(reference, group); }
    for (const group of sameSchema.values()) for (let index = 1; index < group.length; index++) assert.equal(generated[group[0]] === generated[group[index]], original[group[0]] === original[group[index]]);
    console.log(JSON.stringify({ nativeExports: Object.keys(original).length, fullErrorComparisons: comparisons, routeMediaComparisons: routeComparisons, retainedNativeFunctions: functions.length }));
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});
