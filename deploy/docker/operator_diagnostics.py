#!/usr/bin/env python3
"""Retain bounded, original comparison diagnostics separately from the ELF archive."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import stat


MAX_FILE_BYTES = 4 * 1024 * 1024
MAX_TOTAL_BYTES = 16 * 1024 * 1024 - 64 * 1024
MAX_LOG_BYTES = 64 * 1024
VARIANTS = ('old', 'candidate')
REPETITIONS = ('old-a1', 'old-a2', 'candidate')
KINDS = ('candidate-elf', 'baseline-elf')
BINARIES = ('server', 'runtime')
ROOT_FILES = ('comparison.json', 'source-preflight.json', 'archive-tags.json', 'diagnostic.json')
VARIANT_FILES = ('report.json', 'precheck.json', 'harness.json', 'recipes.json',
                 'environment.json', 'build-command.json', 'cleanup.json',
                 'owned-resources.json')
ELF_FILES = ('diagnostics.json', *(tool + '-version.txt' for tool in
             ('readelf', 'objdump', 'objcopy', 'size')),
             *(binary + '-' + tool + '.txt' for binary in BINARIES
               for tool in ('readelf', 'objdump', 'size')))
FILES = (*ROOT_FILES, *(variant + '/' + name for variant in VARIANTS for name in VARIANT_FILES),
         *(variant + '/' + kind + '/' + name
           for variant in VARIANTS for kind in KINDS for name in ELF_FILES))
FILES = tuple(dict.fromkeys((*FILES,
    *(variant + '/' + name for variant in REPETITIONS for name in VARIANT_FILES),
    *(variant + '/application-elf/' + name for variant in REPETITIONS
      for name in (*ELF_FILES, 'native-identity.json', 'native-input.sha256', 'native-recipe.sha256')))))
FILES += tuple(variant + '/forensics/' + name for variant in ('old-a1', 'old-a2')
               for name in ('coverage.json', 'capture-status.txt', 'sizes.txt', 'producer-tools.txt',
                            'forensic.Dockerfile', 'forensic.Dockerfile.dockerignore',
                            'server-prestrip-readelf.txt', 'server-stripped-readelf.txt',
                            'runtime-prestrip-readelf.txt', 'runtime-stripped-readelf.txt'))


def section_observations(payloads, names=None, labels=('old', 'candidate')):
    """Diagnostic only: never change the comparison's admission decision."""
    names = names or [variant + '/candidate-elf/diagnostics.json' for variant in VARIANTS]
    if any(name not in payloads for name in names):
        return {'status': 'unavailable', 'reason': 'missing original candidate ELF diagnostics'}
    def reject_nonfinite(token):
        raise ValueError('Non-finite diagnostic JSON number: ' + token)
    try:
        old, candidate = (json.loads(payloads[name], parse_constant=reject_nonfinite) for name in names)
        result = {}
        for binary in BINARIES:
            left, right = old[binary], candidate[binary]
            for original in (left, right):
                if not isinstance(original, dict) or not isinstance(original['sha256'], str) or not re.fullmatch(r'[0-9a-f]{64}', original['sha256']) or \
                        type(original['size_bytes']) is not int or original['size_bytes'] <= 0 or \
                        not isinstance(original['sections'], dict) or not original['sections']:
                    raise ValueError('Invalid original ELF identity')
                for name, section in original['sections'].items():
                    if not isinstance(name, str) or not name or not isinstance(section, dict) or \
                            not isinstance(section['sha256'], str) or not re.fullmatch(r'[0-9a-f]{64}', section['sha256']) or \
                            type(section['size_bytes']) is not int or section['size_bytes'] < 0:
                        raise ValueError('Invalid original section identity')
            sections = sorted(set(left['sections']) | set(right['sections']))
            result[binary] = {
                labels[0] + '_sha256': left['sha256'], labels[1] + '_sha256': right['sha256'],
                labels[0] + '_size_bytes': left['size_bytes'], labels[1] + '_size_bytes': right['size_bytes'],
                'different_sections': {name: {labels[0]: left['sections'].get(name),
                                              labels[1]: right['sections'].get(name)}
                                       for name in sections
                                       if left['sections'].get(name) != right['sections'].get(name)},
            }
        return {'status': 'observed', 'binaries': result}
    except (ValueError, TypeError, KeyError, AttributeError):
        return {'status': 'unavailable', 'reason': 'malformed original ELF diagnostics; raw files retained'}


