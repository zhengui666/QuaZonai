import { builtinModels } from '@earendil-works/pi-ai/providers/all';
import type { Api, CredentialStore, Model } from '@earendil-works/pi-ai';
import type { Models, Provider } from '@earendil-works/pi-ai/models';
import type { ServiceTier } from '../../../packages/contracts/src/index.ts';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

// Providers, catalogs and auth resolution belong to official pi-ai. Modules
// are trusted host configuration; credentials never become research input.
export async function configuredModels(modules: readonly string[] = [], credentials?: CredentialStore) {
  const models = builtinModels({ credentials });
  for (const path of modules) {
    const module = await import(pathToFileURL(resolve(path)).href);
    models.setProvider(module.default as Provider);
  }
  await models.refresh({ allowNetwork: false });
  return models;
}

export function supportsServiceTier(model: Model<Api>) {
  return model.api === 'openai-codex-responses' || model.provider === 'openai' && model.api === 'openai-responses';
}

export function subscriptionSpeed(models: Models) {
  // Index QZ-admitted studies before native work resumes, including compaction.
  const tiers = new Map<string, ServiceTier>();
  const streamSimple: Models['streamSimple'] = (model, transcript, options) => {
    if (!supportsServiceTier(model)) return models.streamSimple(model, transcript, options);
    return models.streamSimple(model, transcript, { ...options, async onPayload(payload, selected) {
      const transformed = await options?.onPayload?.(payload, selected);
      const tier = options?.sessionId ? tiers.get(options.sessionId) : undefined;
      // Durable and pi-ai's simple API omit serviceTier; use the public payload hook.
      return tier ? { ...(transformed ?? payload) as object, service_tier: tier } : transformed;
    } });
  };
  const completeSimple: Models['completeSimple'] = (model, transcript, options) => supportsServiceTier(model)
    ? streamSimple(model, transcript, options).result() : models.completeSimple(model, transcript, options);
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
