// Control-flow regressions only; scripted command observations are not systemd evidence.
// The existing native browser workflow must still execute the actual shipped units.
import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { NativeUserServices } from './native-user-services.mjs';

async function fixture(t, { terminal = {}, stopError, groupError } = {}) {
  const root = await mkdtemp(resolve(tmpdir(), 'quazonai-unit-control-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const name = 'quazonai-unit-control-api.service';
  const group = `/user.slice/user-1000.slice/user@1000.service/app.slice/${name}`;
  const unit = { kind: 'api', name, path: resolve(root, name),
    dropin: resolve(root, `${name}.d`), links: [], createdDropin: false, group: null };
  const events = [];
  let stopped = false;
  const services = new NativeUserServices({
    root, env: {}, interrupted: () => false,
    run: async (_stage, command, args, options) => {
      assert.equal(options.privateOutput, true);
      assert.equal(options.cleanup, true);
      if (command === '/usr/bin/journalctl') { events.push('journal'); return ''; }
      assert.equal(command, '/usr/bin/systemctl');
      if (args.includes('show')) {
        events.push(stopped ? 'show-stopped' : 'show-running');
        const values = { Id: name, LoadState: 'loaded', MainPID: stopped ? '0' : '12345',
          ActiveState: stopped ? 'inactive' : 'active', SubState: stopped ? 'dead' : 'running',
          ControlGroup: stopped ? '' : group, Result: 'success',
          ExecMainCode: '1', ExecMainStatus: '0', ...terminal };
        return Object.entries(values).map(([key, value]) => `${key}=${value}`).join('\n') + '\n';
      }
      if (args.includes('stop')) {
        events.push('stop'); stopped = true;
        if (stopError) throw new Error(stopError);
        return '';
      }
      if (args.includes('disable')) { events.push('disable'); return ''; }
      if (args.includes('daemon-reload')) { events.push('reload'); return ''; }
      throw new Error('Unexpected test control command');
    },
  });
  services.units = [unit];
  services.registrationAttempted = true;
  services.quiescent = false;
  services.emptyGroup = async candidate => {
    events.push('empty-group');
    assert.equal(candidate.group, group, 'Retain the group even if process verification never ran');
    if (groupError) throw new Error(groupError);
  };
  return { services, unit, events };
}

test('partial startup retains its cgroup before stop clears native properties', async t => {
  const { services, unit, events } = await fixture(t);
  assert.equal(unit.group, null);
  await services.stop('api', { cleanup: true });
  assert.deepEqual(events, ['show-running', 'stop', 'show-stopped', 'empty-group']);
  assert.equal(services.evidence.snapshots.at(-1).phase, 'stopped');
});

test('successful flow cannot accept forced or timed-out final shutdown', async t => {
  const { services, events } = await fixture(t, {
    terminal: { Result: 'timeout', ExecMainCode: '2', ExecMainStatus: '9' },
  });
  await assert.rejects(services.cleanup({ graceful: true }), {
    code: 'ERR_ASSERTION', actual: 'timeout', expected: 'success',
  });
  assert.deepEqual(events, ['show-running', 'stop', 'show-stopped', 'empty-group']);
  assert.equal(services.quiescent, false);
  assert.equal(services.evidence.cleanup_complete, false);
  assert.ok(!events.includes('disable') && !events.includes('journal'));
});

test('already failed flow still proves emptiness before removing runtime links', async t => {
  const { services, events } = await fixture(t, {
    terminal: { Result: 'exit-code', ExecMainStatus: '1' },
  });
  await services.cleanup({ graceful: false });
  assert.equal(services.quiescent, true);
  assert.equal(services.evidence.cleanup_complete, true);
  assert.deepEqual(events, ['show-running', 'stop', 'show-stopped', 'empty-group',
    'journal', 'disable', 'reload', 'empty-group']);
});

test('unconfirmed cgroup shutdown retains registration and fails cleanup', async t => {
  const { services, events } = await fixture(t, { groupError: 'Group remains populated' });
  await assert.rejects(services.cleanup({ graceful: false }), /Group remains populated/);
  assert.equal(services.quiescent, false);
  assert.equal(services.evidence.cleanup_complete, false);
  assert.ok(!events.includes('disable') && !events.includes('reload'));
});

test('lost stop acknowledgement cannot be reported as confirmed shutdown', async t => {
  const { services, events } = await fixture(t, { stopError: 'Lost stop acknowledgement' });
  await assert.rejects(services.cleanup({ graceful: true }), /Lost stop acknowledgement/);
  assert.equal(services.quiescent, false);
  assert.equal(services.evidence.cleanup_complete, false);
  assert.deepEqual(events, ['show-running', 'stop']);
});
