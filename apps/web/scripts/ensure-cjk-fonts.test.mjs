import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const script = fileURLToPath(new URL('./ensure-cjk-fonts.sh', import.meta.url));
function run({ installed = true, font = 'Noto Sans CJK SC' } = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'qz-font-install-'));
  const marker = join(dir, 'installed'); const calls = join(dir, 'calls');
  if (installed) writeFileSync(marker, 'yes');
  const mock = (name, body) => writeFileSync(join(dir, name), `#!/bin/bash\nset -eu\n${body}\n`, { mode: 0o755 });
  mock('dpkg-query', '[ -f "$FONT_MARKER" ] || exit 1\nprintf "install ok installed\\n"');
  mock('fc-match', 'printf "%s\\n" "$FONT_NAME"');
  // Intercept the bounded installer; never invoke apt or sudo in unit tests.
  mock('timeout', 'printf "%s\\n" "$@" > "$FONT_CALLS"\ntouch "$FONT_MARKER"');
  try {
    const result = spawnSync('bash', [script], { encoding: 'utf8', timeout: 8000,
      env: { ...process.env, PATH: `${dir}:${process.env.PATH}`, FONT_MARKER: marker, FONT_CALLS: calls, FONT_NAME: font } });
    return { status: result.status, args: existsSync(calls) ? readFileSync(calls, 'utf8').split('\n') : [] };
  } finally { rmSync(dir, { recursive: true, force: true }); }
}
test('an installed official CJK package does not run the installer', () => {
  const result = run(); assert.equal(result.status, 0); assert.deepEqual(result.args, []);
});
test('a missing package uses bounded, noninteractive install without updating indexes', () => {
  const result = run({ installed: false }); assert.equal(result.status, 0);
  for (const option of ['--signal=TERM', '--kill-after=5s', '60s', 'sudo', '-n', 'DEBIAN_FRONTEND=noninteractive',
    'Acquire::Retries=0', 'Acquire::http::Timeout=10', 'Acquire::https::Timeout=10', 'DPkg::Lock::Timeout=15', 'fonts-noto-cjk']) assert.ok(result.args.includes(option), option);
  assert.ok(!result.args.includes('update'));
});
test('fontconfig silently falling back to serif fails verification', () => {
  assert.notEqual(run({ font: 'Noto Serif CJK SC' }).status, 0);
});
