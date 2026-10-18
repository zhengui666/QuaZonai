import Fastify from 'fastify';
import cookie from '@fastify/cookie';
import staticPlugin from '@fastify/static';
import { Pool } from 'pg';
import { mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { Cursor, Extension, TaskId } from '@earendil-works/pi-durable';
import { openResearch } from '../../research/src/host.ts';
import { API } from '../../../packages/contracts/src/index.ts';
import type { ExtensionView, ResearchSummary, WorkspaceView, UsageView } from '../../../packages/contracts/src/index.ts';
import { Application, json, object, observation, portfolio, Problem } from './facts.ts';
import type { ServiceOptions } from './config.ts';
import { science } from './science.ts';
import { registerAuth } from './auth.ts';
import { registerModels } from './models.ts';
import { observationView, portfolioView, researchDetail, researchSummary } from './projections.ts';
import { workbench } from './workbench.ts';

const context = BACKGROUND_CONTEXT;
export async function createService(options: ServiceOptions) {
  const config = structuredClone(options.config);
  await mkdir(config.dataDir, { recursive: true });
  const extensionFile = join(config.dataDir, 'extensions.json');
  let desired: Record<string, boolean> = {};
  let moduleNames: Record<string, string> = {};
  try {
    const saved = JSON.parse(await readFile(extensionFile, 'utf8'));
    desired = saved.enabled ?? saved; moduleNames = saved.modules ?? {};
  } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
  const installed: Extension[] = [science];
  const extensions = new Map<string, ExtensionView>([['qz.science', { name: 'qz.science', version: '1', enabled: true, kind: 'science', state: 'loaded', error: null }]]);
  const requiredExtensions = new Set(['qz.science']);
  function install(extension: Extension, enabled = true, version: string | null = null, kind = 'research') {
    if (extensions.has(extension.name)) throw new Problem('extension_name_conflict');
    const wanted = desired[extension.name] ?? enabled;
    if (wanted) installed.push(extension);
    extensions.set(extension.name, { name: extension.name, version, enabled: wanted, kind, state: wanted ? 'loaded' : 'disabled', error: null });
  }
  for (const extension of options.extensions ?? []) install(extension);
  for (const path of config.research.modules ?? []) {
    const name = moduleNames[path];
    if (name && desired[name] === false) extensions.set(name, { name, version: null, enabled: false, kind: 'research', state: 'disabled', error: null });
    else { const extension = (await import(pathToFileURL(path).href)).default as Extension; moduleNames[path] = extension.name; install(extension); }
  }
  for (const configured of config.extensions) {
    if (extensions.has(configured.name)) throw new Problem('extension_name_conflict');
    const enabled = desired[configured.name] ?? configured.enabled;
    try {
      if (enabled) {
        const extension = (await import(pathToFileURL(configured.module).href)).default as Extension;
        if (extension.name !== configured.name) throw new Problem('extension_module_name_mismatch');
        install(extension, configured.enabled, configured.version ?? null, configured.kind);
      } else extensions.set(configured.name, { name: configured.name, version: configured.version ?? null, enabled: false, kind: configured.kind ?? 'research', state: 'disabled', error: null });
    } catch { extensions.set(configured.name, { name: configured.name, version: configured.version ?? null, enabled, kind: configured.kind ?? 'research', state: 'error', error: 'extension_load_failed' }); }
  }
  const research = await openResearch({ ...config.research, modules: [], extensions: installed, models: options.models, storage: options.storage });
  for (const extension of research.registry.snapshot().installed()) if (!extensions.has(extension.name)) {
    requiredExtensions.add(extension.name);
    extensions.set(extension.name, { name: extension.name, version: null, enabled: true, kind: 'host', state: 'loaded', error: null });
  }
  const app = Fastify({ logger: false, bodyLimit: 1024 * 1024, routerOptions: { ignoreTrailingSlash: true } });
  const archive = config.archiveDatabase ? new Pool({ connectionString: config.archiveDatabase, max: 2 }) : null;
  app.addHook('onClose', async () => { await research.close(); await archive?.end(); });
  try {
  app.setErrorHandler((error, _request, reply) => {
    const message = error instanceof Error ? error.message : 'internal_error';
    const status = error instanceof Problem ? error.statusCode : /conflict|already|trial_budget_exhausted/.test(message) ? 409 : /not_found/.test(message) ? 404 : (error as { statusCode?: number }).statusCode ?? (/invalid|required|unavailable/.test(message) ? 400 : 500);
    const code = error instanceof Problem || /^[a-z][a-z0-9_]+$/.test(message) ? message : status < 500 ? 'invalid_request' : 'internal_error';
    reply.code(status).send({ error: code });
  });
  await app.register(cookie);
  registerAuth(app, research, config);
  await research.harness.commit(async (tx) => {
    const facts = await tx.doc(Application);
    facts.settings ??= json({ defaultModel: config.defaultModel, thinkingLevel: config.thinkingLevel });
  }, context);
  const work = workbench(research, config);
  await work.recoverResearch();
  app.get('/health/live', async (_request, reply) => reply.code(204).send());
  app.get(`${API}/research/models`, () => research.availableModels());
  app.get(`${API}/datasets`, async () => config.datasets.map(({ data: _data, policy: _policy, strategy: _strategy, ...view }) => view));
  app.get<{ Querystring: { cursor?: string } }>(`${API}/research`, async (request) => {
    const cursor = request.query.cursor ? object(JSON.parse(request.query.cursor), 'cursor') as Cursor : undefined;
    const page = await research.list(cursor); const items: ResearchSummary[] = []; const admitted = (await work.snapshot())?.researchRequests;
    for (const row of page.items) items.push(await researchSummary(research, row.id, admitted));
    return { items, ...(page.next ? { cursor: page.next } : {}) };
  });
  app.get<{ Params: { id: string } }>(`${API}/research/:id`, (request) => researchDetail(research, Number(request.params.id)));
  app.get<{ Params: { id: string } }>(`${API}/research/:id/diagnostics`, (request) => research.diagnostics(Number(request.params.id)));
  app.post<{ Params: { id: string } }>(`${API}/research/:id/resume`, async (request, reply) => {
    const body = object(request.body);
    if (Object.keys(body).length !== 1 || typeof body.submissionId !== 'number' || !Number.isSafeInteger(body.submissionId) || body.submissionId < 1) throw new Problem('invalid_research_resume');
    return reply.code(202).send(await research.resume(Number(request.params.id), body.submissionId));
  });
  app.post(`${API}/research`, async (request, reply) => reply.code(202).send(await work.startResearch(request.body)));
  app.post<{ Params: { id: string } }>(`${API}/research/:id/stop`, async (request, reply) => {
    const id = Number(request.params.id); await research.stop(id); return reply.code(202).send(await researchDetail(research, id));
  });
  app.get(`${API}/usage`, async (): Promise<UsageView> => {
    const usage = await research.usage(); const sources = [...Object.values(usage.models), ...Object.values(usage.tools)];
    return { inputTokens: sources.length ? sources.reduce((sum, value) => sum + value.input, 0) : null,
      outputTokens: sources.length ? sources.reduce((sum, value) => sum + value.output, 0) : null,
      cost: null, activeTasks: (await research.harness.inspect(context)).tasks.length };
  });
  app.get(`${API}/portfolios`, async () => Object.values((await work.snapshot())!.portfolios).map((item) => portfolioView(portfolio(item))));
  app.get<{ Params: { id: string } }>(`${API}/portfolios/:id`, async (request) => portfolioView(await work.getPortfolio(request.params.id)));
  app.post(`${API}/portfolios`, async (request, reply) => { const view = await work.startPortfolio(request.body); return reply.code(202).send(portfolioView(await work.getPortfolio(view.id))); });
  app.get(`${API}/observations`, async () => Object.values((await work.snapshot())!.observations).map((item) => observationView(observation(item))));
  app.post(`${API}/observations`, async (request, reply) => { const view = await work.startObservation(request.body); return reply.code(202).send(observationView(await work.getObservation(view.id))); });
  app.post<{ Params: { id: string } }>(`${API}/observations/:id/refresh`, async (request) => { await work.refreshObservation(request.params.id); return observationView(await work.getObservation(request.params.id)); });
  app.post<{ Params: { id: string } }>(`${API}/observations/:id/stop`, async (request) => {
    const id = request.params.id;
    const taskId = await research.harness.commit(async (tx) => {
      const facts = await tx.doc(Application); const value = facts.observations[id]; if (!value) throw new Problem('observation_not_found', 404);
      const fact = observation(value); fact.view.status = 'STOPPED'; facts.observations[id] = json(fact); return fact.taskId;
    }, context);
    await research.harness.abortTask(taskId as TaskId, context);
    await research.harness.waitForTask(taskId as TaskId, context);
    return observationView(await work.getObservation(id));
  });
  registerModels(app, research, work.getSettings);
  app.get(`${API}/workspace`, async (): Promise<WorkspaceView> => {
    const facts = (await work.snapshot())!; const views = Object.values(facts.observations).map(observation);
    return { name: config.name, connections: config.connections.map((connection) => ({ id: connection.id, name: connection.name, mode: connection.mode,
      connected: views.some((fact) => fact.connection.id === connection.id && !!fact.snapshot && !fact.view.error && fact.view.status === 'OBSERVING') })) };
  });
  app.get(`${API}/extensions`, async () => [...extensions.values()].map(item => ({ ...item, required: requiredExtensions.has(item.name) })));
  app.patch<{ Params: { name: string } }>(`${API}/extensions/:name`, async (request) => {
    const found = extensions.get(request.params.name); if (!found) throw new Problem('extension_not_found', 404);
    const enabled = object(request.body).enabled; if (typeof enabled !== 'boolean') throw new Problem('enabled_boolean_required');
    if (!enabled && requiredExtensions.has(found.name)) throw new Problem('service_extension_required', 409);
    await research.harness.commit(async (tx) => {
      const next = { ...desired, [found.name]: enabled };
      const temporary = `${extensionFile}.${crypto.randomUUID()}.tmp`;
      await writeFile(temporary, JSON.stringify({ enabled: next, modules: moduleNames }), { mode: 0o600 });
      await rename(temporary, extensionFile); desired = next;
      (await tx.doc(Application)).extensionEnabled[found.name] = enabled;
      found.enabled = enabled; const loaded = !!research.registry.snapshot().extension(found.name);
      found.state = loaded === enabled ? enabled ? 'loaded' : 'disabled' : 'pending_restart';
    }, context);
    return { ...found, required: requiredExtensions.has(found.name) };
  });
  if (archive) for (const kind of ['projects', 'runs'] as const) app.get(`${API}/archive/${kind}`, async () => {
    const client = await archive.connect().catch(() => { throw new Problem('archive_unavailable', 503); });
    try {
      await client.query('BEGIN READ ONLY');
      const sql = kind === 'projects' ? 'SELECT id, name, state, revision::text, created_at::text, updated_at::text FROM app.projects ORDER BY app.projects.created_at DESC LIMIT 100' : 'SELECT id, project_id, kind, state, revision::text, created_at::text, started_at::text, finished_at::text, terminal_reason_code FROM app.runs ORDER BY app.runs.created_at DESC LIMIT 100';
      const result = await client.query(sql); await client.query('COMMIT'); return { items: result.rows };
    } catch { await client.query('ROLLBACK'); throw new Problem('archive_unavailable', 503); } finally { client.release(); }
  });
  if (config.staticDir) {
    await app.register(staticPlugin, { root: config.staticDir });
    app.setNotFoundHandler((request, reply) => request.url.startsWith('/api/') ? reply.code(404).send({ error: 'not_found' }) : reply.sendFile('index.html'));
  } else app.setNotFoundHandler((_request, reply) => reply.code(404).send({ error: 'not_found' }));
  await app.ready();
  return { app, research, config, work, close: () => app.close() };
  } catch (error) { await research.close(); await archive?.end(); throw error; }
}
export const buildService = createService;
export type Service = Awaited<ReturnType<typeof createService>>;
