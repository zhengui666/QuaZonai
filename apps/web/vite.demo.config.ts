import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { VitePWA } from 'vite-plugin-pwa';
import { createRequire } from 'node:module';
import { projectEditor } from './demo/project-editor';
import { demoResponse, records } from './demo/records';

const { validateResponse } = createRequire(import.meta.url)('./src/generated/responses.cjs') as typeof import('./src/generated/responses.cjs');

export default defineConfig({
  envDir: false,
  optimizeDeps: { include: ['@quazonai/web/response-contract'] },
  server: { host: '127.0.0.1', port: 4179, strictPort: true },
  plugins: [react(), VitePWA({ devOptions: { enabled: false } }), {
    name: 'synthetic-preview',
    configResolved(config) {
      if (config.command !== 'serve') throw new Error('Synthetic preview is not a production build');
      for (const { contract, value } of records.values()) {
        if (!validateResponse(contract, 'get', 200, value, 'application/json')) {
          throw new Error(`Invalid synthetic response: ${contract}`);
        }
      }
    },
    transformIndexHtml() {
      return [{ tag: 'aside', attrs: { role: 'note', 'aria-label': '合成预览说明',
        style: 'padding:12px;background:#fff3cd;color:#3b2e00;font:16px/1.5 sans-serif' },
      children: 'SYNTHETIC / FIXTURE · 合成界面预览，尚非完整 Demo。项目名称和说明可临时编辑（同一预览实例共享，重启清空）；其他写入禁用。PASS、资格及权重都是未经计算的假设记录；没有真实账号或交付，请勿输入凭据。', injectTo: 'body-prepend' }];
    },
    configureServer(server) {
      const edit = projectEditor();
      // This server has no upstream proxy or database connection.
      server.middlewares.use(async (request, response, next) => {
        const url = new URL(request.url ?? '/', 'http://127.0.0.1');
        const pathname = url.pathname;
        if (!pathname.startsWith('/api/')) return next();
        let body: unknown;
        if (request.method === 'PATCH') {
          const origin = request.headers.origin;
          if (origin && origin !== `http://${request.headers.host}`) { response.writeHead(403); response.end(); return; }
          try {
            const chunks: Buffer[] = []; let size = 0;
            for await (const chunk of request) {
              const bytes = Buffer.from(chunk); size += bytes.length; chunks.push(bytes);
              if (size > 16384) { response.writeHead(413); response.end(); return; }
            }
            body = JSON.parse(Buffer.concat(chunks).toString('utf8'));
          } catch { response.writeHead(400); response.end(); return; }
        }
        const key = request.headers['idempotency-key'];
        const result = edit(request.method ?? 'GET', pathname, body, typeof key === 'string' ? key : undefined) ?? demoResponse(request.method ?? 'GET', pathname, url.searchParams.get('partition'));
        response.writeHead(result.status, { 'Content-Type': 'binary' in result ? 'application/octet-stream' : result.status === 200 ? 'application/json' : 'application/problem+json',
          'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' });
        response.end('binary' in result ? result.value : JSON.stringify(result.value));
      });
    },
  }],
});
