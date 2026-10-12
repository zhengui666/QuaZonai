import { DEFAULT_COMPACTION_POLICY } from '@earendil-works/pi-durable';
import type { HarnessSettings } from '@earendil-works/pi-durable';

// Starting values, not a provider performance guarantee. These are live host
// settings; native pi.agent retains each conversation's model/thinking choice.
export function researchSettings(value: HarnessSettings = {}): HarnessSettings {
  const settings = {
    ...value,
    stream: { timeoutMs: 120_000, maxRetries: 0, maxRetryDelayMs: 60_000, ...value.stream },
    retry: { enabled: true, maxRetries: 6, baseDelayMs: 2_000, maxAgentDelayMs: 60_000, ...value.retry },
    compaction: { ...DEFAULT_COMPACTION_POLICY, ...value.compaction },
    progress: { partialIntervalMs: 250, outputIntervalMs: 250, ...value.progress },
    toolExecution: value.toolExecution ?? 'parallel',
    steeringMode: value.steeringMode ?? 'one-at-a-time',
    followUpMode: value.followUpMode ?? 'one-at-a-time',
    contextRetentionMs: value.contextRetentionMs ?? 600_000,
  } satisfies HarnessSettings;
  for (const [group, fields] of Object.entries({ stream: settings.stream, retry: settings.retry, compaction: settings.compaction, progress: settings.progress })) {
    for (const [key, number] of Object.entries(fields)) {
      if (key.endsWith('Ms') || key.endsWith('Tokens') || key === 'maxRetries') {
        if (!Number.isSafeInteger(number) || (number as number) < 0 || (key === 'timeoutMs' && number === 0)) throw new Error(`invalid_settings_${group}_${key}`);
      }
    }
  }
  if (!Number.isSafeInteger(settings.contextRetentionMs) || settings.contextRetentionMs < 0) throw new Error('invalid_settings_contextRetentionMs');
  return settings;
}
