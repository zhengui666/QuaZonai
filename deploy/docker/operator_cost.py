#!/usr/bin/env python3
"""Measure the shipped operator payload; never substitute debug sizes or estimates."""
import argparse
import gzip
import inspect
import json
from pathlib import Path
import re
import shutil
import subprocess
import time


MAX_ARCHIVE = 2_000_000_000  # Same conservative limit as release.archive_images.
COMMAND_DEADLINE = None  # Hosted comparison only; ordinary reports retain their behavior.
CLEANUP_DEADLINE = None  # The hosted job reserves this time before artifact upload.


def command_timeout(limit=300):
    if COMMAND_DEADLINE is None:
        return None
    remaining = COMMAND_DEADLINE - time.time()
    if remaining <= 0:
        raise TimeoutError('Measurement command budget exhausted; retain partial evidence.')
    return min(limit, remaining)


def run(args):
    return subprocess.run(args, check=True, capture_output=True, text=True,
                          timeout=command_timeout()).stdout.strip()


def observe():
    disk = shutil.disk_usage(Path.cwd())
    return {'monotonic_ns': time.monotonic_ns(), 'wall_time_ns': time.time_ns(),
            'disk_total_bytes': disk.total, 'disk_used_bytes': disk.used, 'disk_free_bytes': disk.free}


def mark(directory, phase):
    directory.mkdir(parents=True, exist_ok=True)
    (directory / (phase + '.json')).write_text(json.dumps(observe(), indent=2) + '\n')


class CountedArchive:
    # Match the actual release GzipFile header without retaining another large archive.
    name = 'quazonai-image-application.tar.gz'

    def __init__(self):
        self.bytes = 0

    def write(self, body):
        self.bytes += len(body)
        if self.bytes >= MAX_ARCHIVE:
            raise ValueError('Measured application archive exceeds the release asset limit.')
        return len(body)

    def flush(self):
        pass


def archive_cleanup_records(evidence):
    """Read the persistent journal before any alias mutation, including retries."""
    if evidence is None or not evidence.exists():
        return []
    records = json.loads(evidence.read_text())
    def image_id(value):
        return isinstance(value, str) and re.fullmatch(r'sha256:[0-9a-f]{64}', value) is not None
    def complete(record):
        if not isinstance(record, dict) or record.get('status') != 'complete':
            return False
        original, measured = record.get('original_id'), record.get('measured_id')
        return (isinstance(record.get('tag'), str)
                and re.fullmatch(r'quazonai-bundle/application:[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}', record['tag'])
                and 'original_id' in record and (original is None or image_id(original))
                and image_id(measured) and record.get('observed_before_cleanup') == measured
                and 'observed_after_cleanup' in record and record['observed_after_cleanup'] == original
                and record.get('outcome') == ('removed' if original is None else 'restored' if original != measured else 'unchanged'))
    if not isinstance(records, list) or not all(complete(record) for record in records):
        raise ValueError('Previous temporary archive alias cleanup is unconfirmed; journal preserved.')
    return records


def archive_cleanup_confirmed(evidence):
    try:
        archive_cleanup_records(evidence)
        return True
    except (OSError, ValueError):
        return False


