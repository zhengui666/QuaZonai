#!/usr/bin/env node
/**
 * Real browser -> deployment Caddy -> packaged Rust -> PostgreSQL acceptance.
 * Shipped systemd user units run the real API/Worker; verify idle Worker automatic
 * restart and retained sessions/original command receipts after an actual API stop/start.
 * Requires an explicitly supplied, disposable loopback PostgreSQL administrator.
 * Never reads .env, reuses an existing application database, or seeds domain rows.
 */
import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { chmod, copyFile, cp, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { NativeUserServices } from './native-user-services.mjs';

const web = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repo = resolve(web, '../..');
const report = resolve(process.env.QUAZONAI_WEB_TEST_REPORT_DIR ?? resolve(web, 'test-results/native-summary'));
const binary = resolve(process.env.QUAZONAI_WEB_TEST_BIN ?? resolve(repo, 'target/debug/server'));
const psql = process.env.QUAZONAI_WEB_TEST_PSQL ?? 'psql';
const caddy = process.env.CADDY_BIN ? resolve(process.env.CADDY_BIN) : 'caddy';
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
const screenshots = [];
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
let userServices;
let privateArtifactsRetained = false;

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
  if (options.recordStage !== false) stages.push({ name, exit_code: result.code, signal: result.signal, duration_ms: Date.now() - started });
  if (name.startsWith('browser')) await privateRedactions();
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
    if (services.some((service) => service.exited && !service.retired)) throw new Error('A test-owned service exited before readiness');
    try {
      const response = await fetch(`${baseUrl}/health/live`, { signal: AbortSignal.timeout(2_000) });
      if (response.status === 204) return;
    } catch (error) {
      if (error.message?.startsWith('Application health')) throw error;
    }
    await new Promise((fulfil) => setTimeout(fulfil, 250));
  }
  throw new Error('Real Rust application readiness timed out');
}

