import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, writeFile, rename, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { release, tmpdir } from 'node:os';
import { randomUUID } from 'node:crypto';
import { canonicalJsonDigest } from '../../research/src/native-json.ts';
import { checkpointCpuDigest, checkpointTreeDigest, createCheckpointAcceptanceDriver, RuncRecoveryDriver } from '../src/checkpoint-runc.ts';
import { fileDigest, runCompletionWrapper } from '../src/completion-wrapper.ts';

test('completion wrapper waits for committed baseline release and retains real file output and exit evidence', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-completion-'));
  try {
    const requestId = 'a'.repeat(64); const input = { scientific: 'input' };
    await writeFile(join(root, 'request.json'), JSON.stringify(input));
    await writeFile(join(root, 'job.cjs'), "process.stdin.resume(); process.stdin.on('end', () => {process.stdout.write(JSON.stringify({value:17}));process.stderr.write('diagnostic')});");
    await writeFile(join(root, 'execution.json'), JSON.stringify({ schemaVersion: 1, requestId, implementationDigest: 'sha256:' + 'b'.repeat(64),
      inputDigest: canonicalJsonDigest(input), binary: process.execPath, args: [join(root, 'job.cjs')], environment: {} }));
    const operation = runCompletionWrapper(root);
    for (;;) { try { await readFile(join(root, 'wrapper.ready')); break; } catch { await new Promise(resolve => setTimeout(resolve, 5)); } }
    await assert.rejects(readFile(join(root, 'stdout.partial')), { code: 'ENOENT' });
    await writeFile(join(root, 'start'), 'start'); await operation;
    assert.equal(await readFile(join(root, 'stdout.partial'), 'utf8'), '{"value":17}');
    const completion = JSON.parse(await readFile(join(root, 'exit.json'), 'utf8'));
    assert.equal(completion.requestId, requestId); assert.equal(completion.exitCode, 0); assert.equal(completion.signal, null);
    assert.deepEqual(completion.stdout, await fileDigest(join(root, 'stdout.partial')));
    assert.deepEqual(completion.stderr, await fileDigest(join(root, 'stderr.log')));
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('missing acceptance evidence never advertises a recovery profile, and actual byte changes alter the tree digest', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-profile-'));
  try {
    const driver = new RuncRecoveryDriver({ dataRoot: root, rootfs: root, rootfsDigest: 'c'.repeat(64), implementationDigest: 'sha256:' + 'd'.repeat(64), catalogRoots: [], profilePath: join(root, 'missing.json') });
    assert.equal(await driver.profile(), null);
    await writeFile(join(root, 'payload'), 'before'); const before = await checkpointTreeDigest(root);
    await writeFile(join(root, 'payload'), 'after'); assert.notEqual(await checkpointTreeDigest(root), before);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('controlled runC protocol publishes a complete generation before restoring matching file state', async () => {
  // This exercises adapter persistence and command sequencing, not CRIU or
  // scientific correctness. Native acceptance has its own required executable.
  const root = await mkdtemp(join(tmpdir(), 'qz-checkpoint-'));
  try {
    const tools = join(root, 'tools'); await mkdir(tools); const rootfs = join(root, 'rootfs'); await mkdir(rootfs);
    const request = { schemaVersion: 2 as const, invocationId: randomUUID(), implementationDigest: 'sha256:' + 'd'.repeat(64), command: 'evaluate-factor', input: { value: 3 }, args: [], files: {}, recoveryProfileId: '', instanceId: 'test' };
    const fakeRunc = join(tools, 'runc'); const state = join(root, 'runtime.json'); const trace = join(root, 'trace');
    await writeFile(fakeRunc, `#!${process.execPath}\nconst fs=require('node:fs'), path=require('node:path');const args=process.argv.slice(4);const state=${JSON.stringify(state)},trace=${JSON.stringify(trace)};fs.appendFileSync(trace,JSON.stringify(args)+'\\n');const op=args[0];if(op==='state'){if(!fs.existsSync(state)){process.stderr.write('container does not exist');process.exit(1)}process.stdout.write(fs.readFileSync(state));}else if(op==='run'||op==='restore'){fs.writeFileSync(state,JSON.stringify({status:'running',pid:${process.pid},bundle:args[args.indexOf('--bundle')+1]}));}else if(op==='checkpoint'){if(fs.existsSync(${JSON.stringify(join(root, 'fail-dump'))})){process.stderr.write('controlled dump failure');process.exit(1);}const p=args[args.indexOf('--image-path')+1];fs.mkdirSync(p,{recursive:true});fs.writeFileSync(path.join(p,'inventory.img'),'controlled-image');fs.unlinkSync(state);}else if(op==='pause'||op==='resume'){const value=JSON.parse(fs.readFileSync(state));value.status=op==='pause'?'paused':'running';fs.writeFileSync(state,JSON.stringify(value));}else if(op==='kill'||op==='delete'){fs.rmSync(state,{force:true});}\n`, { mode: 0o755 });
    const { driver, profile } = createCheckpointAcceptanceDriver({ dataRoot: join(root, 'data'), rootfs, rootfsDigest: await checkpointTreeDigest(rootfs), implementationDigest: request.implementationDigest,
      catalogRoots: [], profilePath: join(root, 'unverified.json'), runc: fakeRunc }, ['evaluate-factor']);
    request.recoveryProfileId = profile.profileId;
    const id = canonicalJsonDigest(request); const envelope = { ...request, requestId: id };
    assert.equal(await driver.profile(), null, 'acceptance driver cannot advertise itself');
    await driver.prepare(envelope); const directory = join(root, 'data', 'executions', id); const work = join(directory, 'work');
    await rm(state); // The gated bootstrap dies before wrapper.ready exists.
    await assert.rejects(driver.checkpoint(id, 0), /checkpoint_candidate_incomplete/);
    await driver.prepare(envelope); // Only the never-released wrapper restarts.
    await writeFile(join(work, 'wrapper.ready'), id); await writeFile(join(work, 'progress.ndjson'), 'saved-prefix\n');
    const captured = await driver.checkpoint(id, 0);
    assert.equal(captured.generation, 0);
    // Crash after candidate manifest fsync but before directory publication:
    // reentry must not include its old manifest in its new file-hash inventory.
    await rename(join(directory, 'checkpoints', '0'), join(directory, 'checkpoints', '0.pending'));
    const recaptured = await driver.checkpoint(id, 0);
    assert.equal(recaptured.generation, 0);
    const commands = (await readFile(trace, 'utf8')).trim().split('\n').map(line => JSON.parse(line) as string[]);
    assert.equal(commands.some(args => args.includes('--leave-running')), false);
    assert.equal((await driver.inspect(id)).phase, 'stopped');
    await writeFile(join(work, 'progress.ndjson'), 'wrong-future-tail\n');
    await driver.restore(id, 0);
    assert.equal(await readFile(join(work, 'progress.ndjson'), 'utf8'), 'saved-prefix\n');
    await driver.start(id); assert.equal(await readFile(join(work, 'start'), 'utf8'), 'start\n');
    assert.equal((await driver.inspect(id)).phase, 'running');
    await writeFile(join(root, 'fail-dump'), 'failure'); await assert.rejects(driver.checkpoint(id, 1), /controlled dump failure/);
    assert.equal(JSON.parse(await readFile(state, 'utf8')).status, 'paused', 'failed capture stops unprotected progress');
    await rm(join(root, 'fail-dump')); const next = await driver.checkpoint(id, 1); assert.equal(next.generation, 1);
    await driver.restore(id, 1); await driver.start(id);
    await driver.cancel(id); await assert.rejects(driver.restore(id, 0), /checkpoint_execution_cancelled/);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('a complete invalid scientific JSON result settles as a known failed completion', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-completion-invalid-'));
  try {
    const id = 'a'.repeat(64); const directory = join(root, 'executions', id); const work = join(directory, 'work'); await mkdir(work, { recursive: true });
    const request = { input: { n: 1 }, implementationDigest: 'sha256:' + 'c'.repeat(64) };
    await writeFile(join(directory, 'admission.json'), JSON.stringify(request));
    await writeFile(join(work, 'stdout.partial'), '{broken'); await writeFile(join(work, 'stderr.log'), '');
    await writeFile(join(work, 'exit.json'), JSON.stringify({ schemaVersion: 1, requestId: id, implementationDigest: request.implementationDigest, inputDigest: canonicalJsonDigest(request.input), exitCode: 0, signal: null,
      stdout: await fileDigest(join(work, 'stdout.partial')), stderr: await fileDigest(join(work, 'stderr.log')) }));
    const driver = new RuncRecoveryDriver({ dataRoot: root, rootfs: root, rootfsDigest: '', implementationDigest: request.implementationDigest, catalogRoots: [], profilePath: join(root, 'none') });
    const inspection = await driver.inspect(id); assert.equal(inspection.phase, 'failed'); assert.equal(inspection.completion?.error, 'checkpoint_result_invalid');
    await writeFile(join(work, 'stdout.partial'), '{"forged":true}'); await assert.rejects(driver.inspect(id), /checkpoint_completion_invalid/);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('profile polling caches accepted bytes but admission and restore revalidate actual frozen inputs', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-profile-boundary-'));
  try {
    const rootfs = join(root, 'rootfs'); await mkdir(rootfs); await writeFile(join(rootfs, 'native'), 'original');
    const runc = join(root, 'runc'); const criu = join(root, 'criu');
    await writeFile(runc, `#!${process.execPath}\nif(process.argv.includes('--version'))process.stdout.write('runc version 1.5.2\\n');else{process.stderr.write('container does not exist');process.exitCode=1;}\n`, { mode: 0o755 });
    await writeFile(criu, `#!${process.execPath}\nprocess.stdout.write('Version: 4.3\\n');\n`, { mode: 0o755 });
    const implementationDigest = 'sha256:' + (await fileDigest(join(rootfs, 'native'))).sha256; const rootfsDigest = await checkpointTreeDigest(rootfs);
    const body = { schemaVersion: 1, verifiedCommands: ['evaluate-factor'], checkpointIntervalMs: 60_000,
      runtime: { runc: '1.5.2', criu: '4.3', kernel: release(), cpuDigest: await checkpointCpuDigest() }, rootfsDigest, frozenInputs: [],
      evidence: [{ command: 'evaluate-factor', reportDigest: 'd'.repeat(64) }] };
    const profileId = canonicalJsonDigest(body); const profilePath = join(root, 'profile.json');
    await writeFile(profilePath, JSON.stringify({ ...body, profileId }));
    const dataRoot = join(root, 'data'); const driver = new RuncRecoveryDriver({ dataRoot, rootfs, rootfsDigest, implementationDigest, catalogRoots: [], profilePath, runc, criu, binary: '/native' });
    assert.equal((await driver.profile())?.profileId, profileId);
    await writeFile(join(rootfs, 'native'), 'changed');
    assert.equal((await driver.profile())?.profileId, profileId, 'poll does not rescan immutable datasets');
    const content = { schemaVersion: 2 as const, invocationId: randomUUID(), implementationDigest, command: 'evaluate-factor', input: {}, args: [], files: {}, recoveryProfileId: profileId, instanceId: 'instance' };
    const requestId = canonicalJsonDigest(content); const request = { ...content, requestId };
    await assert.rejects(driver.prepare(request), /checkpoint_profile_unverified/);
    const directory = join(dataRoot, 'executions', requestId); const generation = join(directory, 'checkpoints', '0'); await mkdir(generation, { recursive: true });
    const candidate = { schemaVersion: 1, generation: 0, requestId, profileId, implementationDigest, createdAt: 'now', files: [] };
    await writeFile(join(generation, 'manifest.json'), JSON.stringify({ ...candidate, contentDigest: canonicalJsonDigest(candidate) }));
    await assert.rejects(driver.restore(requestId, 0), /checkpoint_profile_unverified/);
  } finally { await rm(root, { recursive: true, force: true }); }
});