def archive_bytes(image, version, cleanup_evidence=None):
    global COMMAND_DEADLINE
    if not re.fullmatch(r'[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}', version):
        raise ValueError('Archive measurement requires an exact supported image tag.')
    # Docker serializes RepoTags in the archive. Use the actual release spelling,
    # not a measurement-only alias, and preserve any previously assigned image.
    local = 'quazonai-bundle/application:' + version
    if COMMAND_DEADLINE is not None and (CLEANUP_DEADLINE is None or CLEANUP_DEADLINE <= COMMAND_DEADLINE):
        raise ValueError('Bounded archive measurement requires reserved alias cleanup time.')
    def assigned():
        identities = run(['docker', 'image', 'ls', '--no-trunc', '--quiet', local]).splitlines()
        if len(identities) > 1 or any(not re.fullmatch(r'sha256:[0-9a-f]{64}', identity) for identity in identities):
            raise ValueError('Archive measurement tag identity is ambiguous.')
        return identities[0] if identities else None
    records = archive_cleanup_records(cleanup_evidence)
    previous = assigned()
    measured = run(['docker', 'image', 'inspect', '--format', '{{.Id}}', image])
    if not re.fullmatch(r'sha256:[0-9a-f]{64}', measured):
        raise ValueError('Archive measurement image identity is invalid.')
    record = {'tag': local, 'original_id': previous, 'measured_id': measured, 'status': 'pending'}
    records.append(record)
    def retain_cleanup():
        if cleanup_evidence:
            cleanup_evidence.write_text(json.dumps(records, indent=2) + '\n')
    retain_cleanup()
    destination = CountedArchive()
    try:
        if previous != measured:
            run(['docker', 'image', 'tag', measured, local])
        if assigned() != measured:
            raise ValueError('Archive measurement tag changed before serialization.')
        command = ['docker', 'image', 'save', local]
        if COMMAND_DEADLINE is not None:
            # Bound the stream read as well as wait(); a stalled pipe must not
            # consume the job's evidence-upload reserve.
            command = ['timeout', '--signal=TERM', '--kill-after=10s',
                       str(command_timeout(600)) + 's', *command]
        with subprocess.Popen(command, stdout=subprocess.PIPE) as process:
            try:
                with gzip.GzipFile(fileobj=destination, mode='wb', compresslevel=1, mtime=0) as archive:
                    shutil.copyfileobj(process.stdout, archive)
                if process.wait() != 0:
                    raise ValueError('Application image archive measurement failed.')
            except BaseException:
                process.kill()
                process.wait()
                raise
    except Exception as error:
        record['measurement_error'] = type(error).__name__ + ': ' + str(error)
        raise
    finally:
        measurement_deadline = COMMAND_DEADLINE
        if CLEANUP_DEADLINE is not None:
            COMMAND_DEADLINE = CLEANUP_DEADLINE
        try:
            current = record['observed_before_cleanup'] = assigned()
            if current != measured:
                raise ValueError('Archive measurement tag changed; its current assignment was preserved.')
            if previous and previous != measured:
                run(['docker', 'image', 'tag', previous, local])
            elif previous is None:
                run(['docker', 'image', 'rm', local])
            record['observed_after_cleanup'] = assigned()
            if record['observed_after_cleanup'] != previous:
                raise ValueError('The original archive tag assignment could not be confirmed; current assignment was preserved.')
            record['status'] = 'complete'
            record['outcome'] = 'removed' if previous is None else 'restored' if previous != measured else 'unchanged'
        except Exception as error:
            record['status'] = 'blocked'
            record['error'] = type(error).__name__ + ': ' + str(error)
            raise
        finally:
            COMMAND_DEADLINE = measurement_deadline
            retain_cleanup()
    return destination.bytes


def image_identity(image, revision, version=None):
    value = json.loads(run(['docker', 'image', 'inspect', image]))[0]
    labels = value['Config'].get('Labels') or {}
    if labels.get('org.opencontainers.image.revision') != revision:
        raise ValueError('Measured image does not match the requested source revision.')
    if version is not None:
        if labels.get('org.opencontainers.image.version') != version:
            raise ValueError('Measured image does not match the requested packaging version.')
        if (value.get('Os'), value.get('Architecture')) != ('linux', 'amd64'):
            raise ValueError('Measured image does not match the native producer platform.')
    if not re.fullmatch(r'sha256:[0-9a-f]{64}', value['Id']) or type(value['Size']) is not int or value['Size'] <= 0:
        raise ValueError('Measured image has an invalid identity or size.')
    return {'id': value['Id'], 'size_bytes': value['Size']}


def native_identity(source):
    """Recompute from the current checkout, never from an image's self-report."""
    source = source.resolve()
    return json.loads(run(['node', str(source / 'deploy/docker/native-inputs.mjs'),
                           'identity', str(source), 'linux/amd64']))