function cleanup(graceful) {
  cleanupPromise ??= (async () => {
    let failure;
    let processesStopped = true;
    if (userServices) {
      try { await userServices.cleanup({ graceful }); }
      catch (error) { failure ??= error; }
      processesStopped = userServices.quiescent;
      try {
        for (const unit of userServices.units) {
          if (unit.log !== undefined) await writeFile(resolve(report, `native-${unit.kind}.log`),
            diagnosticsSafe ? redact(unit.log) : 'Diagnostics withheld: private redaction manifest unavailable.\n',
            { mode: 0o600 });
        }
        await writeFile(resolve(report, 'native-user-services.json'),
          JSON.stringify(userServices.evidence, null, 2), { mode: 0o600 });
      } catch (error) { failure ??= error; }
    }
    for (const service of [...services].reverse()) {
      try {
        terminate(service, 'SIGTERM');
        const force = setTimeout(() => terminate(service, 'SIGKILL'), 5_000);
        await service.done;
        clearTimeout(force);
        await writeFile(resolve(report, `${service.name}.log`), diagnosticsSafe
          ? redact(service.stdout + service.stderr)
          : 'Diagnostics withheld: private redaction manifest unavailable.\n', { mode: 0o600 });
      } catch (error) { processesStopped = false; failure ??= error; }
    }
    if (!processesStopped) {
      privateArtifactsRetained = Boolean(privateDir);
      throw failure ?? new Error('Test service stop could not be verified; private state retained');
    }
    if (databaseCreationAttempted) {
      try { await sql('drop-owned-database', `DROP DATABASE IF EXISTS "${database}" WITH (FORCE);\n`, adminEnv, true); }
      catch (error) { failure ??= error; }
    }
    if (roleCreationAttempted) {
      try { await sql('drop-owned-role', `DROP ROLE IF EXISTS "${role}";\n`, adminEnv, true); }
      catch (error) { failure ??= error; }
    }
    if (failure) {
      privateArtifactsRetained = Boolean(privateDir);
      throw failure;
    }
    if (privateDir) {
      privateArtifactsRetained = true;
      await rm(privateDir, { recursive: true, force: true });
      privateArtifactsRetained = false;
    }
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
  privateArtifactsRetained = true;
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
  const backendPort = await freePort();
  let frontendPort = await freePort();
  while (frontendPort === backendPort) frontendPort = await freePort();
  const baseUrl = `http://127.0.0.1:${frontendPort}`;
  // Exercise a private release copy, not the source checkout or Vite preview.
  const release = resolve(privateDir, 'release');
  const installedBinary = resolve(release, 'bin/server');
  const publicFiles = resolve(release, 'web');
  await mkdir(dirname(installedBinary), { recursive: true, mode: 0o755 });
  await copyFile(binary, installedBinary);
  await chmod(installedBinary, 0o755);
  await cp(resolve(web, 'dist'), publicFiles, { recursive: true, errorOnExist: true, force: false });
  const index = await readFile(resolve(publicFiles, 'index.html'), 'utf8');
  if (!index.includes('<html') || index.includes('/@vite/client')) throw new Error('Expected the real production web build');
  stages.push({ name: 'package-native-release', exit_code: 0 });

  const gatewayConfig = resolve(privateDir, 'Caddyfile');
  const actualConfig = await readFile(resolve(repo, 'deploy/Caddyfile'), 'utf8');
  // Only TLS issuance and the administrator listener are disabled on this
  // disposable loopback fixture. The actual production route/policy stays intact.
  await writeFile(gatewayConfig, `{\n  admin off\n  auto_https off\n}\n${actualConfig}`, { mode: 0o600 });
  const gatewayEnv = {
    ...childEnv, QUAZONAI_SITE: baseUrl, QUAZONAI_WEB_ROOT: publicFiles,
    QUAZONAI_API_UPSTREAM: `127.0.0.1:${backendPort}`,
    XDG_DATA_HOME: resolve(privateDir, 'caddy-data'), XDG_CONFIG_HOME: resolve(privateDir, 'caddy-config'),
  };
  await run('caddy-version', caddy, ['version'], { env: gatewayEnv, timeout: 10_000 });
  await run('caddy-validate', caddy, ['validate', '--config', gatewayConfig, '--adapter', 'caddyfile'],
    { env: gatewayEnv, timeout: 10_000 });
  userServices = new NativeUserServices({
    repo, root: privateDir, release, binary: installedBinary, run, env: childEnv,
    interrupted: () => stopping,
  });
  await userServices.install({
    DATABASE_URL: applicationUrl, STATE_DIR: state, BIND: `127.0.0.1:${backendPort}`,
    PUBLIC_URL: baseUrl, DEVELOPMENT_HTTP: 'true', WORKER_PARALLELISM: '1',
    RUNTIME_TARGETS: '[]', DOWNSTREAM_TARGETS: '[]', RUST_LOG: 'warn',
  });
  const first = await userServices.start('api');
  const firstWorker = await userServices.start('worker');
  const gateway = launch(caddy, ['run', '--config', gatewayConfig, '--adapter', 'caddyfile'], { env: gatewayEnv, cwd: release });
  gateway.name = 'caddy'; services.push(gateway);
  await waitReady(baseUrl);
  stages.push({ name: 'real-api-ready', exit_code: 0 });

  const fixture = resolve(privateDir, 'fixture.json');
  const browser = async (phase) => {
    await writeFile(fixture, JSON.stringify({ baseUrl, phase, redactionsFile }), { mode: 0o600 });
    await run(`browser-${phase}`, process.execPath, [resolve(web, 'node_modules/@playwright/test/cli.js'),
      'test', '--config', 'playwright.native.config.ts'], {
      cwd: web, timeout: 240_000,
      env: { ...childEnv, QUAZONAI_WEB_E2E_FIXTURE: fixture, QUAZONAI_WEB_E2E_ORIGIN: baseUrl },
    });
  };
  await browser('before-restart');

  await userServices.assertRunning('api', first);
  // This browser scenario creates a project, not research. Test the supervisor
  // with an actually idle Worker; active-job recovery has separate native tests.
  await sql('require-idle-native-worker',
    "DO $idle$ BEGIN IF EXISTS(SELECT 1 FROM app.runs) THEN RAISE EXCEPTION 'Expected an idle Worker fixture'; END IF; END $idle$;\n",
    { ...adminEnv, PGDATABASE: database });
  const restartedWorker = await userServices.crashWorker(firstWorker);

  // The gateway/database/state/cookie key remain unchanged. Only the same API
  // unit stops and starts; systemd must observe a normal exit, not a forced kill.
  const oldPid = first.pid;
  await userServices.stop('api');
  const unavailable = await fetch(`${baseUrl}/health/live`, { signal: AbortSignal.timeout(5_000) });
  if (unavailable.status !== 502 || (await unavailable.text()).includes('<html')) {
    throw new Error('A stopped API must remain a gateway error, not the SPA shell');
  }
  const shell = await fetch(baseUrl, { headers: { Accept: 'text/html' }, signal: AbortSignal.timeout(5_000) });
  if (shell.status !== 200 || await shell.text() !== index) throw new Error('The real static shell must remain available during API downtime');
  stages.push({ name: 'gateway-with-stopped-api', exit_code: 0 });

  const restarted = await userServices.start('api');
  if (restarted.pid === oldPid || restarted.invocation === first.invocation) throw new Error('API restart did not create a new invocation');
  await waitReady(baseUrl);
  stages.push({ name: 'restarted-api-ready', exit_code: 0, previous_pid: oldPid, current_pid: restarted.pid });
  await browser('after-restart');
  await userServices.assertRunning('api', restarted);
  await userServices.assertRunning('worker', restartedWorker);
  for (const mode of ['light', 'dark']) {
    for (const width of [1440, 768, 390]) {
      const name = `projects-${mode}-${width}.png`;
      screenshots.push({ name, bytes: await readFile(resolve(privateDir, name)) });
    }
  }
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
try { await cleanup(!failure && !stopping); }
catch (error) { failure ??= error; }
if (adminEnv) {
  // Failed tests or cleanup never publish images from the private runtime.
  if (!failure) for (const { name, bytes } of screenshots) {
    await writeFile(resolve(report, name), bytes, { mode: 0o600 });
  }
  await writeFile(resolve(report, 'result.json'), JSON.stringify({ schema_version: 1,
    status: failure ? 'FAILED' : 'PASSED', stages,
    error: failure ? redact(failure.message) : null,
    acceptance_scope: 'shipped systemd user units with real packaged API/Worker and production Caddy routes; idle Worker native automatic restart, direct local entry, retained session/project/receipt/theme after normal API stop/start, CSRF, both themes in three viewports and absent legacy login routes; no public TLS, host boot, active-job restore or complete Issue62 acceptance',
    private_artifacts_retained: privateArtifactsRetained,
    screenshots: failure ? [] : screenshots.map(({ name }) => name),
  }, null, 2), { mode: 0o600 });
}
if (failure) {
  console.error(redact(failure.message));
  process.exitCode = interruptedExitCode ?? 1;
} else {
  console.info(`Real browser/API acceptance passed. Sanitized evidence: ${report}`);
}
