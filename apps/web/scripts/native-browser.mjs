#!/usr/bin/env node
/**
 * Real browser -> Vite preview -> Rust -> PostgreSQL acceptance.
 * Requires an explicitly supplied, disposable loopback PostgreSQL administrator.
 * Never reads .env, reuses an existing application database, or seeds domain rows.
 */
import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const web = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repo = resolve(web, '../..');
const report = resolve(process.env.QUAZONAI_WEB_TEST_REPORT_DIR ?? resolve(web, 'test-results/native-summary'));
const binary = resolve(process.env.QUAZONAI_WEB_TEST_BIN ?? resolve(repo, 'target/debug/server'));
const psql = process.env.QUAZONAI_WEB_TEST_PSQL ?? 'psql';
// Child processes receive only tooling essentials, never the administrator URL,
// ambient database credentials, GitHub tokens, or the invoking shell's secrets.
const childEnv = Object.fromEntries([
  'PATH', 'HOME', 'USER', 'LOGNAME', 'TMPDIR', 'TMP', 'TEMP', 'LANG', 'LC_ALL',
  'TZ', 'CI', 'NO_COLOR', 'PLAYWRIGHT_BROWSERS_PATH', 'XDG_CACHE_HOME',
  'XDG_RUNTIME_DIR', 'LD_LIBRARY_PATH',
].filter((key) => process.env[key] !== undefined).map((key) => [key, process.env[key]]));
const privateValues = new Set();
const stages = [];
const services = [];
const commands = new Set();
let diagnosticsSafe = true;
let interruptedExitCode;
let privateDir;
let redactionsFile;
let adminEnv;
let database;
let role;
// Creation may commit even when the client never receives its acknowledgement.
let databaseCreationAttempted = false;
let roleCreationAttempted = false;
let cleanupPromise;
let stopping = false;

