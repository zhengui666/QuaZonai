import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const script = fileURLToPath(new URL('./ensure-cjk-fonts.sh', import.meta.url));
function run({ installed = true, font = 'WenQuanYi Zen Hei,文泉驛正黑,文泉驿正黑' } = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'qz-font-install-'));
  const marker = join(dir, 'installed'); const calls = join(dir, 'calls');
  if (installed) writeFileSync(marker, 'yes');
  const mock = (name, body) => writeFileSync(join(dir, name), `#!/bin/bash\nset -eu\n${body}\n`, { mode: 0o755 });
  mock('dpkg-query', '[ -f "$FONT_MARKER" ] || exit 1\nprintf "install ok installed\\n"');
  mock('fc-match', 'printf "%s\\n" "$FONT_NAME"');
  // Any new installation path is a regression, even if a prerequisite is absent.
  for (const name of ['sudo', 'apt-get', 'curl', 'wget']) {
    mock(name, 'printf "%s\\n" "$0" "$@" >> "$FONT_CALLS"\nexit 98');
  }
  try {
    const result = spawnSync('bash', [script], { encoding: 'utf8', timeout: 8000,
      env: { ...process.env, PATH: `${dir}:${process.env.PATH}`, FONT_MARKER: marker, FONT_CALLS: calls, FONT_NAME: font } });
    return { status: result.status, args: existsSync(calls) ? readFileSync(calls, 'utf8').split('\n') : [] };
  } finally { rmSync(dir, { recursive: true, force: true }); }
}
test('the existing Playwright CJK package is verified without another installer', () => {
  const result = run(); assert.equal(result.status, 0); assert.deepEqual(result.args, []);
});
test('a missing Playwright prerequisite fails without downloading a substitute', () => {
  const result = run({ installed: false }); assert.notEqual(result.status, 0);
  assert.deepEqual(result.args, []);
});
test('fontconfig silently falling back to serif fails verification', () => {
  assert.notEqual(run({ font: 'Noto Serif CJK SC' }).status, 0);
});
test('a similar family name does not satisfy the exact sans family', () => {
  assert.notEqual(run({ font: 'WenQuanYi Zen Hei Serif' }).status, 0);
  assert.equal(run({ font: 'WenQuanYi Zen Hei' }).status, 0);
});
