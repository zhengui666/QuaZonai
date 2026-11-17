import { discoveredOpenAIProvider, unknownOutputLimit } from './model-catalog.ts';
import { builtinModels } from '@earendil-works/pi-ai/providers/all';
import type { Api, CredentialStore, Model } from '@earendil-works/pi-ai';
import type { Models, Provider } from '@earendil-works/pi-ai/models';
import type { ServiceTier } from '../../../packages/contracts/src/index.ts';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { defineDoc } from '@earendil-works/pi-durable';

export const ModelTiers = defineDoc<{ tiers: Record<string, ServiceTier> }>({
  kind: 'qz.model-tiers', version: 1, scope: 'session', initial: () => ({ tiers: {} }),
  checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127,
});

// Providers, catalogs and auth resolution belong to official pi-ai. Modules
// are trusted host configuration; credentials never become research input.
export async function configuredModels(modules: readonly string[] = [], credentials?: CredentialStore) {
  const models = builtinModels({ credentials });
  for (const id of ['openai', 'openai-codex']) { const provider = models.getProvider(id); if (provider) models.setProvider(discoveredOpenAIProvider(provider)); }
  for (const path of modules) {
    const module = await import(pathToFileURL(resolve(path)).href);
    models.setProvider(module.default as Provider);
  }
  await models.refresh({ allowNetwork: true });
  return models;
}

export function supportsServiceTier(model: Model<Api>) {
  return model.api === 'openai-codex-responses' || model.provider === 'openai' && model.api === 'openai-responses';
}

export function subscriptionSpeed(models: Models, resolveTier?: (sessionId: string) => Promise<ServiceTier | undefined>) {
  // Index QZ-admitted studies before native work resumes, including compaction.
  const tiers = new Map<string, ServiceTier>();
  const streamSimple: Models['streamSimple'] = (model, transcript, supplied) => {
    // Also cover old Durable checkpoints which captured stream options before
    // the current host settings took effect. Keep provider/model native limits.
    const options = supplied ?? {};
    if (String(options.reasoning) === 'ultra') throw new Error('thinking_level_unsupported');
    if (!supportsServiceTier(model)) return models.streamSimple(model, transcript, options);
    return models.streamSimple(model, transcript, { ...options, async onPayload(payload, selected) {
      const transformed = await options?.onPayload?.(payload, selected);
      const tier = options?.sessionId ? tiers.get(options.sessionId) ?? await resolveTier?.(options.sessionId) : undefined;
      // Durable and pi-ai's simple API omit serviceTier; use the public payload hook.
      const body = { ...(transformed ?? payload) as Record<string, unknown> };
      if (tier) body.service_tier = tier;
      if (unknownOutputLimit(model)) delete body.max_output_tokens;
      return body;
    } });
  };
  const completeSimple: Models['completeSimple'] = (model, transcript, supplied) => {
    const options = supplied ?? {};
    if (String(options.reasoning) === 'ultra') throw new Error('thinking_level_unsupported');
    return supportsServiceTier(model) ? streamSimple(model, transcript, options).result() : models.completeSimple(model, transcript, options);
  };
  return {
    tiers,
    models: new Proxy(models, { get(target, key) {
      if (key === 'streamSimple') return streamSimple;
      if (key === 'completeSimple') return completeSimple;
      const value = Reflect.get(target, key);
      return typeof value === 'function' ? value.bind(target) : value;
    } }),
  };
}
