/** Test-only production Runtime orchestration. No domain rows are written here. */
import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import { constants } from 'node:fs';
import { lstat, open, readFile, realpath, writeFile } from 'node:fs/promises';
import { isAbsolute, relative, resolve } from 'node:path';

const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const digest = /^sha256:[0-9a-f]{64}$/;
const containerId = /^[0-9a-f]{64}$/;
const journalReader = `import json,sqlite3,sys,pathlib
p=pathlib.Path(sys.argv[1]); c=sqlite3.connect(p.as_uri()+'?mode=ro',uri=True)
c.row_factory=sqlite3.Row
with c:
 m=c.execute('SELECT instance_id FROM runtime_meta WHERE singleton=1').fetchone()
 rows=[dict(r) for r in c.execute('SELECT run_id,external_id,attempt_no,container_id,spec_json,phase,terminal_state FROM runtime_jobs ORDER BY run_id')]
print(json.dumps({'instance_id':m['instance_id'],'jobs':rows}))`;

export function ownedContainer(inspect, journal, image) {
  assert.match(journal.instance_id, uuid);
  assert.match(image, digest);
  assert.match(inspect.Id, containerId);
  assert.equal(inspect.Image, image);
  const labels = inspect.Config?.Labels;
  assert.equal(labels?.['io.quazonai.runtime'], journal.instance_id);
  const row = journal.jobs.find(job => job.external_id === labels['io.quazonai.external-id']);
  assert.ok(row, 'Container must have an invocation-owned journal identity');
  assert.match(row.run_id, uuid);
  assert.equal(labels['io.quazonai.run'], row.run_id);
  assert.equal(labels['io.quazonai.attempt'], String(row.attempt_no));
  assert.ok(['JOB', 'TOMBSTONE'].includes(labels['io.quazonai.role']));
  assert.equal(row.external_id, `${row.run_id}/${row.attempt_no}`);
  const spec = JSON.parse(row.spec_json);
  assert.equal(spec.run_id, row.run_id);
  assert.equal(spec.external_job_id, row.external_id);
  assert.equal(spec.attempt_no, row.attempt_no);
  assert.equal(spec.image_ref, image);
  if (row.container_id) assert.equal(inspect.Id, row.container_id);
  return row;
}

