import { stdin, stderr } from 'node:process';
import { createInterface } from 'node:readline/promises';
import { hostname } from 'node:os';
import { connect } from './client.ts';
import { CliError, loadProfile, origin, saveProfile } from './profile.ts';
import type { Profile } from './profile.ts';
import type { SessionView } from '../../../packages/contracts/src/index.ts';

export async function hiddenPassword(signal?: AbortSignal): Promise<string> {
  if (!stdin.isTTY || !stderr.isTTY) throw new CliError('login_requires_user_terminal');
  stderr.write('Password: ');
  stdin.setRawMode(true); stdin.resume();
  try {
    return await new Promise<string>((resolve, reject) => {
      let password = '';
      const finish = (error?: CliError) => {
        stdin.off('data', data); signal?.removeEventListener('abort', abort);
        if (error) reject(error); else resolve(password);
      };
      const abort = () => finish(new CliError('login_cancelled'));
      const data = (chunk: Buffer) => {
        for (const character of chunk.toString('utf8')) {
          if (character === '\u0003' || character === '\u0004') { abort(); return; }
          if (character === '\r' || character === '\n') { finish(); return; }
          if (character === '\u007f' || character === '\b') password = password.slice(0, -1);
          else if (character >= ' ') password += character;
        }
      };
      signal?.addEventListener('abort', abort, { once: true });
      if (signal?.aborted) abort(); else stdin.on('data', data);
    });
  } finally { stdin.setRawMode(false); stdin.pause(); stderr.write('\n'); }
}
export async function deviceLogin(profile: Profile, password: string, name = hostname(), path?: string, signal?: AbortSignal) {
  const client = await connect(profile);
  try {
    const receipt = await client.request<{ token: string; id: string }>('POST', '/devices/login', JSON.stringify({ password, name }), signal);
    if (typeof receipt.token !== 'string' || !receipt.token || typeof receipt.id !== 'string') throw new CliError('login_response_invalid');
    await saveProfile({ ...profile, token: receipt.token }, path);
    return { id: receipt.id, authenticated: true }; // Never return the capability.
  } finally { await client.close(); }
}
export async function login(options: { origin?: string; name?: string; replace?: boolean; developmentHttp?: boolean; caCertificate?: string; signal?: AbortSignal }) {
  let saved: Profile | undefined;
  try { saved = await loadProfile(); } catch (error) { if (!(error instanceof CliError) || error.code !== 'login_required') throw error; }
  if (saved && !options.replace) {
    if (options.origin && origin(options.origin) !== saved.origin) throw new CliError('login_replace_required');
    const client = await connect(saved);
    try {
      const session = await client.request<SessionView>('GET', '/session', undefined, options.signal);
      if (!session.authenticated) throw new CliError('login_replace_required');
      return session;
    } finally { await client.close(); }
  }
  if (!stdin.isTTY || !stderr.isTTY) throw new CliError('login_requires_user_terminal');
  let address = options.origin ?? saved?.origin;
  if (!address) {
    const terminal = createInterface({ input: stdin, output: stderr });
    try { address = await terminal.question('QuaZonai origin: ', { signal: options.signal }); } finally { terminal.close(); }
  }
  const profile: Profile = { schema_version: 1, origin: origin(address), token: '', development_http: options.developmentHttp ?? address.startsWith('http://'), ca_certificate: options.caCertificate ?? saved?.ca_certificate ?? null };
  const client = await connect(profile);
  try {
    const session = await client.request<SessionView>('GET', '/session', undefined, options.signal);
    if (session.setupRequired) throw new CliError('set_instance_password_in_auth_ui');
  } finally { await client.close(); }
  return deviceLogin(profile, await hiddenPassword(options.signal), options.name, undefined, options.signal);
}
