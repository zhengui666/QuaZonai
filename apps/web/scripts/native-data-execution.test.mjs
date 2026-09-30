// Ownership/control-flow checks only. No mock result counts as OCI acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import { NativeDataExecution, ownedContainer } from './native-data-execution.mjs';

const instance = '00000000-0000-0000-0000-000000000001';
const run = '00000000-0000-0000-0000-000000000002';
const image = `sha256:${'a'.repeat(64)}`;
const id = 'b'.repeat(64);
function fixture() {
  const journal = { instance_id: instance, jobs: [{ run_id: run, external_id: `${run}/1`, attempt_no: 1,
    container_id: id, spec_json: JSON.stringify({ run_id: run, external_job_id: `${run}/1`, attempt_no: 1, image_ref: image }) }] };
  const inspect = { Id: id, Image: image, Config: { Labels: { 'io.quazonai.runtime': instance,
    'io.quazonai.run': run, 'io.quazonai.external-id': `${run}/1`, 'io.quazonai.attempt': '1', 'io.quazonai.role': 'JOB' } } };
  return { journal, inspect };
}
test('exact persisted identity and pinned image establish test ownership', () => {
  const { journal, inspect } = fixture();
  assert.equal(ownedContainer(inspect, journal, image), journal.jobs[0]);
  journal.jobs[0].container_id = null; // A Docker create can commit before its ACK.
  assert.equal(ownedContainer(inspect, journal, image), journal.jobs[0]);
});
for (const mismatch of ['runtime', 'run', 'external-id', 'attempt', 'role', 'image', 'id', 'spec', 'unknown']) {
  test(`uncertain ${mismatch} ownership fails closed`, () => {
    const { journal, inspect } = fixture();
    if (mismatch === 'image') inspect.Image = `sha256:${'c'.repeat(64)}`;
    else if (mismatch === 'id') inspect.Id = 'c'.repeat(64);
    else if (mismatch === 'spec') journal.jobs[0].spec_json = '{}';
    else if (mismatch === 'unknown') journal.jobs = [];
    else inspect.Config.Labels[`io.quazonai.${mismatch}`] = 'unexpected';
    assert.throws(() => ownedContainer(inspect, journal, image));
  });
}
test('cleanup preflights every owned label before removing any container', async () => {
  const { journal, inspect } = fixture(); const calls = [];
  const subject = new NativeDataExecution({});
  subject.started = true; subject.image = image; subject.journal = async () => journal;
  subject.docker = async (_name, args, cleanup) => {
    assert.equal(cleanup, true); calls.push(args[0]);
    if (args[0] === 'ps') return `${id}\n${'c'.repeat(64)}\n`;
    if (args[0] === 'inspect') return JSON.stringify([{ ...inspect, Id: args[1] }]);
    throw new Error('Unverified resources must not be removed');
  };
  await assert.rejects(subject.cleanupContainers());
  assert.deepEqual(calls, ['ps', 'inspect', 'inspect']); assert.equal(subject.evidence.cleanup_complete, false);
});
test('unreadable journal retains state rather than guessing ownership', async () => {
  const subject = new NativeDataExecution({}); subject.started = true;
  subject.journal = async () => { throw new Error('read-only journal unavailable'); };
  subject.docker = async () => { assert.fail('No guessed cleanup'); };
  await assert.rejects(subject.cleanupContainers(), /journal unavailable/);
  assert.equal(subject.evidence.cleanup_complete, false);
});
test('an explicit execution request cannot pass when its image is absent', async () => {
  const before = process.env.QUAZONAI_NATIVE_JOB_IMAGE;
  delete process.env.QUAZONAI_NATIVE_JOB_IMAGE;
  try { await assert.rejects(new NativeDataExecution({}).prepare(), /immutable Job image/); }
  finally { if (before !== undefined) process.env.QUAZONAI_NATIVE_JOB_IMAGE = before; }
});
test('completed owned cleanup verifies empty labels before permitting state deletion', async () => {
  const { journal, inspect } = fixture(); const calls = []; let listed = false;
  const subject = new NativeDataExecution({}); subject.started = true; subject.image = image;
  subject.journal = async () => journal;
  subject.docker = async (_name, args, cleanup) => {
    assert.equal(cleanup, true); calls.push(args);
    if (args[0] === 'ps') { if (listed) return ''; listed = true; return `${id}\n`; }
    if (args[0] === 'inspect') return JSON.stringify([inspect]);
    assert.deepEqual(args, ['rm', '--force', id]); return id;
  };
  await subject.cleanupContainers();
  assert.equal(subject.evidence.cleanup_complete, true); assert.equal(calls.length, 4);
});

test('interrupted explicit execution cannot launch a new Runtime or pass', async () => {
  const subject = new NativeDataExecution({ stopping: () => true, launch: () => assert.fail('No process after interruption') });
  await assert.rejects(subject.start(), /interrupted/); assert.equal(subject.started, false);
});
