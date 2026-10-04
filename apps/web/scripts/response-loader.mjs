// No transport retries here. A write can be committed before its validator
// arrives; only a later explicit validation attempt may initiate another load.
export class ResponseValidatorLoadError extends Error {
  constructor(moduleId, cause) {
    super('Response validator could not be loaded', { cause });
    this.name = 'ResponseValidatorLoadError';
    this.moduleId = moduleId;
  }
}
export function createValidatorLoader(loaders) {
  const pending = new Map();
  function load(moduleId) {
    if (pending.has(moduleId)) return pending.get(moduleId);
    const loader = loaders[moduleId];
    if (typeof loader !== 'function') return Promise.reject(new ResponseValidatorLoadError(moduleId, new Error('Unknown generated validator module')));
    const promise = Promise.resolve().then(loader).then(namespace => {
      // Always use the canonical CJS default object in Node and Vite. A broken
      // module namespace must reject rather than become a permissive fallback.
      if (!namespace?.default || typeof namespace.default !== 'object') throw new Error('Invalid validator module namespace');
      return namespace.default;
    }).catch(cause => {
      if (pending.get(moduleId) === promise) pending.delete(moduleId);
      throw new ResponseValidatorLoadError(moduleId, cause);
    });
    pending.set(moduleId, promise);
    return promise;
  }
  return async function validate(descriptor, value) {
    const module = await load(descriptor.module);
    const native = module[descriptor.name];
    if (typeof native !== 'function') throw new ResponseValidatorLoadError(descriptor.module, new Error('Missing generated validator binding'));
    // AJV's .errors is shared mutable state. Keep native validation and the full
    // snapshot in this SAME synchronous continuation, before another await.
    const valid = native(value);
    const errors = native.errors === null || native.errors === undefined ? null : structuredClone(native.errors);
    return { valid, errors };
  };
}
