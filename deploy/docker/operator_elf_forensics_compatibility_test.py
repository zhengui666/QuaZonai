"""Bounded GNU observations and immutable released-image compatibility checks."""
import copy
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import operator_compare as comparison
import operator_compare_test as comparison_tests
import operator_elf_forensics as forensic


# These mutation cases require a FUNC import and an unversioned GLOBAL export.
# Neither is guaranteed by the host's /usr/bin/true. Keep its real GNU output
# in the integration checks below, and use explicit parser input for mutations.
LOADER_MUTATION_FIXTURE = """\
Dynamic section at offset 0x2000 contains 3 entries:
  Tag        Type                         Name/Value
 0x0000000000000001 (NEEDED)               Shared library: [libc.so.6]
 0x000000006ffffffb (FLAGS_1)              Flags: PIE
 0x0000000000000000 (NULL)                 0x0

Symbol table '.dynsym' contains 3 entries:
   Num:    Value          Size Type    Bind   Vis      Ndx Name
     0: 0000000000000000     0 NOTYPE  LOCAL  DEFAULT  UND
     1: 0000000000000000     0 FUNC    GLOBAL DEFAULT  UND imported_function@GLIBC_2.2.5 (2)
     2: 0000000000001100    16 FUNC    GLOBAL DEFAULT   14 exported_function

Version symbols section '.gnu.version' contains 3 entries:
 Addr: 0x0000000000000400  Offset: 0x000400  Link: 6 (.dynsym)
  000:   0 (*local*)       2 (GLIBC_2.2.5)   1 (*global*)

Version needs section '.gnu.version_r' contains 1 entry:
 Addr: 0x0000000000000410  Offset: 0x000410  Link: 7 (.dynstr)
  000000: Version: 1  File: libc.so.6  Cnt: 1
  0x0010:   Name: GLIBC_2.2.5  Flags: none  Version: 2
"""


class CompatibilityTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.binary = self.root / 'original'
        shutil.copyfile('/usr/bin/true', self.binary)
        self.loader = subprocess.check_output(['readelf', '--wide', '--dyn-syms', '--dynamic', '--version-info', str(self.binary)], text=True)

    def test_real_gnu_loader_output_resolves_names_providers_and_versions(self):
        result = forensic.loader_semantics(self.loader)
        self.assertTrue(result['needed'])
        self.assertTrue(result['symbols'])
        self.assertTrue(result['version_requirements'])
        self.assertTrue(any(symbol['version'] and symbol['version']['provider'] in result['needed'] for symbol in result['symbols']))
        self.assertFalse(any('value' in symbol or 'index' in symbol for symbol in result['symbols']))

    def test_relocated_addresses_and_renumbered_version_ids_preserve_semantic_comparison(self):
        original = forensic.loader_semantics(self.loader)
        renamed = re.sub(r'(Version: )([23])(?=\s*$)', lambda match: match[1] + str(5 - int(match[2])), self.loader, flags=re.MULTILINE)
        head, version = renamed.split("Version symbols section '.gnu.version'", 1)
        head = re.sub(r' \(([23])\)(?=\s*$)', lambda match: ' (' + str(5 - int(match[1])) + ')', head, flags=re.MULTILINE)
        table, needs = version.split('Version needs section', 1)
        table = re.sub(r'\b([23])(?= \()', lambda match: str(5 - int(match[1])), table)
        head = re.sub(r'^(\s*\d+:\s+)[0-9a-f]+', r'\g<1>0000000012345678', head, flags=re.MULTILINE)
        renamed = head + "Version symbols section '.gnu.version'" + table + 'Version needs section' + needs
        self.assertEqual(forensic.loader_semantics(renamed), original)

    def test_loader_required_changes_are_observed_and_missing_or_invalid_tables_block(self):
        loader = LOADER_MUTATION_FIXTURE
        result = forensic.loader_semantics(loader)
        needed = result['needed'][0]
        changed = loader.replace(needed, 'changed-library.so')
        self.assertNotEqual(forensic.loader_semantics(changed), result)
        row = next(line for line in loader.splitlines() if re.match(r'\s*1:', line))
        for invalid in (loader.replace(row, ''), loader.replace("Version needs section '.gnu.version_r'", 'missing'),
                        loader.replace(row, row.replace('FUNC', 'INVALID')),
                        loader + '\nreadelf: Warning: corrupt version table\n'):
            with self.subTest(invalid=invalid[-60:]), self.assertRaises(ValueError):
                forensic.loader_semantics(invalid)
        symbol = next(value for value in result['symbols'] if value['binding'] == 'GLOBAL' and value['name'])
        changed = loader.replace(symbol['name'] + '@', 'changed_symbol@')
        self.assertNotEqual(forensic.loader_semantics(changed), result)

    def test_localizing_exported_unversioned_symbol_changes_loader_semantics(self):
        loader = LOADER_MUTATION_FIXTURE
        original = forensic.loader_semantics(loader)
        symbol = next(value for value in original['symbols'] if value['name'] and value['binding'] == 'GLOBAL'
                      and value['definition'] == 'defined' and value['version_scope'] == 'global')
        number = int(re.search(r'^\s*(\d+):[^\n]+\s' + re.escape(symbol['name']) + r'\s*$',
                               loader, re.MULTILINE)[1])
        head, table = loader.split("Version symbols section '.gnu.version'", 1)
        for row in re.finditer(r'^[ \t]*([0-9a-f]+):[ \t]+(.*)$', table, re.MULTILINE):
            entries = list(re.finditer(r'([0-9a-f]+)(h?)\s*\(([^)]+)\)', row[2]))
            offset = number - int(row[1], 16)
            if 0 <= offset < len(entries):
                entry = entries[offset]
                self.assertEqual(entry.groups(), ('1', '', '*global*'))
                start, end = row.start(2) + entry.start(), row.start(2) + entry.end()
                table = table[:start] + '0 (*local*)' + table[end:]
                break
        else:
            self.fail('Exported symbol has no GNU version entry')
        changed = forensic.loader_semantics(head + "Version symbols section '.gnu.version'" + table)
        localized = next(value for value in changed['symbols'] if value['name'] == symbol['name'])
        self.assertEqual(localized, {**symbol, 'version_scope': 'local'})
        self.assertNotEqual(changed, original)

    def test_real_frames_decode_keeps_complete_hash_and_removes_only_generated_text(self):
        before = self.binary.read_bytes()
        result = forensic.frame_decode(self.binary, self.root)
        self.assertEqual(result['status'], 'complete', result)
        self.assertGreater(result['cie_count'], 0)
        self.assertGreater(result['fde_count'], 0)
        self.assertTrue(result['output_hash_complete'])
        self.assertEqual(len(result['output_sha256']), 64)
        self.assertGreater(result['output_bytes'], 0)
        self.assertFalse(list(self.root.glob('frames-*')))
        self.assertEqual(self.binary.read_bytes(), before)

    def test_frame_output_bound_and_warnings_fail_closed_with_retained_status(self):
        with patch.object(forensic, 'MIB', 1):
            result = forensic.frame_decode(self.binary, self.root)
        self.assertEqual(result['status'], 'blocked')
        self.assertLessEqual(result['output_bytes'], 64)
        self.assertNotEqual(result['exit_code'], 0)
        body = b'0000 00000014 00000000 CIE\n0018 00000014 0000001c FDE\nreadelf: Warning: fixture\n'
        def warned(command, output, maximum):
            output.write_bytes(body)
            return 0
        with patch.object(forensic, 'bounded_tool', side_effect=warned):
            result = forensic.frame_decode(self.binary, self.root)
        self.assertEqual(result['status'], 'blocked')
        self.assertEqual(result['warning_count'], 1)
        self.assertEqual(result['output_sha256'], hashlib.sha256(body).hexdigest())
        self.assertFalse(list(self.root.glob('frames-*')))

    def test_frame_output_scan_stops_at_measurement_cutoff_with_cleanup_time_remaining(self):
        body = b'0000 00000014 00000000 CIE\n0018 00000014 0000001c FDE\n'
        def decoded(command, output, maximum):
            output.write_bytes(body)
            return 0
        for observed_times, hashed_bytes in (([101], 0), ([99, 100], len(body.splitlines(keepends=True)[0])),
                                             ([98, 99, 100], len(body))):
            with self.subTest(observed_times=observed_times), \
                    patch.object(forensic, 'bounded_tool', side_effect=decoded), \
                    patch.object(forensic.cost, 'COMMAND_DEADLINE', 100), \
                    patch.object(forensic.cost, 'CLEANUP_DEADLINE', 280), \
                    patch.object(forensic.time, 'time', side_effect=observed_times):
                result = forensic.frame_decode(self.binary, self.root)
            self.assertEqual(result['status'], 'blocked')
            self.assertIn('Measurement deadline', result['error'])
            self.assertFalse(result['output_hash_complete'])
            self.assertIsNone(result['output_sha256'])
            self.assertEqual(result['hashed_bytes'], hashed_bytes)
            self.assertEqual(result['output_bytes'], len(body))
            self.assertFalse(list(self.root.glob('frames-*')))

    def test_real_exception_sections_and_program_header_relation(self):
        # Ask the installed toolchain instead of assuming Debian's multiarch path.
        compiler = shutil.which('c++')
        self.assertIsNotNone(compiler, 'C++ toolchain required for the native ELF fixture')
        resolved = subprocess.run([compiler, '-print-file-name=libstdc++.so.6'],
                                  check=True, capture_output=True, text=True, timeout=10)
        library = Path(resolved.stdout.strip())
        self.assertTrue(library.is_absolute() and library.is_file(),
                        'The C++ toolchain must resolve an installed libstdc++.so.6')
        library = library.resolve()
        headers = forensic.bounded_readelf(library, self.root / 'headers.txt')
        covered = forensic.coverage(library, headers)
        result = forensic.native_structure(covered, headers)
        self.assertTrue(result['gnu_eh_frame_matches_header'])
        self.assertIn('.gcc_except_table', result['section_sha256'])
        self.assertTrue(result['build_id'])
        for name in ('.gcc_except_table', '.eh_frame', '.eh_frame_hdr'):
            missing = copy.deepcopy(covered)
            missing['sections'] = [value for value in missing['sections'] if value['name'] != name]
            with self.assertRaises(ValueError):
                forensic.native_structure(missing, headers)
        wrong = copy.deepcopy(covered)
        next(value for value in wrong['sections'] if value['name'] == '.eh_frame_hdr')['offset'] += 1
        with self.assertRaisesRegex(ValueError, 'PT_GNU_EH_FRAME'):
            forensic.native_structure(wrong, headers)
        with self.assertRaisesRegex(ValueError, 'build-ID'):
            forensic.native_structure(covered, headers.replace('Build ID:', 'unavailable:'))

    def reference_fixture(self, bad_identity=False, overflow=False):
        calls, owner, identifier = [], None, 'a' * 64
        def run(args):
            nonlocal owner
            calls.append(args)
            if args[:3] == ['docker', 'info', '--format']:
                return str(self.root)
            if args[:3] == ['docker', 'image', 'inspect']:
                return json.dumps([{'Id': 'sha256:' + identifier, 'Os': 'linux', 'Architecture': 'amd64',
                    'RepoDigests': [comparison.REFERENCE_IMAGE],
                    'Config': {'Labels': {'org.opencontainers.image.revision': 'wrong' if bad_identity else comparison.REFERENCE_REVISION}}}])
            if args[:2] == ['docker', 'create']:
                owner = args[args.index('--label') + 1].split('=', 1)[1]
                return identifier
            if args[:3] == ['docker', 'container', 'inspect']:
                return json.dumps([{'Id': identifier, 'Config': {'Labels': {'quazonai.measurement': owner}}}])
            return ''
        original_run = subprocess.run
        def copy_file(args, **kwargs):
            calls.append(args)
            # Exercise the real child file-size limit without invoking Docker.
            script = 'from pathlib import Path; Path(' + repr(args[-1]) + ').write_bytes(b"x" * ' + str(40 if overflow else 12) + ')'
            return original_run([sys.executable, '-B', '-c', script], **kwargs)
        with patch.dict('os.environ', {'GITHUB_ACTIONS': 'true', 'RUNNER_ENVIRONMENT': 'github-hosted'}), \
                patch.object(comparison.cost.shutil, 'disk_usage', return_value=shutil._ntuple_diskusage(100_000_000_000, 0, 50_000_000_000)), \
                patch.object(comparison.cost, 'run', side_effect=run), \
                patch.object(comparison.subprocess, 'run', side_effect=copy_file), \
                patch.object(forensic, 'MAX_REFERENCE_ELF_BYTES', 24), \
                patch.object(forensic, 'bounded_readelf', return_value='fixture headers'), \
                patch.object(forensic, 'coverage', return_value={'sha256': 'b' * 64}), \
                patch.object(forensic, 'observe_native', return_value={'status': 'complete'}):
            result = comparison.reference_native(self.root / ('overflow' if overflow else 'reference'), time.time() + 600)
        return result, calls

    def test_released_reference_is_never_started_and_shared_images_are_never_removed(self):
        result, calls = self.reference_fixture()
        self.assertEqual(result['status'], 'complete', result)
        self.assertEqual(result['cleanup']['status'], 'complete')
        self.assertTrue(any(call[:2] == ['docker', 'pull'] for call in calls))
        self.assertEqual(sum(call[:2] == ['docker', 'cp'] for call in calls), 2)
        self.assertEqual(sum(call[:3] == ['docker', 'container', 'rm'] for call in calls), 1)
        self.assertFalse(any(call[:2] in (['docker', 'run'], ['docker', 'start']) for call in calls))
        self.assertFalse(any(call[:3] in (['docker', 'image', 'rm'], ['docker', 'buildx', 'build']) for call in calls))
        self.assertEqual(2 * forensic.MAX_REPETITION_BYTES + forensic.MAX_REFERENCE_BYTES + forensic.MAX_REPORT_BYTES,
                         forensic.MAX_EXTRA_BYTES)

    def test_reference_identity_and_byte_limit_fail_before_dependent_work(self):
        result, calls = self.reference_fixture(bad_identity=True)
        self.assertEqual(result['status'], 'blocked')
        self.assertFalse(any(call[:2] in (['docker', 'create'], ['docker', 'cp']) for call in calls))
        result, calls = self.reference_fixture(overflow=True)
        self.assertEqual(result['status'], 'blocked')
        self.assertEqual(result['cleanup']['status'], 'complete')
        self.assertLessEqual((self.root / 'overflow/server').stat().st_size, 24)
        self.assertEqual(sum(call[:2] == ['docker', 'cp'] for call in calls), 1)

    def test_producer_checks_accept_raw_address_changes_but_never_waive_semantics_or_aa_hashes(self):
        loader = forensic.loader_semantics(self.loader)
        native = {'status': 'complete', 'loader': loader, 'link_time_strip_checks': True,
                  'original_sha256': 'a' * 64, 'structure': {'section_sha256': {'.dynsym': 'a' * 64}}}
        reference = {'status': 'complete', 'cleanup': {'status': 'complete'},
                     'native': dict.fromkeys(('server', 'runtime'), native)}
        first = {'hosted_comparison': {'native_compatibility': copy.deepcopy(reference['native'])}}
        second = copy.deepcopy(first)
        second['hosted_comparison']['native_compatibility']['server']['structure']['section_sha256']['.dynsym'] = 'b' * 64
        observations = {'binaries': {binary + '-' + phase: {'whole_equal': True}
                                    for binary in ('server', 'runtime') for phase in ('prestrip', 'stripped')}}
        self.assertEqual(comparison.producer_checks(reference, first, second, observations)['status'], 'complete')
        for mutation in ('needed', 'strip', 'unwind', 'prestrip-aa', 'stripped-aa'):
            modified, raw = copy.deepcopy(second), copy.deepcopy(observations)
            value = modified['hosted_comparison']['native_compatibility']['server']
            if mutation == 'needed': value['loader']['needed'] = ['changed.so']
            elif mutation == 'strip': value['link_time_strip_checks'] = False
            elif mutation == 'unwind': value['status'] = 'blocked'
            else: raw['binaries']['server-' + mutation.split('-')[0]]['whole_equal'] = False
            self.assertEqual(comparison.producer_checks(reference, first, modified, raw)['status'], 'blocked')

    def test_native_observations_cover_pre_and_post_and_static_symbols_block(self):
        fixture = comparison_tests.ComparisonTests()
        for failure in (False, True):
            result, calls, builds = fixture.measure_fixture(self.root / str(failure), application_only=True,
                prestrip=True, compatibility=True, native_failure=failure)
            self.assertEqual(result['diagnostic_status'], 'blocked' if failure else 'complete', result)
            for binary in ('server', 'runtime'):
                value = result['hosted_comparison']['native_compatibility'][binary]
                self.assertIn('loader', value)
                self.assertIn('loader', value['prestrip'])
                self.assertEqual(value['link_time_strip_checks'], not failure)
            self.assertEqual(len(builds), 1)
            self.assertEqual(result['cleanup']['status'], 'complete')

    def test_unavailable_reference_stops_before_any_native_build(self):
        with patch.object(comparison, 'reference_native', return_value={'status': 'blocked'}) as reference, \
                patch.object(comparison, 'measure') as measure:
            result = comparison.diagnose_application(Path('old'), Path('candidate'), 'b' * 40,
                self.root / 'failed-reference', 'ci', comparison_tests.BUILDKIT, time.time() + 600,
                prestrip=True, compatibility=True)
        self.assertEqual(result['diagnostic_status'], 'blocked')
        self.assertEqual(result['candidate_status'], 'not-started')
        self.assertFalse(result['admissible'])
        measure.assert_not_called()
        reference.assert_called_once()


if __name__ == '__main__':
    unittest.main()
