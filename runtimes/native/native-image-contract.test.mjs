import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
test('isolated science image retains its native stack without a retired HTTP gateway', () => {
  const dockerfile = fs.readFileSync(new URL('./native-job.Dockerfile', import.meta.url), 'utf8');
  assert.match(dockerfile, /FROM scratch/);
  assert.match(dockerfile, /ENTRYPOINT \["\/usr\/local\/bin\/job"\]/);
  assert.match(dockerfile, /native-manifest\.json/);
  assert.doesNotMatch(dockerfile, /nautilus\/0\./);
  const assembler = fs.readFileSync(new URL('./build-native-image.mjs', import.meta.url), 'utf8');
  assert.match(assembler, /'job-binary'/); assert.match(assembler, /CARGO_TARGET_DIR/);
  assert.doesNotMatch(assembler, /wasm32-unknown-unknown|\/opt\/rust|rustup/);
  assert.match(assembler, /x86_64-unknown-linux-gnu/);
});
test('release handoff has no retired compiler image or Wasm target dependency', () => {
  const read = name => fs.readFileSync(new URL('../../' + name, import.meta.url), 'utf8');
  const dockerfile = read('deploy/docker/Dockerfile');
  assert.doesNotMatch(dockerfile, /from=compiler|\/opt\/rust|\/compiler/);
  assert.match(dockerfile, /from=native/);
  for (const file of ['.github/actions/container/action.yml', '.github/workflows/ci.yml', '.github/workflows/rust-regression.yml']) {
    assert.doesNotMatch(read(file), /wasm32-unknown-unknown|outputs\.compiler|smoke\.py/);
  }
  assert.match(read('.github/actions/container/action.yml'), /native=\$\{\{ steps\.artifacts\.outputs\.directory \}\}/);
});
