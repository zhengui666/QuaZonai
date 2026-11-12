import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import { mkdir, mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { executeNative } from '../../research/src/native-execution.ts';
import { canonical } from '../../research/src/documents.ts';
import { object } from './facts.ts';

export interface PmScienceCheckpoint {
  read(key: string): Promise<JsonObject | undefined>;
  write(key: string, value: JsonObject): Promise<void>;
}

/** Compare actual native rows with the digest taken while the original fit
 * read those rows. Paths, matching metadata and a fresh digest taken only at
 * independent evaluation cannot establish that the original history survived.
 */
export async function verifyOriginalPrefix(source: JsonObject, catalog: string, binary: NativeTarget, dataDir: string, context: Context): Promise<JsonObject> {
  const expected = object(source.catalogPrefix, 'original_fit_catalog_prefix_evidence');
  const selection = object(object(object(source.request).forecast).selection, 'original_fit_prefix_selection');
  if (expected.method !== 'NAUTILUS_SELECTED_PREFIX_SHA256_V2' || typeof expected.content_sha256 !== 'string'
    || !/^[0-9a-f]{64}$/.test(expected.content_sha256) || canonical((expected.selection!)) !== canonical((selection))) throw new Error('pm_original_catalog_prefix_evidence_invalid');
  const root = join(dataDir, 'scratch'); await mkdir(root, { recursive: true });
  const directory = await mkdtemp(join(root, 'pm-source-prefix-'));
  try {
    const actual = await executeNative(binary, 'catalog-prefix-digest', selection, ['--catalog', catalog], directory, context);
    if (canonical(actual) !== canonical(expected)) throw new Error('pm_original_catalog_prefix_changed');
    return actual;
  } finally { await rm(directory, { recursive: true, force: true }); }
}