function redact(value) {
  let text = String(value);
  for (const secret of [...privateValues].sort((a, b) => b.length - a.length)) {
    if (secret) text = text.split(secret).join('[REDACTED]');
  }
  return text.replace(/otpauth:\/\/[^\s"'<>]+/gi, '[REDACTED OTP URI]')
    .replace(/\b[0-9]{6}\b/g, '[REDACTED SIX DIGITS]');
}

async function privateRedactions() {
  if (!redactionsFile) return;
  try {
    for (const line of (await readFile(redactionsFile, 'utf8')).split('\n').filter(Boolean)) {
      const value = JSON.parse(line);
      if (typeof value === 'string') privateValues.add(value);
    }
  } catch (error) {
    // Fail closed: do not publish raw browser diagnostics when their redaction
    // manifest cannot be read. Only the exit code remains available.
    diagnosticsSafe = false;
    throw new Error(`Private test diagnostic manifest unavailable: ${error.code ?? 'invalid manifest'}`);
  }
}

function launch(command, args, { env = childEnv, cwd = repo, stdin = '' } = {}) {
  const child = spawn(command, args, {
    cwd, env, detached: process.platform !== 'win32', stdio: ['pipe', 'pipe', 'pipe'],
  });
  const record = { child, exited: false, stdout: '', stderr: '', overflow: false };
  const collect = (key) => (chunk) => {
    if (record[key].length + chunk.length > 4 * 1024 * 1024) {
      record.overflow = true;
      terminate(record, 'SIGTERM');
    } else record[key] += chunk.toString('utf8');
  };
  child.stdout.on('data', collect('stdout'));
  child.stderr.on('data', collect('stderr'));
  record.done = new Promise((fulfil) => {
    child.once('error', (error) => {
      record.exited = true;
      record.stderr += `${error.code ?? 'SPAWN_ERROR'}: process could not start`;
      fulfil({ code: 127, signal: null });
    });
    child.once('close', (code, signal) => {
      record.exited = true;
      fulfil({ code: code ?? 128, signal });
    });
  });
  child.stdin.on('error', () => { /* The process exit is the authoritative result. */ });
  child.stdin.end(stdin);
  return record;
}

function terminate(record, signal) {
  if (record.exited || !record.child.pid) return;
  try {
    if (process.platform === 'win32') record.child.kill(signal);
    else process.kill(-record.child.pid, signal); // Only this invocation's process group.
  } catch (error) { if (error.code !== 'ESRCH') throw error; }
}

async function run(name, command, args, options = {}) {
  if (stopping && !options.cleanup) throw new Error('Native browser acceptance interrupted');
  const started = Date.now();
  const record = launch(command, args, options);
  commands.add(record);
  const timeout = setTimeout(() => terminate(record, 'SIGTERM'), options.timeout ?? 120_000);
  const hardTimeout = setTimeout(() => terminate(record, 'SIGKILL'), (options.timeout ?? 120_000) + 5_000);
  const result = await record.done;
  commands.delete(record);
  clearTimeout(timeout); clearTimeout(hardTimeout);
  stages.push({ name, exit_code: result.code, signal: result.signal, duration_ms: Date.now() - started });
  if (name === 'browser') await privateRedactions();
  if (!options.privateOutput) {
    await writeFile(resolve(report, `${name}.log`), redact(record.stdout + record.stderr), { mode: 0o600 });
  }
  if (result.code !== 0 || record.overflow) {
    throw new Error(`${name} failed (exit ${result.code}${record.overflow ? ', output limit exceeded' : ''})`);
  }
  return record.stdout;
}

function databaseUrl(admin, name, username = admin.username, password = admin.password) {
  const result = new URL(admin.href);
  result.username = username; result.password = password; result.pathname = `/${name}`;
  return result.href;
}

async function sql(name, text, env = adminEnv, cleanup = false) {
  return run(name, psql, ['--no-psqlrc', '--set', 'ON_ERROR_STOP=1', '--quiet'], {
    env, stdin: text, timeout: 30_000, cleanup,
  });
}

async function freePort() {
  const server = createServer();
  await new Promise((fulfil, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', fulfil);
  });
  const address = server.address();
  await new Promise((fulfil, reject) => server.close((error) => error ? reject(error) : fulfil()));
  if (!address || typeof address === 'string') throw new Error('Loopback port allocation failed');
  return address.port;
}

async function waitReady(baseUrl) {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline && !stopping) {
    if (services.some((service) => service.exited)) throw new Error('A test-owned service exited before readiness');
    try {
      const response = await fetch(`${baseUrl}/api/v2/bootstrap/status`, { signal: AbortSignal.timeout(2_000) });
      if (response.ok) {
        const body = await response.json();
        if (body.schema_version === 1 && body.initialized === false && body.setup_allowed === true) return;
        throw new Error('Fresh application did not expose the expected uninitialized state');
      }
    } catch (error) {
      if (error.message?.startsWith('Fresh application')) throw error;
    }
    await new Promise((fulfil) => setTimeout(fulfil, 250));
  }
  throw new Error('Real Rust application readiness timed out');
}

function cleanup() {
  cleanupPromise ??= (async () => {
    let failure;
    for (const service of [...services].reverse()) {
      try {
        terminate(service, 'SIGTERM');
        const force = setTimeout(() => terminate(service, 'SIGKILL'), 5_000);
        await service.done;
        clearTimeout(force);
        await writeFile(resolve(report, `${service.name}.log`), diagnosticsSafe
          ? redact(service.stdout + service.stderr)
          : 'Diagnostics withheld: private redaction manifest unavailable.\n', { mode: 0o600 });
      } catch (error) { failure ??= error; }
    }
    if (databaseCreationAttempted) {
      try { await sql('drop-owned-database', `DROP DATABASE IF EXISTS "${database}" WITH (FORCE);\n`, adminEnv, true); }
      catch (error) { failure ??= error; }
    }
    if (roleCreationAttempted) {
      try { await sql('drop-owned-role', `DROP ROLE IF EXISTS "${role}";\n`, adminEnv, true); }
      catch (error) { failure ??= error; }
    }
    if (privateDir) await rm(privateDir, { recursive: true, force: true });
    if (failure) throw failure;
  })();
  return cleanupPromise;
}

async function main() {
  const input = process.env.QUAZONAI_WEB_TEST_ADMIN_URL;
  if (!input) throw new Error('Set QUAZONAI_WEB_TEST_ADMIN_URL explicitly to a disposable loopback PostgreSQL administrator');
  const admin = new URL(input);
  if (!['postgres:', 'postgresql:'].includes(admin.protocol)
    || !['127.0.0.1', 'localhost', '[::1]'].includes(admin.hostname)
    || !admin.port || admin.pathname !== '/postgres' || admin.search || admin.hash
    || !admin.username || !admin.password) {
    throw new Error('Test administrator must be a password-bearing, explicit-port loopback PostgreSQL /postgres URL without query or fragment');
  }
  if (!isAbsolute(binary)) throw new Error('Native application binary path must be absolute');
  privateValues.add(input); privateValues.add(decodeURIComponent(admin.password));
  const suffix = randomBytes(12).toString('hex');
  database = `web_e2e_${suffix}`; role = `web_app_${suffix}`;
  const password = randomBytes(32).toString('hex');
  privateValues.add(password);
  adminEnv = { ...childEnv, PGHOST: admin.hostname.replace(/^\[|\]$/g, ''), PGPORT: admin.port,
    PGUSER: decodeURIComponent(admin.username), PGPASSWORD: decodeURIComponent(admin.password), PGDATABASE: 'postgres',
    PGSSLMODE: 'disable', PGCONNECT_TIMEOUT: '5', PGAPPNAME: 'quazonai-web-native-acceptance',
    PGOPTIONS: '-c statement_timeout=15000 -c lock_timeout=5000',
  };
  await mkdir(report, { recursive: true, mode: 0o700 });
  privateDir = await mkdtemp(resolve(tmpdir(), 'quazonai-web-native-'));
  redactionsFile = resolve(privateDir, 'redactions.jsonl');
  await writeFile(redactionsFile, '', { mode: 0o600 });
  const ownerUrl = databaseUrl(admin, database);
  const applicationUrl = databaseUrl(admin, database, role, password);
  privateValues.add(ownerUrl); privateValues.add(applicationUrl);
  const ownerEnv = { ...childEnv, DATABASE_URL: ownerUrl };
  const applicationEnv = { ...childEnv, DATABASE_URL: applicationUrl, RUST_LOG: 'warn' };
  await sql('require-fresh-names', `DO $fresh$ BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname='${role}')
       OR EXISTS (SELECT 1 FROM pg_database WHERE datname='${database}') THEN
      RAISE EXCEPTION 'Native acceptance resource name collision';
    END IF;
  END $fresh$;\n`);
  roleCreationAttempted = true;
  await sql('create-owned-role', `CREATE ROLE "${role}" LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION PASSWORD '${password}';\n`);
  databaseCreationAttempted = true;
  await sql('create-owned-database', `CREATE DATABASE "${database}";\n`);
  await run('migrations', binary, ['migrate', '--application-role', role], { env: ownerEnv });
  const state = resolve(privateDir, 'state');
  await run('init-state', binary, ['init-state', '--state-dir', state], { env: applicationEnv });
  const bootstrap = JSON.parse(await run('bootstrap', binary, ['bootstrap'], { env: applicationEnv, privateOutput: true }));
  if (bootstrap.schema_version !== 1 || typeof bootstrap.capability_id !== 'string'
    || typeof bootstrap.capability !== 'string' || bootstrap.capability.length !== 43) {
    throw new Error('Native bootstrap command returned an invalid contract');
  }
  privateValues.add(bootstrap.capability); privateValues.add(bootstrap.capability_id);
  const backendPort = await freePort();
  let frontendPort = await freePort();
  while (frontendPort === backendPort) frontendPort = await freePort();
  const baseUrl = `http://127.0.0.1:${frontendPort}`;
  const server = launch(binary, ['serve', '--state-dir', state, '--bind', `127.0.0.1:${backendPort}`,
    '--public-url', baseUrl, '--development-http'], { env: applicationEnv });
  server.name = 'rust-server'; services.push(server);
  const preview = launch(process.execPath, [resolve(web, 'node_modules/vite/bin/vite.js'), 'preview',
    '--host', '127.0.0.1', '--port', String(frontendPort), '--strictPort'], {
    cwd: web, env: { ...childEnv, QUAZONAI_DEV_API_ORIGIN: `http://127.0.0.1:${backendPort}` },
  });
  preview.name = 'vite-preview'; services.push(preview);
  await waitReady(baseUrl);
  stages.push({ name: 'real-api-ready', exit_code: 0 });
  const fixture = resolve(privateDir, 'fixture.json');
  await writeFile(fixture, JSON.stringify({ baseUrl, capabilityId: bootstrap.capability_id,
    capability: bootstrap.capability, redactionsFile }), { mode: 0o600 });
  await run('browser', process.execPath, [resolve(web, 'node_modules/@playwright/test/cli.js'),
    'test', '--config', 'playwright.native.config.ts'], {
    cwd: web, timeout: 240_000,
    env: { ...childEnv, QUAZONAI_WEB_E2E_FIXTURE: fixture, QUAZONAI_WEB_E2E_ORIGIN: baseUrl },
  });
}

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.once(signal, () => {
    stopping = true;
    interruptedExitCode = signal === 'SIGINT' ? 130 : 143;
    for (const record of [...commands, ...services]) {
      terminate(record, 'SIGTERM');
      setTimeout(() => terminate(record, 'SIGKILL'), 5_000).unref();
    }
    // Settle native command exits and resource bookkeeping before cleanup.
    // Never drop a database while its migration command is still running.
  });
}
let failure;
try { await main(); }
catch (error) { failure = error; }
if (interruptedExitCode) failure ??= new Error('Native browser acceptance interrupted');
try { await privateRedactions(); }
catch (error) { failure ??= error; }
try { await cleanup(); }
catch (error) { failure ??= error; }
if (adminEnv) {
  await writeFile(resolve(report, 'result.json'), JSON.stringify({ schema_version: 1,
    status: failure ? 'FAILED' : 'PASSED', stages,
    error: failure ? redact(failure.message) : null,
    acceptance_scope: 'real first TOTP enrollment, project writes, CSRF, mobile layout and logout; not complete Issue62 acceptance',
    private_artifacts_retained: false,
  }, null, 2), { mode: 0o600 });
}
if (failure) {
  console.error(redact(failure.message));
  process.exitCode = interruptedExitCode ?? 1;
} else {
  console.info(`Real browser/API acceptance passed. Sanitized evidence: ${report}`);
}
