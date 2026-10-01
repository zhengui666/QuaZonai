"""Offline archive lifecycle contracts; stub native outputs are not native acceptance."""
import contextlib
from copy import deepcopy
import io
import json
from pathlib import Path
import socket
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import binance_vision as vision
import source_plugins as plugins
from test_binance_vision import SELECTION, DECLARED, archive_bytes, checksum


class ArchivePluginTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.network = patch.object(socket, 'create_connection', side_effect=AssertionError('network forbidden'))
        self.network.start()
        self.addCleanup(self.network.stop)
        self.root = Path(self.temp.name)
        self.archive = self.root / 'original archive.zip'
        self.archive.write_bytes(archive_bytes())
        self.checksum = self.root / 'original archive.CHECKSUM'
        self.checksum.write_bytes(checksum(self.archive.read_bytes()))
        self.provenance = self.root / 'original provenance.json'
        self.provenance.write_text(json.dumps(DECLARED))
        self.bundle = self.root / 'frozen bundle'
        self.manifest = vision.freeze(SELECTION, self.archive, self.checksum, self.bundle,
                                      provenance_path=self.provenance)
        self.instruments = self.root / 'original instruments.json'
        self.instruments.write_text('[{"CurrencyPair":{"fixture":"validated by native tests separately"}}]')
        self.binary = self.root / 'native converter; literal'
        self.binary.write_text('#!/bin/sh\nexit 1\n')
        self.binary.chmod(0o700)
        self.output = self.root / 'native output; literal'
        self.args = SimpleNamespace(acquisition=self.bundle / 'archive.json', instruments=self.instruments,
                                    native_bin=self.binary, output=self.output)
        self.selection_args = ['--symbol', 'BTCUSDT', '--base-asset', 'BTC', '--quote-asset', 'USDT',
                               '--day', '2025-01-01', '--interval', '1m']

    def invoke(self, argv):
        with contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertEqual(plugins.main(argv), 0)
        return json.loads(out.getvalue())

    def publication(self):
        receipt = plugins.archive_receipt(self.manifest)
        hashes = {'acquisition_sha256': plugins.sha256(self.args.acquisition),
                  'instrument_definitions_sha256': plugins.sha256(self.instruments)}
        report = {'schema_version': 1, 'native_version': '0.63.0', 'source_provider': vision.PROVIDER['id'],
                  'source_schema': vision.SCHEMA, 'source_record_kind': 'OHLCV_CANDLE',
                  'instruments': 1, 'instrument_versions': 1, 'bars': 2, 'catalog_relative_path': 'catalog',
                  'source_evidence_relative_path': 'source-evidence.json', 'native_readback_verified': True,
                  'coverage': 'UNPROVEN', 'historical_availability': 'UNVERIFIED',
                  'research_qualified': False, 'registered_in_quazonai': False,
                  'limitations': ['Synthetic framing only; native execution tested separately'],
                  'receipt_basis': receipt, 'source_provenance_kind': self.manifest['provenance']['kind'],
                  'supported_zip_profile': 'CLASSIC_SINGLE_MEMBER_STORED_OR_DEFLATE_V1', **hashes}
        evidence = {'schema_version': 1, 'source_acquisition_path': str(self.args.acquisition),
                    'acquisition': deepcopy(self.manifest), 'instrument_definitions': json.loads(self.instruments.read_text()),
                    'receipt_basis': deepcopy(receipt), 'supported_zip_profile': report['supported_zip_profile'], **hashes}
        return report, evidence

    def publish_stub(self, report, evidence):
        self.output.mkdir()
        (self.output / 'catalog').mkdir()
        (self.output / 'catalog/fixture.parquet').write_bytes(b'PAR1-stub-only-PAR1')
        (self.output / 'source-evidence.json').write_text(json.dumps(evidence))
        (self.output / 'import-report.json').write_text(json.dumps(report))

    def runner(self, report=None, evidence=None, change=None):
        expected_report, expected_evidence = self.publication()
        def run(argv, **kwargs):
            self.assertEqual(argv, [str(self.binary), 'ingest-archive-candles', '--acquisition', str(self.args.acquisition),
                                   '--instruments', str(self.instruments), '--output', str(self.output)])
            self.assertFalse(kwargs['shell'])
            self.assertEqual(kwargs['timeout'], 3600)
            self.publish_stub(report or expected_report, evidence or expected_evidence)
            if change:
                change()
            return SimpleNamespace(returncode=0)
        return run

    def use_synthetic_archive(self):
        synthetic = {**deepcopy(DECLARED), 'kind': 'SYNTHETIC'}
        self.provenance.write_text(json.dumps(synthetic))
        self.bundle = self.root / 'synthetic'
        self.manifest = vision.freeze(SELECTION, self.archive, self.checksum, self.bundle, provenance_path=self.provenance)
        self.args.acquisition = self.bundle / 'archive.json'

    def preparation_args(self, origin='FIXTURE', pit_status='UNVERIFIED'):
        declaration, selection = self.root / 'declaration.json', self.root / 'selection.json'
        declared = {'schema_version': 1, 'registered_ref': 'synthetic-archive', 'storage_version': 'fixture-v1',
                    'origin': origin, 'pit_status': pit_status,
                    'revision_policy': 'AS_KNOWN_THEN' if pit_status == 'VERIFIED' else 'UNKNOWN'}
        declaration.write_text(json.dumps(declared, indent=4) + '\n')
        selection.write_text('{"selection":"native validation is tested separately"}')
        return SimpleNamespace(native_output=self.output, declaration=declaration, selection=selection,
                               output=self.root / ('prepared-' + origin + '-' + pit_status), native_bin=self.binary)

    def preparation_runner(self, args, body):
        def prepare(argv, **kwargs):
            self.assertEqual(argv, [str(self.binary), '--catalog', str(self.output / 'catalog'),
                '--declaration', str(args.declaration), '--selection', str(args.selection), '--output', str(args.output)])
            args.output.mkdir()
            (args.output / 'catalog').mkdir()
            (args.output / 'catalog/fixture.parquet').write_bytes(b'PAR1-stub-only-PAR1')
            (args.output / 'catalog-metadata.json').write_bytes(body)
            return SimpleNamespace(returncode=0)
        return prepare

    def test_registry_plan_inspect_freeze_verify_are_real_offline_operations(self):
        provider = vision.PROVIDER['id']
        self.assertEqual(self.invoke(['plan', provider, *self.selection_args]), vision.plan(SELECTION))
        inspected = self.invoke(['inspect', provider, *self.selection_args, '--archive', str(self.archive),
                                 '--checksum', str(self.checksum)])
        self.assertNotIn('rows', inspected)
        self.assertEqual(inspected['counts']['rows'], '2')
        output = self.root / 'second frozen'
        frozen = self.invoke(['freeze', provider, *self.selection_args, '--archive', str(self.archive),
                              '--checksum', str(self.checksum), '--output', str(output),
                              '--provenance', str(self.provenance)])
        verified = self.invoke(['verify', provider, '--acquisition', str(output / 'archive.json')])
        self.assertEqual(verified['integrity'], 'VERIFIED')
        self.assertEqual(frozen['provenance'], DECLARED)
        self.assertEqual((output / 'raw/archive.zip').read_bytes(), self.archive.read_bytes())
        self.assertEqual((output / 'raw/archive.CHECKSUM').read_bytes(), self.checksum.read_bytes())
        self.assertFalse(verified['admission']['research_qualified'])
        descriptor = next(item for item in self.invoke(['plugins']) if item['id'] == provider)
        self.assertEqual(descriptor['public_network_operations'], [])
        self.assertNotIn('download', descriptor['capabilities'])

    def test_conversion_binds_original_bytes_identity_count_and_receipt_without_shell(self):
        original = self.args.acquisition.read_bytes(), self.archive.read_bytes(), self.instruments.read_bytes()
        with patch.object(plugins.subprocess, 'run', side_effect=self.runner()) as run:
            result = plugins.archive_convert(self.args)
        run.assert_called_once()
        self.assertEqual(result['plugin'], vision.PROVIDER['id'])
        self.assertEqual(result['status'], 'NATIVE_ARTIFACTS_VALIDATED')
        self.assertFalse(result['admission']['research_qualified'])
        self.assertEqual(result['native_report']['receipt_basis']['declared_observed_at'],
                         DECLARED['retrieval']['archive']['completed_at'])
        self.assertEqual(result['native_report']['receipt_basis']['ts_init_ns'],
                         str(vision.utc_ns(DECLARED['retrieval']['archive']['completed_at'])))
        self.assertEqual(original, (self.args.acquisition.read_bytes(), self.archive.read_bytes(), self.instruments.read_bytes()))

    def test_unknown_or_unrecorded_receipt_refuses_before_native(self):
        for kind in ('UNKNOWN', 'OPERATOR_DECLARED', 'SYNTHETIC'):
            self.provenance.write_text(json.dumps({'kind': kind, 'retrieval': None}))
            bundle = self.root / kind
            vision.freeze(SELECTION, self.archive, self.checksum, bundle, provenance_path=self.provenance)
            args = SimpleNamespace(**{**vars(self.args), 'acquisition': bundle / 'archive.json'})
            with self.subTest(kind=kind), patch.object(plugins.subprocess, 'run') as run, self.assertRaises(ValueError):
                plugins.archive_convert(args)
            run.assert_not_called()
            self.assertFalse(self.output.exists())

    def test_synthetic_receipt_is_never_promoted(self):
        self.use_synthetic_archive()
        with patch.object(plugins.subprocess, 'run', side_effect=self.runner()):
            result = plugins.archive_convert(self.args)
        self.assertEqual(result['native_report']['receipt_basis']['kind'], 'SYNTHETIC')
        self.assertEqual(result['native_report']['source_provenance_kind'], 'SYNTHETIC')
        self.assertFalse(result['admission']['registered_in_quazonai'])

    def test_false_native_report_or_receipt_is_not_accepted(self):
        modifications = [('source_provider', 'coinbase-candles'), ('source_schema', 'wrong'), ('bars', 1),
                         ('research_qualified', True), ('supported_zip_profile', 'ZIP64'),
                         ('source_provenance_kind', 'SYNTHETIC')]
        for key, value in modifications:
            self.output = self.root / ('invalid-' + key)
            self.args.output = self.output
            report, evidence = self.publication()
            report[key] = value
            with self.subTest(key=key), patch.object(plugins.subprocess, 'run', side_effect=self.runner(report, evidence)), self.assertRaises(ValueError):
                plugins.archive_convert(self.args)
            self.assertTrue((self.output / 'import-report.json').exists())
        for target in ('report', 'evidence'):
            for key, value in [('kind', 'ATTESTED'), ('source_clock', 'imported_at'),
                               ('ts_init_ns', '0'), ('declared_observed_at', '2026-09-30T00:00:03Z')]:
                self.output = self.root / (target + '-' + key)
                self.args.output = self.output
                report, evidence = self.publication()
                (report if target == 'report' else evidence)['receipt_basis'][key] = value
                with self.subTest(target=target, key=key), patch.object(plugins.subprocess, 'run', side_effect=self.runner(report, evidence)), self.assertRaises(ValueError):
                    plugins.archive_convert(self.args)

    def test_original_bundle_change_after_native_is_rejected_and_artifacts_retained(self):
        path = self.bundle / 'raw/archive.zip'
        with patch.object(plugins.subprocess, 'run', side_effect=self.runner(change=lambda: path.write_bytes(b'changed'))), self.assertRaises(ValueError):
            plugins.archive_convert(self.args)
        self.assertTrue((self.output / 'import-report.json').exists())

    def test_archive_verify_requires_original_manifest_name(self):
        with self.assertRaises(ValueError):
            plugins.archive_verify(SimpleNamespace(acquisition=self.bundle / 'other.json'))

    def test_archive_prepare_preserves_metadata_bytes_and_rejects_changed_receipt(self):
        with patch.object(plugins.subprocess, 'run', side_effect=self.runner()):
            plugins.archive_convert(self.args)
        args = self.preparation_args()
        prepared = args.output
        body = args.declaration.read_bytes()
        with patch.object(plugins.subprocess, 'run', side_effect=self.preparation_runner(args, body)):
            result = plugins.prepare_source(vision.PROVIDER['id'], args)
        self.assertEqual(result['status'], 'CATALOG_PREPARED')
        self.assertEqual(result['catalog_registration'], {
            'root': str(prepared / 'catalog'), 'metadata_file': str(prepared / 'catalog-metadata.json')})
        self.assertEqual(result['metadata_bytes'], len(body))
        self.assertEqual(result['metadata_sha256'], plugins.sha256(prepared / 'catalog-metadata.json'))
        self.assertEqual((prepared / 'catalog-metadata.json').read_bytes(), body)
        self.assertFalse(result['admission']['research_qualified'])
        report_path = self.output / 'import-report.json'
        report = json.loads(report_path.read_bytes())
        report['receipt_basis']['ts_init_ns'] = '0'
        report_path.write_text(json.dumps(report))
        args.output = self.root / 'rejected'
        with patch.object(plugins.subprocess, 'run') as run, self.assertRaises(ValueError):
            plugins.prepare_source(vision.PROVIDER['id'], args)
        run.assert_not_called()

    def test_synthetic_archive_rejects_real_declaration_before_native_or_output(self):
        self.use_synthetic_archive()
        self.publish_stub(*self.publication())
        for pit_status in ('UNVERIFIED', 'VERIFIED', 'INVALID'):
            args = self.preparation_args('REAL', pit_status)
            originals = {path: path.read_bytes() for path in self.root.rglob('*') if path.is_file()}
            with self.subTest(pit_status=pit_status), \
                    patch.object(plugins.subprocess, 'run', side_effect=AssertionError('native must not run')) as run:
                with self.assertRaisesRegex(ValueError, 'origin REAL contradicts preserved SYNTHETIC'):
                    plugins.prepare_source(vision.PROVIDER['id'], args)
                run.assert_not_called()
                self.assertFalse(args.output.exists())
                self.assertEqual({path: path.read_bytes() for path in self.root.rglob('*') if path.is_file()}, originals)

    def test_synthetic_archive_preserves_non_real_declarations_and_source_evidence(self):
        self.use_synthetic_archive()
        self.publish_stub(*self.publication())
        for origin in ('SYNTHETIC', 'FIXTURE', 'LEGACY_UNKNOWN'):
            for pit_status in ('UNVERIFIED', 'INVALID', 'VERIFIED'):
                args = self.preparation_args(origin, pit_status)
                body = args.declaration.read_bytes()
                originals = {path: path.read_bytes() for path in self.root.rglob('*') if path.is_file()}
                with self.subTest(origin=origin, pit_status=pit_status), \
                        patch.object(plugins.subprocess, 'run', side_effect=self.preparation_runner(args, body)) as run:
                    result = plugins.prepare_source(vision.PROVIDER['id'], args)
                run.assert_called_once()
                self.assertEqual(result['status'], 'CATALOG_PREPARED')
                self.assertEqual((args.output / 'catalog-metadata.json').read_bytes(), body)
                self.assertEqual(result['metadata_sha256'], plugins.sha256(args.output / 'catalog-metadata.json'))
                for name, filename in (('import_report', 'import-report.json'), ('source_evidence', 'source-evidence.json')):
                    self.assertEqual(result['source_artifacts'][name]['sha256'], plugins.sha256(self.output / filename))
                self.assertEqual({path: path.read_bytes() for path in originals}, originals)
                self.assertEqual(result['admission']['historical_availability'], 'UNVERIFIED')
                self.assertFalse(result['admission']['research_qualified'])
                self.assertFalse(result['admission']['registered_in_quazonai'])

    def test_declared_archive_keeps_native_authority_over_origin_and_pit(self):
        self.publish_stub(*self.publication())
        for pit_status in ('UNVERIFIED', 'VERIFIED', 'INVALID'):
            args = self.preparation_args('REAL', pit_status)
            body = args.declaration.read_bytes()
            with self.subTest(pit_status=pit_status), \
                    patch.object(plugins.subprocess, 'run', side_effect=self.preparation_runner(args, body)) as run:
                result = plugins.prepare_source(vision.PROVIDER['id'], args)
            run.assert_called_once()
            self.assertEqual(result['status'], 'CATALOG_PREPARED')
            self.assertEqual((args.output / 'catalog-metadata.json').read_bytes(), body)
            self.assertFalse(result['admission']['research_qualified'])

    def test_native_validation_is_owned_by_the_registered_plugin(self):
        report, evidence = self.publication()
        seen = []
        example = plugins.SourcePlugin({'id': 'example'}, {}, lambda r, e, **context: seen.append((r, e, context)))
        declared = {'origin': 'REAL', 'pit_status': 'VERIFIED', 'revision_policy': 'AS_KNOWN_THEN'}
        with patch.dict(plugins.PLUGINS, {'example': example}):
            plugins.validate_native('example', report, evidence)
            plugins.validate_native('example', report, evidence, declaration=declared)
        self.assertEqual(seen, [(report, evidence, {'declaration': None}),
                                (report, evidence, {'declaration': declared})])
        with self.assertRaises(ValueError):
            plugins.validate_native('hf-snapshot', report, evidence)


if __name__ == '__main__':
    unittest.main()
