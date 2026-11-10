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
for binary in job source-tools catalog-prepare market-history; do
  cat > "$CARGO_TARGET_DIR/$profile/$binary" <<EOF
#!/bin/sh
# $precision:$binary
printf '%s\\n' '{"nautilusVersion":"test","nautilusRevision":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","precision":"$precision","jobVersion":"test"}'
EOF
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
      for (const binary of ['job', 'source-tools', 'catalog-prepare']) {
        const expected = `standard:${binary}`;
        assert.ok((await readFile(join(destination, binary), 'utf8')).includes(expected));
      }
      for (const [name, binary] of [['job-high-precision', 'job'], ['catalog-prepare-high-precision', 'catalog-prepare'], ['market-history', 'market-history']]) {
        assert.ok((await readFile(join(destination, name), 'utf8')).includes(`high:${binary}`));
      }
      const manifest = JSON.parse(await readFile(join(destination, 'native-manifest.json'), 'utf8'));
      assert.equal(manifest.nautilusRevision, 'a'.repeat(40));
      assert.equal(manifest.binaries.job.precision, 'standard');
      assert.equal(manifest.binaries['job-high-precision'].precision, 'high');
      assert.notEqual(manifest.binaries.job.digest, manifest.binaries['job-high-precision'].digest);
    }
  } finally { await rm(directory, { recursive: true, force: true }); }
});
