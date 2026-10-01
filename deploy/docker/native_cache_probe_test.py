"""Probe admission/evidence/ownership checks; no Docker or native compilation."""
import argparse
import copy
import json
import os
from pathlib import Path
import signal
import shutil
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import native_cache_probe as probe
import operator_cost_test


# Match the official BuildKit client/graph.go JSON tags and progressui/display.go
# rawJSONDisplay SolveStatus encoder, inspected 2026-10-01. These unit fixtures
# validate rejection logic; only the hosted raw log can establish an actual hit.
# https://github.com/moby/buildkit/blob/master/client/graph.go
# https://github.com/moby/buildkit/blob/master/util/progress/progressui/display.go

def events():
    vertices = []
    for index, stage in enumerate(('native-inputs', 'server', 'operator'), 1):
        command = ('node deploy/docker/native-inputs.mjs prepare /source /native "$TARGETPLATFORM" > /native-inputs.json'
                   if index == 1 else 'sh deploy/docker/native-build.sh ' + stage)
        vertex = {'digest': 'sha256:' + str(index) * 64,
                  'name': '[' + stage + ' 3/3] RUN ' + command,
                  'started': '2026-10-01T12:00:00Z', 'completed': '2026-10-01T12:00:01Z'}
        if index != 1:
            vertex['cached'] = True
        vertices.append(vertex)
    return [{'vertexes': vertices}, {'statuses': [], 'logs': []}]


def raw(events):
    return '\n'.join(json.dumps(event) for event in events)


