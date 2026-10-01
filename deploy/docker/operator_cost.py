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


def run(args):
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout.strip()


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


def archive_bytes(image, version):
    if not re.fullmatch(r'[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}', version):
        raise ValueError('Archive measurement requires an exact supported image tag.')
    # Docker serializes RepoTags in the archive. Use the actual release spelling,
    # not a measurement-only alias, and preserve any previously assigned image.
    local = 'quazonai-bundle/application:' + version
    def assigned():
        identities = run(['docker', 'image', 'ls', '--no-trunc', '--quiet', local]).splitlines()
        if len(identities) > 1 or any(not re.fullmatch(r'sha256:[0-9a-f]{64}', identity) for identity in identities):
            raise ValueError('Archive measurement tag identity is ambiguous.')
        return identities[0] if identities else None
    previous = assigned()
    measured = run(['docker', 'image', 'inspect', '--format', '{{.Id}}', image])
    if not re.fullmatch(r'sha256:[0-9a-f]{64}', measured):
        raise ValueError('Archive measurement image identity is invalid.')
    if previous != measured:
        run(['docker', 'image', 'tag', measured, local])
    destination = CountedArchive()
    try:
        if assigned() != measured:
            raise ValueError('Archive measurement tag changed before serialization.')
        with subprocess.Popen(['docker', 'image', 'save', local], stdout=subprocess.PIPE) as process:
            try:
                with gzip.GzipFile(fileobj=destination, mode='wb', compresslevel=1, mtime=0) as archive:
                    shutil.copyfileobj(process.stdout, archive)
                if process.wait() != 0:
                    raise ValueError('Application image archive measurement failed.')
            except BaseException:
                process.kill()
                process.wait()
                raise
    finally:
        if assigned() != measured:
            raise ValueError('Archive measurement tag changed; its current assignment was preserved.')
        if previous and previous != measured:
            run(['docker', 'image', 'tag', previous, local])
            if assigned() != previous:
                raise ValueError('The original archive tag assignment could not be confirmed; current tags were preserved.')
        elif previous is None:
            run(['docker', 'image', 'rm', local])
            if assigned() is not None:
                raise ValueError('The archive tag was reassigned during cleanup; its current assignment was preserved.')
    return destination.bytes


def image_identity(image, revision):
    value = json.loads(run(['docker', 'image', 'inspect', image]))[0]
    if (value['Config'].get('Labels') or {}).get('org.opencontainers.image.revision') != revision:
        raise ValueError('Measured image does not match the requested source revision.')
    if not re.fullmatch(r'sha256:[0-9a-f]{64}', value['Id']) or type(value['Size']) is not int or value['Size'] <= 0:
        raise ValueError('Measured image has an invalid identity or size.')
    return {'id': value['Id'], 'size_bytes': value['Size']}


def payload_inventory(root, revision, layout):
    # This self-contained function also runs in the actual, read-only image.
    import hashlib
    import json
    import os
    from pathlib import Path
    import stat

    root = Path(root)
    selected = {'single': {'source-tools'}, 'legacy': {'catalog-prepare', 'polymarket-history'}}
    if layout not in selected:
        raise ValueError('Unknown operator executable layout.')
    expected = selected[layout]
    launchers = {'catalog-prepare', 'polymarket-history'} if layout == 'single' else set()
    if not stat.S_ISDIR(root.lstat().st_mode):
        raise ValueError('Operator payload must be a regular directory.')
    files, binaries, scripts, hashes = {}, {}, {}, {}
    for path in sorted(root.rglob('*')):
        mode = path.lstat().st_mode
        if stat.S_ISDIR(mode):
            continue
        if not stat.S_ISREG(mode):
            raise ValueError('Operator payload contains a symlink or non-regular file: ' + str(path))
        relative = path.relative_to(root).as_posix()
        size = path.stat().st_size
        files[relative] = size
        with path.open('rb') as stream:
            header = stream.read(20)
            stream.seek(0)
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        hashes[relative] = digest
        if header.startswith(b'\x7fELF'):
            if relative not in {'bin/' + name for name in expected}:
                raise ValueError('Unexpected operator ELF: ' + relative)
            if not mode & 0o111 or not os.access(path, os.X_OK):
                raise ValueError('Selected operator ELF is not executable: ' + relative)
            if len(header) != 20 or header[4] not in (1, 2) or header[5] not in (1, 2) or \
                    int.from_bytes(header[16:18], 'little' if header[5] == 1 else 'big') not in (2, 3):
                raise ValueError('Selected operator ELF has an invalid executable header: ' + relative)
            binaries[path.name] = size
        elif relative.startswith('bin/'):
            if relative not in {'bin/' + name for name in launchers} or \
                    not header.startswith(b'#!/bin/sh\n') or not mode & 0o111 or not os.access(path, os.X_OK):
                raise ValueError('Selected operator launcher is not a regular executable shell script: ' + relative)
            scripts[path.name] = size
    if set(binaries) != expected or set(scripts) != launchers:
        raise ValueError('Operator executable or launcher payload is missing.')
    native_build = json.loads((root / 'build-metrics.json').read_text())
    if native_build['revision'] != revision:
        raise ValueError('Native build measurements do not identify the measured release payload.')
    return {'layout': layout,
            # Historical field retained; it always means actual ELF bytes.
            'stripped_binary_bytes': binaries, 'launcher_bytes': scripts,
            'operator_payload_bytes': sum(files.values()),
            'operator_payload_file_bytes': files, 'operator_payload_sha256': hashes,
            'native_build': native_build}


