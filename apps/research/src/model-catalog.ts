import type { Api, Model, ModelThinkingLevel } from '@earendil-works/pi-ai';
import { clampThinkingLevel, getSupportedThinkingLevels } from '@earendil-works/pi-ai/models';
import type { Provider, RefreshModelsContext } from '@earendil-works/pi-ai/models';
import type { ThinkingLevel, ServiceTier } from '../../../packages/contracts/src/index.ts';

type Capabilities = { levels: ThinkingLevel[]; advertised: ThinkingLevel[]; defaultLevel: ThinkingLevel; tiers: ServiceTier[]; outputLimitKnown: boolean; priceKnown: boolean };
const capabilities = new WeakMap<object, Capabilities>();
export function thinkingLevels(model: Model<Api>): ThinkingLevel[] { return capabilities.get(model)?.levels ?? [...getSupportedThinkingLevels(model)]; }
export function defaultThinking(model: Model<Api>, requested: ThinkingLevel): ThinkingLevel {
  if (requested === 'ultra' && !thinkingLevels(model).includes(requested)) throw new Error('thinking_level_unsupported');
  const current = capabilities.get(model);
  if (current) return current.levels.includes(requested) ? requested : current.defaultLevel;
  return clampThinkingLevel(model, requested as ModelThinkingLevel);
}
export function unavailableThinkingLevels(model: Model<Api>) { const value = capabilities.get(model); return value?.advertised.filter(level => !value.levels.includes(level)).map(level => ({ level, reason: 'Ultra 是 Codex 多 Agent 推理模式；当前 Pi 运行链尚未实现其语义，不能仅发送 ultra 或改成 Max' })) ?? []; }
export function modelServiceTiers(model: Model<Api>): ServiceTier[] | undefined { return capabilities.get(model)?.tiers; }
export function unknownOutputLimit(model: Model<Api>) { return capabilities.get(model)?.outputLimitKnown === false; }
export function unknownPrice(model: Model<Api>) { return capabilities.get(model)?.priceKnown === false; }
const levels = new Set(['off', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max', 'ultra']);
type RemoteModel = { slug: string; display_name?: string; visibility?: string; supported_in_api?: boolean; context_window?: number; max_output_tokens?: number; input_modalities?: string[]; default_reasoning_level?: string; supported_reasoning_levels?: { effort: string }[]; service_tiers?: { id: string }[] };

/** Only capability fields are consumed; remote model prompts are never instructions. */
export function readCatalog(body: unknown, provider: Provider): Model<Api>[] {
  const data = body as { models?: RemoteModel[]; data?: { id: string }[] };
  const known = new Map(provider.getModels().map(model => [model.id, model]));
  if (!Array.isArray(data.models)) {
    // API-key /models is an identity list, not a capability catalog. Do not invent
    // runtime contracts for unknown model IDs from that weaker response.
    if (!Array.isArray(data.data)) throw new Error('model_catalog_invalid');
    return data.data.flatMap(item => { const model = known.get(item.id); return model ? [model] : []; });
  }
  return data.models.filter(item => item.visibility !== 'hide' && item.supported_in_api !== false).map(item => {
    if (typeof item.slug !== 'string' || !item.slug || !Number.isSafeInteger(item.context_window) || item.context_window! <= 0) throw new Error('model_catalog_invalid');
    const supported = (item.supported_reasoning_levels ?? []).map(value => value.effort).filter(value => levels.has(value)) as ThinkingLevel[];
    if (!supported.length || !supported.includes(item.default_reasoning_level as ThinkingLevel)) throw new Error('model_catalog_invalid');
    const prior = known.get(item.slug);
    const model: Model<Api> = { ...(prior ?? {}), id: item.slug, provider: provider.id, api: provider.id === 'openai-codex' ? 'openai-codex-responses' : 'openai-responses', baseUrl: provider.baseUrl!, name: item.display_name ?? item.slug, reasoning: supported.some(value => value !== 'off'), input: item.input_modalities?.includes('image') ? ['text', 'image'] : ['text'], contextWindow: item.context_window!,
      // Subscription discovery does not quote API prices. These are pi-ai's
      // unpriced accounting placeholders, never a price or zero-cost claim.
      cost: prior?.cost ?? { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      maxTokens: item.max_output_tokens ?? prior?.maxTokens ?? item.context_window!,
      thinkingLevelMap: Object.fromEntries(supported.filter(value => value !== 'ultra').map(value => [value, value === 'off' ? 'none' : value])),
    };
    const runtime: ThinkingLevel[] = supported.filter(value => value !== 'ultra');
    if (!runtime.length) throw new Error('model_catalog_runtime_unsupported');
    capabilities.set(model, { levels: runtime, advertised: supported, defaultLevel: runtime.includes(item.default_reasoning_level as ThinkingLevel) ? item.default_reasoning_level as ThinkingLevel : runtime[0]!, tiers: ['default', ...(item.service_tiers?.some(value => value.id === 'priority') ? ['priority' as const] : [])], outputLimitKnown: item.max_output_tokens !== undefined || prior !== undefined, priceKnown: prior !== undefined });
    return model;
  });
}

/** Use pi-ai's public provider refresh and OAuth refresh; no alternate auth client. */
export function discoveredOpenAIProvider(provider: Provider, fetcher: typeof fetch = fetch): Provider {
  // Bundled metadata remains usable for old persisted tasks during discovery failure.
  // The catalog API reports refresh failures; it never labels this list current.
  let current: readonly Model<Api>[] = provider.getModels();
  let checkedAt = 0;
  let lastToken: string | undefined;
  return { ...provider, getModels: () => current, getAllModels: () => current,
    async refreshModels(context: RefreshModelsContext) {
      if (context.stored) {
        const restored = context.stored.models.filter(model => model.provider === provider.id && (!model.type || model.type === 'chat')) as Model<Api>[];
        for (const model of restored) {
          const saved = (model as Model<Api> & { qzCatalog?: Capabilities }).qzCatalog;
          if (saved) capabilities.set(model, saved);
        }
        if (!await context.publish({ update() { current = restored; } })) return;
      }
      if (!context.allowNetwork) return;
      const credential = context.credential;
      if (!credential) { await context.publish({ persist: null, update() { current = []; checkedAt = 0; lastToken = undefined; } }); return; }
      const token = credential.type === 'oauth' ? credential.access : credential.key;
      if (!token) throw new Error('model_catalog_auth_unavailable');
      if (!context.force && token === lastToken && Date.now() - checkedAt < 60_000) return;
      const headers: Record<string, string> = { Authorization: `Bearer ${token}` };
      const base = context.baseUrl ?? provider.baseUrl!;
      const url = provider.id === 'openai-codex' ? `${base}/codex/models?client_version=0.153.0` : `${base}/models`;
      if (provider.id === 'openai-codex' && credential.type === 'oauth' && typeof credential.accountId === 'string') headers['ChatGPT-Account-Id'] = credential.accountId;
      const response = await fetcher(url, { headers, signal: AbortSignal.any([context.signal, AbortSignal.timeout(15000)]) });
      if (!response.ok) throw new Error(`model_catalog_http_${response.status}`);
      const next = readCatalog(await response.json(), provider);
      const now = Date.now();
      await context.publish({ persist: { models: next.map(model => ({ ...model, ...(capabilities.has(model) ? { qzCatalog: capabilities.get(model) } : {}) })), checkedAt: now }, update() { current = next; checkedAt = now; lastToken = token; } });
    },
  };
}
