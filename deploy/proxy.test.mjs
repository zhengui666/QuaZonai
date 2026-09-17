import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { createServer as createSocket } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';

// Real Caddy, synthetic HTTP peer. These tests prove gateway behavior, not
// authentication, database, scientific validity or production acceptance.
const binary = process.env.CADDY_BIN || 'caddy';
const index = '<!doctype html><title>gateway fixture</title><main>fixture-shell</main>';
const headerNames = ['Content-Security-Policy', 'X-Content-Type-Options', 'X-Frame-Options', 'Referrer-Policy', 'Permissions-Policy'];

async function unusedPort() {
  const socket = createSocket();
  socket.listen(0, '127.0.0.1');
  await once(socket, 'listening');
  const port = socket.address().port;
  await new Promise((resolve, reject) => socket.close((error) => error ? reject(error) : resolve()));
  return port;
}

test('native personal-hosting gateway', { timeout: 45_000 }, async (t) => {
  // Compare actual response values with the existing console policy, not a new one.
  const established = await readFile(new URL('../apps/web/Caddyfile', import.meta.url), 'utf8');
  const staticHeaders = headerNames.map((name) => {
    const match = established.match(new RegExp(`^\\s*${name}\\s+"([^"]+)"\\s*$`, 'm'));
    assert.ok(match, `Established header missing: ${name}`);
    return [name, match[1]];
  });
  const assertStaticHeaders = (response) => {
    for (const [name, value] of staticHeaders) assert.equal(response.headers.get(name), value, `${response.url}: ${name}`);
  };
  const root = await mkdtemp(join(tmpdir(), 'quazonai-gateway-'));
  let child;
  let exited;
  let spawnError;
  let logs = '';
  const received = [];
  const upstream = createServer(async (request, response) => {
    let body = '';
    for await (const chunk of request) body += chunk.toString('utf8');
    const receipt = {
      method: request.method, url: request.url, body,
      host: request.headers.host, origin: request.headers.origin,
      key: request.headers['idempotency-key'],
    };
    received.push(receipt);
    if (request.url === '/health/live') {
      response.writeHead(204).end();
    } else if (request.url === '/api/events') {
      response.writeHead(200, { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-store' });
      response.write('data: first-event\n\n');
      // Deliberately leave the upstream open. A buffering proxy will time out.
    } else if (request.url?.startsWith('/api/echo?')) {
      response.writeHead(200, { 'Content-Type': 'application/json',
        'Content-Security-Policy': "default-src 'none'", 'Cache-Control': 'no-store' }).end(JSON.stringify(receipt));
    } else {
      const status = request.url === '/api/unavailable' ? 503 : 404;
      response.writeHead(status, { 'Content-Type': 'application/problem+json' })
        .end(JSON.stringify({ status, detail: 'upstream-fixture-error' }));
    }
  });

  t.after(async () => {
    if (child?.pid && child.exitCode === null && child.signalCode === null) {
      child.kill('SIGTERM');
      await Promise.race([exited, delay(1500, undefined, { ref: false })]);
      if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
      await exited;
    }
    upstream.closeAllConnections();
    if (upstream.listening) await new Promise((resolve) => upstream.close(resolve));
    await rm(root, { recursive: true, force: true });
  });

  const web = join(root, 'web');
  await mkdir(join(web, 'assets'), { recursive: true });
  await writeFile(join(web, 'index.html'), index);
  await writeFile(join(web, 'assets', 'app.js'), 'window.fixtureLoaded = true;');
  await writeFile(join(web, 'sw.js'), '// service-worker-fixture');
  await writeFile(join(web, 'manifest.webmanifest'), '{"name":"fixture"}');
  await writeFile(join(web, 'robots.txt'), 'User-agent: *\nDisallow: /\n');
  upstream.listen(0, '127.0.0.1');
  await once(upstream, 'listening');
  const port = await unusedPort();
  const origin = `http://127.0.0.1:${port}`;
  const configuration = join(root, 'Caddyfile');
  const actual = await readFile(new URL('./Caddyfile', import.meta.url), 'utf8');
  // Only process administration and certificate issuance differ in the test.
  await writeFile(configuration, `{\n\tadmin off\n\tauto_https off\n}\n${actual}`);
  const env = {
    ...process.env,
    QUAZONAI_SITE: origin,
    QUAZONAI_WEB_ROOT: web,
    QUAZONAI_API_UPSTREAM: `127.0.0.1:${upstream.address().port}`,
    XDG_DATA_HOME: join(root, 'data'), XDG_CONFIG_HOME: join(root, 'config'),
  };
  execFileSync(binary, ['validate', '--config', configuration, '--adapter', 'caddyfile'], {
    env, timeout: 10_000, stdio: 'pipe',
  });
  child = spawn(binary, ['run', '--config', configuration, '--adapter', 'caddyfile'], {
    env, stdio: ['ignore', 'pipe', 'pipe'],
  });
  exited = new Promise((resolve) => {
    child.once('exit', resolve);
    child.once('error', (error) => { spawnError = error; resolve(); });
  });
  for (const stream of [child.stdout, child.stderr]) {
    stream.on('data', (chunk) => { logs = (logs + chunk.toString()).slice(-8192); });
  }
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (spawnError || child.exitCode !== null || child.signalCode !== null) break;
    try {
      const response = await fetch(origin, { signal: AbortSignal.timeout(500) });
      ready = response.status === 200 && await response.text() === index;
      if (ready) break;
    } catch { /* A newly spawned listener can still be starting. */ }
    await delay(50);
  }
  assert.ok(ready, `Caddy did not start: ${spawnError?.message ?? ''}\n${logs}`);

  await t.test('serves the actual shell, deep navigation and HEAD', async () => {
    const response = await fetch(`${origin}/research/fixture`, { headers: { Accept: 'text/html' } });
    assert.equal(response.status, 200);
    assert.equal(await response.text(), index);
    assert.equal(response.headers.get('cache-control'), 'no-cache');
    assertStaticHeaders(response);
    const head = await fetch(`${origin}/research/fixture`, { method: 'HEAD', headers: { Accept: 'text/html' } });
    assert.equal(head.status, 200);
    assert.equal(await head.text(), '');
    assertStaticHeaders(head);
  });
  await t.test('PWA files remain real files and missing assets never become HTML', async () => {
    for (const path of ['/sw.js', '/manifest.webmanifest', '/assets/app.js', '/robots.txt']) {
      const response = await fetch(origin + path);
      assert.equal(response.status, 200);
      assert.equal(response.headers.get('cache-control'), 'no-cache');
      assertStaticHeaders(response);
      assert.ok(!(await response.text()).includes('fixture-shell'));
    }
    for (const path of ['/assets/missing.js', '/missing.js', '/missing.webmanifest', '/assets/missing']) {
      const response = await fetch(origin + path, { headers: { Accept: 'text/html' } });
      assert.equal(response.status, 404, path);
      assertStaticHeaders(response);
      assert.ok(!(await response.text()).includes('fixture-shell'));
    }
  });
  await t.test('preserves method, raw path/query, body, Host, Origin and idempotency key', async () => {
    const path = '/api/echo?case=1&next=%2F';
    const body = JSON.stringify({ fixture: 'original-request' });
    const response = await fetch(origin + path, {
      method: 'POST', body,
      headers: { Origin: origin, 'Content-Type': 'application/json', 'Idempotency-Key': 'fixture-key' },
    });
    assert.equal(response.status, 200);
    assert.equal(response.headers.get('content-security-policy'), "default-src 'none'");
    assert.equal(response.headers.get('cache-control'), 'no-store');
    for (const name of headerNames.slice(1)) assert.equal(response.headers.get(name), null, `API must own ${name}`);
    assert.deepEqual(await response.json(), {
      method: 'POST', url: path, body, host: `127.0.0.1:${port}`, origin, key: 'fixture-key',
    });
  });
  await t.test('keeps backend 404/503 responses and never replays a mutation', async () => {
    for (const path of ['/api', '/health', '/api/missing', '/health/missing', '/api/unavailable']) {
      const count = received.length;
      const response = await fetch(origin + path, { method: 'POST', body: 'fixture', headers: { Accept: 'text/html' } });
      const status = path === '/api/unavailable' ? 503 : 404;
      assert.equal(response.status, status);
      assert.equal(response.headers.get('content-type'), 'application/problem+json');
      for (const name of headerNames) assert.equal(response.headers.get(name), null, `${path} must retain upstream headers`);
      assert.deepEqual(await response.json(), { status, detail: 'upstream-fixture-error' });
      assert.equal(received.length, count + 1);
    }
    const live = await fetch(`${origin}/health/live`);
    assert.equal(live.status, 204);
    for (const name of headerNames) assert.equal(live.headers.get(name), null);
  });
  await t.test('does not serve the shell for non-navigation methods', async () => {
    const response = await fetch(`${origin}/research/fixture`, { method: 'POST', headers: { Accept: 'text/html' } });
    assert.ok([404, 405].includes(response.status));
    assert.ok(!(await response.text()).includes('fixture-shell'));
  });
  await t.test('streams the first SSE event while the upstream is still open', async () => {
    const abort = new AbortController();
    const timer = setTimeout(() => abort.abort(), 3000);
    try {
      const response = await fetch(`${origin}/api/events`, { signal: abort.signal });
      assert.equal(response.status, 200);
      assert.match(response.headers.get('content-type'), /^text\/event-stream/);
      const reader = response.body.getReader();
      let text = '';
      while (!text.includes('\n\n')) {
        const chunk = await reader.read();
        assert.equal(chunk.done, false);
        text += new TextDecoder().decode(chunk.value);
      }
      assert.equal(text, 'data: first-event\n\n');
      await reader.cancel();
    } finally {
      clearTimeout(timer);
      abort.abort();
    }
  });
  await t.test('an unavailable backend remains an error, not a successful shell', async () => {
    upstream.closeAllConnections();
    await new Promise((resolve) => upstream.close(resolve));
    const response = await fetch(`${origin}/api/missing`, { headers: { Accept: 'text/html' }, signal: AbortSignal.timeout(3000) });
    assert.equal(response.status, 502);
    assert.ok(!(await response.text()).includes('fixture-shell'));
  });
});