def measure_image(image, revision, layout=None):
    measurement = image_identity(image, revision)
    container = ['docker', 'run', '--rm', '--network', 'none', '--read-only', '--no-healthcheck',
                 '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges:true', '--entrypoint']
    # Use the resolved ID throughout so a concurrent tag reassignment cannot
    # switch the inspected binaries or archive after the revision check.
    identity = measurement['id']
    paths = ['/opt/quazonai/bin/server', '/opt/quazonai/bin/runtime']
    sizes = run([*container, '/usr/bin/stat', identity, '-c', '%s', *paths]).splitlines()
    checksums = run([*container, '/usr/bin/sha256sum', identity, *paths]).splitlines()
    if len(sizes) != 2 or len(checksums) != 2 or any(
            not re.fullmatch(r'[0-9a-f]{64}  ' + re.escape(path), line)
            for path, line in zip(paths, checksums)):
        raise ValueError('Application binary measurement is incomplete.')
    measurement['stripped_application_binary_bytes'] = dict(zip(('server', 'runtime'), map(int, sizes)))
    measurement['application_binary_sha256'] = dict(zip(('server', 'runtime'),
                                                       (line.split()[0] for line in checksums)))
    if layout is None:
        run([*container, '/bin/sh', identity, '-c',
             'test ! -e /opt/quazonai/operator && test ! -L /opt/quazonai/operator && ! command -v python3'])
        measurement['operator_payload'] = 'absent, checked in the actual baseline image'
    else:
        program = 'import json\n' + inspect.getsource(payload_inventory) + '\nprint(json.dumps(payload_inventory(' + \
                  repr('/opt/quazonai/operator') + ', ' + repr(revision) + ', ' + repr(layout) + ')))\n'
        measurement['payload'] = json.loads(run([*container, '/usr/bin/python3', identity,
                                                '-E', '-s', '-B', '-c', program]))
    return measurement


def report(directory, baseline, candidate, revision, version, layout='single', emit=True):
    marks = {name: json.loads((directory / (name + '.json')).read_text())
             for name in ('candidate-start', 'candidate-end', 'baseline-start', 'baseline-end')}
    def elapsed(kind):
        value = marks[kind + '-end']['monotonic_ns'] - marks[kind + '-start']['monotonic_ns']
        if value < 0:
            raise ValueError('Invalid build measurement chronology.')
        return value / 1_000_000_000
    before = measure_image(baseline, revision)
    after = measure_image(candidate, revision, layout)
    payload = after.pop('payload')
    before['compressed_archive_bytes'] = archive_bytes(before['id'], version)
    after['compressed_archive_bytes'] = archive_bytes(after['id'], version)
    result = {'schema_version': 1, 'revision': revision, 'version': version,
              'scope': 'same-source application-base without operator versus full candidate; actual bytes, no estimates',
              'baseline': before, 'candidate': after, 'payload': payload,
              'delta_image_bytes': after['size_bytes'] - before['size_bytes'],
              'delta_compressed_archive_bytes': after['compressed_archive_bytes'] - before['compressed_archive_bytes'],
              'normal_candidate_build_seconds': elapsed('candidate'),
              'warm_baseline_build_seconds': elapsed('baseline'), 'observations': marks,
              'cache_conditions': [
                  'The normal candidate builds first with the existing configured GHA/BuildKit caches; this is not a cold-build claim.',
                  'The same-source baseline builds afterward and can reuse common candidate layers; its timing is not an independent cold comparison.',
                  'Native operator-only build seconds are measured in its producer stage with shared Cargo caches; a cached stage can retain that original measurement.',
                  'Disk observations are before/after samples, not a continuous peak measurement.'],
              'archive_method': 'actual docker image save stream, gzip level 1, mtime 0; no archive retained',
              'admission': 'Measured costs require review; this report does not establish source or research qualification.'}
    (directory / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
    if emit:
        print(json.dumps(result, indent=2))
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
    parser.add_argument('--layout', choices=('single', 'legacy'), default='single')
    args = parser.parse_args()
    if args.command == 'mark':
        if not args.phase:
            parser.error('mark requires --phase')
        mark(args.directory, args.phase)
    else:
        if not all((args.baseline, args.candidate, args.revision, args.version)):
            parser.error('report requires both actual images, revision and version')
        report(args.directory, args.baseline, args.candidate, args.revision, args.version, args.layout)


if __name__ == '__main__':
    main()
