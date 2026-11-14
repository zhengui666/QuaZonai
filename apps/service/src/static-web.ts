import type { FastifyInstance } from 'fastify';
import staticPlugin from '@fastify/static';
import { relative, sep } from 'node:path';

export async function registerStaticWeb(app: FastifyInstance, root: string) {
  await app.register(staticPlugin, { root, preCompressed: true, cacheControl: false, setHeaders(response, file) {
    const name = relative(root, file).split(sep).join('/');
    response.setHeader('Vary', 'Accept-Encoding');
    // Only Vite content-addressed assets may survive an application update.
    if (/^assets\/[^/]+-[\w-]{8,}\.(?:js|css|woff2?|svg|png|jpe?g|webp|avif)(?:\.(?:br|gz))?$/.test(name)) {
      response.setHeader('Cache-Control', 'public, max-age=31536000, immutable');
    } else response.setHeader('Cache-Control', 'no-cache');
  } });
  app.setNotFoundHandler((request, reply) => {
    const pathname = new URL(request.url, 'http://localhost').pathname;
    // Never return HTML for an expired deployment's lazy JS/CSS URL.
    if (pathname.startsWith('/api/') || pathname.startsWith('/assets/') || /\.[^/]+$/.test(pathname)
      || !['GET', 'HEAD'].includes(request.method)) return reply.code(404).send({ error: 'not_found' });
    return reply.sendFile('index.html');
  });
}
