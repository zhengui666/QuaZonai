#!/usr/bin/env python3
"""Hosted, unpublished cost comparison against the pinned two-ELF full image."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import uuid

import operator_cost as cost


OLD_REVISION = 'bfa3cfc625a752fdb554d9136b9b2eb4412c6f24'
OPERATOR_RECIPE = 'CARGO_PROFILE_RELEASE_DEBUG=0 cargo build --locked --release -p job --features polymarket-history,catalog-prepare'


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def harness_identity():
    source = Path(__file__).resolve().parents[2]
    revision = cost.run(['git', '-C', str(source), 'rev-parse', 'HEAD'])
    tree = cost.run(['git', '-C', str(source), 'rev-parse', 'HEAD^{tree}'])
    if any(not re.fullmatch(r'[0-9a-f]{40}', value) for value in (revision, tree)) or \
            cost.run(['git', '-C', str(source), 'status', '--porcelain', '--untracked-files=no']):
        raise ValueError('Measurement harness must be a clean, exact checkout.')
    names = ('deploy/docker/operator_compare.py', 'deploy/docker/operator_cost.py',
             '.github/workflows/operator-cost-comparison.yml')
    return {'revision': revision, 'tree': tree,
            'file_sha256': {name: hashlib.sha256((source / name).read_bytes()).hexdigest() for name in names}}


def verify_sources(old, candidate, revision):
    if not re.fullmatch(r'[0-9a-f]{40}', revision) or revision == OLD_REVISION:
        raise ValueError('Candidate must be a distinct, exact source SHA.')
    recipes = []
    for source, expected, bins in ((old, OLD_REVISION, '--bin catalog-prepare --bin polymarket-history'),
                                   (candidate, revision, '--bin source-tools')):
        if cost.run(['git', '-C', str(source), 'rev-parse', 'HEAD']) != expected or \
                cost.run(['git', '-C', str(source), 'status', '--porcelain']):
            raise ValueError('Measurement requires clean checkouts at the exact requested revisions.')
        dockerfile = (source / 'deploy/docker/Dockerfile').read_text()
        normalized = ' '.join(dockerfile.replace('\\\n', ' ').split())
        if OPERATOR_RECIPE + ' ' + bins + ' &&' not in normalized or \
                'ENV RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2' not in dockerfile:
            raise ValueError('The comparison no longer uses the approved native recipe.')
        images = re.findall(r'^FROM (\S+)', dockerfile, re.MULTILINE)
        external = [image for image in images if ':' in image or '@' in image]
        if not external or any(not re.fullmatch(r'[^\s@]+@sha256:[0-9a-f]{64}', image) for image in external):
            raise ValueError('Every external build image must be pinned to a digest.')
        rust = [image for image in external if image.startswith('rust:1.98.1-bookworm@')]
        if len(rust) != 1:
            raise ValueError('The pinned native compiler image is missing.')
        hashes = {name: hashlib.sha256((source / name).read_bytes()).hexdigest()
                  for name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'apps/web/package-lock.json')}
        recipes.append({'revision': expected, 'base_images': external, 'locked_inputs_sha256': hashes,
                        'dockerfile_sha256': hashlib.sha256(dockerfile.encode()).hexdigest(),
                        'operator_build_command': OPERATOR_RECIPE + ' ' + bins, 'rust_image': rust[0],
                        'platform': 'linux/amd64'})
    for field in ('base_images', 'locked_inputs_sha256', 'rust_image', 'platform'):
        if recipes[0][field] != recipes[1][field]:
            raise ValueError('Old and candidate comparison inputs differ: ' + field)
    return recipes


def build_command(source, builder, image, revision, version, target=None):
    command = ['docker', 'buildx', 'build', '--builder', builder, '--progress', 'plain',
               '--platform', 'linux/amd64', '--load', '--tag', image,
               '--build-arg', 'VERSION=' + version, '--build-arg', 'REVISION=' + revision,
               '--file', str(source / 'deploy/docker/Dockerfile')]
    if target:
        command += ['--target', target]
    return [*command, str(source)]


def measured_build(directory, phase, command):
    cost.mark(directory, phase + '-start')
    try:
        with (directory / (phase + '.log')).open('w') as output:
            subprocess.run(command, stdout=output, stderr=subprocess.STDOUT, check=True)
    finally:
        cost.mark(directory, phase + '-end')


def elapsed(directory, phase):
    begin = json.loads((directory / (phase + '-start.json')).read_text())
    end = json.loads((directory / (phase + '-end.json')).read_text())
    if end['monotonic_ns'] < begin['monotonic_ns']:
        raise ValueError('Invalid build measurement chronology.')
    return (end['monotonic_ns'] - begin['monotonic_ns']) / 1e9


def measure(old, candidate, revision, variant, directory, version, builder_image):
    if os.environ.get('GITHUB_ACTIONS') != 'true' or os.environ.get('RUNNER_ENVIRONMENT') != 'github-hosted':
        raise ValueError('Run this expensive comparison only in its GitHub-hosted workflow.')
    if not re.fullmatch(r'moby/buildkit@sha256:[0-9a-f]{64}', builder_image):
        raise ValueError('Both builders require the same resolved official BuildKit image digest.')
    directory.mkdir(parents=True, exist_ok=True)
    harness = harness_identity()
    write_json(directory / 'harness.json', harness)
    recipes = verify_sources(old, candidate, revision)
    recipe = recipes[0 if variant == 'old' else 1]
    source = old if variant == 'old' else candidate
    selected_revision = recipe['revision']
    layout = 'legacy' if variant == 'old' else 'single'
    builder = 'operator-measure-' + uuid.uuid4().hex
    full_image = 'quazonai-operator-measure:' + variant
    base_image = full_image + '-base'
    environment = {'builder_name': builder, 'builder_image': builder_image,
                   'cpu_count': os.cpu_count(), 'memory': Path('/proc/meminfo').read_text().splitlines()[0],
                   'cpu_models': sorted({line.split(':', 1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines()
                                         if line.startswith('model name')}),
                   'docker_version': cost.run(['docker', 'version']),
                   'buildx_version': cost.run(['docker', 'buildx', 'version']),
                   'runner_os': os.environ.get('RUNNER_OS'), 'runner_arch': os.environ.get('RUNNER_ARCH'),
                   'runner_image': os.environ.get('ImageOS'), 'runner_image_version': os.environ.get('ImageVersion')}
    # Inspect identical pinned prerequisites in the host Docker store. This
    # does not prewarm the docker-container builder's separate content store:
    # the timed cold build includes that builder's base-image transfers.
    for image in recipe['base_images']:
        cost.run(['docker', 'pull', '--platform', 'linux/amd64', image])
    environment['rustc'] = cost.run(['docker', 'run', '--rm', '--network', 'none', '--entrypoint',
                                    '/bin/sh', recipe['rust_image'], '-c', 'rustc -Vv && cargo -V'])
    write_json(directory / 'environment.json', environment)
    write_json(directory / 'recipes.json', recipes)
    created = False
    try:
        cost.run(['docker', 'buildx', 'create', '--name', builder, '--driver', 'docker-container',
                  '--driver-opt', 'image=' + builder_image])
        created = True
        environment['builder_inspect'] = cost.run(['docker', 'buildx', 'inspect', '--bootstrap', builder])
        environment['initial_cache'] = cost.run(['docker', 'buildx', 'du', '--builder', builder])
        write_json(directory / 'environment.json', environment)
        command = build_command(source, builder, full_image, selected_revision, version)
        write_json(directory / 'build-command.json', command)
        # A fresh docker-container builder owns new, empty cache mounts. No
        # external cache is imported; --no-cache alone would not prove this.
        measured_build(directory, 'candidate', command)
        cold_identity = cost.image_identity(full_image, selected_revision)['id']
        measured_build(directory, 'warm', command)
        if cost.image_identity(full_image, selected_revision)['id'] != cold_identity:
            raise ValueError('The warm replay changed the full image; compare its outputs before proceeding.')
        measured_build(directory, 'baseline', build_command(source, builder, base_image,
                                                            selected_revision, version, 'application-base'))
        report = cost.report(directory, base_image, full_image, selected_revision, version, layout, emit=False)
        report['hosted_comparison'] = {
            'variant': variant, 'old_revision': OLD_REVISION, 'candidate_revision': revision,
            'recipe': recipe, 'environment': environment, 'harness': harness,
            'independent_cold_full_build_seconds': elapsed(directory, 'candidate'),
            'same_builder_warm_full_build_seconds': elapsed(directory, 'warm'),
            'warm_observations': {phase: json.loads((directory / (phase + '.json')).read_text())
                                  for phase in ('warm-start', 'warm-end')},
            'cold_scope': 'Fresh disposable docker-container builder and empty cache mounts, no cache import; timed build includes builder base-image transfers. Host prerequisite pulls do not prewarm its content store.',
            'warm_scope': 'Same revision, populated builder, layer-cache replay; retained producer duration is not a new compile duration.',
            'gha_layer_cache_only_replay': 'deferred to the separate cache and producer-identity batch; not measured',
            'actual_link_process_count': None,
            'link_count_scope': 'Deferred, not instrumented; shipped ELF count below is artifact evidence, not linker process telemetry. No linker-count reduction is claimed.',
            'shipped_elf_count': len(report['payload']['stripped_binary_bytes']),
            'runtime_packages': cost.run(['docker', 'run', '--rm', '--network', 'none', '--read-only',
                                          '--entrypoint', '/usr/bin/dpkg-query', cold_identity,
                                          '-W', '-f', '${Package}\t${Version}\n'])}
        report['cache_conditions'] = [
            report['hosted_comparison']['cold_scope'], report['hosted_comparison']['warm_scope'],
            'The same-source no-operator baseline follows both full builds and can reuse their layers.',
            'Disk observations are before/after samples, not a continuous peak measurement.']
        write_json(directory / 'report.json', report)
    finally:
        if created:
            cost.run(['docker', 'buildx', 'rm', builder])


def compare(old, candidate, revision):
    before, after = old['hosted_comparison'], candidate['hosted_comparison']
    if old['revision'] != OLD_REVISION or candidate['revision'] != revision or \
            before['variant'] != 'old' or after['variant'] != 'candidate' or \
            old['version'] != candidate['version'] or \
            any(item['old_revision'] != OLD_REVISION or item['candidate_revision'] != revision for item in (before, after)):
        raise ValueError('Comparison reports do not identify the requested revisions and common release tag.')
    for field in ('base_images', 'locked_inputs_sha256', 'rust_image', 'platform'):
        if before['recipe'][field] != after['recipe'][field]:
            raise ValueError('Measurement recipes differ: ' + field)
    for field in ('builder_image', 'buildx_version', 'rustc', 'runner_os', 'runner_arch', 'cpu_count', 'memory'):
        if before['environment'][field] != after['environment'][field]:
            raise ValueError('Measurement environments differ: ' + field)
    if before['environment']['builder_name'] == after['environment']['builder_name']:
        raise ValueError('Cold measurements must use independent disposable builders.')
    if before['harness'] != after['harness']:
        raise ValueError('Measurement harness identities differ.')
    if before['runtime_packages'] != after['runtime_packages']:
        raise ValueError('The images resolved different runtime package versions.')
    deltas = {'full_image_bytes': candidate['candidate']['size_bytes'] - old['candidate']['size_bytes'],
              'full_compressed_archive_bytes': candidate['candidate']['compressed_archive_bytes'] - old['candidate']['compressed_archive_bytes'],
              'operator_payload_bytes': candidate['payload']['operator_payload_bytes'] - old['payload']['operator_payload_bytes'],
              'stripped_elf_bytes': sum(candidate['payload']['stripped_binary_bytes'].values()) - sum(old['payload']['stripped_binary_bytes'].values()),
              'launcher_bytes': sum(candidate['payload']['launcher_bytes'].values()) - sum(old['payload']['launcher_bytes'].values())}
    timings = {key: {'old': before[key], 'candidate': after[key], 'delta': after[key] - before[key]}
               for key in ('independent_cold_full_build_seconds', 'same_builder_warm_full_build_seconds')}
    return {'schema_version': 1, 'scope': 'pinned old full image versus candidate full image; hosted actual bytes',
            'old_revision': OLD_REVISION, 'candidate_revision': revision, 'version': candidate['version'],
            'delta': deltas, 'build_seconds': timings,
            'application_binaries_identical': old['candidate']['application_binary_sha256'] == candidate['candidate']['application_binary_sha256'],
            'same_cpu_model': before['environment']['cpu_models'] == after['environment']['cpu_models'],
            'required_size_reductions_observed': all(deltas[key] < 0 for key in (
                'full_image_bytes', 'full_compressed_archive_bytes', 'operator_payload_bytes', 'stripped_elf_bytes')),
            'old': old, 'candidate': candidate,
            'admission': 'Cost evidence only; behavior, scientific isolation, final-head CI and independent review remain separate requirements.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest='command', required=True)
    measurement = subcommands.add_parser('measure')
    measurement.add_argument('--old-source', type=Path, required=True)
    measurement.add_argument('--candidate-source', type=Path, required=True)
    measurement.add_argument('--revision', required=True)
    measurement.add_argument('--variant', choices=('old', 'candidate'), required=True)
    measurement.add_argument('--directory', type=Path, required=True)
    measurement.add_argument('--version', required=True)
    measurement.add_argument('--builder-image', required=True)
    comparison = subcommands.add_parser('compare')
    comparison.add_argument('--old', type=Path, required=True)
    comparison.add_argument('--candidate', type=Path, required=True)
    comparison.add_argument('--revision', required=True)
    comparison.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.command == 'measure':
        measure(args.old_source.resolve(), args.candidate_source.resolve(), args.revision, args.variant,
                args.directory.resolve(), args.version, args.builder_image)
    else:
        result = compare(json.loads(args.old.read_text()), json.loads(args.candidate.read_text()), args.revision)
        write_json(args.output, result)
        print(json.dumps({key: value for key, value in result.items() if key not in ('old', 'candidate')}, indent=2))
        if not result['required_size_reductions_observed']:
            parser.exit(1, 'Required full image, archive, payload and ELF reductions were not all observed; see the retained comparison.\n')


if __name__ == '__main__':
    main()
