import { randomUUID } from 'node:crypto';
import { readFile, rename, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { FastifyInstance } from 'fastify';
import { API } from '../../../packages/contracts/src/index.ts';
import type { InstanceSettingsUpdate, InstanceSettingsView } from '../../../packages/contracts/src/index.ts';
import { bindScienceData, scienceDataWithoutBinding, validateNautilusConfiguration } from './config.ts';
import type { ServiceConfig } from './config.ts';
import { json, object, Problem } from './facts.ts';
import { canonical } from '../../research/src/documents.ts';

function contents(value: Pick<InstanceSettingsView, 'backtest' | 'connections'>): string {
  return canonical(json({ backtest: value.backtest, connections: value.connections }));
}

function validate(value: unknown, revisionKey: 'revision' | 'expectedRevision'): InstanceSettingsView | InstanceSettingsUpdate {
  const body = object(value, 'instance_settings');
  if (Object.keys(body).some(key => ![revisionKey, 'backtest', 'connections'].includes(key))
    || !Number.isSafeInteger(body[revisionKey]) || Number(body[revisionKey]) < 0
    || !Object.hasOwn(body, 'backtest') || !Array.isArray(body.connections)) throw new Problem('instance_settings_invalid');
  if (body.connections.some(connection => !connection || typeof connection !== 'object' || Array.isArray(connection)
    || Object.keys(connection).some(key => !['id', 'name', 'mode', 'url', 'clientId', 'accountId', 'tokenEnv'].includes(key)))) throw new Problem('nautilus_connection_invalid');
  validateNautilusConfiguration({ nautilus: body.backtest === null ? {} : { backtest: body.backtest as never }, connections: body.connections as never });
  return structuredClone(value) as InstanceSettingsView | InstanceSettingsUpdate;
}

/** An atomic owner configuration file applies to new admissions only. Durable
 * research, mandate and observation facts retain their captured transports. */
export async function instanceSettings(config: ServiceConfig) {
  const path = join(config.dataDir, 'instances.json');
  let current: InstanceSettingsView = { revision: 0, backtest: structuredClone(config.nautilus?.backtest ?? null), connections: structuredClone(config.connections) };
  let tail: Promise<unknown> = Promise.resolve();
  let closing = false;

  function apply(value: InstanceSettingsView) {
    const backtest = value.backtest;
    config.nautilus = backtest ? { backtest: structuredClone(backtest) } : {};
    config.connections = structuredClone(value.connections);
    for (const dataset of config.datasets) {
      const data = scienceDataWithoutBinding(dataset.data);
      dataset.data = backtest ? bindScienceData(data, backtest) : data;
    }
  }
  try {
    const stored = JSON.parse(await readFile(path, 'utf8'));
    current = validate(stored, 'revision') as InstanceSettingsView;
    apply(current);
  } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }

  const view = () => structuredClone(current);
  function save(value: unknown): Promise<InstanceSettingsView> {
    const input = validate(value, 'expectedRevision') as InstanceSettingsUpdate;
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
      const temporary = `${path}.${randomUUID()}.tmp`;
      try {
        await writeFile(temporary, JSON.stringify(next, null, 2) + '\n', { mode: 0o600, flag: 'wx' });
        await rename(temporary, path);
      } catch {
        await rm(temporary, { force: true }).catch(() => undefined);
        throw new Problem('instance_settings_save_failed', 503);
      }
      // No await separates the committed file from the configuration observed
      // by newly admitted work. Existing frozen facts are never rewritten.
      apply(next); current = next;
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
