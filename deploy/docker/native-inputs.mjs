#!/usr/bin/env node
// Conservative Docker native source boundary. No dependency discovery or downloads.
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const MEMBERS = ['apps/cli', 'apps/job', 'apps/runtime', 'apps/server',
  'crates/contracts', 'crates/domain', 'crates/integrations', 'crates/store'];
const FILES = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml',
  'deploy/docker/native-inputs.mjs', 'deploy/docker/native-build.sh'];
const TREES = [...MEMBERS, 'migrations', 'contracts/generated', 'tests/contracts'];
const PLATFORM = 'linux/amd64';
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const inside = (name, tree) => name === tree || name.startsWith(tree + '/');
const included = name => FILES.includes(name) || TREES.some(tree => inside(name, tree));
const fail = message => { throw new Error('Native input boundary: ' + message); };

export function identity(source, platform = PLATFORM) {
  source = path.resolve(source);
  if (platform !== PLATFORM) fail('unsupported platform ' + platform);
  function safe(name) {
    if (path.isAbsolute(name) || name.split('/').includes('..')) fail('escaping input ' + name);
    let current = source;
    for (const part of name.split('/')) {
      current = path.join(current, part);
      if (fs.lstatSync(current).isSymbolicLink()) fail('symlink input ' + name);
    }
    return current;
  }
  const read = name => fs.readFileSync(safe(name));
  // Keep this deliberately narrow: changing workspace layout needs an explicit
  // boundary update, not an inferred list that could silently lose a member.
  const workspace = read('Cargo.toml').toString().match(/^\[workspace\]\s*\n([\s\S]*?)(?=^\[|(?![\s\S]))/m)?.[1];
  const members = workspace?.match(/^members\s*=\s*(\[[^\]]*\])\s*$/m)?.[1];
  let selected;
  try { selected = JSON.parse(members); } catch { fail('workspace members must remain an explicit JSON-compatible list'); }
  if (JSON.stringify([...selected].sort()) !== JSON.stringify(MEMBERS)) fail('workspace members changed; update the native closure');
  if (/^(?:exclude|default-members)\s*=/m.test(workspace)) fail('unsupported workspace selection');
  if (read('.dockerignore').toString().trim() !== '.git\ntarget\nnode_modules\n*.log\n.env*') {
    fail('Docker context exclusions changed; review the native closure');
  }
  // Dockerfile-specific rules take precedence over the root file. They must
  // admit both control files so the real filtered context can verify them too.
  const dockerIgnore = read('deploy/docker/Dockerfile.dockerignore');
  if (sha(dockerIgnore) !== 'cabb2f0c1d5b64c6fb75e74a4d42d5719d05cf4af2e80180bcc0acaa2fa68afc') {
    fail('Dockerfile-specific exclusions changed; review the native closure');
  }
  // Cargo discovers configuration from the working directory and its ancestors.
  // None is currently supported; detect new root/member config before filtering.
  const configParents = new Set(['', 'apps', 'crates', ...MEMBERS]);
  for (const parent of configParents) {
    if (fs.lstatSync(path.join(source, parent, '.cargo'), { throwIfNoEntry: false })) fail('unsupported Cargo configuration at ' + parent + '/.cargo');
  }
  if (fs.lstatSync(path.join(source, 'rust-toolchain'), { throwIfNoEntry: false })) fail('unsupported alternate rust-toolchain');
  const entries = [];
  function visit(name) {
    const file = safe(name);
    const stat = fs.lstatSync(file);
    if (stat.isSymbolicLink()) fail('symlink input ' + name);
    if (stat.isDirectory()) {
      for (const child of fs.readdirSync(file).sort()) {
        if (['.git', 'target', 'node_modules', '.cargo'].includes(child) || child.endsWith('.log') || child.startsWith('.env')) {
          fail('unsupported context-excluded native input ' + name + '/' + child);
        }
        visit(name + '/' + child);
      }
    } else if (stat.isFile()) {
      entries.push({ path: name, mode: stat.mode & 0o777, sha256: sha(fs.readFileSync(file)) });
    } else fail('non-regular input ' + name);
  }
  for (const name of [...FILES, ...TREES]) visit(name);
  entries.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  // Every local manifest path must be represented in the copied closure. This
  // includes explicit targets and local dependencies, not just workspace members.
  for (const name of ['Cargo.toml', ...MEMBERS.map(member => member + '/Cargo.toml')]) {
    for (const match of read(name).toString().matchAll(/\bpath\s*=\s*["']([^"']+)["']/g)) {
      if (path.posix.isAbsolute(match[1])) fail('absolute manifest path ' + match[1]);
      const target = path.posix.normalize(path.posix.join(path.posix.dirname(name), match[1]));
      if (!included(target) || !fs.existsSync(path.join(source, target))) fail('unsupported manifest path ' + target);
    }
  }
  // Whole trees include JSON/SQL/build scripts as well as Rust. External literal
  // include dependencies in production sources must be explicitly in the closure.
  for (const entry of entries.filter(entry => entry.path.includes('/src/') && entry.path.endsWith('.rs'))) {
    const text = read(entry.path).toString();
    for (const match of text.matchAll(/\binclude(?:_str|_bytes)?!\s*\(\s*([^)]*)\)/g)) {
      const literal = match[1].trim().match(/^"([^"\n]+)"\s*,?$/);
      if (!literal) fail('unsupported dynamic include in ' + entry.path);
      const target = path.posix.normalize(path.posix.join(path.posix.dirname(entry.path), literal[1]));
      if (!entries.some(entry => entry.path === target)) fail('external compile-time input ' + target);
    }
  }
  const dockerfile = read('deploy/docker/Dockerfile').toString();
  const native = dockerfile.match(/^FROM rust:[\s\S]*?(?=^FROM caddy:)/m)?.[0];
  const collector = dockerfile.match(/^FROM node:[^\n]+ AS node\n[\s\S]*?(?=^FROM node AS web\n)/m)?.[0];
  const frontend = dockerfile.split('\n')[0];
  if (!native || !collector) fail('native Docker recipe not found');
  const recipe_sha256 = sha(JSON.stringify({ platform, frontend, collector, native,
    context_rules_sha256: { root: sha(read('.dockerignore')), dockerfile: sha(dockerIgnore) },
    tools: entries.filter(entry => entry.path.startsWith('deploy/docker/')) }));
  return { schema_version: 1, platform, recipe_sha256,
    input_sha256: sha(JSON.stringify({ platform, recipe_sha256, entries })), entries };
}

export function prepare(source, destination, platform = PLATFORM) {
  const manifest = identity(source, platform);
  // A new output directory avoids retaining any stale/removed source input.
  fs.mkdirSync(destination);
  for (const entry of manifest.entries) {
    const target = path.join(destination, entry.path);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.copyFileSync(path.join(source, entry.path), target);
    fs.chmodSync(target, entry.mode);
  }
  fs.writeFileSync(path.join(destination, '.native-input.sha256'), manifest.input_sha256 + '\n');
  fs.writeFileSync(path.join(destination, '.native-recipe.sha256'), manifest.recipe_sha256 + '\n');
  return manifest;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, source, destinationOrPlatform, platform] = process.argv.slice(2);
  if (command === 'prepare' && source && destinationOrPlatform) {
    console.log(JSON.stringify(prepare(source, destinationOrPlatform, platform), null, 2));
  } else if (command === 'identity' && source) {
    console.log(JSON.stringify(identity(source, destinationOrPlatform), null, 2));
  } else fail('usage: native-inputs.mjs identity SOURCE [PLATFORM] | prepare SOURCE DESTINATION [PLATFORM]');
}
