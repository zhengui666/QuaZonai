"""Native evidence preservation, bounded output and complete GNU range coverage."""
import copy
import base64
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
import unittest
from unittest.mock import patch

import operator_compare as comparison
import operator_compare_test as comparison_tests
import operator_elf_forensics as forensic
import operator_diagnostics as diagnostics

COMMIT, BUILDKIT = comparison_tests.COMMIT, comparison_tests.BUILDKIT


class ForensicTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.binary = self.root / 'server'
        shutil.copyfile('/usr/bin/true', self.binary)

    def headers(self, binary=None):
        return comparison.cost.run(['readelf', '--wide', '--file-header', '--program-headers',
                                    '--section-headers', '--notes', '--string-dump=.comment', str(binary or self.binary)])

    def test_append_preserves_exact_production_bytes_and_original_context_rules(self):
        for final_newline in (True, False):
            source = self.root / str(final_newline)
            (source / 'deploy/docker').mkdir(parents=True)
            production = b'FROM pinned AS server\nRUN \\\n    sh deploy/docker/native-build.sh server\n# original final comment' + (b'\n' if final_newline else b'')
            ignore = b'**\n!original\n'
            helper = b'#!/bin/sh\n# retain original bytes and arguments\ncargo build --locked --release -p server -p runtime\n'
            (source / 'deploy/docker/Dockerfile').write_bytes(production)
            (source / 'deploy/docker/Dockerfile.dockerignore').write_bytes(ignore)
            (source / 'deploy/docker/native-build.sh').write_bytes(helper)
            path, identity = forensic.prepare_dockerfile(source, source / 'evidence')
            self.assertEqual(path.read_bytes(), production + forensic.SUFFIX)
            self.assertEqual(Path(str(path) + '.dockerignore').read_bytes(), ignore)
            self.assertEqual(identity['production_dockerfile_sha256'], hashlib.sha256(production).hexdigest())
            self.assertEqual(identity['executed_dockerfile_sha256'], hashlib.sha256(path.read_bytes()).hexdigest())
            self.assertEqual(identity['linker_argv']['status'], 'unavailable')
            self.assertEqual(identity['production_entrypoint'], 'sh deploy/docker/native-build.sh server')
            self.assertNotIn('original_command', identity)
            self.assertEqual(identity['native_build_sha256'], hashlib.sha256(helper).hexdigest())
            self.assertEqual(identity['native_build_bytes'], len(helper))
            self.assertEqual((path.parent / identity['native_build_artifact']).read_bytes(), helper)
            self.assertNotIn(b'ENV ', forensic.SUFFIX)
            self.assertNotIn(b'cargo build', forensic.SUFFIX)
            self.assertIn(b'FROM server AS application-forensics', forensic.SUFFIX)
            self.assertIn(b'target=/build/target,readonly', forensic.SUFFIX)
            with self.assertRaises(FileExistsError):
                forensic.prepare_dockerfile(source, source / 'evidence')

    def test_capture_shell_checks_original_budget_before_copy_and_bounds_symbol_output(self):
        suffix = forensic.SUFFIX.decode()
        script = 'set -eu;' + suffix.split('    set -eu;', 1)[1]
        self.assertEqual(subprocess.run(['/bin/sh', '-n'], input=script, text=True, capture_output=True).returncode, 0)
        self.assertLess(script.index('test "$total" -gt'), script.index('cp /build/target/release/server'))
        self.assertIn(str(forensic.MAX_RAW_BYTES), script)
        self.assertIn('ulimit -f 65536; readelf --wide --symbols', script)
        self.assertLessEqual(2 * forensic.MAX_REPETITION_BYTES + forensic.MAX_REPORT_BYTES, forensic.MAX_EXTRA_BYTES)
        self.assertEqual(forensic.MAX_EXTRA_BYTES, 1024 ** 3)

    def test_actual_helper_bytes_are_retained_without_inferring_cargo_arguments(self):
        source = self.root / 'source'
        (source / 'deploy/docker').mkdir(parents=True)
        (source / 'deploy/docker/Dockerfile').write_bytes(b'FROM pinned AS server\nRUN \\\n    sh deploy/docker/native-build.sh server\n')
        (source / 'deploy/docker/Dockerfile.dockerignore').write_bytes(b'**\n!original\n')
        helper = b'#!/bin/sh\n# changed producer configuration\n: "$@"'
        (source / 'deploy/docker/native-build.sh').write_bytes(helper)
        path, identity = forensic.prepare_dockerfile(source, self.root / 'captured')
        self.assertEqual((path.parent / 'native-build.sh').read_bytes(), helper)
        self.assertEqual(identity['native_build_sha256'], hashlib.sha256(helper).hexdigest())
        self.assertEqual(identity['production_entrypoint'], 'sh deploy/docker/native-build.sh server')
        self.assertNotIn('original_command', identity)
        self.assertEqual(identity['linker_argv']['status'], 'unavailable')

    def test_missing_entrypoint_oversized_and_symlink_helpers_fail_before_artifact_write(self):
        for mutation in ('missing-entrypoint', 'ambiguous-entrypoint', 'oversized', 'symlink'):
            with self.subTest(mutation=mutation):
                source = self.root / mutation
                (source / 'deploy/docker').mkdir(parents=True)
                original = b'FROM pinned AS server\nRUN \\\n    sh deploy/docker/native-build.sh server\n'
                if mutation == 'missing-entrypoint':
                    original = original.replace(b'native-build.sh server', b'other-helper.sh server')
                elif mutation == 'ambiguous-entrypoint':
                    original += b'    sh deploy/docker/native-build.sh server\n'
                (source / 'deploy/docker/Dockerfile').write_bytes(original)
                (source / 'deploy/docker/Dockerfile.dockerignore').write_bytes(b'**\n!original\n')
                helper = source / 'deploy/docker/native-build.sh'
                if mutation == 'symlink':
                    helper.symlink_to(self.binary)
                else:
                    helper.write_bytes(b'x' * (forensic.MAX_NATIVE_HELPER_BYTES + 1) if mutation == 'oversized' else b'#!/bin/sh\n')
                with self.assertRaises(ValueError):
                    forensic.prepare_dockerfile(source, source / 'evidence')
                self.assertFalse((source / 'evidence/forensic.Dockerfile').exists())
                self.assertFalse((source / 'evidence/native-build.sh').exists())

    def test_every_original_byte_is_covered_including_shstrtab_headers_gaps_and_trailer(self):
        with self.binary.open('ab') as output:
            output.write(b'untouched forensic trailer')
        body = self.binary.read_bytes()
        result = forensic.coverage(self.binary, self.headers())
        self.assertEqual(result['sha256'], hashlib.sha256(body).hexdigest())
        self.assertEqual(result['coverage_bytes'], len(body))
        self.assertTrue(result['all_file_bytes_covered'])
        names = [row['name'] for row in result['ranges']]
        for expected in ('elf-header', 'program-headers', 'section-headers', 'trailer'):
            self.assertIn(expected, names)
        self.assertTrue(any(name.endswith(':.shstrtab') for name in names))
        self.assertTrue(any(name.startswith('gap:') for name in names))
        self.assertEqual(sum(row['size_bytes'] for row in result['ranges']), len(body))
        for row in result['ranges']:
            original = body[row['offset']:row['offset'] + row['size_bytes']]
            self.assertEqual(row['sha256'], hashlib.sha256(original).hexdigest())
        self.assertEqual(self.binary.read_bytes(), body)

    def test_byte_change_in_previously_missed_string_table_is_observed_without_normalization(self):
        headers = self.headers()
        before = forensic.coverage(self.binary, headers)
        string_table = next(row for row in before['ranges'] if row['name'].endswith(':.shstrtab'))
        body = bytearray(self.binary.read_bytes())
        offset = body.index(b'.text\0', string_table['offset'])
        body[offset + 1] = ord('u')
        self.binary.write_bytes(body)
        after = forensic.coverage(self.binary, self.headers())
        self.assertNotEqual(before['sha256'], after['sha256'])
        changed = next(row for row in after['ranges'] if row['name'].endswith(':.shstrtab'))
        self.assertNotEqual(string_table['sha256'], changed['sha256'])
        self.assertEqual(self.binary.read_bytes(), body)

    def test_missing_duplicate_overlapping_out_of_bounds_sections_and_bad_headers_block(self):
        headers = self.headers()
        text_row = next(line for line in headers.splitlines() if re.search(r'\]\s+\.text\s', line))
        parts = text_row.split(']', 1)[1].split()
        overlap = text_row.split(']', 1)[0] + '] ' + ' '.join(parts[:3] + ['000000'] + parts[4:])
        outside = text_row.split(']', 1)[0] + '] ' + ' '.join(parts[:3] + ['ffffffff'] + parts[4:])
        cases = [headers.replace(text_row, ''), headers.replace(text_row, text_row + '\n' + text_row),
                 headers.replace(text_row, overlap), headers.replace(text_row, outside),
                 headers.replace('.shstrtab', '.missing'), headers.replace('Size of this header:', 'missing-header:')]
        for value in cases:
            with self.subTest(case=cases.index(value)), self.assertRaises(ValueError):
                forensic.coverage(self.binary, value)
        self.binary.write_bytes(b'not an ELF')
        with self.assertRaises(ValueError):
            forensic.coverage(self.binary, headers)

    def test_readelf_output_cap_is_enforced_by_kernel_before_file_growth(self):
        output = self.root / 'bounded-readelf.txt'
        with patch.object(forensic, 'MIB', 128), self.assertRaises(subprocess.CalledProcessError):
            forensic.bounded_readelf(self.binary, output)
        self.assertLessEqual(output.stat().st_size, 128)

    def test_original_copy_budget_rejects_before_large_docker_copy(self):
        calls = []
        def run(args):
            calls.append(args)
            Path(args[-1]).write_text(str(forensic.MAX_REPETITION_BYTES + 1) + ' server\n')
        with patch.object(forensic.cost, 'run', side_effect=run), self.assertRaisesRegex(ValueError, 'before copying'):
            forensic.extract_originals('container', self.root / 'extract')
        self.assertEqual(len(calls), 1)
        self.assertTrue(calls[0][-2].endswith('/sizes.txt'))

    def test_original_copy_retains_exact_bytes_and_explicitly_blocks_partial_capture(self):
        for status in ('complete', 'blocked-original-byte-budget'):
            with self.subTest(status=status):
                source = {'capture-status.txt': (status + '\n').encode(), 'producer-tools.txt': b'original versions\n'}
                if status == 'complete':
                    source.update(server=self.binary.read_bytes(), runtime=self.binary.read_bytes())
                    source.update({'server-symbols.txt': b'original ordered symbols\n', 'runtime-symbols.txt': b'original ordered symbols\n'})
                source['sizes.txt'] = ''.join(str(len(body)) + ' ' + name + '\n' for name, body in source.items()).encode()
                def run(args):
                    Path(args[-1]).write_bytes(source[args[-2].rsplit('/', 1)[1]])
                directory = self.root / status
                with patch.object(forensic.cost, 'run', side_effect=run):
                    if status == 'complete':
                        result = forensic.extract_originals('container', directory)
                        self.assertEqual(result['status'], 'complete')
                        self.assertEqual((directory / 'server').read_bytes(), self.binary.read_bytes())
                    else:
                        with self.assertRaisesRegex(ValueError, status):
                            forensic.extract_originals('container', directory)
                        self.assertEqual((directory / 'capture-status.txt').read_bytes(), source['capture-status.txt'])

    def test_aa_capture_never_starts_candidate_and_reports_coverage_even_when_whole_elfs_differ(self):
        fixture = comparison_tests.ComparisonTests()
        for same in (False, True):
            with self.subTest(same=same):
                reports = [fixture.diagnostic_report('a1'), fixture.diagnostic_report('a2')]
                covered = forensic.coverage(self.binary, self.headers())
                for report in reports:
                    report['hosted_comparison']['forensic_recipe'] = {'prefix': 'same', 'suffix': 'same', 'linker_argv': 'unavailable'}
                    report['hosted_comparison']['forensic_coverage'] = {'binaries': {
                        name: copy.deepcopy(covered) for name in ('server-prestrip', 'runtime-prestrip', 'server-stripped', 'runtime-stripped')}}
                if not same:
                    reports[1]['hosted_comparison']['application_diagnostics']['server']['sha256'] = 'f' * 64
                    reports[1]['hosted_comparison']['forensic_coverage']['binaries']['server-prestrip']['sha256'] = 'f' * 64
                with patch.object(comparison, 'measure', side_effect=reports) as measure, \
                        patch.object(forensic, 'compare_symbol_text', return_value={'status': 'complete'}) as symbols:
                    result = comparison.diagnose_application(Path('old'), Path('candidate'), COMMIT,
                        self.root / str(same), 'ci', BUILDKIT, time.time() + 1000, prestrip=True)
                self.assertEqual(measure.call_count, 2)
                self.assertEqual(symbols.call_count, 2)
                self.assertTrue(all(call.kwargs['prestrip'] for call in measure.call_args_list))
                self.assertEqual(result['candidate_status'], 'not-started')
                self.assertEqual(result['diagnostic_status'], 'complete')
                self.assertFalse(result['admissible'])
                self.assertEqual(result['linker_argv']['status'], 'unavailable')
                self.assertEqual(result['forensic_observations']['binaries']['server-prestrip']['whole_equal'], same)
                saved = json.loads((self.root / str(same) / 'diagnostic.json').read_text())
                self.assertEqual(set(saved['forensic_observations']['ordered_symbol_text']), {'server', 'runtime'})

    def test_small_forensic_artifact_retains_reports_excludes_raw_and_has_eight_mib_cap(self):
        source, output = self.root / 'original', self.root / 'small'
        extra = source / 'old-a1/forensics'
        extra.mkdir(parents=True)
        (extra / 'server').write_bytes(self.binary.read_bytes())
        (extra / 'server-symbols.txt').write_text('symbols belong to the large original archive')
        (extra / 'coverage.json').write_text('{"qualification":"DIAGNOSTIC_ONLY"}\n')
        helper = b'#!/bin/sh\n# original actual helper bytes\n: "$@"'
        (extra / 'native-build.sh').write_bytes(helper)
        result = diagnostics.collect(source, output, COMMIT, forensic=True)
        self.assertEqual(result['maximum_total_bytes'], 8 * 1024 * 1024)
        self.assertTrue((output / 'old-a1/forensics/coverage.json').exists())
        self.assertFalse((output / 'old-a1/forensics/server').exists())
        self.assertFalse((output / 'old-a1/forensics/server-symbols.txt').exists())
        self.assertTrue((extra / 'server').exists())
        self.assertEqual((output / 'old-a1/forensics/native-build.sh').read_bytes(), helper)
        self.assertEqual(result['files']['old-a1/forensics/native-build.sh']['sha256'], hashlib.sha256(helper).hexdigest())

    def test_production_measurement_forensic_branch_retains_recipe_and_cleanup_with_no_full_build(self):
        fixture = comparison_tests.ComparisonTests()
        result, calls, builds = fixture.measure_fixture(self.root / 'measurement', application_only=True, prestrip=True)
        self.assertEqual(result['diagnostic_status'], 'complete', result)
        self.assertEqual(result['measurement_status'], 'not-performed')
        self.assertFalse(result['admissible'])
        self.assertEqual(len(builds), 1)
        self.assertEqual(builds[0][builds[0].index('--target') + 1], 'application-forensics')
        self.assertTrue(builds[0][builds[0].index('--file') + 1].endswith('/forensic.Dockerfile'))
        self.assertEqual(result['hosted_comparison']['forensic_capture']['status'], 'complete')
        self.assertEqual(result['cleanup']['status'], 'complete')
        self.assertEqual(len(result['cleanup']['removed']), 3)
        self.assertFalse(any('prune' in command for command in calls))

    def test_changed_production_prefix_blocks_before_build_and_failed_capture_keeps_original_cleanup(self):
        fixture = comparison_tests.ComparisonTests()
        for error in ('prefix_mismatch', 'helper_mismatch', 'forensic_failure'):
            result, calls, builds = fixture.measure_fixture(self.root / error, application_only=True,
                                                          prestrip=True, **{error: True})
            self.assertEqual(result['diagnostic_status'], 'blocked', result)
            self.assertEqual(result['cleanup']['status'], 'complete')
            self.assertFalse(result['admissible'])
            if error == 'prefix_mismatch':
                self.assertFalse(builds)
                self.assertIn('differ from the verified production recipe', result['error'])
            elif error == 'helper_mismatch':
                self.assertFalse(builds)
                self.assertIn('Retained native helper differs from the verified source recipe', result['error'])
            else:
                self.assertEqual(len(builds), 1)
                self.assertIn('blocked-original-byte-budget', result['error'])
                self.assertEqual(set(result['hosted_comparison']['application_diagnostics']), {'server', 'runtime'})

    def test_first_capture_failure_stops_before_second_native_build(self):
        report = comparison_tests.ComparisonTests().diagnostic_report('a1')
        report.update(diagnostic_status='blocked', error='blocked-original-byte-budget')
        with patch.object(comparison, 'measure', return_value=report) as measure:
            result = comparison.diagnose_application(Path('old'), Path('candidate'), COMMIT,
                self.root / 'failed-first', 'ci', BUILDKIT, time.time() + 1000, prestrip=True)
        self.assertEqual(measure.call_count, 1)
        self.assertEqual(result['diagnostic_status'], 'blocked')
        self.assertEqual(result['candidate_status'], 'not-started')
        self.assertFalse(result['admissible'])

    def test_ordered_symbol_lines_capture_reordering_missing_lines_and_original_hashes(self):
        cases = ((b'first\nsecond\n', b'second\nfirst\n', 2),
                 (b'first\nsecond\n', b'first\n', 1),
                 (b'first\n', b'first\nsecond\n', 1),
                 (b'first\n', b'first', 1),
                 (b'same\n', b'same\n', 0))
        for a, b, expected in cases:
            with self.subTest(a=a, b=b):
                left, right = self.root / 'a1-symbols', self.root / 'a2-symbols'
                left.write_bytes(a); right.write_bytes(b)
                result = forensic.compare_symbol_text(left, right)
                self.assertEqual(result['status'], 'complete')
                self.assertEqual(result['total_differing_lines'], expected)
                self.assertEqual(result['files']['a1']['sha256'], hashlib.sha256(a).hexdigest())
                self.assertEqual(result['files']['a2']['sha256'], hashlib.sha256(b).hexdigest())
                self.assertEqual((left.read_bytes(), right.read_bytes()), (a, b))
                for pair in result['differing_lines']:
                    for side, body in (('a1', a), ('a2', b)):
                        original = body.splitlines(keepends=True)
                        if pair[side] is None:
                            self.assertGreater(pair['line_number'], len(original))
                        else:
                            self.assertEqual(base64.b64decode(pair[side]['original_prefix_base64']), original[pair['line_number'] - 1])

    def test_long_symbol_lines_are_counted_exactly_with_bounded_original_prefixes(self):
        left, right = self.root / 'a1-symbols', self.root / 'a2-symbols'
        prefix = b'\xff' + b'x' * (forensic.MAX_SYMBOL_PREVIEW_BYTES + 200_000)
        a, b = prefix + b'A\nunchanged\n', prefix + b'B\nunchanged\n'
        left.write_bytes(a); right.write_bytes(b)
        result = forensic.compare_symbol_text(left, right)
        self.assertEqual(result['total_differing_lines'], 1)
        self.assertEqual(result['files']['a1']['line_count'], 2)
        self.assertEqual(result['files']['a2']['sha256'], hashlib.sha256(b).hexdigest())
        pair = result['differing_lines'][0]
        self.assertTrue(pair['a1']['line_truncated'])
        self.assertEqual(base64.b64decode(pair['a1']['original_prefix_base64']), a[:forensic.MAX_SYMBOL_PREVIEW_BYTES])
        self.assertNotEqual(pair['a1']['sha256'], pair['a2']['sha256'])
        self.assertLess(len(json.dumps(result).encode()), forensic.MAX_SYMBOL_DIFF_BYTES)

    def test_many_symbol_changes_and_output_cap_never_truncate_total_counts_or_hashes(self):
        left, right = self.root / 'a1-symbols', self.root / 'a2-symbols'
        a, b = b'old\n' * 200, b'new\n' * 200
        left.write_bytes(a); right.write_bytes(b)
        result = forensic.compare_symbol_text(left, right)
        self.assertEqual(result['total_differing_lines'], 200)
        self.assertEqual(len(result['differing_lines']), 32)
        self.assertEqual(result['differing_lines'][-1]['line_number'], 32)
        self.assertTrue(result['sample_truncated'])
        self.assertFalse(result['output_truncated'])
        with patch.object(forensic, 'MAX_SYMBOL_DIFF_BYTES', 16 * 1024 + 1):
            limited = forensic.compare_symbol_text(left, right)
        self.assertEqual(limited['status'], 'complete')
        self.assertEqual(limited['total_differing_lines'], 200)
        self.assertTrue(limited['output_truncated'])
        self.assertEqual(limited['differing_lines'], [])
        self.assertEqual(limited['files']['a1']['sha256'], hashlib.sha256(a).hexdigest())
        left.write_bytes((b'\x00' * forensic.MAX_SYMBOL_PREVIEW_BYTES + b'A\n') * 100)
        right.write_bytes((b'\x01' * forensic.MAX_SYMBOL_PREVIEW_BYTES + b'B\n') * 100)
        escaped = forensic.compare_symbol_text(left, right)
        self.assertEqual(escaped['total_differing_lines'], 100)
        self.assertLessEqual(len(json.dumps(escaped, indent=2).encode()), forensic.MAX_SYMBOL_DIFF_BYTES)

    def test_unavailable_symbol_file_is_explicit_without_a_fabricated_zero_difference(self):
        left, right = self.root / 'a1-symbols', self.root / 'a2-symbols'
        left.write_bytes(b'original\n')
        result = forensic.compare_symbol_text(left, right)
        self.assertEqual(result['status'], 'unavailable')
        self.assertIsNone(result['total_differing_lines'])
        self.assertIn('reason', result)
        right.symlink_to(left)
        self.assertEqual(forensic.compare_symbol_text(left, right)['status'], 'unavailable')

    def test_symbol_comparison_respects_remaining_measurement_deadline(self):
        left, right = self.root / 'a1-symbols', self.root / 'a2-symbols'
        left.write_bytes(b'original\n'); right.write_bytes(b'original\n')
        with patch.object(forensic.cost, 'COMMAND_DEADLINE', time.time() - 1):
            result = forensic.compare_symbol_text(left, right)
        self.assertEqual(result['status'], 'unavailable')
        self.assertIsNone(result['total_differing_lines'])
        self.assertIn('budget exhausted', result['reason'])

    def test_symbol_details_survive_small_artifact_and_unavailable_pair_blocks_completion(self):
        left, right = self.root / 'a1-symbols', self.root / 'a2-symbols'
        left.write_bytes(b'original-one\noriginal-two\n')
        right.write_bytes(b'original-two\noriginal-one\n')
        symbols = forensic.compare_symbol_text(left, right)
        source = self.root / 'source'
        source.mkdir()
        body = json.dumps({'forensic_observations': {'ordered_symbol_text': {'server': symbols}}}).encode()
        (source / 'diagnostic.json').write_bytes(body)
        diagnostics.collect(source, self.root / 'small-symbols', COMMIT, forensic=True)
        self.assertEqual((self.root / 'small-symbols/diagnostic.json').read_bytes(), body)
        fixture = comparison_tests.ComparisonTests()
        reports = [fixture.diagnostic_report('a1'), fixture.diagnostic_report('a2')]
        for report in reports:
            report['hosted_comparison'].update(forensic_recipe={'same': True}, forensic_coverage={})
        with patch.object(comparison, 'measure', side_effect=reports) as measure, \
                patch.object(forensic, 'compare', return_value={'status': 'observed'}), \
                patch.object(forensic, 'compare_symbol_text', return_value={'status': 'unavailable', 'reason': 'missing file'}):
            result = comparison.diagnose_application(Path('old'), Path('candidate'), COMMIT,
                self.root / 'missing-pair', 'ci', BUILDKIT, time.time() + 1000, prestrip=True)
        self.assertEqual(measure.call_count, 2)
        self.assertEqual(result['diagnostic_status'], 'blocked')
        self.assertEqual(result['candidate_status'], 'not-started')
        self.assertFalse(result['admissible'])


if __name__ == '__main__':
    unittest.main()
