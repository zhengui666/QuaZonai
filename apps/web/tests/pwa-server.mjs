// Loopback-only SYNTHETIC browser lifecycle fixture. Not a product API server.
// It serves the real build and varies a comment in the actual generated worker
// to exercise browser update/activation without modifying any product file.
import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { resolve, extname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('../dist/', import.meta.url));
let release = 1;
const mime = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.svg': 'image/svg+xml', '.webmanifest': 'application/manifest+json' };
const policy = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; font-src 'self' data:; worker-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'";
const session = { schema_version: 1, authenticated_at: '2026-09-08T00:00:00Z', expires_at: '2030-09-09T00:00:00Z', trusted_device_id: null, recent_authentication_required: false };
const server = createServer(async (request, response) => {
  const json = (value, status = 200) => { response.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' }); response.end(JSON.stringify(value)); };
  try {
    const path = new URL(request.url, 'http://127.0.0.1:4180').pathname;
    if (path === '/__fixture__/health') return json({ fixture: 'SYNTHETIC', ready: true });
    if (path === '/__fixture__/release' && request.method === 'POST') return json({ release: ++release });
    if (path === '/api/v2/bootstrap/status') return json({ schema_version: 1, initialized: true, setup_allowed: false });
    if (path === '/api/v2/auth/session') return json(session);
    if (path === '/api/v2/projects') return json({ schema_version: 1, items: [], next_cursor: null });
    if (path === '/api/fixture-private') return json({ fixture: 'SYNTHETIC-PRIVATE-CACHE-MARKER' });
    if (path.startsWith('/api/') || !['GET', 'HEAD'].includes(request.method)) return json({ fixture: 'NOT_IMPLEMENTED' }, 404);
    const target = resolve(root, '.' + decodeURIComponent(path === '/' ? '/index.html' : path));
    const location = relative(root, target);
    if (location.startsWith('..') || location.includes('\0') || !(await stat(target)).isFile()) return json({ fixture: 'NOT_FOUND' }, 404);
    let contents = await readFile(target);
    if (path === '/sw.js') contents = Buffer.concat([contents, Buffer.from(`\n// Synthetic lifecycle release ${release}\n`)]);
    response.writeHead(200, {
      'Content-Type': mime[extname(target)] ?? 'application/octet-stream',
      'Cache-Control': path.startsWith('/assets/') ? 'public, max-age=31536000, immutable' : 'no-cache',
      'Content-Security-Policy': policy, 'X-Content-Type-Options': 'nosniff',
      'Referrer-Policy': 'no-referrer', 'X-Frame-Options': 'DENY',
    });
    response.end(request.method === 'HEAD' ? undefined : contents);
  } catch { if (!response.headersSent) json({ fixture: 'NOT_FOUND' }, 404); else response.end(); }
});
server.listen(4180, '127.0.0.1', () => console.log('Synthetic PWA lifecycle fixture listening on loopback:4180'));
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => { server.closeAllConnections(); server.close(() => process.exit(0)); });