def collect(source, destination, revision, *, forensic=False):
    if not re.fullmatch(r'[0-9a-f]{40}', revision):
        raise ValueError('Require the exact reviewed harness revision.')
    if source.is_symlink() or not source.is_dir():
        raise ValueError('Diagnostic source must be an existing regular directory.')
    if destination.exists() or destination.is_symlink():
        raise ValueError('Preserve existing diagnostic output; use a fresh directory.')
    source, destination = source.resolve(), destination.resolve()
    if destination == source or source in destination.parents:
        raise ValueError('Keep the small diagnostic output outside the original archive.')
    payloads, missing, total = {}, [], 0
    maximum = min(MAX_TOTAL_BYTES, 8 * 1024 * 1024 - 64 * 1024) if forensic else MAX_TOTAL_BYTES
    for name in FILES:
        path = source
        absent = False
        for part in Path(name).parts:
            path = path / part
            try:
                mode = path.lstat().st_mode
            except FileNotFoundError:
                absent = True
                break
            if stat.S_ISLNK(mode):
                raise ValueError('Refuse symlink diagnostic input: ' + name)
        if absent:
            missing.append(name)
            continue
        if not stat.S_ISREG(mode) or path.stat().st_size > MAX_FILE_BYTES:
            raise ValueError('Diagnostic input is not a bounded regular file: ' + name)
        with path.open('rb') as stream:
            body = stream.read(MAX_FILE_BYTES + 1)
        total += len(body)
        if len(body) > MAX_FILE_BYTES or total > maximum:
            raise ValueError('Small diagnostic budget exceeded; original archive is preserved.')
        payloads[name] = body
    manifest = {
        'schema_version': 1, 'harness_revision': revision,
        'qualification': 'DIAGNOSTIC_ONLY',
        'scope': 'Original diagnostic bytes only; not packaging qualification or a root-cause claim.',
        'input_bytes': total, 'missing_files': missing,
        'maximum_total_bytes': maximum + 64 * 1024,
        'files': {name: {'bytes': len(body), 'sha256': hashlib.sha256(body).hexdigest()}
                  for name, body in payloads.items()},
        'section_observations': section_observations(payloads),
        'repetition_observations': {
            relation: section_observations(payloads,
                [name + '/application-elf/diagnostics.json' for name in names], names)
            for relation, names in (('AA', ('old-a1', 'old-a2')), ('BC', ('old-a1', 'candidate')))},
    }
    metadata = (json.dumps(manifest, indent=2, allow_nan=False) + '\n').encode()
    if len(metadata) > 64 * 1024:
        raise ValueError('Diagnostic manifest exceeded its reserved budget; originals preserved.')
    destination.mkdir()
    for name, body in payloads.items():
        output = destination / name
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(body)
    (destination / 'manifest.json').write_bytes(metadata)
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--destination', type=Path, required=True)
    parser.add_argument('--revision', required=True)
    parser.add_argument('--forensic-aa-only', action='store_true')
    args = parser.parse_args()
    result = collect(args.source, args.destination, args.revision, forensic=args.forensic_aa_only)
    summary = json.dumps({'input_bytes': result['input_bytes'],
                          'missing_files': result['missing_files'],
                          'section_observations': result['section_observations'],
                          'repetition_observations': result['repetition_observations']}, allow_nan=False)
    print(summary if len(summary.encode()) <= MAX_LOG_BYTES
          else 'Detailed section observations retained in the bounded diagnostic artifact.')


if __name__ == '__main__':
    main()