class ProbeTests(unittest.TestCase):
    def test_rawjson_requires_collector_execution_and_both_heavy_hits(self):
        self.assertEqual(set(probe.cached_evidence(raw(events()))), {'native-inputs', 'server', 'operator'})
        for change in ('missing', 'miss', 'collector-hit', 'unfinished', 'error', 'ambiguous', 'wrong-type'):
            values = events()
            vertices = values[0]['vertexes']
            if change == 'missing':
                vertices.pop()
            elif change == 'miss':
                vertices[1].pop('cached')
            elif change == 'collector-hit':
                vertices[0]['cached'] = True
            elif change == 'unfinished':
                vertices[2].pop('completed')
            elif change == 'error':
                vertices[2]['error'] = 'build failed'
            elif change == 'ambiguous':
                vertices.append({**vertices[2], 'digest': 'sha256:' + '4' * 64})
            else:
                vertices[2]['cached'] = 'true'
            with self.subTest(change=change), self.assertRaises(ValueError):
                probe.cached_evidence(raw(values))
        for malformed in ('not JSON', '{}', '{"vertexes":null}', '{"vertexes":[{}]}'):
            with self.subTest(raw=malformed), self.assertRaises(ValueError):
                probe.cached_evidence(malformed)

    def test_actual_hash_identity_and_original_records_must_match(self):
        before, expected, _ = operator_cost_test.CostTests().native_fixture()
        probe.compare_snapshots(before, copy.deepcopy(before), expected)
        for change in ('hash', 'input', 'recipe', 'elapsed', 'disk'):
            after = copy.deepcopy(before)
            if change == 'hash':
                after['elf_sha256']['server'] = 'e' * 64
            elif change in ('input', 'recipe'):
                after['native_build'][change + '_sha256'] = 'e' * 64
            elif change == 'elapsed':
                after['native_build']['original_native_build_elapsed_seconds'] = 0
            else:
                after['native_build']['original_disk_after_bytes'] += 1
            with self.subTest(change=change), self.assertRaises(ValueError):
                probe.compare_snapshots(before, after, expected)

    def test_duplicate_initial_vertex_keeps_completion_but_new_execution_does_not(self):
        values = events()
        server = values[0]['vertexes'][1]
        initial = {key: server[key] for key in ('digest', 'name')}
        values.append({'vertexes': [initial]})
        self.assertEqual(probe.cached_evidence(raw(values))['server'], server)
        for later in ({**initial, 'started': '2026-10-01T12:00:02Z'},
                      {**initial, 'error': 'later failure'},
                      {**initial, 'cached': True},
                      {**server, 'cached': False}):
            with self.subTest(later=later), self.assertRaises(ValueError):
                probe.cached_evidence(raw(values + [{'vertexes': [later]}]))

    def test_noise_is_unique_and_outside_current_native_and_final_copies(self):
        with tempfile.TemporaryDirectory(prefix='probe noise ') as temporary:
            root = Path(temporary)
            (root / 'deploy/docker').mkdir(parents=True)
            source = Path(__file__).with_name('Dockerfile').read_text()
            (root / 'deploy/docker/Dockerfile').write_text(source)
            item = probe.add_noise(root, 'abc')
            self.assertTrue((root / item['path']).is_file())
            with self.assertRaises(FileExistsError):
                probe.add_noise(root, 'abc')
            (root / 'deploy/docker/Dockerfile').write_text(source + '\nCOPY *.md /opt/docs/\n')
            with self.assertRaisesRegex(ValueError, 'final image'):
                probe.add_noise(root, 'def')

    def test_command_timeout_and_log_cap_are_real_and_bounded(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / 'raw.log'
            start = time.monotonic()
            with self.assertRaises(TimeoutError):
                probe.command([sys.executable, '-c', 'import time; print("started",flush=True); time.sleep(30)'],
                              start + 0.25, log=log)
            self.assertLess(time.monotonic() - start, 5)
            self.assertTrue(log.is_file())
            with self.assertRaisesRegex(ValueError, 'bounded evidence'):
                probe.command([sys.executable, '-c', 'print("x"*5000)'], time.monotonic() + 2, log=log, limit=100)
            self.assertEqual(log.stat().st_size, 100)

    def test_insufficient_budget_fails_without_any_docker_command(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(directory=Path(temporary) / 'evidence', deadline=time.time(),
                revision='a' * 40, version='ci', builder='builder', candidate='candidate', source=Path(temporary))
            with patch.object(probe, 'command') as command:
                result = probe.probe(args)
            command.assert_not_called()
            self.assertEqual(result['execution_status'], 'blocked')
            self.assertEqual(result['status'], 'failed')
            self.assertEqual(result['cleanup_status'], 'passed')
            self.assertEqual(json.loads((args.directory / 'report.json').read_text()), result)

    def test_timeout_terminates_descendant_when_leader_exits_before_or_on_interrupt(self):
        def live(pid):
            try:
                state = Path('/proc', str(pid), 'stat').read_text().split(') ', 1)[1].split()[0]
            except FileNotFoundError:
                return False
            return state != 'Z'

        with tempfile.TemporaryDirectory(prefix='probe process group ') as temporary:
            for exits_first in (True, False):
                pidfile = Path(temporary) / ('child-' + str(exits_first))
                child = ('import os,signal,time; from pathlib import Path; '
                         'signal.signal(signal.SIGINT,signal.SIG_IGN); '
                         f'Path({str(pidfile)!r}).write_text(str(os.getpid())); time.sleep(30)')
                leader = ('import subprocess,sys,time; from pathlib import Path; '
                          f'subprocess.Popen([sys.executable,"-c",{child!r}]); '
                          f'p=Path({str(pidfile)!r})\n'
                          'while not p.exists(): time.sleep(0.01)\n' +
                          ('sys.exit(0)' if exits_first else 'time.sleep(30)'))
                pid = None
                try:
                    started = time.monotonic()
                    with self.subTest(exits_first=exits_first), self.assertRaises(TimeoutError):
                        probe.command([sys.executable, '-c', leader], started + 1)
                    self.assertLess(time.monotonic() - started, 5)
                    pid = int(pidfile.read_text())
                    deadline = time.monotonic() + 2
                    while live(pid) and time.monotonic() < deadline:
                        time.sleep(0.01)
                    self.assertFalse(live(pid), 'owned descendant survived command timeout')
                finally:
                    if pid is None and pidfile.exists():
                        pid = int(pidfile.read_text())
                    if pid is not None and live(pid):
                        os.kill(pid, signal.SIGKILL)

    def test_preexisting_tag_collision_is_preserved_without_building(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(directory=Path(temporary) / 'evidence', deadline=time.time() + 240,
                revision='a' * 40, version='ci', builder='builder', candidate='candidate', source=Path(temporary))
            with patch.object(probe, 'command', side_effect=[args.revision, '']) as command, \
                    patch.object(probe, 'tag_identity', return_value='sha256:' + 'b' * 64):
                result = probe.probe(args)
            self.assertEqual(result['execution_status'], 'failed')
            self.assertIn('tag already exists', result['error'])
            self.assertEqual(result['build_state'], 'not-started')
            self.assertFalse(any(call.args[0][0] == 'docker' for call in command.call_args_list))

    def test_owned_cleanup_never_removes_changed_tag_or_container(self):
        image, other, container = 'sha256:' + 'a' * 64, 'sha256:' + 'b' * 64, 'c' * 64
        with tempfile.TemporaryDirectory() as temporary:
            owned = Path(temporary) / 'owned'
            owned.mkdir()
            cidfile = owned / 'container.id'
            cidfile.write_text(container)
            value = [{'Config': {'Labels': {probe.OWNER: 'other-owner'}}, 'Image': image}]
            with patch.object(probe, 'command', return_value=json.dumps(value)) as command, \
                    patch.object(probe, 'tag_identity', return_value=other):
                errors = probe.cleanup(owned, 'mine', [(cidfile, image)], 'tag', image, 'completed', time.monotonic() + 5)
            self.assertEqual(len(errors), 2)
            self.assertFalse(any('rm' in call.args[0] for call in command.call_args_list))

    def test_only_confirmed_owned_objects_are_removed(self):
        image, container = 'sha256:' + 'a' * 64, 'c' * 64
        with tempfile.TemporaryDirectory() as temporary:
            owned = Path(temporary) / 'owned'
            owned.mkdir()
            cidfile = owned / 'container.id'
            cidfile.write_text(container)
            value = [{'Config': {'Labels': {probe.OWNER: 'mine'}}, 'Image': image}]
            with patch.object(probe, 'command', side_effect=[json.dumps(value), '', '']) as command, \
                    patch.object(probe, 'tag_identity', side_effect=[image, None]):
                errors = probe.cleanup(owned, 'mine', [(cidfile, image)], 'tag', image, 'completed', time.monotonic() + 5)
            self.assertEqual(errors, [])
            self.assertEqual(command.call_args_list[1].args[0], ['docker', 'container', 'rm', '--force', container])
            self.assertEqual(command.call_args_list[2].args[0], ['docker', 'image', 'rm', 'tag'])
            self.assertFalse(owned.exists())

    def test_uncertain_solve_is_failed_cleanup_not_a_cancellation_claim(self):
        with tempfile.TemporaryDirectory() as temporary:
            owned = Path(temporary) / 'owned'
            owned.mkdir()
            with patch.object(probe, 'command') as command:
                errors = probe.cleanup(owned, 'mine', [], 'tag', None, 'uncertain', time.monotonic() + 5)
            command.assert_not_called()
            self.assertIn('termination is unconfirmed', errors[0])
            self.assertTrue(owned.exists())
            errors = probe.cleanup(owned, 'mine', [], 'tag', None, 'completed', time.monotonic() + 5)
            self.assertIn('no verified output identity', errors[0])


if __name__ == '__main__':
    unittest.main()
