import { randomUUID } from 'node:crypto';
import type { AuthPrompt, ModelThinkingLevel } from '@earendil-works/pi-ai';
import { getSupportedThinkingLevels } from '@earendil-works/pi-ai/models';
import type { FastifyInstance } from 'fastify';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { ResearchHost } from '../../research/src/host.ts';
import { API } from '../../../packages/contracts/src/index.ts';
import type { ModelAuthType, ModelCatalog, ModelLoginView, SettingsView } from '../../../packages/contracts/src/index.ts';
import { Application, object, Problem, settings, text } from './facts.ts';

function safeLink(value: string): boolean {
  try { const url = new URL(value); return !url.username && !url.password && (url.protocol === 'https:' || (url.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname))); }
  catch { return false; }
}
type Login = { view: ModelLoginView; controller: AbortController; completion: Promise<void>; respond?: (value: string) => void; promptNo: number; answered: number };

/** Native provider interactions only. Secrets never enter application documents or views. */
export function registerModels(app: FastifyInstance, host: ResearchHost, getSettings: () => Promise<SettingsView>) {
  const logins = new Map<string, Login>();
  const cancelled = new Set<string>();
  let closing = false;
  const active = (provider: string) => [...logins.values()].find(login => login.view.providerId === provider && login.view.status === 'running');
  function find(id: string) { const login = logins.get(id); if (!login) throw new Problem('model_login_not_found', 404); return login; }
  function view(login: Login) { return structuredClone(login.view); }
  async function cancel(login: Login) {
    if (login.view.status === 'running') login.controller.abort();
    await login.completion;
    return view(login);
  }
  app.addHook('onClose', async () => { closing = true; await Promise.all([...logins.values()].map(cancel)); logins.clear(); });

  app.get(`${API}/models`, async (): Promise<ModelCatalog> => {
    const entries = await Promise.all(host.models.getProviders().map(async provider => {
      const authTypes: ModelAuthType[] = [...(provider.auth.oauth?.login ? ['oauth' as const] : []), ...(provider.auth.apiKey?.login ? ['api_key' as const] : [])];
      const pending = active(provider.id);
      const login = pending ? { id: pending.view.id, type: pending.view.type } : null;
      const base = { id: provider.id, name: provider.name, authTypes, login };
      try {
        const configured = !!await host.models.checkAuth(provider.id);
        return { provider: { ...base, configured, error: null }, models: configured ? await host.availableModels(provider.id) : [] };
      } catch { return { provider: { ...base, configured: false, error: 'provider_status_unavailable' }, models: [] }; }
    }));
    return { providers: entries.map(entry => entry.provider), models: entries.flatMap(entry => entry.models) };
  });
  app.post(`${API}/models/login`, async (request, reply) => {
    const body = object(request.body); const id = text(body.requestId, 'request_id');
    const providerId = text(body.providerId, 'provider_id'); const type = body.type;
    if (type !== 'api_key' && type !== 'oauth') throw new Problem('model_auth_type_unsupported');
    const previous = logins.get(id);
    if (previous) {
      if (previous.view.providerId !== providerId || previous.view.type !== type) throw new Problem('request_id_conflict', 409);
      return reply.code(202).send(view(previous));
    }
    if (closing) throw new Problem('service_closing', 503);
    if (cancelled.has(id)) throw new Problem('model_login_cancelled', 409);
    const provider = host.models.getProvider(providerId);
    if (!provider) throw new Problem('provider_not_found', 404);
    if (!(type === 'oauth' ? provider.auth.oauth?.login : provider.auth.apiKey?.login)) throw new Problem('model_auth_type_unsupported');
    if (active(providerId)) throw new Problem('model_login_in_progress', 409);
    const login: Login = { view: { id, providerId, type, status: 'running', prompt: null, events: [], error: null }, controller: new AbortController(), completion: Promise.resolve(), promptNo: 0, answered: 0 };
    logins.set(id, login);
    login.completion = (async () => {
      const deviceId = await host.harness.commit(async tx => (await tx.doc(Application)).modelDeviceId ??= randomUUID(), BACKGROUND_CONTEXT);
      login.controller.signal.throwIfAborted();
      await host.models.login(providerId, type, {
      signal: login.controller.signal,
      prompt: (prompt: AuthPrompt) => new Promise<string>((resolve, reject) => {
        const signal = prompt.signal ? AbortSignal.any([login.controller.signal, prompt.signal]) : login.controller.signal;
        const promptId = ++login.promptNo;
        const cleanup = () => {
          signal.removeEventListener('abort', abort);
          if (login.view.prompt?.id === promptId) { login.view.prompt = null; delete login.respond; }
        };
        const abort = () => { cleanup(); reject(signal.reason); };
        if (signal.aborted) { abort(); return; }
        const { signal: _signal, ...publicPrompt } = prompt;
        login.view.prompt = { ...structuredClone(publicPrompt), id: promptId };
        login.respond = value => { login.answered = promptId; cleanup(); resolve(value); };
        signal.addEventListener('abort', abort, { once: true });
      }),
      notify: event => {
        if (login.controller.signal.aborted || login.view.status !== 'running') return;
        if (event.type === 'auth_url' && safeLink(event.url)) login.view.events.push({ type: 'auth_url', url: event.url, ...(event.instructions ? { instructions: event.instructions } : {}) });
        else if (event.type === 'device_code' && safeLink(event.verificationUri)) login.view.events.push({ type: 'device_code', userCode: event.userCode, verificationUri: event.verificationUri });
        else if (event.type === 'info' || event.type === 'progress') login.view.events.push({ type: event.type, message: event.message });
      },
    }, { agentName: 'QuaZonai', getDeviceId: () => deviceId });
    })().then(() => {
      login.view.status = 'done';
    }, () => {
      login.view.status = login.controller.signal.aborted ? 'cancelled' : 'failed';
      login.view.error = login.view.status === 'failed' ? 'model_login_failed' : null;
    }).finally(() => { login.view.prompt = null; login.view.events = []; delete login.respond; });
    return reply.code(202).send(view(login));
  });
  app.get<{ Params: { id: string } }>(`${API}/models/login/:id`, request => view(find(request.params.id)));
  app.post<{ Params: { id: string } }>(`${API}/models/login/:id/respond`, request => {
    const login = find(request.params.id); const body = object(request.body);
    if (!Number.isSafeInteger(body.promptId) || Number(body.promptId) <= 0) throw new Problem('model_prompt_required');
    if (body.promptId === login.answered) return view(login);
    if (!login.respond || login.view.prompt?.id !== body.promptId) throw new Problem('model_prompt_changed', 409);
    const value = text(body.value, 'model_auth_response');
    if (login.view.prompt.type === 'select' && !login.view.prompt.options.some(option => option.id === value)) throw new Problem('model_auth_option_invalid');
    login.respond(value);
    return view(login);
  });
  app.delete<{ Params: { id: string } }>(`${API}/models/login/:id`, async (request, reply) => {
    cancelled.add(request.params.id);
    const login = logins.get(request.params.id);
    if (!login) return reply.code(204).send();
    return cancel(login);
  });
  app.delete<{ Params: { provider: string } }>(`${API}/models/providers/:provider`, async request => {
    const id = request.params.provider;
    if (!host.models.getProvider(id)) throw new Problem('provider_not_found', 404);
    const login = active(id); if (login) await cancel(login);
    try { await host.models.logout(id); return { providerId: id, configured: !!await host.models.checkAuth(id) }; }
    catch { throw new Problem('model_logout_failed', 503); }
  });

  app.get(`${API}/settings`, getSettings);
  app.patch(`${API}/settings`, async request => {
    const patch = object(request.body);
    if (Object.keys(patch).some(key => !['defaultModel', 'thinkingLevel'].includes(key))) throw new Problem('settings_field_not_mutable');
    await host.harness.commit(async tx => {
      const facts = await tx.doc(Application); const current = settings(facts.settings);
      const next = { ...current, ...patch } as SettingsView;
      if (next.defaultModel !== null) {
        const ref = object(next.defaultModel, 'model');
        if (Object.keys(ref).some(key => !['provider', 'modelId'].includes(key))) throw new Problem('model_invalid');
        const model = host.models.getModel(text(ref.provider, 'provider'), text(ref.modelId, 'model_id'));
        if (!model || !(await host.availableModels(model.provider)).some(item => item.modelId === model.id)) throw new Problem('model_unavailable', 409);
        if (next.thinkingLevel !== null && !getSupportedThinkingLevels(model).includes(next.thinkingLevel as ModelThinkingLevel)) throw new Problem('thinking_level_unsupported');
      } else if (next.thinkingLevel !== null) throw new Problem('model_required');
      facts.settings = { ...facts.settings, ...patch };
    }, BACKGROUND_CONTEXT);
    return getSettings();
  });
}
