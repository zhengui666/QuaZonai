import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
test('isolated science image retains its native stack without a retired HTTP gateway', () => {
  const dockerfile = fs.readFileSync(new URL('./native-job.Dockerfile', import.meta.url), 'utf8');
  assert.match(dockerfile, /FROM scratch/);
  assert.match(dockerfile, /ENTRYPOINT \["\/usr\/local\/bin\/job"\]/);
  assert.match(dockerfile, /nautilus\/0\.67\.0;clarabel\/0\.11\.1;wasmi\/2\.0\.0/);
  const assembler = fs.readFileSync(new URL('./build-native-image.mjs', import.meta.url), 'utf8');
  assert.match(assembler, /'job-binary'/); assert.match(assembler, /CARGO_TARGET_DIR/);
  assert.match(assembler, /wasm32-unknown-unknown/); assert.match(assembler, /\/opt\/rust\/bin\/rustc/);
});
