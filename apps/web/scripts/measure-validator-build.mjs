import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import assert from 'node:assert/strict';

// Full application entry closure and all shipped bytes, not a contract-only
// microbenchmark. Dynamic imports are deliberately excluded only from startup.
export function measureBuild(directory, { assertModular = false } = {}) {
  const manifest = JSON.parse(fs.readFileSync(path.join(directory, '.vite/manifest.json'), 'utf8'));
  const entries = Object.entries(manifest).filter(([, entry]) => entry.isEntry).map(([key]) => key);
  assert.ok(entries.length > 0, 'Missing Vite application entry');
  const visited = new Set(); const eagerFiles = new Set();
  function visit(key) {
    if (visited.has(key)) return; visited.add(key);
    const entry = manifest[key]; assert.ok(entry, `Missing import ${key}`);
    eagerFiles.add(entry.file);
    for (const file of [...entry.css ?? [], ...entry.assets ?? []]) eagerFiles.add(file);
    for (const key of entry.imports ?? []) visit(key);
  }
  entries.forEach(visit);
  const files = []; function walk(folder, prefix = '') { for (const entry of fs.readdirSync(folder, { withFileTypes: true })) {
    const relative = prefix + entry.name;
    if (entry.isDirectory()) walk(path.join(folder, entry.name), relative + '/');
    else files.push({ path: relative, bytes: fs.statSync(path.join(directory, relative)).size });
  } } walk(directory);
  const worker = ts.createSourceFile('sw.js', fs.readFileSync(path.join(directory, 'sw.js'), 'utf8'), 99, true, ts.ScriptKind.JS);
  const precache = new Set();
  function readPrecache(node) {
    if (ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression) && node.expression.name.text === 'precacheAndRoute') {
      assert.ok(ts.isArrayLiteralExpression(node.arguments[0]), 'Unexpected Workbox precache shape');
      for (const entry of node.arguments[0].elements) {
        assert.ok(ts.isObjectLiteralExpression(entry));
        const property = entry.properties.find(property => ts.isPropertyAssignment(property) && property.name.getText(worker) === 'url');
        assert.ok(property && ts.isStringLiteral(property.initializer)); precache.add(property.initializer.text);
      }
    }
    ts.forEachChild(node, readPrecache);
  } readPrecache(worker);
  assert.ok(precache.size > 0);
  for (const file of files.filter(file => file.path.startsWith('assets/') && /\.(js|css)$/.test(file.path))) {
    assert.ok(precache.has(file.path), `Uncached lazy asset ${file.path}`);
    assert.ok(file.bytes <= 2 * 1024 * 1024, `Oversized PWA asset ${file.path}`);
  }
  if (assertModular) {
    assert.equal(Object.keys(manifest).some(key => /(?:^|\/)responses\.cjs/.test(key)), false, 'Eager facade reached the production bundle');
    const lazy = files.filter(file => /\/schema-.*\.js$/.test(file.path));
    assert.ok(lazy.length > 0); assert.ok(lazy.every(file => !eagerFiles.has(file.path)), 'Response chunk entered static startup closure');
  }
  const bytesFor = paths => [...paths].reduce((sum, file) => sum + fs.statSync(path.join(directory, file)).size, 0);
  const staticFiles = files.filter(file => /\.(js|css|html|svg|webmanifest)$/.test(file.path));
  return { applicationEntries: entries, eagerFiles: [...eagerFiles].sort(), eagerBytes: bytesFor(eagerFiles),
    totalStaticAssets: staticFiles.length, totalStaticBytes: staticFiles.reduce((sum, file) => sum + file.bytes, 0),
    totalBuildFiles: files.length, totalBuildBytes: files.reduce((sum, file) => sum + file.bytes, 0),
    precachedAssets: precache.size, precachedBytes: bytesFor(precache),
    largestAssets: staticFiles.sort((a, b) => b.bytes - a.bytes).slice(0, 5) };
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2); const assertModular = args.includes('--assert-modular');
  const directory = path.resolve(args.find(argument => !argument.startsWith('--')) ?? fileURLToPath(new URL('../dist/', import.meta.url)));
  console.log(JSON.stringify(measureBuild(directory, { assertModular }), null, 2));
}
