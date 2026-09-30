import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import Ajv2020 from 'ajv/dist/2020.js';
import standaloneCode from 'ajv/dist/standalone/index.js';
import ts from 'typescript';
import { partitionStandalone } from './validator-partition.mjs';
const require = createRequire(import.meta.url);
const mappings = new Map([['validate0', '#/A'], ['validate1', '#/B'], ['validate2', '#/A']]);
function runtime(partition) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'qz-partition-'));
  fs.mkdirSync(path.join(directory, 'modules'));
  fs.symlinkSync(new URL('../node_modules', import.meta.url), path.join(directory, 'node_modules'));
  for (const [file, content] of partition.modules) fs.writeFileSync(path.join(directory, file), content);
  const exports = Object.fromEntries([...partition.exports].map(([name, target]) => [name, require(path.join(directory, target.path))[target.name]]));
  return { exports, dispose: () => fs.rmSync(directory, { recursive: true, force: true }) };
}
function emit(document, refs) {
  const objects = new WeakMap(); const provenance = new Map();
  function index(value, pointer) { if (value && typeof value === 'object') { objects.set(value, pointer); for (const [key, child] of Object.entries(value)) index(child, pointer + '/' + key.replaceAll('~', '~0').replaceAll('/', '~1')); } }
  index(document, '#');
  const ajv = new Ajv2020({ strict: false, inlineRefs: false, code: { source: true, lines: true, process(code, environment) {
    const pointer = objects.get(environment.schema);
    if (pointer) {
      const ast = ts.createSourceFile('process.js', code, 99, true, ts.ScriptKind.JS);
      function visit(node) { if (ts.isReturnStatement(node) && ts.isFunctionExpression(node.expression) && node.expression.name) provenance.set(node.expression.name.text, pointer); ts.forEachChild(node, visit); } visit(ast);
    }
    return code;
  } } });
  ajv.addSchema(document, 'urn:quazonai:fixture');
  const exports = Object.fromEntries(Object.entries(refs).map(([name, pointer]) => { ajv.getSchema('urn:quazonai:fixture' + pointer); return [name, 'urn:quazonai:fixture' + pointer]; }));
  return { ajv, partition: partitionStandalone(standaloneCode(ajv, exports), provenance) };
}

test('symbol identities preserve shadowed names, property keys and shorthand values', () => {
  const partition = partitionStandalone(`exports.a=validate0; exports.b=validate1;
const schema0 = { answer: 42 };
function validate0(value) { function local(validate1) { return validate1; } const object={validate1}; return local(value) === value && object.validate1(value); }
function validate1(value) { const schema0={ answer: 'local' }; return schema0.answer === 'local' && value === 42; }`, mappings);
  const run = runtime(partition);
  try { assert.equal(run.exports.a(42), true); assert.equal(run.exports.a(0), false); assert.equal(run.exports.b(42), true); }
  finally { run.dispose(); }
});

test('same-schema native function instances are retained and aliases stay identical', () => {
  const partition = partitionStandalone(`exports.a=validate0;exports.alias=validate0;exports.b=validate2;
function validate0(data){validate0.errors=data?null:[{message:'a'}];return !!data;}
function validate2(data){validate2.errors=data?null:[{message:'b'}];return !!data;}`, mappings);
  assert.equal(partition.modules.size, 1);
  const run = runtime(partition);
  try {
    assert.equal(run.exports.a, run.exports.alias); assert.notEqual(run.exports.a, run.exports.b);
    run.exports.a(false); run.exports.b(true); assert.deepEqual(run.exports.a.errors, [{ message: 'a' }]); assert.equal(run.exports.b.errors, null);
  } finally { run.dispose(); }
});

test('pure shared constants stay one object and wrappers preserve hoisting/metadata ordering', () => {
  const partition = partitionStandalone(`exports.a=validate0;exports.b=validate1;
const wrapper0={validate:validate0};const schema0={nested:{value:2}};
function validate0(data){return data === null ? schema0 : wrapper0.validate.evaluated.props;}
validate0.evaluated={props:true,dynamicProps:false};
function validate1(data){return data===null ? schema0 : validate0(data);}`, mappings);
  const run = runtime(partition);
  try { assert.equal(run.exports.a(null), run.exports.b(null)); assert.equal(run.exports.a(1), true); }
  finally { run.dispose(); }
});

