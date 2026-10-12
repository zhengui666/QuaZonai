import { Agent, fetch } from 'undici';
import { readFile } from 'node:fs/promises';
const API = '/api/v3'; // Source protocol: packages/contracts/src/index.ts.
import { CliError, origin } from './profile.ts';
import type { Profile } from './profile.ts';

export async function connect(profile: Profile) {
  const base = origin(profile.origin);
  const dispatcher = new Agent(profile.ca_certificate ? { connect: { ca: await readFile(profile.ca_certificate, 'utf8') } } : {});
  return {
    async request<T = unknown>(method: string, path: string, body?: string, signal?: AbortSignal): Promise<T> {
      let response;
      try {
        response = await fetch(`${base}${API}${path}`, {
          method, body, dispatcher, redirect: 'error', signal: signal ? AbortSignal.any([signal, AbortSignal.timeout(30_000)]) : AbortSignal.timeout(30_000),
          headers: { accept: 'application/json', 'x-quazonai-cli': '1', ...(body !== undefined ? { 'content-type': 'application/json' } : {}), ...(profile.token ? { authorization: `Bearer ${profile.token}` } : {}) },
        });
      } catch { throw new CliError(method === 'GET' ? 'service_unavailable' : 'write_outcome_unknown_retain_original_request'); }
      if (!response.ok) {
        let code = 'http_error';
        try { const problem = await response.json() as { error?: unknown }; if (typeof problem.error === 'string' && /^[a-zA-Z0-9_]+$/.test(problem.error)) code = problem.error; } catch { /* No raw response/credential output. */ }
        throw new CliError(code, response.status);
      }
      if (response.status === 204) return null as T;
      try { return await response.json() as T; }
      catch { throw new CliError(method === 'GET' ? 'invalid_response' : 'write_outcome_unknown_retain_original_request'); }
    },
    async close() { await dispatcher.close(); },
  };
}
export type Client = Awaited<ReturnType<typeof connect>>;
