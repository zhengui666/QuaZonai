import { builtinModels } from '@earendil-works/pi-ai/providers/all';
import type { CredentialStore } from '@earendil-works/pi-ai';
import type { Provider } from '@earendil-works/pi-ai/models';
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