def payload_inventory(root, layout='shared', application_root='/opt/quazonai/bin'):
    # This function is also serialized into the actual read-only image. Keep it
    # self-contained, and retain observations before the host validates them.
    import hashlib
    import json
    import os
    from pathlib import Path
    import stat

    if layout not in ('standalone', 'shared'):
        raise ValueError('Unknown operator executable layout.')
    root, application_root = Path(root), Path(application_root)
    files, hashes, modes, binaries, scripts, elfs, errors = {}, {}, {}, {}, {}, {}, []

    def inspect_file(path, name):
        try:
            mode = path.lstat().st_mode
            if not stat.S_ISREG(mode):
                raise ValueError('Not a regular file: ' + str(path))
            with path.open('rb') as stream:
                header = stream.read(20)
                stream.seek(0)
                digest = hashlib.file_digest(stream, 'sha256').hexdigest()
            executable = bool(mode & 0o111) and os.access(path, os.X_OK)
            is_elf = header.startswith(b'\x7fELF')
            if is_elf:
                if (len(header) != 20 or header[4:7] != b'\x02\x01\x01'
                        or int.from_bytes(header[16:18], 'little') not in (2, 3)
                        or int.from_bytes(header[18:20], 'little') != 62 or not executable):
                    errors.append('Invalid native linux/amd64 executable ELF: ' + str(path))
                elfs[name] = digest
            return path.stat().st_size, digest, mode & 0o7777, is_elf, executable
        except (OSError, ValueError) as error:
            errors.append(str(error))
            return None

    try:
        if not stat.S_ISDIR(root.lstat().st_mode):
            raise ValueError('Operator payload must be a regular directory.')
        for path in sorted(root.rglob('*')):
            relative = path.relative_to(root).as_posix()
            mode = path.lstat().st_mode
            if stat.S_ISDIR(mode):
                if relative != 'bin':
                    errors.append('Unexpected operator directory: ' + relative)
                continue
            files[relative] = path.lstat().st_size
            name = path.name if path.parent == root / 'bin' else relative
            observed = inspect_file(path, name)
            if observed is None:
                continue
            size, digest, mode, is_elf, executable = observed
            hashes[relative], modes[relative] = digest, mode
            if is_elf:
                binaries[name] = size
            elif path.parent == root / 'bin':
                scripts[name] = size
                if not executable:
                    errors.append('Operator launcher is not executable: ' + relative)
    except (OSError, ValueError) as error:
        errors.append(str(error))
    for name in ('server', 'runtime'):
        observed = inspect_file(application_root / name, name)
        if observed is not None and not observed[3]:
            errors.append('Application binary is not ELF: ' + name)
    try:
        if 'build-metrics.json' not in hashes:
            raise ValueError('Native producer record must be a regular payload file.')
        native = json.loads((root / 'build-metrics.json').read_text())
    except (OSError, ValueError) as error:
        native = None
        errors.append('Invalid native producer record: ' + str(error))
    return {'layout': layout, 'stripped_binary_bytes': binaries, 'launcher_bytes': scripts,
            'elf_sha256': elfs, 'operator_payload_bytes': sum(files.values()),
            'operator_payload_file_bytes': files, 'operator_payload_sha256': hashes,
            'operator_payload_file_modes': modes, 'native_build': native,
            'inventory_errors': errors}


def payload_program(layout='shared'):
    if layout not in ('standalone', 'shared'):
        raise ValueError('Unknown operator executable layout.')
    return ('import json\n' + inspect.getsource(payload_inventory)
            + '\nprint(json.dumps(payload_inventory(' + repr('/opt/quazonai/operator')
            + ', ' + repr(layout) + ')))\n')


def source_operator_modules(source=None):
    source = Path(source) if source is not None else Path(__file__).resolve().parents[2]
    modules = {'source_plugins.py', 'acquire.py', 'providers.py', 'snapshot.py', 'binance_vision.py'}
    lines = (source / 'deploy/docker/Dockerfile').read_text(encoding='utf-8').splitlines()
    if any(line.lstrip().startswith('COPY ') and 'runtimes/data/hf_dataset.py' in line.split()[1:-1]
           for line in lines):
        modules.add('hf_dataset.py')
    return modules


