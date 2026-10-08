"""Isolated release-harness wiring; never start Docker, systemd or the frontend."""
import contextlib
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

import smoke
import smoke_shell


class ShellHarnessTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='shell acceptance [行情] ')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.bundle = self.root / 'bundle $HOME %n'
        self.bundle.mkdir()
        self.installation = self.root / 'installation with spaces [native] %n $HOME'
        self.log = self.root / 'commands'
        (self.bundle / 'codex.sh').write_text('# CI fixture; no services\n')
        self.manager('')

    def manager(self, body):
        (self.bundle / 'manage.sh').write_text('''set -euo pipefail
QZ_BUNDLE=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
qz_fail() { printf '%s\\n' "$*" >&2; exit 1; }
''' + body)

    def execute(self, command):
        return subprocess.run(command, capture_output=True, text=True, check=False,
                              env={**os.environ, 'QZ_TEST_LOG': str(self.log)})

    def test_installed_manager_argv_preserves_literal_paths(self):
        args = smoke_shell.command(self.bundle, 'source', self.installation, '--', 'prepare', 'synthetic')
        self.assertEqual(args, ['bash', str(self.bundle / 'manage.sh'), 'source', '--directory',
                                str(self.installation), '--', 'prepare', 'synthetic'])
        self.assertNotIn('python', ' '.join(args))

    def test_shell_program_treats_dynamic_arguments_as_data(self):
        value = 'a b [行情] $HOME $(touch must-not-exist); "quoted"\nlast'
        result = self.execute(smoke_shell.shell_command(self.bundle, 'printf "%s" "$1"', value))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, value)
        self.assertFalse((self.root / 'must-not-exist').exists())

    def test_config_uses_private_ephemeral_file_not_secret_argv(self):
        config = {'bundle': str(self.bundle), 'password': 'synthetic private fixture'}
        seen = []
        def call(bundle, program, path, *args, capture):
            selected = Path(path)
            self.assertEqual(stat.S_IMODE(selected.stat().st_mode), 0o600)
            self.assertEqual(json.loads(selected.read_text()), config)
            self.assertNotIn(config['password'], program)
            self.assertNotIn(config['password'], args)
            seen.append(selected)
            return 'fixture-result'
        with patch.object(smoke_shell, 'shell', side_effect=call):
            self.assertEqual(smoke_shell.configured(config, 'qz_unit "$1"', capture=True), 'fixture-result')
        self.assertFalse(seen[0].exists())

    def test_invoke_propagates_expected_and_unexpected_statuses(self):
        with patch.object(smoke.subprocess, 'run', return_value=Mock(returncode=1)) as run:
            smoke.invoke(self.bundle, 'apply-update', self.installation, succeeds=False)
            self.assertEqual(run.call_args.args[0], smoke_shell.command(self.bundle, 'apply-update', self.installation))
            with self.assertRaisesRegex(AssertionError, 'Unexpected apply-update exit status: 1'):
                smoke.invoke(self.bundle, 'apply-update', self.installation)

    def test_installed_source_uses_shell_and_original_invocation_identity(self):
        config = {'root': str(self.installation), 'bundle': str(self.bundle)}
        source, target = self.root / 'input [行情]', self.root / 'output %n'
        def invoke(config_arg, root, operation, builder):
            self.assertEqual((config_arg, root, operation), (config, self.root, 'prepare'))
            self.assertEqual(builder('a' * 32), smoke_shell.command(
                self.bundle, 'source', self.installation, '--invocation-id', 'a' * 32,
                '--read-only', str(source), '--output-parent', str(target), '--', 'prepare', 'fixture'))
            return {'original_assertions': 'retained'}
        with patch.object(smoke, 'invoke_source_process', side_effect=invoke):
            self.assertEqual(smoke.invoke_installed_source(config, self.root, ['prepare', 'fixture'],
                                                         [source], target), {'original_assertions': 'retained'})

    def test_source_path_preserves_trailing_spaces(self):
        self.manager('qz_source_path() { printf "%s" "$1"; }\n')
        value = self.root / 'source trailing  '
        self.assertEqual(smoke_shell.source_path(value, {'bundle': str(self.bundle)}), value)

    def test_source_container_vector_comes_from_installed_shell(self):
        self.manager('''qz_source_container() {
    QZ_SOURCE_COMMAND=(docker run --name "quazonai-source-$4" --network "$3" --env 'literal=$HOME [行情]' --entrypoint /usr/bin/python3)
}
''')
        command = smoke_shell.source_container({'bundle': str(self.bundle)}, {'image': 'test-only'}, 'none', 'b' * 32)
        self.assertEqual(command, ['docker', 'run', '--name', 'quazonai-source-' + 'b' * 32,
                                   '--network', 'none', '--env', 'literal=$HOME [行情]',
                                   '--entrypoint', '/usr/bin/python3'])

    def test_abrupt_shutdown_bypasses_recovery_after_actual_policy_helper(self):
        self.manager('''qz_restarts() { printf 'restart:%s\\n' "$2" >> "$QZ_TEST_LOG"; }
qz_main() (
    trap 'printf "unexpected recovery\\n" >> "$QZ_TEST_LOG"' EXIT
    qz_restarts unused false
    printf 'unexpected migration\\n' >> "$QZ_TEST_LOG"
)
''')
        result = self.execute(smoke_shell.fault_command(self.bundle, 'after-restart-disable', 'apply-update', self.installation))
        self.assertEqual(result.returncode, 99, result.stderr)
        self.assertEqual(self.log.read_text(), 'restart:false\n')

    def test_before_activation_interrupt_keeps_startup_boundary(self):
        self.manager('''qz_activate() { printf 'unexpected activation\\n' >> "$QZ_TEST_LOG"; }
qz_main() { printf 'processors started\\n' >> "$QZ_TEST_LOG"; qz_activate unused unused; }
''')
        result = self.execute(smoke_shell.fault_command(self.bundle, 'before-activation', 'deploy', self.installation))
        self.assertEqual(result.returncode, 99, result.stderr)
        self.assertEqual(self.log.read_text(), 'processors started\n')

    def test_retry_allows_observation_but_rejects_ddl_and_worker_rewrite(self):
        self.manager('''qz_compose() { printf 'compose:%s\\n' "$2" >> "$QZ_TEST_LOG"; }
qz_configure_worker() { printf 'unexpected rewrite\\n' >> "$QZ_TEST_LOG"; }
qz_main() { qz_compose unused ps; }
''')
        result = self.execute(smoke_shell.fault_command(self.bundle, 'resume-without-ddl', 'deploy', self.installation))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.log.read_text(), 'compose:ps\n')
        for call, message in [('qz_compose unused run migrate', 'starting retry repeated DDL'),
                               ('qz_configure_worker unused', 'starting retry rewrote the Worker')]:
            result = self.execute(smoke_shell.shell_command(self.bundle,
                                  smoke_shell.FAULTS['resume-without-ddl'] + call))
            self.assertEqual(result.returncode, 1)
            self.assertIn(message, result.stderr)
        self.assertEqual(self.log.read_text(), 'compose:ps\n')

    def test_post_admission_race_still_runs_real_idle_and_recovery(self):
        self.manager('''qz_require_idle() { printf 'idle\\n' >> "$QZ_TEST_LOG"; }
qz_main() {
    qz_require_idle unused
    (trap 'printf "recover unchanged release\\n" >> "$QZ_TEST_LOG"' EXIT
     printf 'admissions closed\\n' >> "$QZ_TEST_LOG"
     qz_require_idle unused
     printf 'unexpected migration\\n' >> "$QZ_TEST_LOG")
}
''')
        result = self.execute(smoke_shell.fault_command(self.bundle, 'race-after-admissions-close', 'apply-update', self.installation))
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(self.log.read_text(), 'idle\nadmissions closed\nidle\nrecover unchanged release\n')

    def test_reuse_proof_rejects_any_docker_or_exec_attempt(self):
        for operation in ('docker info', 'exec docker run fixture'):
            self.manager('qz_main() { ' + operation + '; }\n')
            forbidden = self.root / 'forbidden'
            forbidden.unlink(missing_ok=True)
            result = self.execute(smoke_shell.reuse_command({'bundle': str(self.bundle), 'root': str(self.installation)},
                                                           'c' * 32, [], self.root, ['prepare'], forbidden))
            self.assertEqual(result.returncode, 98, result.stderr)
            self.assertEqual(len(forbidden.read_text().splitlines()), 1)

    def test_reuse_gate_needs_exact_rejection_and_no_launch(self):
        config = {'bundle': str(self.bundle), 'root': str(self.installation)}
        reason = 'Deployment failed: Source output must be a new absolute child of the mounted output parent.'
        for status, error, succeeds in [(1, reason, True), (0, reason, False), (1, 'different failure', False)]:
            with self.subTest(status=status, error=error), tempfile.TemporaryDirectory(dir=self.root) as temporary:
                self.manager(f'qz_main() {{ printf "%s\\n" {json.dumps(error)} >&2; exit {status}; }}\n')
                if succeeds:
                    smoke.verify_source_output_reuse(config, Path(temporary), ['prepare'], [], self.root)
                else:
                    with self.assertRaisesRegex(AssertionError, 'preflight proof failed'):
                        smoke.verify_source_output_reuse(config, Path(temporary), ['prepare'], [], self.root)

    def test_publication_transfers_bridge_beside_extracted_shell_helpers(self):
        root = Path(smoke.__file__).resolve().parents[2]
        workflow = (root / '.github/workflows/release-version.yml').read_text()
        self.assertIn('cp deploy/docker/smoke.py deploy/docker/smoke_shell.py deploy/docker/release.py', workflow)
        self.assertIn('deploy/docker/release_support.py', workflow)
        self.assertIn('"$RUNNER_TEMP/release-assets/smoke_shell.py"', workflow)
        self.assertIn('"$RUNNER_TEMP/release-assets/release_support.py"', workflow)
        self.assertLess(workflow.index('tar -xzf "$RUNNER_TEMP/release-assets/quazonai-deploy.tar.gz"'),
                        workflow.index('cp "$RUNNER_TEMP/release-assets/smoke.py"'))
        self.assertIn('test -f manage.sh && test -f codex.sh && test -f json.awk', workflow)
        self.assertIn('test ! -e manage.py && test ! -e codex.py', workflow)
        action = (root / '.github/actions/container/action.yml').read_text()
        syntax = next(line for line in action.splitlines() if 'for script in' in line)
        self.assertIn('deploy/docker/manage.sh', syntax)
        self.assertIn('deploy/docker/codex.sh', syntax)

    def test_no_host_python_guard_keeps_ci_itself_allowed(self):
        # The fixture never delegates to a real Docker binary. Only blocked
        # commands execute, so this test cannot start any service.
        with patch.object(smoke.shutil, 'which', return_value='/not-used/docker'):
            with self.assertRaisesRegex(AssertionError, 'host Python'):
                with smoke.installation_tools(self.root / 'tools'):
                    result = subprocess.run(['python3', '-c', 'raise SystemExit(0)'], check=False)
                    self.assertEqual(result.returncode, 97)
            self.assertEqual(json.loads((self.root / 'tools/commands.jsonl').read_text()),
                             {'tool': 'python3', 'operation': '-c', 'blocked': True})


if __name__ == '__main__':
    unittest.main()
