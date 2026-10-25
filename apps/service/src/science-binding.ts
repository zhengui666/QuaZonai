import type { JsonObject } from '@earendil-works/pi-durable';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { validateRemoteTarget } from '../../research/src/native-execution.ts';
import { bindScienceData } from './config.ts';
import type { ServiceConfig } from './config.ts';
import type { PmRecord } from './pm-facts.ts';
import { json } from './facts.ts';

/** A resumed scientific operation belongs to its admitted instance. Current
 * configuration may select a new instance only when admitting new work. */
export function boundScienceTarget(config: Pick<ServiceConfig, 'jobBinary' | 'nautilus'>, data: JsonObject): NativeTarget {
  if (data.nautilusTarget !== undefined) return validateRemoteTarget(data.nautilusTarget);
  if (process.env.QZ_REQUIRE_REMOTE_NAUTILUS === '1' || config.nautilus?.backtest) throw new Error('legacy_local_nautilus_task_requires_migration');
  if (config.jobBinary) return config.jobBinary;
  throw new Error('nautilus_backtest_not_configured');
}

/** Transport is a mandate fact, not a shared immutable dataset revision. */
export function pmScienceData(record: Pick<PmRecord, 'artifacts'>, data: JsonObject): JsonObject {
  const binding = record.artifacts['nautilus-binding'];
  const original = json(data);
  return binding ? bindScienceData(original, validateRemoteTarget(binding.target)) : original;
}
