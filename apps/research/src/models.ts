import { builtinModels } from '@earendil-works/pi-ai/providers/all';
import type { Provider } from '@earendil-works/pi-ai/models';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

// Providers, catalogs and auth resolution belong to official pi-ai. Modules
// are trusted host configuration; credentials never become research input.
export async function configuredModels(modules: readonly string[] = []) {
  const models = builtinModels();
  for (const path of modules) {
    const module = await import(pathToFileURL(resolve(path)).href);
    models.setProvider(module.default as Provider);
  }
  await models.refresh({ allowNetwork: false });
  return models;
}