def native_validation_errors(payload, expected, application_hashes, layout='shared', source=None):
    if layout not in ('standalone', 'shared'):
        raise ValueError('Unknown operator executable layout.')
    errors = list(payload.get('inventory_errors', []))
    operators = {'source-tools'} if layout == 'shared' else {'catalog-prepare', 'polymarket-history'}
    launchers = {'catalog-prepare', 'polymarket-history'} if layout == 'shared' else set()
    names = {'server', 'runtime'} | operators
    expected_files = {'build-metrics.json'} | source_operator_modules(source) | {
        'bin/' + name for name in operators | launchers}
    if payload.get('layout') != layout:
        errors.append('Measured operator layout does not match the explicit selector.')
    files = payload.get('operator_payload_file_bytes', {})
    hashes = payload.get('operator_payload_sha256', {})
    modes = payload.get('operator_payload_file_modes', {})
    if set(files) != expected_files or set(hashes) != expected_files or set(modes) != expected_files:
        errors.append('Operator payload file set is incomplete or contains unexpected files.')
    if (any(type(size) is not int or size < 0 for size in files.values())
            or type(payload.get('operator_payload_bytes')) is not int
            or payload['operator_payload_bytes'] != sum(files.values())):
        errors.append('Operator payload size measurements are incomplete.')
    if any(not isinstance(value, str) or not re.fullmatch(r'[0-9a-f]{64}', value) for value in hashes.values()):
        errors.append('Operator payload hash measurements are incomplete.')
    if any(type(mode) is not int or not 0 <= mode <= 0o7777 for mode in modes.values()):
        errors.append('Operator payload mode measurements are incomplete.')
    for name in operators | launchers:
        mode = modes.get('bin/' + name)
        if type(mode) is not int or mode != 0o755:
            errors.append('Operator executable mode is not 0755: ' + name)
    sizes = payload.get('stripped_binary_bytes', {})
    if (set(sizes) != operators or any(type(value) is not int or value <= 0 for value in sizes.values())
            or any(files.get('bin/' + name) != value for name, value in sizes.items())):
        errors.append('Native binary size measurements do not match the selected ELF layout.')
    scripts = payload.get('launcher_bytes', {})
    if set(scripts) != launchers:
        errors.append('Operator launcher set does not match the selected layout.')
    import hashlib
    for name in launchers:
        body = ('#!/bin/sh\nexec /opt/quazonai/operator/bin/source-tools ' + name + ' "$@"\n').encode()
        if (scripts.get(name) != len(body) or files.get('bin/' + name) != len(body)
                or hashes.get('bin/' + name) != hashlib.sha256(body).hexdigest()):
            errors.append('Operator launcher bytes differ from the fixed exec body: ' + name)
    native = payload.get('native_build')
    if not isinstance(native, dict):
        errors.append('Native producer record is missing or malformed.')
        native = {}
    # Producer schema 1 is historical revision-bound evidence. Schema 2 keeps
    # its original input-bound meaning, including all original timing samples.
    if native.get('schema_version') != 2:
        errors.append('Native producer schema 2 required; historical schema 1 is not input-bound evidence.')
    for key in ('input_sha256', 'recipe_sha256', 'platform'):
        value = expected.get(key)
        valid = value == 'linux/amd64' if key == 'platform' else isinstance(value, str) and re.fullmatch(r'[0-9a-f]{64}', value)
        if not valid or native.get(key) != value:
            errors.append('Native producer does not match current-source ' + key + '.')
    elfs = payload.get('elf_sha256', {})
    if (set(elfs) != names or native.get('elf_sha256') != elfs
            or any(not isinstance(value, str) or not re.fullmatch(r'[0-9a-f]{64}', value) for value in elfs.values())
            or any(elfs.get(name) != value for name, value in application_hashes.items())
            or set(application_hashes) != {'server', 'runtime'}
            or any(elfs.get(name) != hashes.get('bin/' + name) for name in operators)):
        errors.append('Native producer ELF hashes do not identify the measured payload and common application.')
    for key in ('original_native_build_elapsed_seconds', 'original_disk_before_bytes', 'original_disk_after_bytes'):
        if type(native.get(key)) is not int or native[key] < 0:
            errors.append('Missing original producer measurement: ' + key)
    return errors


