#!/usr/bin/env python3
"""Measure the shipped operator payload; never substitute debug sizes or estimates."""
import argparse
import gzip
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
    if value['Config']['Labels'].get('org.opencontainers.image.revision') != revision:
        raise ValueError('Measured image does not match the requested source revision.')
    return {'id': value['Id'], 'size_bytes': value['Size']}


def report(directory, baseline, candidate, revision, version):
    marks = {name: json.loads((directory / (name + '.json')).read_text())
             for name in ('candidate-start', 'candidate-end', 'baseline-start', 'baseline-end')}
    def elapsed(kind):
        value = marks[kind + '-end']['monotonic_ns'] - marks[kind + '-start']['monotonic_ns']
        if value < 0:
            raise ValueError('Invalid build measurement chronology.')
        return value / 1_000_000_000
    before = image_identity(baseline, revision)
    after = image_identity(candidate, revision)
    container = ['docker', 'run', '--rm', '--network', 'none', '--read-only', '--no-healthcheck',
                 '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges:true', '--entrypoint']
    run([*container, '/bin/sh', baseline, '-c', 'test ! -e /opt/quazonai/operator && ! command -v python3'])
    before['operator_payload'] = 'absent, checked in the actual baseline image'
    for image, measurement in ((baseline, before), (candidate, after)):
        sizes = run([*container, '/usr/bin/stat', image, '-c', '%s',
                     '/opt/quazonai/bin/server', '/opt/quazonai/bin/runtime']).splitlines()
        if len(sizes) != 2:
            raise ValueError('Application binary size measurement is incomplete.')
        measurement['stripped_application_binary_bytes'] = dict(zip(('server', 'runtime'), map(int, sizes)))
    program = """import json
from pathlib import Path
root=Path('/opt/quazonai/operator')
bins={name:(root/'bin'/name).stat().st_size for name in ('catalog-prepare','polymarket-history')}
print(json.dumps({'stripped_binary_bytes':bins,'operator_payload_bytes':sum(path.stat().st_size for path in root.rglob('*') if path.is_file()),'native_build':json.loads((root/'build-metrics.json').read_text())}))
"""
    payload = json.loads(run([*container, '/usr/bin/python3', candidate, '-E', '-s', '-B', '-c', program]))
    if payload['native_build']['revision'] != revision or any(value <= 0 for value in payload['stripped_binary_bytes'].values()):
        raise ValueError('Native build measurements do not identify the measured release payload.')
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
    print(json.dumps(result, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('mark', 'report'))
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--phase', choices=('candidate-start', 'candidate-end', 'baseline-start', 'baseline-end'))
    parser.add_argument('--baseline')
    parser.add_argument('--candidate')
    parser.add_argument('--revision')
    parser.add_argument('--version')
    args = parser.parse_args()
    if args.command == 'mark':
        if not args.phase:
            parser.error('mark requires --phase')
        mark(args.directory, args.phase)
    else:
        if not all((args.baseline, args.candidate, args.revision, args.version)):
            parser.error('report requires both actual images, revision and version')
        report(args.directory, args.baseline, args.candidate, args.revision, args.version)


if __name__ == '__main__':
    main()
