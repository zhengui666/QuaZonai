import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { executeNative } from '../plugins/native-portfolio.ts';

test('native subprocess temporary files stay inside the owned scientific directory', async () => {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-native-scratch-'));
  try {
    const binary = join(directory, 'job.mjs');
    await writeFile(binary, `#!${process.execPath}
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { writeFile, rm } from 'node:fs/promises';
for await (const chunk of process.stdin) {}
const file = join(tmpdir(), 'qz-native-temp-' + process.pid);
await writeFile(file, 'transport-only temporary content');
await rm(file);
process.stdout.write(JSON.stringify({ temporaryRoot: tmpdir(), cwd: process.cwd() }));
`, { mode: 0o700 });
    const result = await executeNative(binary, 'simulate', {}, [], directory, BACKGROUND_CONTEXT);
    assert.equal(result.cwd, directory);
    assert.equal(result.temporaryRoot, directory);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