def validate_native_build(payload, expected, application_hashes, layout='shared', source=None):
    errors = native_validation_errors(payload, expected, application_hashes, layout, source=source)
    if errors:
        raise ValueError('; '.join(errors))


def measure_image(image, revision, version, layout=None, observations=None):
    measurement = image_identity(image, revision, version)
    if observations is not None:
        observations.update(measurement)
        measurement = observations
    container = ['docker', 'run', '--rm', '--network', 'none', '--read-only', '--no-healthcheck',
                 '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges:true', '--entrypoint']
    identity = measurement['id']
    paths = ['/opt/quazonai/bin/server', '/opt/quazonai/bin/runtime']
    sizes = run([*container, '/usr/bin/stat', identity, '-c', '%s', *paths]).splitlines()
    checksums = run([*container, '/usr/bin/sha256sum', identity, *paths]).splitlines()
    if len(sizes) != 2 or len(checksums) != 2 or any(
            not re.fullmatch(r'[0-9a-f]{64}  ' + re.escape(path), line)
            for path, line in zip(paths, checksums)):
        raise ValueError('Application binary measurement is incomplete.')
    measurement['stripped_application_binary_bytes'] = dict(zip(('server', 'runtime'), map(int, sizes)))
    if any(size <= 0 for size in measurement['stripped_application_binary_bytes'].values()):
        raise ValueError('Application binary size measurement is invalid.')
    measurement['application_elf_sha256'] = dict(zip(('server', 'runtime'), (line.split()[0] for line in checksums)))
    if layout is None:
        run([*container, '/bin/sh', identity, '-c',
             'test ! -e /opt/quazonai/operator && test ! -L /opt/quazonai/operator && ! command -v python3'])
        measurement['operator_payload'] = 'absent, checked in the actual baseline image'
    else:
        measurement['payload'] = json.loads(run([*container, '/usr/bin/python3', identity,
                                                '-E', '-s', '-B', '-c', payload_program(layout)]))
    return measurement


