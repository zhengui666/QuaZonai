import {processAccounts} from './process-accounts.ts';
import {nativeEnvironmentBuild} from './native-environment-build.ts';
import { researchTaskEvidence } from './research-assessments.ts';
import { accountStrategyWorkbench } from './account-strategy.ts';
import { cancelNativeProtection } from './downstream.ts';
import Fastify from 'fastify';
import cookie from '@fastify/cookie';
import staticPlugin from '@fastify/static';
import { mkdir, mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { boundScienceTarget } from './science-binding.ts';
import { nativeJob } from './native.ts';
import { pathToFileURL } from 'node:url';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { Cursor, Extension, TaskId } from '@earendil-works/pi-durable';
import { openResearch } from '../../research/src/host.ts';
import { API } from '../../../packages/contracts/src/index.ts';
import { datasetResearchSettings } from '../../../packages/contracts/src/research.ts';
import type { ExtensionView, ResearchSummary, WorkspaceView, UsageView } from '../../../packages/contracts/src/index.ts';
import { Application, json, object, observation, portfolio, Problem } from './facts.ts';
import type { ServiceOptions } from './config.ts';
import { science } from './science.ts';
import { createParameterScanExtension, parameterScanWorkbench, registerParameterScanRoutes } from './parameter-scans.ts';
import { registerAuth } from './auth.ts';
import { registerModels } from './models.ts';
import { instanceSettings, registerInstanceSettings } from './instance-settings.ts';
import { DataSourceState, dataSources, registerDataSources } from './data-sources.ts';
import { observationView, portfolioView, researchDetail, researchSummary, researchTaskViews } from './projections.ts';
import { workbench } from './workbench.ts';
import { eventStream } from './event-stream.ts';
import { readResearchAdmissions } from './admissions.ts';
import { listFactors, readFactor } from './factors.ts';
import { capitalWorkbench } from './capital-workbench.ts';
import { accountKey, capitalStatus, capitalStatusForBinding, control, transportBindingKey } from './capital-exit.ts';
import { createPmPlatform } from './pm-platform.ts';
import { registerPmRoutes } from './pm-routes.ts';
import { maintenanceExtension, createMaintenance } from './pm-maintenance.ts';
import { registerMaintenanceRoutes } from './pm-maintenance-routes.ts';
import { Study } from '../../research/src/documents.ts';
import { PmDocument, assertPmNotDeleted, pmRecord, pmState } from './pm-facts.ts';
import { assertBusinessAdmission } from './pm-maintenance.ts';
import { registerListRoutes } from './lists.ts';

const context = BACKGROUND_CONTEXT;
export async function createService(options: ServiceOptions) {
  const config = structuredClone(options.config);
  await mkdir(config.dataDir, { recursive: true });
  const instances = await instanceSettings(config);
  const nativeBuild = nativeEnvironmentBuild(config);
  let accounts:Awaited<ReturnType<typeof processAccounts>>|undefined;
  const sources = await dataSources(config);
  const definitions = new Map<string, Extension>();
  const defaults = new Map<string, boolean>();
  const pm = createPmPlatform(config);
  const installed: Extension[] = [science, pm.extension, maintenanceExtension, sources.extension, nativeBuild.extension];
  const extensions = new Map<string, ExtensionView>([['qz.science', { name: 'qz.science', version: '1', enabled: true, kind: 'science', state: 'loaded', error: null }]]);
  const requiredExtensions = new Set(['qz.science']);
  function install(extension: Extension, enabled = true, version: string | null = null, kind = 'research') {
    if (extensions.has(extension.name) || sources.has(extension.name)) throw new Problem('extension_name_conflict');
    definitions.set(extension.name, extension); defaults.set(extension.name, enabled); installed.push(extension);
    extensions.set(extension.name, { name: extension.name, version, enabled, kind, state: 'loaded', error: null });
  }
  for (const extension of options.extensions ?? []) install(extension);
  for (const path of config.research.modules ?? []) {
    install((await import(pathToFileURL(path).href)).default as Extension);
  }
  for (const configured of config.extensions.filter(item => item.kind !== 'data')) {
    if (extensions.has(configured.name)) throw new Problem('extension_name_conflict');
    try {
      const extension = (await import(pathToFileURL(configured.module).href)).default as Extension;
      if (extension.name !== configured.name) throw new Problem('extension_module_name_mismatch');
      install(extension, configured.enabled, configured.version ?? null, configured.kind);
    } catch {
      defaults.set(configured.name, configured.enabled);
      extensions.set(configured.name, { name: configured.name, version: configured.version ?? null, enabled: configured.enabled,
        kind: configured.kind ?? 'research', state: 'error', error: 'extension_load_failed' });
    }
  }
  const scans = createParameterScanExtension(() => research.harness);
  const scientificTasks = new Map(scans.scientificTasks.map(task => [task.definition.name, task]));
  // Pi Durable permits one provider for a task name. Keep these exact shared
  // scientific definitions available while research tools are enabled/disabled.
  for (const [index, extension] of installed.entries()) {
    const tasks = extension.tasks?.filter(task => {
      const scientific = scientificTasks.get(task.definition.name);
      if (scientific && scientific !== task) throw new Problem('native_task_definition_conflict');
      return scientific === undefined;
    });
    if (tasks && tasks.length !== extension.tasks!.length) {
      const shared = { ...extension, tasks }; installed[index] = shared;
      if (definitions.has(extension.name)) definitions.set(extension.name, shared);
    }
  }
  install(scans.extension, true, '1', 'science');
  requiredExtensions.add(scans.extension.name);
  const research = await openResearch({ ...config.research, modules: [], extensions: installed, models: options.models, storage: options.storage, deferResume: true,
    authorizeAdmission: async (tx, input) => {
      assertBusinessAdmission(await tx.doc(Application));
      if (input.campaign) assertPmNotDeleted(pmRecord(pmState(await tx.doc(PmDocument)), input.campaign.mandateRevisionId));
    },
    authorizeResume: async (tx, id) => {
      const parent = (await tx.doc(Study, id)).input?.campaign?.mandateRevisionId;
      if (parent) assertPmNotDeleted(pmRecord(pmState(await tx.doc(PmDocument)), parent));
    },
  });
  await sources.connect(research);
  for (const extension of research.registry.snapshot().installed()) if (!extensions.has(extension.name)) {
    requiredExtensions.add(extension.name);
    extensions.set(extension.name, { name: extension.name, version: null, enabled: true, kind: 'host', state: 'loaded', error: null });
  }
  for (const extension of research.registry.snapshot().installed()) {
    definitions.set(extension.name, extension);
    if (!defaults.has(extension.name)) defaults.set(extension.name, true);
  }
  const enabledPreferences = await research.harness.commit(async tx => {
    const app = await tx.doc(Application);
    for (const [name, enabled] of defaults) app.extensionEnabled[name] ??= enabled;
    for (const name of requiredExtensions) app.extensionEnabled[name] = true;
    return { ...app.extensionEnabled };
  }, context);
  function applyExtensions(enabled: Record<string, boolean>) {
    for (const [name, definition] of definitions) {
      const loaded = !!research.registry.snapshot().extension(name);
      const wanted = requiredExtensions.has(name) || (enabled[name] ?? defaults.get(name) ?? true);
      if (wanted && !loaded) research.registry.install(definition);
      else if (!wanted && loaded) research.registry.uninstall(definition);
    }
  }
  applyExtensions(enabledPreferences);
  const extensionWatch = await research.harness.watchDoc(Application, context);
  extensionWatch?.start(async call => { if (call.kind === 'value' && call.value) applyExtensions(call.value.extensionEnabled); });
  async function extensionViews(): Promise<ExtensionView[]> {
    const enabled = (await research.harness.snapshot(Application, context))?.extensionEnabled ?? {};
    const registry = research.registry.snapshot();
    return [...extensions.values()].map(item => ({ ...item, enabled: enabled[item.name] ?? defaults.get(item.name) ?? true,
      required: requiredExtensions.has(item.name), state: item.error ? 'error' : registry.extension(item.name) ? 'loaded' : 'disabled' }));
  }
  const readTasks = researchTaskViews(research);
  const detail = (id: number) => researchDetail(research, id, readTasks);
  const eventStreams = new Set<() => Promise<void>>();
  const app = Fastify({ logger: false, bodyLimit: 1024 * 1024, routerOptions: { ignoreTrailingSlash: true } });
  app.addHook('onRequest', (_request, _reply, done) => research.withNativeHost(done));
  app.addHook('preClose', async () => { await Promise.all([...eventStreams].map(close => close())); await sources.close(); });
  app.addHook('onClose', async () => { await instances.close(); await extensionWatch?.stop(); await research.close(); await accounts?.close(); });
  try {
  accounts = await processAccounts(config);
  research.harness.resume();
  for (const source of sources.extensions()) if (extensions.has(source.name)) throw new Problem('extension_name_conflict');
  app.setErrorHandler((error, _request, reply) => {
    const message = error instanceof Error ? error.message : 'internal_error';
    const status = error instanceof Problem ? error.statusCode : /conflict|already|trial_budget_exhausted/.test(message) ? 409 : /not_found/.test(message) ? 404 : (error as { statusCode?: number }).statusCode ?? (/invalid|required|unavailable|unsupported|mismatch/.test(message) ? 400 : 500);
    const code = error instanceof Problem || /^[a-z][a-z0-9_]+$/.test(message) ? message : status < 500 ? 'invalid_request' : 'internal_error';
    reply.code(status).send({ error: code });
  });
  await app.register(cookie);
  registerAuth(app, research, config);
  registerListRoutes(app, research, config);
  const maintenance = createMaintenance(research);
  registerMaintenanceRoutes(app, maintenance);
  registerParameterScanRoutes(app, research, parameterScanWorkbench(research, config, scans.stopTask), eventStreams);
  await research.harness.commit(async (tx) => {
    const facts = await tx.doc(Application);
    facts.settings ??= json({ defaultModel: config.defaultModel, thinkingLevel: config.thinkingLevel, serviceTier: config.serviceTier });
  }, context);
  const work = workbench(research, config);
  pm.connect({ host: research, work });
  registerPmRoutes(app, pm.work);
  for (const [path, remove] of [['research', (id: string) => work.removeResearch(Number(id))], ['factors', work.removeFactor], ['portfolios', work.removePortfolio], ['mandates', pm.work.delete]] as const) {
    app.delete<{ Params: { id: string } }>(`${API}/${path}/:id`, request => remove(request.params.id));
  }
  const capital = capitalWorkbench(research, config);
  const strategies = accountStrategyWorkbench(research);
  app.get<{ Params: { id: string } }>(`${API}/observations/:id/strategy`, request => strategies.status(request.params.id));
  app.post<{ Params: { id: string; requestId: string } }>(`${API}/observations/:id/strategy/config/commands/:requestId/verify`, request => strategies.verifyConfiguration(request.params.id, request.params.requestId));
  app.get<{ Params: { id: string } }>(`${API}/observations/:id/strategy/config`, request => strategies.configuration(request.params.id));
  app.post<{ Params: { id: string } }>(`${API}/observations/:id/strategy/config`, request => strategies.configuration(request.params.id, request.body));
  app.get<{ Params: { id: string } }>(`${API}/observations/:id/strategy/fence`, request => strategies.fence(request.params.id));
  app.post<{ Params: { id: string } }>(`${API}/observations/:id/strategy/commands`, async (request, reply) => reply.code(202).send(await strategies.command(request.params.id, request.body)));
  app.get<{ Params: { id: string; requestId: string } }>(`${API}/observations/:id/strategy/commands/:requestId`, request => strategies.receipt(request.params.id, request.params.requestId));
  app.post<{ Params: { id: string } }>(`${API}/observations/:id/sizing/fixed-risk`, request => capital.previewFixedRisk(request.params.id, request.body));
  app.post<{ Params: { id: string } }>(`${API}/observations/:id/orders/preview`, request => capital.previewOrder(request.params.id, request.body));
  app.get('/health/live', async (_request, reply) => reply.code(204).send());
  app.get(`${API}/research/models`, () => research.availableModels());
  app.get(`${API}/datasets`, async () => config.datasets.map(({ data, policy: _policy, strategy, ...view }) => {
    const researchSettings = view.nativeModality === 'TRADE_TICK' && data.nativeInput === undefined ? undefined : datasetResearchSettings(data, strategy ? json(strategy) : undefined);
    return { ...view, nativeModality: view.nativeModality ?? researchSettings?.modality, ...(researchSettings ? { researchSettings } : {}) };
  }));
  // Read-only native file metadata; never admits a dataset or triggers backfill.
  app.post<{ Params: { id: string } }>(`${API}/datasets/:id/coverage`, async request => {
    const dataset = config.datasets.find(row => row.id === request.params.id);
    if (!dataset) throw new Problem('dataset_not_found', 404);
    const catalog = dataset.data.catalog;
    if (typeof catalog !== 'string' || !catalog) throw new Problem('dataset_catalog_required', 422);
    const input = object(request.body, 'catalog_coverage_request');
    // Capture the dataset and transport together before I/O yields to settings changes.
    const target = boundScienceTarget(config, dataset.data);
    const directory = await mkdtemp(join(config.dataDir, 'catalog-coverage-'));
    try {
      return await nativeJob(target, 'catalog-coverage', input, ['--catalog', catalog], directory, context);
    } finally { await rm(directory, { recursive: true, force: true }); }
  });
  app.get<{ Querystring: { cursor?: string } }>(`${API}/research`, async (request) => {
    const cursor = request.query.cursor ? object(JSON.parse(request.query.cursor), 'cursor') as Cursor : undefined;
    const [page, facts, inspection] = await Promise.all([research.list(cursor), readResearchAdmissions(research), research.harness.inspect(context)]);
    const items: ResearchSummary[] = []; const active = new Set(inspection.tasks.map(task => task.record.conversationId));
    for (const row of page.items) {
      const state = await research.state(row.id);
      if (state.research?.input && !state.research.deletedAt) items.push(researchSummary(row.id, state, facts, active.has(row.id)));
    }
    return { items, ...(page.next ? { cursor: page.next } : {}) };
  });
  app.get<{ Params: { id: string } }>(`${API}/research/:id`, (request) => detail(Number(request.params.id)));
  app.get<{ Params: { id: string; taskId: string } }>(`${API}/research/:id/evidence/:taskId`, request => researchTaskEvidence(research, Number(request.params.id), Number(request.params.taskId)));
  app.get<{ Params: { id: string } }>(`${API}/research/:id/events`, async (request, reply) => {
    const id = Number(request.params.id);
    await detail(id);
    const conversation = await research.harness.conversation(id as import('@earendil-works/pi-durable').ConversationId, context);
    if (!conversation) throw new Problem('research_not_found', 404);
    const watches = await Promise.all([conversation.watch(context), research.harness.watchTaskGraph(context)]);
    await eventStream(reply, 'research', () => detail(id), watches, eventStreams);
  });
  app.get<{ Params: { id: string } }>(`${API}/research/:id/reviews`, async request => (await detail(Number(request.params.id))).reviews ?? []);
  app.get<{ Params: { id: string } }>(`${API}/research/:id/diagnostics`, async request => { await detail(Number(request.params.id)); return research.diagnostics(Number(request.params.id)); });
  app.post<{ Params: { id: string } }>(`${API}/research/:id/resume`, async (request, reply) => {
    const body = object(request.body);
    if (Object.keys(body).some(key => !['submissionId', 'instruction'].includes(key)) || ('instruction' in body && (typeof body.instruction !== 'string' || !body.instruction.trim())) || typeof body.submissionId !== 'number' || !Number.isSafeInteger(body.submissionId) || body.submissionId < 1) throw new Problem('invalid_research_resume');
    return reply.code(202).send(await research.resume(Number(request.params.id), body.submissionId, body.instruction as string | undefined));
  });
  app.post(`${API}/research`, async (request, reply) => reply.code(202).send(await work.startResearch(request.body)));
  app.post<{ Params: { id: string } }>(`${API}/research/:id/stop`, async (request, reply) => {
    const id = Number(request.params.id); await research.stop(id); return reply.code(202).send(await detail(id));
  });
  app.get(`${API}/usage`, async (): Promise<UsageView> => {
    const usage = await research.usage(); const sources = [...Object.values(usage.models), ...Object.values(usage.tools)];
    return { inputTokens: sources.length ? sources.reduce((sum, value) => sum + value.input, 0) : null,
      outputTokens: sources.length ? sources.reduce((sum, value) => sum + value.output, 0) : null,
      cost: null, activeTasks: (await research.harness.inspect(context)).tasks.length };
  });
  app.get(`${API}/factors`, async () => listFactors(research, await readResearchAdmissions(research)));
  app.get<{ Params: { id: string } }>(`${API}/factors/:id`, async request => (await readFactor(research, request.params.id, await readResearchAdmissions(research))).view);
  app.get<{ Params: { id: string } }>(`${API}/factors/:id/review`, async request => (await readFactor(research, request.params.id, await readResearchAdmissions(research))).view.review ?? null);
  app.get(`${API}/portfolios`, async () => Promise.all(Object.values((await work.snapshot())!.portfolios).map(portfolio).filter(item => !item.deletedAt).map(async item => portfolioView(await work.getPortfolio(item.view.id)))));
  app.get<{ Params: { id: string } }>(`${API}/portfolios/:id`, async (request) => portfolioView(await work.getPortfolio(request.params.id)));
  app.get<{ Params: { id: string } }>(`${API}/portfolios/:id/events`, async (request, reply) => {
    await work.getPortfolio(request.params.id);
    const [document, graph] = await Promise.all([research.harness.watchDoc(Application, context), research.harness.watchTaskGraph(context)]);
    if (!document) { await graph.stop(); throw new Problem('portfolio_not_found', 404); }
    await eventStream(reply, 'portfolio', async () => portfolioView(await work.getPortfolio(request.params.id)), [document, graph], eventStreams);
  });

  app.get<{ Params: { id: string } }>(`${API}/portfolios/:id/review`, async request => (await work.getPortfolio(request.params.id)).view.review ?? null);
  app.get(`${API}/observations`, async () => { const facts = (await work.snapshot())!; return Object.values(facts.observations).map(item => { const fact = observation(item); return observationView({ ...fact, view: { ...fact.view, capitalExit: capitalStatusForBinding(control(facts.accountControls?.[accountKey(fact.connection, facts.accountControls)]), fact.connection) } }); }); });
  app.get<{ Params: { id: string } }>(`${API}/observations/:id/capital-exits`, request => capital.refreshCapital(request.params.id));
  app.get<{ Params: { id: string; exitId: string } }>(`${API}/observations/:id/capital-exits/:exitId`, request => capital.getCapitalExit(request.params.id, request.params.exitId));
  app.post<{ Params: { id: string } }>(`${API}/observations/:id/capital-exits/commands`, async (request, reply) => reply.code(202).send(await capital.commandCapital(request.params.id, request.body)));
  app.post<{ Params: { id: string } }>(`${API}/observations/:id/automatic`, async request => observationView({ ...(await work.getObservation(request.params.id)), view: await work.configureAutomatic(request.params.id, request.body) }));
  app.post<{ Params: { id: string } }>(`${API}/observations/:id/cancel-protection`, async request => cancelNativeProtection(await work.getObservation(request.params.id)));
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
  app.get(`${API}/accounts/configurations`,()=>accounts!.list());
  app.post(`${API}/accounts/configurations`,request=>accounts!.create(request.body));
  const configuredStrategies=accountStrategyWorkbench(research,id=>accounts!.binding(id));
  app.get<{Params:{id:string}}>(`${API}/accounts/configurations/:id/strategy`,r=>configuredStrategies.status(r.params.id));
  app.get<{Params:{id:string}}>(`${API}/accounts/configurations/:id/strategy/fence`,r=>configuredStrategies.fence(r.params.id));
  app.get<{Params:{id:string}}>(`${API}/accounts/configurations/:id/strategy/config`,r=>configuredStrategies.configuration(r.params.id));
  app.post<{Params:{id:string}}>(`${API}/accounts/configurations/:id/strategy/config`,r=>configuredStrategies.configuration(r.params.id,r.body));
  app.post<{Params:{id:string;requestId:string}}>(`${API}/accounts/configurations/:id/strategy/config/commands/:requestId/verify`,r=>configuredStrategies.verifyConfiguration(r.params.id,r.params.requestId));
  app.post<{Params:{id:string}}>(`${API}/accounts/configurations/:id/strategy/commands`,async(r,reply)=>reply.code(202).send(await configuredStrategies.command(r.params.id,r.body)));
  app.get<{Params:{id:string;requestId:string}}>(`${API}/accounts/configurations/:id/strategy/commands/:requestId`,r=>configuredStrategies.receipt(r.params.id,r.params.requestId));
  registerInstanceSettings(app, instances);
  const builds=nativeBuild.workbench(research,instances);
  app.post(`${API}/settings/instances/build`,request=>builds.start(request.body));
  app.get<{Params:{requestId:string}}>(`${API}/settings/instances/build/:requestId`,request=>builds.read(request.params.requestId));
  app.post(`${API}/settings/instances/select-runner`,request=>builds.select(request.body));
  registerDataSources(app, sources);
  app.get<{ Params: { name: string }; Querystring: { taskId?: string } }>(`${API}/data-sources/:name/events`, async (request, reply) => {
    const taskId = request.query.taskId === undefined ? undefined : Number(request.query.taskId);
    if (taskId !== undefined && (!Number.isSafeInteger(taskId) || taskId < 1)) throw new Problem('data_source_task_id_invalid');
    await sources.taskStatus(request.params.name, taskId);
    const [document, graph] = await Promise.all([research.harness.watchDoc(DataSourceState, context), research.harness.watchTaskGraph(context)]);
    if (!document) { await graph.stop(); throw new Problem('data_source_not_found', 404); }
    await eventStream(reply, 'data-source', () => sources.taskStatus(request.params.name, taskId), [document, graph], eventStreams);
  });

  app.get(`${API}/workspace`, async (): Promise<WorkspaceView> => {
    const facts = (await work.snapshot())!; const views = Object.values(facts.observations).map(observation);
    return { name: config.name, connections: config.connections.map((connection) => ({ id: connection.id, name: connection.name, mode: connection.mode,
      connected: views.some((fact) => fact.connection.id === connection.id && transportBindingKey(fact.connection) === transportBindingKey(connection)
        && fact.connection.tokenEnv === connection.tokenEnv && fact.connection.credentialRef === connection.credentialRef
        && !!fact.snapshot && !fact.view.error && fact.view.status === 'OBSERVING') })) };
  });
  app.get(`${API}/extensions`, async (): Promise<ExtensionView[]> => [...await extensionViews(), ...sources.extensions()]);
  app.patch<{ Params: { name: string } }>(`${API}/extensions/:name`, async (request) => {
    if (sources.has(request.params.name)) {
      await sources.toggle(request.params.name, object(request.body).enabled as boolean);
      return sources.extensions().find(item => item.name === request.params.name);
    }
    const found = extensions.get(request.params.name); if (!found) throw new Problem('extension_not_found', 404);
    const enabled = object(request.body).enabled; if (typeof enabled !== 'boolean') throw new Problem('enabled_boolean_required');
    if (!enabled && requiredExtensions.has(found.name)) throw new Problem('service_extension_required', 409);
    if (enabled && !definitions.has(found.name)) throw new Problem('extension_load_failed', 409);
    await research.harness.commit(async tx => { (await tx.doc(Application)).extensionEnabled[found.name] = enabled; }, context);
    applyExtensions((await research.harness.snapshot(Application, context))!.extensionEnabled);
    return (await extensionViews()).find(item => item.name === found.name)!;
  });
  if (config.staticDir) {
    await app.register(staticPlugin, { root: config.staticDir });
    app.setNotFoundHandler((request, reply) => request.url.startsWith('/api/') ? reply.code(404).send({ error: 'not_found' }) : reply.sendFile('index.html'));
  } else app.setNotFoundHandler((_request, reply) => reply.code(404).send({ error: 'not_found' }));
  await app.ready();
  return { app, research, config, work, pm: pm.work, maintenance, close: () => app.close() };
  } catch (error) { await extensionWatch?.stop(); await research.close(); await accounts?.close(); throw error; }
}
export const buildService = createService;
export type Service = Awaited<ReturnType<typeof createService>>;
