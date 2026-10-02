#!/usr/bin/env python3
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import textwrap
import unittest
from unittest.mock import patch

import operator_diagnostics as diagnostics


class DiagnosticRetentionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / 'original'
        self.source.mkdir()
        self.output = self.root / 'small'

    def put(self, name, body):
        path = self.source / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(body)
        return path

    def collect(self):
        return diagnostics.collect(self.source, self.output, 'a' * 40)

    def test_original_bytes_and_failed_admission_are_retained(self):
        body = b'{"admissible":false,"observed":1}\n'
        self.put('comparison.json', body)
        self.put('old/candidate-elf/server', b'large ELF is not selected')
        self.put('unrelated.log', b'not a diagnostic member')
        result = self.collect()
        self.assertEqual((self.output / 'comparison.json').read_bytes(), body)
        self.assertEqual((self.source / 'comparison.json').read_bytes(), body)
        self.assertEqual(result['files']['comparison.json']['sha256'], hashlib.sha256(body).hexdigest())
        self.assertEqual(set(result['files']), {'comparison.json'})
        self.assertEqual(result['input_bytes'], len(body))
        self.assertIn('old/report.json', result['missing_files'])
        self.assertFalse(json.loads((self.output / 'comparison.json').read_bytes())['admissible'])

    def test_section_differences_keep_both_observations(self):
        old = {binary: {'sha256': 'a' * 64, 'size_bytes': 12,
                        'sections': {'.text': {'size_bytes': 4, 'sha256': 'b' * 64},
                                     '.note.gnu.build-id': {'size_bytes': 8, 'sha256': 'c' * 64}}}
               for binary in diagnostics.BINARIES}
        candidate = json.loads(json.dumps(old))
        candidate['server']['sha256'] = 'd' * 64
        candidate['server']['sections']['.note.gnu.build-id']['sha256'] = 'e' * 64
        for variant, value in [('old', old), ('candidate', candidate)]:
            self.put(variant + '/candidate-elf/diagnostics.json', json.dumps(value).encode())
        result = self.collect()['section_observations']
        self.assertEqual(result['status'], 'observed')
        self.assertEqual(set(result['binaries']['server']['different_sections']), {'.note.gnu.build-id'})
        self.assertEqual(result['binaries']['runtime']['different_sections'], {})
        self.assertEqual(result['binaries']['server']['different_sections']['.note.gnu.build-id']['old']['sha256'], 'c' * 64)

    def test_missing_evidence_is_explicit_not_a_qualification(self):
        result = self.collect()
        self.assertEqual(result['section_observations']['status'], 'unavailable')
        self.assertEqual(result['missing_files'], list(diagnostics.FILES))
        self.assertNotIn('admissible', result)

    def test_malformed_diagnostics_are_kept_without_fabricating_sections(self):
        for variant in diagnostics.VARIANTS:
            self.put(variant + '/candidate-elf/diagnostics.json', b'{not-json')
        result = self.collect()
        self.assertEqual(result['section_observations']['status'], 'unavailable')
        self.assertEqual((self.output / 'old/candidate-elf/diagnostics.json').read_bytes(), b'{not-json')

    def test_invalid_shapes_and_nonfinite_json_keep_original_bytes_and_are_unavailable(self):
        original = {binary: {'sha256': 'a' * 64, 'size_bytes': 12,
                             'sections': {'.text': {'sha256': 'b' * 64, 'size_bytes': 4}}}
                    for binary in diagnostics.BINARIES}
        mutations = [lambda x: x.update(server=[]),
                     lambda x: x['server'].update(sections=[]),
                     lambda x: x['server'].update(sections={}),
                     lambda x: x['server'].update(sha256=None),
                     lambda x: x['server'].update(sha256='A' * 64),
                     lambda x: x['server'].update(size_bytes=-1),
                     lambda x: x['server'].update(size_bytes=True),
                     lambda x: x['server']['sections'].update({'': {'sha256': 'b' * 64, 'size_bytes': 4}}),
                     lambda x: x['server']['sections'].update({'.text': None}),
                     lambda x: x['server']['sections']['.text'].update(size_bytes=-1),
                     lambda x: x['server']['sections']['.text'].update(size_bytes=True),
                     lambda x: x['server'].update(unused=float('nan'))]
        for index, mutate in enumerate(mutations):
            with self.subTest(index=index):
                value = json.loads(json.dumps(original))
                mutate(value)
                body = json.dumps(value).encode()
                for variant in diagnostics.VARIANTS:
                    self.put(variant + '/candidate-elf/diagnostics.json', body)
                output = self.root / ('invalid-' + str(index))
                result = diagnostics.collect(self.source, output, 'a' * 40)
                self.assertEqual(result['section_observations']['status'], 'unavailable')
                self.assertEqual((output / 'old/candidate-elf/diagnostics.json').read_bytes(), body)

    def test_repetition_sections_are_separate_aa_bc_observations_and_original_elfs_stay_excluded(self):
        original = {binary: {'sha256': 'a' * 64, 'size_bytes': 12,
                             'sections': {'.text': {'sha256': 'b' * 64, 'size_bytes': 4}}}
                    for binary in diagnostics.BINARIES}
        for name in diagnostics.REPETITIONS:
            value = json.loads(json.dumps(original))
            if name == 'candidate':
                value['server']['sections']['.text']['sha256'] = 'c' * 64
            self.put(name + '/application-elf/diagnostics.json', json.dumps(value).encode())
            self.put(name + '/application-elf/server', b'original ELF kept in original archive only')
        result = self.collect()
        self.assertEqual(result['qualification'], 'DIAGNOSTIC_ONLY')
        self.assertEqual(result['repetition_observations']['AA']['binaries']['server']['different_sections'], {})
        bc = result['repetition_observations']['BC']['binaries']['server']['different_sections']
        self.assertEqual(bc['.text']['old-a1']['sha256'], 'b' * 64)
        self.assertEqual(bc['.text']['candidate']['sha256'], 'c' * 64)
        self.assertFalse((self.output / 'old-a1/application-elf/server').exists())
        self.assertTrue((self.source / 'old-a1/application-elf/server').exists())

    def test_member_budget_rejects_before_output(self):
        self.put('comparison.json', b'12345')
        with patch.object(diagnostics, 'MAX_FILE_BYTES', 4), self.assertRaises(ValueError):
            self.collect()
        self.assertFalse(self.output.exists())

    def test_total_budget_rejects_before_output(self):
        self.put('comparison.json', b'123')
        self.put('source-preflight.json', b'456')
        with patch.object(diagnostics, 'MAX_TOTAL_BYTES', 5), self.assertRaises(ValueError):
            self.collect()
        self.assertFalse(self.output.exists())

    def test_existing_output_and_nested_output_are_preserved(self):
        self.output.mkdir()
        (self.output / 'evidence').write_text('original')
        with self.assertRaises(ValueError):
            self.collect()
        self.assertEqual((self.output / 'evidence').read_text(), 'original')
        with self.assertRaises(ValueError):
            diagnostics.collect(self.source, self.source / '..' / 'original' / 'small', 'a' * 40)

    def test_file_and_parent_symlinks_are_rejected(self):
        outside = self.root / 'outside'
        outside.mkdir()
        (outside / 'report.json').write_text('private')
        (self.source / 'old').symlink_to(outside, target_is_directory=True)
        with self.assertRaises(ValueError):
            self.collect()
        (self.source / 'old').unlink()
        (self.source / 'comparison.json').symlink_to(outside / 'report.json')
        with self.assertRaises(ValueError):
            self.collect()
        self.assertFalse(self.output.exists())

    def test_invalid_revision_and_non_regular_member_fail(self):
        with self.assertRaises(ValueError):
            diagnostics.collect(self.source, self.output, 'dev')
        (self.source / 'comparison.json').mkdir()
        with self.assertRaises(ValueError):
            self.collect()

    def test_workflow_retains_evidence_without_changing_comparison_exit(self):
        root = Path(diagnostics.__file__).resolve().parents[2]
        workflow = (root / '.github/workflows/operator-cost-comparison.yml').read_text()
        block = workflow.split('      - name: Retain every comparison observation and admission failure\n', 1)[1]
        block = block.split('      - name: Retain bounded original section diagnostics separately\n', 1)[0]
        script = textwrap.dedent(block.split('        run: |\n', 1)[1])
        self.assertIn('Retain original ELF files, diagnostics, reports and failure logs', workflow)
        for mode, status in (('full', 0), ('full', 7), ('application-elf-diagnostic', 0), ('application-elf-diagnostic', 7)):
            with self.subTest(mode=mode, status=status):
                directory = self.root / ('run-' + mode + '-' + str(status))
                helper = directory / 'deploy/docker'
                helper.mkdir(parents=True)
                shutil.copyfile(diagnostics.__file__, helper / 'operator_diagnostics.py')
                (helper / 'operator_compare.py').write_text(
                    'import pathlib,sys\n'
                    'p=pathlib.Path(sys.argv[sys.argv.index("--output")+1])\n'
                    'p.parent.mkdir(parents=True)\n'
                    'p.write_text(\'{"admissible":' + ('true' if status == 0 else 'false') + '}\\n\')\n'
                    'sys.exit(' + str(status) + ')\n')
                result = subprocess.run(['bash', '-e', '-o', 'pipefail', '-c', script],
                                        cwd=directory, env={**os.environ, 'RUNNER_TEMP': str(directory),
                                                            'CANDIDATE_REVISION': 'a' * 40, 'COMPARISON_MODE': mode},
                                        capture_output=True, text=True, timeout=10)
                self.assertEqual(result.returncode, 1 if mode == 'application-elf-diagnostic' else status, result.stderr)
                original = directory / 'operator-comparison/comparison.json'
                copied = directory / 'operator-comparison-diagnostics/comparison.json'
                self.assertEqual(original.read_bytes(), copied.read_bytes())
                self.assertTrue((copied.parent / 'manifest.json').is_file())

    def test_workflow_evidence_time_fits_full_reserve_and_original_archive_stays_unconditional(self):
        workflow = (Path(diagnostics.__file__).resolve().parents[2] / '.github/workflows/operator-cost-comparison.yml').read_text()
        names = ('Retain every comparison observation and admission failure',
                 'Retain bounded original section diagnostics separately',
                 'Retain original ELF files, diagnostics, reports and failure logs')
        limits = []
        for name in names:
            block = workflow.split('      - name: ' + name + '\n', 1)[1].split('      - name:', 1)[0]
            limits.append(int(block.split('timeout-minutes: ', 1)[1].splitlines()[0]))
            self.assertIn('if: always()', block)
        self.assertEqual(limits, [1, 1, 8])
        self.assertEqual(85 + sum(limits), 95)
        self.assertIn('35 * 60', workflow)
        self.assertIn('timeout-minutes: 95', workflow)

    def test_collection_failure_keeps_comparison_failure_and_original_report_visible(self):
        workflow = (Path(diagnostics.__file__).resolve().parents[2] / '.github/workflows/operator-cost-comparison.yml').read_text()
        block = workflow.split('      - name: Retain every comparison observation and admission failure\n', 1)[1]
        block = block.split('      - name: Retain bounded original section diagnostics separately\n', 1)[0]
        script = textwrap.dedent(block.split('        run: |\n', 1)[1])
        helper = self.root / 'deploy/docker'
        helper.mkdir(parents=True)
        (helper / 'operator_compare.py').write_text(
            'import pathlib,sys\n'
            'p=pathlib.Path(sys.argv[sys.argv.index("--output")+1])\n'
            'p.parent.mkdir(parents=True)\n'
            'p.write_text(\'{"admissible":false}\\n\')\n'
            'sys.exit(7)\n')
        (helper / 'operator_diagnostics.py').write_text('raise SystemExit(9)\n')
        completed = subprocess.run(['bash', '-e', '-o', 'pipefail', '-c', script], cwd=self.root,
            env={**os.environ, 'RUNNER_TEMP': str(self.root), 'CANDIDATE_REVISION': 'a' * 40,
                 'COMPARISON_MODE': 'full'}, capture_output=True, text=True, timeout=10)
        self.assertEqual(completed.returncode, 9)
        self.assertEqual(json.loads((self.root / 'operator-comparison/comparison.json').read_text()), {'admissible': False})


if __name__ == '__main__':
    unittest.main()
