import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

// Controlled compiler outputs check artifact handoff, not Rust/native correctness.
test('native build preserves standard outputs before publishing the high-precision companions', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-precision-build-'));
  const tools = join(directory, 'tools'); const target = join(directory, 'target'); const output = join(directory, 'artifacts');
  await mkdir(tools);
  await writeFile(join(tools, 'cargo'), `#!/usr/bin/env bash
set -euo pipefail
profile=debug
precision=standard
for value in "$@"; do
  [[ "$value" != --release ]] || profile=release
  [[ "$value" != market-history,catalog-prepare,hyperliquid-offline ]] || precision=high
done
mkdir -p "$CARGO_TARGET_DIR/$profile/examples"
for binary in job source-tools catalog-prepare market-history examples/browser_catalog_fixture; do
  printf '%s:%s\\n' "$precision" "$binary" > "$CARGO_TARGET_DIR/$profile/$binary"
  chmod +x "$CARGO_TARGET_DIR/$profile/$binary"
done
`, { mode: 0o700 });
  try {
    for (const profile of ['debug', 'release']) {
      const destination = join(output, profile);
      const result = spawnSync('bash', [new URL('./native-build.sh', import.meta.url).pathname, profile, destination], {
        env: { ...process.env, PATH: `${tools}:${process.env.PATH}`, CARGO_TARGET_DIR: target }, encoding: 'utf8',
      });
      assert.equal(result.status, 0, result.stderr);
      for (const binary of ['job', 'source-tools', 'catalog-prepare', 'browser_catalog_fixture']) {
        const expected = binary === 'browser_catalog_fixture' ? `standard:examples/${binary}\n` : `standard:${binary}\n`;
        assert.equal(await readFile(join(destination, binary), 'utf8'), expected);
      }
      for (const [name, binary] of [['job-high-precision', 'job'], ['catalog-prepare-high-precision', 'catalog-prepare'], ['market-history', 'market-history']]) {
        assert.equal(await readFile(join(destination, name), 'utf8'), `high:${binary}\n`);
      }
    }
  } finally { await rm(directory, { recursive: true, force: true }); }
});