test('public AJV self/mutual recursion, wrappers and full errors match', () => {
  const document = { $defs: {
    A: { type: 'object', properties: { child: { $ref: '#/$defs/A' }, peer: { $ref: '#/$defs/B' } }, additionalProperties: false },
    B: { type: 'object', properties: { next: { $ref: '#/$defs/A' }, value: { type: 'integer' } }, required: ['value'], additionalProperties: false },
  } };
  const { ajv, partition } = emit(document, { a: '#/$defs/A', b: '#/$defs/B' });
  const run = runtime(partition);
  try {
    for (const [name, pointer] of [['a', '#/$defs/A'], ['b', '#/$defs/B']]) {
      const native = ajv.getSchema('urn:quazonai:fixture' + pointer);
      for (const value of [null, {}, { child: {} }, { peer: { value: 3 } }, { peer: { value: 'bad' } }, { value: 4, next: { child: {} } }, { extra: true }]) {
        assert.equal(run.exports[name](value), native(value)); assert.deepEqual(run.exports[name].errors, native.errors);
      }
    }
    const earlier = { $defs: { Earlier: { type: 'string' }, ...document.$defs } };
    const other = emit(earlier, { earlier: '#/$defs/Earlier', a: '#/$defs/A', b: '#/$defs/B' }).partition;
    for (const [file, code] of partition.modules) assert.equal(other.modules.get(file), code);
  } finally { run.dispose(); }
});

test('stable schema filenames localize a native constraint edit', () => {
  const document = { $defs: { A: { type: 'string', minLength: 1 }, B: { type: 'number', minimum: 0 } } };
  const first = emit(document, { a: '#/$defs/A', b: '#/$defs/B' }).partition;
  const changed = structuredClone(document); changed.$defs.A.minLength = 2;
  const second = emit(changed, { a: '#/$defs/A', b: '#/$defs/B' }).partition;
  assert.deepEqual([...first.modules.keys()].sort(), [...second.modules.keys()].sort());
  assert.equal([...first.modules].filter(([file, code]) => second.modules.get(file) !== code).length, 1);
});

for (const [name, source] of [
  ['mutable binding', 'let schema0 = {};'], ['multiple declarations', 'const schema0={}, schema1={};'],
  ['effectful initializer', 'const schema0 = process.exit();'], ['computed literal', 'const schema0 = {[validate0()]: 1};'],
  ['spread literal', 'const schema0 = {...value};'], ['getter literal', 'const schema0 = {get value(){return 1}};'],
  ['dynamic require', 'const schema0 = require(variable).default;'], ['unknown runtime', 'const schema0 = require("fs").default;'],
  ['top-level effect', 'validate0(true);'], ['metadata effect', 'validate0.evaluated = (()=>({}))();'],
  ['unknown metadata', 'validate0.evaluated = {surprise: true};'], ['unknown export', 'exports.b = missing;'],
  ['duplicate declaration', 'function validate0(data){return false;}'], ['async function', 'async function validate1(data){return true;}'],
  ['missing provenance', 'function missing(data){return true;}'], ['unreachable function', 'function validate1(data){return true;}'], ['unknown global', 'function validate1(data){return eval(data);}'],
  ['require in function', 'function validate1(data){return require("ajv/dist/runtime/equal").default(data);}'],
]) test(`fails closed: ${name}`, () => assert.throws(() => partitionStandalone(`exports.a=validate0;function validate0(data){return !!data;}\n${source}`, mappings), /Unsupported AJV/));

test('native ESM and CommonJS load exactly the same generated graph', async () => {
  const partition = partitionStandalone('exports.a=validate0;function validate0(data){return !!data;}', mappings);
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'qz-interop-'));
  try {
    fs.mkdirSync(path.join(directory, 'modules'));
    for (const [file, code] of partition.modules) fs.writeFileSync(path.join(directory, file), code);
    const target = partition.exports.get('a'); const filename = path.join(directory, target.path);
    const cjs = require(filename); const esm = await import(pathToFileURL(filename));
    assert.equal(esm.default[target.name], cjs[target.name]); assert.equal(esm[target.name], cjs[target.name]);
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});

test('stable symbol names cannot capture parameters, catch, block or destructuring locals', () => {
  const probe = partitionStandalone('exports.a=validate1;function validate1(data){return true;}', mappings);
  const stable = probe.exports.get('a').name;
  const bodies = [
    `const ${stable}=()=>false;return validate1(data);`,
    `try{throw ()=>false;}catch(${stable}){return validate1(data);}`,
    `{const ${stable}=()=>false;return validate1(data);}`,
    `const {value:${stable}}={value:()=>false};return validate1(data);`,
    `function local(${stable}){return validate1(data);}return local(()=>false);`,
  ];
  for (const body of bodies) assert.throws(() => partitionStandalone(`exports.a=validate0;exports.b=validate1;function validate0(data){${body}}function validate1(data){return true;}`, mappings), /captures lexical binding/);
  assert.throws(() => partitionStandalone(`exports.a=validate0;exports.b=validate1;function validate0(data,${stable}=()=>false){return validate1(data);}function validate1(data){return true;}`, mappings), /captures lexical binding/);
});
test('stable constant names cannot capture pre-existing locals', () => {
  const first = partitionStandalone('exports.a=validate0;const schema0={answer:42};function validate0(data){return schema0.answer;}', mappings);
  const stable = [...first.provenance.values()].flatMap(record => record.bindings).find(binding => binding.kind === 'constant').name;
  assert.throws(() => partitionStandalone(`exports.a=validate0;const schema0={answer:42};function validate0(data){const ${stable}={answer:0};return schema0.answer;}`, mappings), /captures lexical binding/);
});
