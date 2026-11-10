import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Read identity from the exact executable, then bind it to its shipped bytes. */
export async function nativeManifest(binaries) {
  const records = {};
  let upstream;
  for (const [name, binary] of Object.entries(binaries)) {
    const identity = JSON.parse(execFileSync(binary, ['build-identity'], { encoding: 'utf8', env: { PATH: '/usr/bin:/bin' } }));
    if (typeof identity.nautilusVersion !== 'string' || !/^[a-f0-9]{40}$/.test(identity.nautilusRevision)
      || !['standard', 'high'].includes(identity.precision) || typeof identity.jobVersion !== 'string') throw new Error('native_build_identity_invalid');
    const source = { nautilusVersion: identity.nautilusVersion, nautilusRevision: identity.nautilusRevision };
    if (upstream && JSON.stringify(upstream) !== JSON.stringify(source)) throw new Error('native_build_identity_source_mismatch');
    upstream = source;
    const hash = createHash('sha256');
    for await (const chunk of createReadStream(binary)) hash.update(chunk);
    records[name] = { ...identity, digest: `sha256:${hash.digest('hex')}` };
  }
  return { schemaVersion: 1, ...upstream, binaries: records };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const directory = resolve(process.argv[2]);
  const manifest = await nativeManifest({ job: join(directory, 'job'), 'job-high-precision': join(directory, 'job-high-precision') });
  await writeFile(join(directory, 'native-manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
}
