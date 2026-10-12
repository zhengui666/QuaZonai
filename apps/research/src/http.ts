import { createServer } from 'node:http';
import type { ResearchHost } from './host.ts';
import type { Cursor } from '@earendil-works/pi-durable';
import { researchInput, record } from './input.ts';

export function researchServer(host: ResearchHost) {
  let accepting = true;
  const admissions = new Set<Promise<unknown>>();
  return createServer(async (request, response) => {
    response.setHeader('Content-Type', 'application/json');
    try {
      const url = new URL(request.url!, 'http://localhost');
      const path = url.pathname;
      const method = request.method;
      let result: unknown;
      if (method === 'GET' && path === '/health/live') result = {};
      else if (method === 'GET' && path === '/research') {
        const cursor = url.searchParams.get('cursor');
        result = await host.list(cursor ? record(JSON.parse(cursor), 'cursor') as Cursor : undefined);
      }
      else if (method === 'GET' && path === '/models') result = await host.availableModels();
      else if (method === 'GET' && path === '/tasks') result = await host.tasks();
      else if (method === 'GET' && path === '/usage') result = await host.usage();
      else if (method === 'POST' && path === '/maintenance/quiesce') {
        accepting = false;
        await Promise.allSettled([...admissions]);
        result = await host.tasks();
      }
      else if (method === 'POST' && path === '/maintenance/resume') { accepting = true; result = {}; }
      else if (method === 'POST' && path === '/research') {
        const chunks: Buffer[] = [];
        for await (const part of request) chunks.push(part);
        const body = record(JSON.parse(Buffer.concat(chunks).toString('utf8')), 'submission');
        if (typeof body.requestId !== 'string') throw new Error('request_id_required');
        if (!accepting) throw new Error('research_maintenance');
        const admission = host.start(body.requestId, researchInput(body.input));
        admissions.add(admission);
        try { result = await admission; } finally { admissions.delete(admission); }
        response.statusCode = 202;
      } else {
        const match = /^\/research\/(\d+)(\/stop)?$/.exec(path);
        if (!match) { response.statusCode = 404; response.end('{"error":"not_found"}'); return; }
        if (method === 'GET' && !match[2]) result = await host.view(Number(match[1]));
        else if (method === 'POST' && match[2]) { result = await host.stop(Number(match[1])); response.statusCode = 202; }
        else { response.statusCode = 405; response.end('{"error":"method_not_allowed"}'); return; }
      }
      response.end(JSON.stringify(result));
    } catch (error) {
      const message = error instanceof Error ? error.message : 'research_failed';
      if (message === 'request_id_conflict') response.statusCode = 409;
      else if (message === 'research_not_found') response.statusCode = 404;
      else if (message === 'research_maintenance') response.statusCode = 503;
      else if (error instanceof SyntaxError || /(_required|^invalid_|^model_unavailable$)/.test(message)) response.statusCode = 400;
      else response.statusCode = 500;
      response.end(JSON.stringify({ error: response.statusCode === 500 ? 'research_failed' : message }));
    }
  });
}