def report(directory, baseline, candidate, revision, version, layout='shared', emit=True, source=None, strict=True, cleanup_evidence=None):
    if layout not in ('standalone', 'shared'):
        raise ValueError('Unknown operator executable layout.')
    marks = {name: json.loads((directory / (name + '.json')).read_text())
             for name in ('candidate-start', 'candidate-end', 'baseline-start', 'baseline-end')}
    def elapsed(kind):
        value = marks[kind + '-end']['monotonic_ns'] - marks[kind + '-start']['monotonic_ns']
        if value < 0:
            raise ValueError('Invalid build measurement chronology.')
        return value / 1_000_000_000
    result = {'schema_version': 3, 'revision': revision, 'version': version, 'layout': layout,
              'scope': 'same-source application-base without operator versus full candidate; actual bytes, no estimates',
              'normal_candidate_build_seconds': elapsed('candidate'),
              'warm_baseline_build_seconds': elapsed('baseline'), 'observations': marks,
              'validation_errors': []}
    evidence_path = directory / 'image-observations.json'
    save = lambda: evidence_path.write_text(json.dumps(result, indent=2) + '\n')
    save()
    expected = None
    try:
        expected = native_identity(source or Path(__file__).resolve().parents[2])
        result['source_native_identity'] = {key: expected[key] for key in ('input_sha256', 'recipe_sha256', 'platform')}
    except Exception as error:
        result['validation_errors'].append('Current source identity: ' + type(error).__name__ + ': ' + str(error))
    save()
    for name, image, selected_layout in (('baseline', baseline, None), ('candidate', candidate, layout)):
        measurement = result[name] = {}
        try:
            result[name] = measure_image(image, revision, version, selected_layout, observations=measurement)
        except Exception as error:
            measurement['measurement_error'] = type(error).__name__ + ': ' + str(error)
            result['validation_errors'].append(name + ': ' + measurement['measurement_error'])
        save()
    before, after = result['baseline'], result['candidate']
    if 'payload' in after:
        payload = result['payload'] = after.pop('payload')
        try:
            result['validation_errors'].extend(native_validation_errors(
                payload, expected or {}, before.get('application_elf_sha256', {}), layout, source=source))
        except Exception as error:
            result['validation_errors'].append('Payload validation: ' + type(error).__name__ + ': ' + str(error))
    if (before.get('application_elf_sha256') != after.get('application_elf_sha256')
            or before.get('stripped_application_binary_bytes') != after.get('stripped_application_binary_bytes')):
        result['validation_errors'].append('Baseline and candidate application ELFs differ.')
    save()
    # Source/provenance failures do not erase independent byte observations.
    # A failed archive may continue only when its temporary alias was restored;
    # uncertain cleanup must never authorize another mutation of that alias.
    cleanup_path = cleanup_evidence if cleanup_evidence is not None else directory / 'archive-tags.json'
    alias_cleanup_confirmed = archive_cleanup_confirmed(cleanup_path)
    for name, measurement in (('baseline', before), ('candidate', after)):
        if not measurement.get('id'):
            measurement['archive_error'] = 'Archive not attempted: no verified image identity.'
        elif not alias_cleanup_confirmed:
            measurement['archive_error'] = 'Archive not attempted: previous temporary alias cleanup is unconfirmed.'
        else:
            try:
                measurement['compressed_archive_bytes'] = archive_bytes(measurement['id'], version, cleanup_path)
            except Exception as error:
                measurement['archive_error'] = type(error).__name__ + ': ' + str(error)
            alias_cleanup_confirmed = archive_cleanup_confirmed(cleanup_path)
            if not alias_cleanup_confirmed:
                result['validation_errors'].append('Temporary archive alias cleanup is unconfirmed.')
        if 'archive_error' in measurement:
            result['validation_errors'].append(name + ': ' + measurement['archive_error'])
        save()
    if all('size_bytes' in measurement for measurement in (before, after)):
        result['delta_image_bytes'] = after['size_bytes'] - before['size_bytes']
    if all('compressed_archive_bytes' in measurement for measurement in (before, after)):
        result['delta_compressed_archive_bytes'] = after['compressed_archive_bytes'] - before['compressed_archive_bytes']
    result.update({
        'cache_conditions': [
            'The normal candidate builds first with the existing configured GHA/BuildKit caches; this is not a cold-build claim.',
            'The same-source baseline builds afterward and can reuse common candidate layers; its timing is not an independent cold comparison.',
            'Original native operator build seconds and disk observations belong to the producer execution; layer reuse retains them unchanged. Current build elapsed time comes only from the external marks.',
            'Disk observations are before/after samples, not a continuous peak measurement.'],
        'archive_method': 'actual docker image save stream, gzip level 1, mtime 0; no archive retained',
        'admission': 'Measured costs require review; this report does not establish source or research qualification.'})
    save()
    (directory / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
    if emit:
        print(json.dumps(result, indent=2))
    if strict and result['validation_errors']:
        raise ValueError('Measurement failed; complete observations retained: ' + '; '.join(result['validation_errors']))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('mark', 'report'))
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--phase', choices=('candidate-start', 'candidate-end', 'baseline-start', 'baseline-end'))
    parser.add_argument('--baseline')
    parser.add_argument('--candidate')
    parser.add_argument('--revision')
    parser.add_argument('--version')
    parser.add_argument('--source', type=Path, help='Current checkout; defaults to this script’s repository root')
    parser.add_argument('--layout', choices=('standalone', 'shared'), default='shared')
    args = parser.parse_args()
    if args.command == 'mark':
        if not args.phase:
            parser.error('mark requires --phase')
        mark(args.directory, args.phase)
    else:
        if not all((args.baseline, args.candidate, args.revision, args.version)):
            parser.error('report requires both actual images, revision and version')
        report(args.directory, args.baseline, args.candidate, args.revision, args.version, layout=args.layout, source=args.source)


if __name__ == '__main__':
    main()
