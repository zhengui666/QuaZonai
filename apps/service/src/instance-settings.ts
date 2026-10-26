import { randomUUID } from 'node:crypto';
import { readFile, rename, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { FastifyInstance } from 'fastify';
import { API } from '../../../packages/contracts/src/index.ts';
import type { InstanceSettingsUpdate, InstanceSettingsView } from '../../../packages/contracts/src/index.ts';
import { bindScienceData, remoteNativeBinding, scienceDataWithoutBinding, validateNautilusConfiguration } from './config.ts';
import type { ConnectionConfig, ServiceConfig } from './config.ts';
import { json, object, Problem } from './facts.ts';
import { canonical } from '../../research/src/documents.ts';
import { setRemoteNativeCredential } from '../../research/src/native-execution.ts';

interface CredentialState {
  backtestRef?: string;
  connectionRefs: Record<string, string>;
  history: Record<string, string>;
  backtestTokenEnv?: string;
  connectionTokenEnvs: Record<string, string>;
}
type StoredSettings = InstanceSettingsView & { credentialState?: CredentialState };
const credentialRefPattern = /^[a-f0-9]{8}-[a-f0-9]{4}-[1-8][a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/i;

function contents(value: Pick<InstanceSettingsView, 'backtest' | 'connections'>): string {
  return canonical(json({ backtest: value.backtest, connections: value.connections }));
}

function validate(value: unknown, revisionKey: 'revision' | 'expectedRevision'): InstanceSettingsView | InstanceSettingsUpdate {
  const body = object(value, 'instance_settings');
  if (Object.keys(body).some(key => ![revisionKey, 'backtest', 'connections'].includes(key))
    || !Number.isSafeInteger(body[revisionKey]) || Number(body[revisionKey]) < 0
    || !Object.hasOwn(body, 'backtest') || !Array.isArray(body.connections)) throw new Problem('instance_settings_invalid');
  if (body.connections.some(connection => !connection || typeof connection !== 'object' || Array.isArray(connection)
    || Object.keys(connection).some(key => !['id', 'name', 'mode', 'url', 'clientId', 'accountId', 'token', 'tokenEnv'].includes(key)))) throw new Problem('nautilus_connection_invalid');
  validateNautilusConfiguration({ nautilus: body.backtest === null ? {} : { backtest: body.backtest as never }, connections: body.connections as never });
  return structuredClone(value) as InstanceSettingsView | InstanceSettingsUpdate;
}

function readCredentialState(value: unknown): CredentialState {
  if (value === undefined) return { connectionRefs: {}, history: {}, connectionTokenEnvs: {} };
  const state = object(value, 'instance_credentials');
  if (Object.keys(state).some(key => !['backtestRef', 'connectionRefs', 'history', 'backtestTokenEnv', 'connectionTokenEnvs'].includes(key))) throw new Error('instance_credentials_invalid');
  if (state.backtestRef !== undefined && (typeof state.backtestRef !== 'string' || !credentialRefPattern.test(state.backtestRef))) throw new Error('instance_credentials_invalid');
  if (state.backtestTokenEnv !== undefined && (typeof state.backtestTokenEnv !== 'string' || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(state.backtestTokenEnv))) throw new Error('instance_credentials_invalid');
  const readRefs = (value: unknown) => {
    if (value === undefined) return {};
    const entries = object(value, 'instance_credentials');
    if (Object.values(entries).some(reference => typeof reference !== 'string' || !credentialRefPattern.test(reference))) throw new Error('instance_credentials_invalid');
    return entries as Record<string, string>;
  };
  const readHistory = (value: unknown) => {
    if (value === undefined) return {};
    const entries = object(value, 'instance_credentials');
    if (Object.entries(entries).some(([reference, token]) => !credentialRefPattern.test(reference) || typeof token !== 'string')) throw new Error('instance_credentials_invalid');
    return entries as Record<string, string>;
  };
  const readEnvs = (value: unknown) => {
    if (value === undefined) return {};
    const entries = object(value, 'instance_credentials');
    if (Object.values(entries).some(name => typeof name !== 'string' || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(name))) throw new Error('instance_credentials_invalid');
    return entries as Record<string, string>;
  };
  return { ...(typeof state.backtestRef === 'string' ? { backtestRef: state.backtestRef } : {}), connectionRefs: readRefs(state.connectionRefs), history: readHistory(state.history),
    ...(typeof state.backtestTokenEnv === 'string' ? { backtestTokenEnv: state.backtestTokenEnv } : {}), connectionTokenEnvs: readEnvs(state.connectionTokenEnvs) };
}

function migrateLegacyTokens<T extends Pick<InstanceSettingsView, 'backtest' | 'connections'>>(value: T): T {
  const migrate = <T extends { token?: string; tokenEnv?: string }>(item: T): T => {
    const token = !Object.hasOwn(item, 'token') && item.tokenEnv ? process.env[item.tokenEnv] : undefined;
    const { tokenEnv: _tokenEnv, credentialRef: _credentialRef, ...rest } = item as T & { credentialRef?: string };
    return token ? { ...rest, token } as T : rest as T;
  };
  return { ...value, backtest: value.backtest ? migrate(value.backtest) : null, connections: value.connections.map(migrate) };
}

function legacyTokenEnvs(value: InstanceSettingsView): Pick<CredentialState, 'backtestTokenEnv' | 'connectionTokenEnvs'> {
  return { ...(value.backtest?.tokenEnv && !Object.hasOwn(value.backtest, 'token') ? { backtestTokenEnv: value.backtest.tokenEnv } : {}),
    connectionTokenEnvs: Object.fromEntries(value.connections.filter(connection => connection.tokenEnv && !Object.hasOwn(connection, 'token')).map(connection => [connection.id, connection.tokenEnv!])) };
}

function assignCurrentRefs(value: InstanceSettingsView, state: CredentialState): { state: CredentialState; changed: boolean } {
  let changed = false;
  let backtestRef = state.backtestRef;
  if (value.backtest?.token && !backtestRef) { backtestRef = randomUUID(); changed = true; }
  const connectionRefs = { ...state.connectionRefs };
  for (const connection of value.connections) if (connection.token && !connectionRefs[connection.id]) {
    connectionRefs[connection.id] = randomUUID(); changed = true;
  }
  return { state: { ...(backtestRef ? { backtestRef } : {}), connectionRefs, history: { ...state.history },
    ...(state.backtestTokenEnv ? { backtestTokenEnv: state.backtestTokenEnv } : {}), connectionTokenEnvs: { ...state.connectionTokenEnvs } }, changed };
}

function nextCredentialState(current: InstanceSettingsView, next: InstanceSettingsView, state: CredentialState): CredentialState {
  const history = { ...state.history };
  if (current.backtest?.token && state.backtestRef) history[state.backtestRef] = current.backtest.token;
  for (const connection of current.connections) {
    const reference = state.connectionRefs[connection.id];
    if (connection.token && reference) history[reference] = connection.token;
  }
  const sameBacktest = current.backtest?.token && current.backtest.token === next.backtest?.token;
  const backtestRef = next.backtest?.token ? (sameBacktest && state.backtestRef ? state.backtestRef : randomUUID()) : undefined;
  if (backtestRef) delete history[backtestRef];
  const connectionRefs: Record<string, string> = {};
  for (const connection of next.connections) {
    if (!connection.token) continue;
    const old = current.connections.find(item => item.id === connection.id);
    const oldRef = state.connectionRefs[connection.id];
    const reference = old?.token === connection.token && oldRef ? oldRef : randomUUID();
    connectionRefs[connection.id] = reference; delete history[reference];
  }
  return { ...(backtestRef ? { backtestRef } : {}), connectionRefs, history, connectionTokenEnvs: {} };
}

function hasDirectToken(value: InstanceSettingsView): boolean {
  return !!value.backtest?.token || value.connections.some(connection => !!connection.token);
}

/** An atomic owner configuration file applies to new admissions only. Durable
 * research, mandate and observation facts retain their captured transports. */
export async function instanceSettings(config: ServiceConfig) {
  const path = join(config.dataDir, 'instances.json');
  let current: InstanceSettingsView = { revision: 0, backtest: structuredClone(config.nautilus?.backtest ?? null), connections: structuredClone(config.connections) };
  let credentials: CredentialState = { connectionRefs: {}, history: {}, connectionTokenEnvs: {} };
  let tail: Promise<unknown> = Promise.resolve();
  let closing = false;

  async function persist(value: InstanceSettingsView, state: CredentialState) {
    const temporary = `${path}.${randomUUID()}.tmp`;
    try {
      await writeFile(temporary, JSON.stringify({ ...value, credentialState: state }, null, 2) + '\n', { mode: 0o600, flag: 'wx' });
      await rename(temporary, path);
    } catch {
      await rm(temporary, { force: true }).catch(() => undefined);
      throw new Problem('instance_settings_save_failed', 503);
    }
  }

  function apply(value: InstanceSettingsView, state: CredentialState) {
    for (const [reference, token] of Object.entries(state.history)) setRemoteNativeCredential(reference, token);
    const backtest = value.backtest ? { ...value.backtest, ...(!Object.hasOwn(value.backtest, 'token') && state.backtestTokenEnv ? { tokenEnv: state.backtestTokenEnv } : {}) } : null;
    const target = backtest ? remoteNativeBinding(backtest, state.backtestRef) : null;
    config.nautilus = target ? { backtest: target } : {};
    config.connections = value.connections.map(connection => {
      if (Object.hasOwn(connection, 'token')) {
        const { token, tokenEnv: _tokenEnv, ...safe } = connection;
        const reference = state.connectionRefs[connection.id];
        if (token && reference) { setRemoteNativeCredential(reference, token); return { ...safe, credentialRef: reference } as ConnectionConfig; }
        return safe as ConnectionConfig;
      }
      const { tokenEnv: _tokenEnv, ...safe } = connection;
      const legacy = state.connectionTokenEnvs[connection.id] ?? connection.tokenEnv;
      return legacy ? { ...safe, tokenEnv: legacy } as ConnectionConfig : safe as ConnectionConfig;
    });
    for (const dataset of config.datasets) {
      const data = scienceDataWithoutBinding(dataset.data);
      dataset.data = target ? bindScienceData(data, target) : data;
    }
  }

  let loaded = false;
  try {
    const stored = JSON.parse(await readFile(path, 'utf8')) as StoredSettings;
    const { credentialState, ...view } = stored;
    credentials = readCredentialState(credentialState);
    const legacy = legacyTokenEnvs(view as InstanceSettingsView);
    credentials = { ...credentials, ...(!credentials.backtestTokenEnv && legacy.backtestTokenEnv ? { backtestTokenEnv: legacy.backtestTokenEnv } : {}),
      connectionTokenEnvs: { ...legacy.connectionTokenEnvs, ...credentials.connectionTokenEnvs } };
    current = migrateLegacyTokens(validate(view, 'revision') as InstanceSettingsView);
    loaded = true;
  } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }

  if (!loaded) credentials = { ...credentials, ...legacyTokenEnvs(current) };
  current = migrateLegacyTokens(current);
  const assigned = assignCurrentRefs(current, credentials);
  credentials = assigned.state;
  for (const [reference, token] of Object.entries(credentials.history)) setRemoteNativeCredential(reference, token);
  if (assigned.changed || !loaded && hasDirectToken(current)) {
    await persist(current, credentials);
  }
  if (loaded || hasDirectToken(current)) apply(current, credentials);

  const view = () => structuredClone(current);
  function save(value: unknown): Promise<InstanceSettingsView> {
    const raw = validate(value, 'expectedRevision') as InstanceSettingsUpdate;
    const input = migrateLegacyTokens(raw);
    const legacy = legacyTokenEnvs(raw as unknown as InstanceSettingsView);
    const pending = tail.then(async () => {
      if (closing) throw new Problem('service_closing', 503);
      // PUT reconciliation after a lost successful response preserves the newer
      // revision instead of advancing it or replaying an old configuration.
      if (input.expectedRevision !== current.revision) {
        if (contents(input) === contents(current)) return view();
        throw new Problem('instance_settings_revision_conflict', 409);
      }
      if (current.revision === Number.MAX_SAFE_INTEGER) throw new Problem('instance_settings_revision_exhausted', 409);
      const next: InstanceSettingsView = { revision: current.revision + 1, backtest: input.backtest, connections: input.connections };
      const nextCredentials = nextCredentialState(current, next, credentials);
      if (!next.backtest?.token && legacy.backtestTokenEnv) nextCredentials.backtestTokenEnv = legacy.backtestTokenEnv;
      for (const [id, name] of Object.entries(legacy.connectionTokenEnvs)) if (!next.connections.find(connection => connection.id === id)?.token) nextCredentials.connectionTokenEnvs[id] = name;
      await persist(next, nextCredentials);
      apply(next, nextCredentials); current = next; credentials = nextCredentials;
      return view();
    });
    tail = pending.catch(() => undefined);
    return pending;
  }
  return { view, save, async close() { closing = true; await tail; } };
}

export function registerInstanceSettings(app: FastifyInstance, settings: Awaited<ReturnType<typeof instanceSettings>>) {
  app.get(`${API}/settings/instances`, settings.view);
  app.put(`${API}/settings/instances`, request => settings.save(request.body));
}