export class NativeDataExecution {
  constructor({ root, repo, env, run, launch, services, freePort, stopping, privateValues }) {
    Object.assign(this, { root, repo, env, run, launch, services, freePort, stopping, privateValues });
    this.started = false;
    this.evidence = { mode: 'native-execution', containers: [], cleanup_complete: false };
  }
  async docker(name, args, cleanup = false) {
    return this.run(name, 'docker', ['--host', `unix://${this.socket}`, ...args], {
      env: this.env, privateOutput: true, cleanup, timeout: 30_000,
    });
  }
  async prepare() {
    this.image = process.env.QUAZONAI_NATIVE_JOB_IMAGE;
    this.socket = process.env.QUAZONAI_DOCKER_SOCKET;
    assert.match(this.image ?? '', digest, 'Native execution requires the actually built immutable Job image');
    assert.ok(this.socket && isAbsolute(this.socket), 'Explicit native Docker socket required');
    assert.ok((await lstat(this.socket)).isSocket(), 'Native Docker socket must exist');
    const image = JSON.parse(await this.docker('native-data-image', ['image', 'inspect', this.image]));
    assert.equal(image.length, 1); assert.equal(image[0].Id, this.image);
    assert.equal((await this.docker('native-data-cgroups', ['info', '--format', '{{.CgroupVersion}}'])).trim(), '2');
    const fixtureBinary = process.env.QUAZONAI_WEB_NATIVE_FIXTURE_BIN ?? resolve(this.repo, 'target/debug/examples/browser_catalog_fixture');
    const prepareBinary = process.env.QUAZONAI_WEB_CATALOG_PREPARE_BIN ?? resolve(this.repo, 'target/debug/catalog-prepare');
    this.binary = process.env.QUAZONAI_WEB_RUNTIME_BIN ?? resolve(this.repo, 'target/debug/runtime');
    for (const binary of [fixtureBinary, prepareBinary, this.binary]) {
      assert.ok(isAbsolute(binary)); assert.ok((await lstat(binary)).isFile(), 'Actual native binaries required');
    }
    const source = resolve(this.root, 'native-source');
    await this.run('native-candle-acquisition-and-preparation', fixtureBinary, [source, prepareBinary], { env: this.env });
    this.prepared = JSON.parse(await readFile(resolve(source, 'prepared.json'), 'utf8'));
    assert.equal(this.prepared.schema_version, 1);
    assert.equal(this.prepared.catalog_root, resolve(source, 'prepared/catalog'));
    assert.equal(this.prepared.metadata_file, resolve(source, 'prepared/catalog-metadata.json'));
    const metadataBytes = await readFile(this.prepared.metadata_file);
    const metadata = JSON.parse(metadataBytes.toString('utf8'));
    assert.deepEqual(this.prepared.native_readback.map(row => row.event_ns), ['60000000000', '120000000000', '180000000000']);
    assert.ok(this.prepared.native_readback.every(row => row.available_ns === '1704153601000000000'));
    assert.deepEqual(this.prepared.metadata, metadata);
    assert.equal(metadata.origin, 'FIXTURE'); assert.equal(metadata.pit_status, 'UNVERIFIED');
    assert.equal(metadata.row_count, '3'); assert.equal(this.prepared.conversion.native_report.native_readback_verified, true);
    const port = await this.freePort();
    this.endpoint = `http://127.0.0.1:${port}`;
    this.credential = randomBytes(32).toString('hex'); this.privateValues.add(this.credential);
    const credentialFile = resolve(this.root, 'native-runtime-credential');
    await writeFile(credentialFile, this.credential, { mode: 0o600 });
    this.state = resolve(this.root, 'native-runtime-state');
    this.config = resolve(this.root, 'native-runtime.json');
    await writeFile(this.config, JSON.stringify({ schema_version: 1, state_dir: this.state,
      credential_file: credentialFile, docker_socket: this.socket, bind: `127.0.0.1:${port}`,
      images: [{ job_kind: 'DATA_VALIDATE', image_ref: this.image }],
      catalogs: [{ root: this.prepared.catalog_root, metadata_file: this.prepared.metadata_file }],
      max_cpu: 1, max_memory_mib: 1024, max_wall_seconds: 120, max_output_bytes: 67108864,
      max_parallel_jobs: 1, max_pending_jobs: 4, storage_quota_bytes: 268435456,
    }), { mode: 0o600 });
    await this.start();
    const peer = { schema_version: 1, mode: 'native-execution', endpoint: this.endpoint,
      credential: this.credential, runtime_targets: [{ origin: this.endpoint, addresses: [`127.0.0.1:${port}`] }],
      metadata, image: this.image };
    await writeFile(resolve(this.root, 'native-data-peer.json'), JSON.stringify(peer), { mode: 0o600 });
    return peer;
  }
  async start() {
    assert.equal(this.stopping(), false, 'Native execution interrupted before Runtime startup');
    assert.ok(!this.current || this.current.exited);
    this.started = true;
    this.current = this.launch(this.binary, ['serve', '--config', this.config], { env: this.env });
    this.current.expectedStopExitCode = 0;
    this.current.name = `native-runtime-${this.services.filter(item => item.name?.startsWith('native-runtime-')).length}`;
    this.services.push(this.current);
    const deadline = Date.now() + 30_000;
    while (Date.now() < deadline && !this.stopping()) {
      assert.equal(this.current.exited, false, 'Production Runtime exited before readiness');
      try {
        const response = await fetch(`${this.endpoint}/runtime/v1/capabilities`, {
          headers: { Authorization: `Bearer ${this.credential}` }, signal: AbortSignal.timeout(2_000),
        });
        if (response.status === 200) {
          const capabilities = await response.json();
          assert.ok(capabilities.image_refs.some(item => item.job_kind === 'DATA_VALIDATE' && item.image_ref === this.image));
          return;
        }
      } catch (error) { if (error.code === 'ERR_ASSERTION') throw error; }
      await new Promise(done => setTimeout(done, 100));
    }
    throw new Error('Production Runtime readiness timed out');
  }
  async journal(cleanup = false) {
    return JSON.parse(await this.run('native-runtime-read-only-journal', 'python3', ['-B', '-c', journalReader,
      resolve(this.state, 'journal.sqlite')], { env: this.env, privateOutput: true, recordStage: false, cleanup }));
  }
  async inspect({ expectedJobs, successfulRun, failedRun }) {
    const journal = await this.journal();
    assert.equal(journal.jobs.length, expectedJobs);
    const listed = (await this.docker('native-data-container-cardinality', ['ps', '-aq', '--no-trunc', '--filter',
      `label=io.quazonai.runtime=${journal.instance_id}`])).trim().split('\n').filter(Boolean).sort();
    assert.deepEqual(listed, journal.jobs.map(row => row.container_id).sort(), 'No extra or missing native container');
    const observations = [];
    for (const row of journal.jobs) {
      assert.equal(row.attempt_no, 1); assert.equal(row.phase, 'TERMINAL');
      assert.match(row.container_id, containerId);
      const [inspection] = JSON.parse(await this.docker('native-data-container-inspect', ['inspect', row.container_id]));
      ownedContainer(inspection, journal, this.image);
      assert.equal(inspection.State.Running, false);
      assert.equal(inspection.HostConfig.NetworkMode, 'none');
      assert.equal(inspection.HostConfig.ReadonlyRootfs, true);
      const spec = JSON.parse(row.spec_json);
      const dataset = spec.inputs.filter(input => input.kind === 'DATASET');
      assert.equal(dataset.length, 1);
      const mount = inspection.Mounts.filter(item => item.Destination === `/input/catalogs/${dataset[0].revision_id}`);
      assert.equal(mount.length, 1); assert.equal(mount[0].Source, this.prepared.catalog_root);
      assert.equal(mount[0].RW, false);
      if (row.run_id === successfulRun) {
        assert.equal(row.terminal_state, 'SUCCEEDED'); assert.equal(inspection.State.ExitCode, 0);
      } else {
        assert.equal(row.run_id, failedRun); assert.equal(row.terminal_state, 'FAILED');
        assert.notEqual(inspection.State.ExitCode, 0);
      }
      observations.push({ run_id: row.run_id, external_id: row.external_id, attempt_no: row.attempt_no,
        container_id: row.container_id, image: inspection.Image, exit_code: inspection.State.ExitCode,
        state: row.terminal_state, spec: JSON.parse(row.spec_json) });
    }
    this.evidence.source = { origin: this.prepared.metadata.origin, pit_status: this.prepared.metadata.pit_status,
      original_receipt_ns: this.prepared.original_receipt_ns,
      native_readback: this.prepared.native_readback };
    this.evidence.instance_id = journal.instance_id;
    this.evidence.containers = observations;
    return { instance_id: journal.instance_id, containers: observations };
  }
  async corrupt() {
    // The fixture emits native BAR discovery after verifying the original rows.
    // These are stable harness-owned directories, not a hostile-filesystem sandbox.
    const root = resolve(this.root, 'native-source/prepared/catalog');
    assert.equal(this.prepared.catalog_root, root, 'Corruption is scoped to the invocation-owned catalog');
    assert.ok((await lstat(root)).isDirectory(), 'Native catalog must be a directory, not a symlink');
    assert.equal(await realpath(root), root, 'Native catalog root must be canonical without symlinks');
    const partitions = this.prepared.native_partitions;
    assert.ok(Array.isArray(partitions), 'Native fixture partition manifest required');
    assert.equal(partitions.length, 1, 'Exactly one native candle partition must be corrupted');
    const partition = partitions[0];
    assert.equal(partition?.data_kind, 'BAR', 'Only a discovered native BAR partition may be corrupted');
    assert.deepEqual(this.prepared.selection.selection.bar_types, [partition.bar_type]);
    assert.equal(typeof partition.relative_path, 'string');
    assert.ok(!isAbsolute(partition.relative_path) && !partition.relative_path.includes('\\')
      && partition.relative_path.split('/').every(part => part && part !== '.' && part !== '..'),
    'Native partition path must be canonical and relative');
    const path = resolve(root, partition.relative_path);
    assert.equal(relative(root, path), partition.relative_path, 'Native partition must be inside the exact catalog root');
    assert.equal(await realpath(path), path, 'Native partition must not traverse symlinks');
    assert.ok((await lstat(path)).isFile(), 'Native partition must be a regular file');
    assert.ok(Number.isSafeInteger(partition.size_bytes) && partition.size_bytes > 0, 'Native partition byte count required');
    const file = await open(path, constants.O_RDWR | constants.O_NOFOLLOW);
    try {
      const stat = await file.stat();
      assert.ok(stat.isFile() && stat.nlink === 1, 'Native partition must be a regular file without shared hard links');
      assert.equal(stat.size, partition.size_bytes, 'Native partition byte count changed');
      const before = await file.readFile();
      assert.equal(before.length, partition.size_bytes, 'Native partition byte count changed');
      const corrupt = Buffer.from('explicit test-owned corrupt native parquet');
      assert.notDeepEqual(before, corrupt, 'Corruption must change the native partition bytes');
      const { bytesWritten } = await file.write(corrupt, 0, corrupt.length, 0);
      assert.equal(bytesWritten, corrupt.length);
      await file.truncate(corrupt.length);
      const after = await readFile(path);
      assert.deepEqual(after, corrupt);
      this.evidence.corruption = { ...partition,
        before_size_bytes: before.length, after_size_bytes: after.length };
    } finally { await file.close(); }
  }
  // Called only after the shipped Worker and every owned Runtime process stopped.
  // A create ACK may be lost before container_id was persisted: reconcile exact
  // invocation label + frozen journal spec, never delete by a global name/image.
  async cleanupContainers() {
    if (!this.started) { this.evidence.cleanup_complete = true; return; }
    const journal = await this.journal(true);
    assert.match(journal.instance_id, uuid);
    const ids = (await this.docker('list-owned-native-data-containers', ['ps', '-aq', '--no-trunc', '--filter',
      `label=io.quazonai.runtime=${journal.instance_id}`], true)).trim().split('\n').filter(Boolean);
    const inspected = [];
    for (const id of ids) {
      assert.match(id, containerId);
      const [inspection] = JSON.parse(await this.docker('verify-owned-native-data-container', ['inspect', id], true));
      assert.equal(inspection.Id, id); ownedContainer(inspection, journal, this.image);
      inspected.push(id);
    }
    for (const id of inspected) await this.docker('remove-owned-native-data-container', ['rm', '--force', id], true);
    const remaining = await this.docker('require-no-owned-native-data-containers', ['ps', '-aq', '--no-trunc', '--filter',
      `label=io.quazonai.runtime=${journal.instance_id}`], true);
    assert.equal(remaining.trim(), '');
    this.evidence.cleanup_complete = true;
  }
}
