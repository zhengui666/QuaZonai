#!/usr/bin/env python3
"""Hosted, unpublished cost comparison against the pinned two-ELF full image."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import stat
import subprocess
import tempfile
import time
import uuid

import operator_cost as cost


OLD_REVISION = 'b58ec6d153d5b211b2941bf0d603ea3a3108a68b'
PRIOR_PACKAGING_REVISION = '56aad24e0c73b31a9255e37adaa0abcd812a7c42'
REPORT_SCHEMA = 3
MIN_FREE_BYTES = 40_000_000_000
LOCKED_INPUTS = ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'apps/web/package-lock.json')
HARNESS_FILES = ('deploy/docker/operator_compare.py', 'deploy/docker/operator_cost.py',
                 'deploy/docker/native-inputs.mjs', 'deploy/docker/native-build.sh',
                 '.github/workflows/operator-cost-comparison.yml')
OPERATOR_RECIPE = 'CARGO_PROFILE_RELEASE_DEBUG=0 cargo build --locked --release -p job --features polymarket-history,catalog-prepare'
LAUNCHERS = ('catalog-prepare', 'polymarket-history')
LAUNCHER_COPIES = ''.join('COPY --chmod=755 deploy/docker/operator/' + name +
                          ' /opt/quazonai/operator/bin/' + name + '\n' for name in LAUNCHERS)
NATIVE_SUBSTITUTIONS = (
    ('--bin catalog-prepare --bin polymarket-history', '--bin source-tools'),
    ('    install -Dm755 target/release/catalog-prepare /operator/bin/catalog-prepare\n'
     '    install -Dm755 target/release/polymarket-history /operator/bin/polymarket-history\n'
     '    strip /operator/bin/catalog-prepare /operator/bin/polymarket-history\n',
     '    install -Dm755 target/release/source-tools /operator/bin/source-tools\n'
     '    strip /operator/bin/source-tools\n'),
    ('"catalog-prepare":"%s","polymarket-history":"%s"', '"source-tools":"%s"'),
    ('      "$(sha256sum /operator/bin/catalog-prepare | cut -d \' \' -f 1)" \\\n'
     '      "$(sha256sum /operator/bin/polymarket-history | cut -d \' \' -f 1)" \\\n',
     '      "$(sha256sum /operator/bin/source-tools | cut -d \' \' -f 1)" \\\n'),
)


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')


def harness_identity():
    source = Path(__file__).resolve().parents[2]
    revision = cost.run(['git', '-C', str(source), 'rev-parse', 'HEAD'])
    tree = cost.run(['git', '-C', str(source), 'rev-parse', 'HEAD^{tree}'])
    if any(not re.fullmatch(r'[0-9a-f]{40}', value) for value in (revision, tree)) or \
            cost.run(['git', '-C', str(source), 'status', '--porcelain', '--untracked-files=no']):
        raise ValueError('Measurement harness must be a clean, exact checkout.')
    names = HARNESS_FILES
    return {'revision': revision, 'tree': tree,
            'file_sha256': {name: hashlib.sha256((source / name).read_bytes()).hexdigest() for name in names}}


def substitute_native_recipe(body):
    """Apply only the reviewed build/install/strip/operator-hash substitutions."""
    for before, after in NATIVE_SUBSTITUTIONS:
        if body.count(before) != 1:
            raise ValueError('Pinned native helper does not contain the exact approved operator recipe.')
        body = body.replace(before, after, 1)
    return body


def verify_sources(old, candidate, revision):
    if not re.fullmatch(r'[0-9a-f]{40}', revision) or revision == OLD_REVISION:
        raise ValueError('Candidate must be a distinct, exact source SHA.')
    recipes, dockerfiles, helpers, common_inputs = [], [], [], []
    for source, expected, layout, bins in ((old, OLD_REVISION, 'standalone', '--bin catalog-prepare --bin polymarket-history'),
                                          (candidate, revision, 'shared', '--bin source-tools')):
        if cost.run(['git', '-C', str(source), 'rev-parse', 'HEAD']) != expected or \
                cost.run(['git', '-C', str(source), 'status', '--porcelain']):
            raise ValueError('Measurement requires clean checkouts at the exact requested revisions.')
        tree = cost.run(['git', '-C', str(source), 'rev-parse', 'HEAD^{tree}'])
        if not re.fullmatch(r'[0-9a-f]{40}', tree):
            raise ValueError('Measurement source tree identity is invalid.')
        dockerfile = (source / 'deploy/docker/Dockerfile').read_bytes().decode('utf-8')
        helper = (source / 'deploy/docker/native-build.sh').read_bytes().decode('utf-8')
        dockerfiles.append(dockerfile)
        helpers.append(helper)
        common_inputs.append({name: hashlib.sha256((source / name).read_bytes()).hexdigest()
                              for name in ('deploy/docker/native-inputs.mjs', '.dockerignore',
                                           'deploy/docker/Dockerfile.dockerignore')})
        normalized = ' '.join(helper.replace('\\\n', ' ').split())
        if OPERATOR_RECIPE + ' ' + bins + ' install' not in normalized or \
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
                  for name in LOCKED_INPUTS}
        recipes.append({'revision': expected, 'tree': tree, 'layout': layout,
                        'base_images': external, 'locked_inputs_sha256': hashes,
                        'dockerfile_sha256': hashlib.sha256(dockerfile.encode()).hexdigest(),
                        'native_build_sha256': hashlib.sha256(helper.encode()).hexdigest(),
                        'common_inputs_sha256': common_inputs[-1],
                        'operator_build_command': OPERATOR_RECIPE + ' ' + bins, 'rust_image': rust[0],
                        'platform': 'linux/amd64'})
    for field in ('base_images', 'locked_inputs_sha256', 'rust_image', 'platform', 'common_inputs_sha256'):
        if recipes[0][field] != recipes[1][field]:
            raise ValueError('Old and candidate comparison inputs differ: ' + field)
    if helpers[1] != substitute_native_recipe(helpers[0]):
        raise ValueError('Only the approved native operator substitutions may differ; the server and common helper must be identical.')
    anchor = 'COPY --from=operator /operator/ /opt/quazonai/operator/\n'
    if dockerfiles[0].count(anchor) != 1 or dockerfiles[1] != dockerfiles[0].replace(anchor, anchor + LAUNCHER_COPIES, 1):
        raise ValueError('Only the two exact final-stage launcher COPY instructions may differ.')
    for name in LAUNCHERS:
        launcher = candidate / 'deploy/docker/operator' / name
        wanted = '#!/bin/sh\nexec /opt/quazonai/operator/bin/source-tools ' + name + ' "$@"\n'
        if not stat.S_ISREG(launcher.lstat().st_mode) or launcher.read_bytes() != wanted.encode():
            raise ValueError('Candidate launcher must be an exact regular fixed-path script: ' + name)
    for ancestor in (OLD_REVISION, PRIOR_PACKAGING_REVISION):
        cost.run(['git', '-C', str(candidate), 'merge-base', '--is-ancestor', ancestor, revision])
    # No helper block is discarded: bind the entire old recipe and the verified
    # positive substitution above, including every server/common instruction.
    common = hashlib.sha256(json.dumps({'dockerfile': dockerfiles[0], 'native_build': helpers[0],
                                       'common_inputs': common_inputs[0]}, sort_keys=True).encode()).hexdigest()
    for source, recipe in zip((old, candidate), recipes):
        identity = cost.native_identity(source)
        if type(identity.get('schema_version')) is not int or identity['schema_version'] != 1 or identity.get('platform') != 'linux/amd64' or any(
                not isinstance(identity.get(key), str) or not re.fullmatch(r'[0-9a-f]{64}', identity[key])
                for key in ('input_sha256', 'recipe_sha256')):
            raise ValueError('Native source collector returned malformed identity.')
        recipe['common_recipe_sha256'] = common
        recipe['native_identity'] = {key: identity[key] for key in ('input_sha256', 'recipe_sha256', 'platform')}
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
            subprocess.run(command, stdout=output, stderr=subprocess.STDOUT, check=True,
                           timeout=cost.command_timeout(45 * 60))
    finally:
        cost.mark(directory, phase + '-end')


def elapsed(directory, phase):
    begin = json.loads((directory / (phase + '-start.json')).read_text())
    end = json.loads((directory / (phase + '-end.json')).read_text())
    if end['monotonic_ns'] < begin['monotonic_ns']:
        raise ValueError('Invalid build measurement chronology.')
    return (end['monotonic_ns'] - begin['monotonic_ns']) / 1e9


def cache_is_empty(output):
    lines = [line.strip() for line in output.splitlines() if line.strip()]
    return (bool(lines) and re.fullmatch(r'ID\s+RECLAIMABLE\s+SIZE\s+LAST ACCESSED', lines[0]) is not None
            and any(re.fullmatch(r'Total:\s+0B', line) for line in lines[1:])
            and all(re.fullmatch(r'(Reclaimable|Total|Shared|Private):\s+0B', line) for line in lines[1:]))


def environment_identity(builder, builder_image):
    components = json.loads(cost.run(['docker', 'version', '--format', '{{json .Server.Components}}']))
    containerd = [item for item in components if item.get('Name') == 'containerd']
    if len(containerd) != 1:
        raise ValueError('Docker did not report one containerd identity.')
    return {'builder_name': builder, 'builder_image': builder_image,
            'cpu_count': os.cpu_count(), 'memory': Path('/proc/meminfo').read_text().splitlines()[0],
            'cpu_models': sorted({line.split(':', 1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines()
                                  if line.startswith('model name')}),
            'docker_version': cost.run(['docker', 'version']), 'containerd': containerd[0],
            'buildx_version': cost.run(['docker', 'buildx', 'version']),
            'runner_os': os.environ.get('RUNNER_OS'), 'runner_arch': os.environ.get('RUNNER_ARCH'),
            'runner_image': os.environ.get('ImageOS'), 'runner_image_version': os.environ.get('ImageVersion'),
            'runner_name': os.environ.get('RUNNER_NAME'),
            'runner_boot_id': Path('/proc/sys/kernel/random/boot_id').read_text().strip(),
            'run_id': os.environ.get('GITHUB_RUN_ID'), 'run_attempt': os.environ.get('GITHUB_RUN_ATTEMPT')}


def extract_application(directory, image, owner, resources):
    """Retain original ELFs and GNU binutils diagnostics, without rewriting them."""
    directory.mkdir(parents=True, exist_ok=True)
    name = owner + '-extract-' + directory.name
    identity = cost.run(['docker', 'create', '--name', name, '--label', 'quazonai.measurement=' + owner, image])
    if not re.fullmatch(r'[0-9a-f]{64}', identity):
        raise ValueError('Extraction container did not return a valid identity.')
    resources['containers'][name] = identity
    write_json(directory.parent / 'owned-resources.json', resources)
    result = {}
    for tool in ('readelf', 'objdump', 'objcopy', 'size'):
        (directory / (tool + '-version.txt')).write_text(cost.run([tool, '--version']) + '\n')
    for binary in ('server', 'runtime'):
        destination = directory / binary
        cost.run(['docker', 'cp', identity + ':/opt/quazonai/bin/' + binary, str(destination)])
        with destination.open('rb') as stream:
            if stream.read(4) != b'\x7fELF':
                raise ValueError('Extracted application binary is not an ELF: ' + binary)
            stream.seek(0)
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        headers = cost.run(['readelf', '--wide', '--file-header', '--program-headers', '--section-headers', '--notes', str(destination)])
        (directory / (binary + '-readelf.txt')).write_text(headers + '\n')
        sections = cost.run(['objdump', '--section-headers', str(destination)])
        (directory / (binary + '-objdump.txt')).write_text(sections + '\n')
        (directory / (binary + '-size.txt')).write_text(cost.run(['size', '--format=SysV', '--radix=10', str(destination)]) + '\n')
        hashes = {}
        lines = sections.splitlines()
        with tempfile.TemporaryDirectory(prefix='sections-', dir=directory) as temporary:
            for index, line in enumerate(lines[:-1]):
                match = re.match(r'^\s*\d+\s+(\S+)\s+([0-9a-fA-F]+)\s+', line)
                if match and 'CONTENTS' in lines[index + 1]:
                    section, size = match.groups()
                    target = Path(temporary) / str(index)
                    # objcopy implements ELF section extraction. Supplying an
                    # output file preserves the original binary byte-for-byte.
                    cost.run(['objcopy', '--dump-section', section + '=' + str(target),
                              str(destination), str(Path(temporary) / 'copy')])
                    body = target.read_bytes()
                    if len(body) != int(size, 16):
                        raise ValueError('ELF section length does not match objdump: ' + section)
                    hashes[section] = {'size_bytes': len(body), 'sha256': hashlib.sha256(body).hexdigest()}
        if not hashes:
            raise ValueError('No ELF section contents were diagnosed: ' + binary)
        result[binary] = {'size_bytes': destination.stat().st_size, 'sha256': digest, 'sections': hashes}
    write_json(directory / 'diagnostics.json', result)
    return result


def cleanup_owned(directory, resources, archive_evidence=None):
    """Delete only names whose recorded identity and exclusive ownership match."""
    result = {'status': 'complete', 'removed': [], 'errors': [], 'archive_tags': []}
    archive_evidence = archive_evidence if archive_evidence is not None else directory / 'archive-tags.json'
    if archive_evidence.exists():
        try:
            result['archive_tags'] = json.loads(archive_evidence.read_text())
            cost.archive_cleanup_records(archive_evidence)
            expected_tags = {}
            for record in result['archive_tags']:
                if record['status'] != 'complete':
                    raise ValueError('Archive alias restoration/removal was not confirmed; see archive-tags.json.')
                expected_tags[record['tag']] = record['original_id']
            for tag, expected in expected_tags.items():
                current = cost.run(['docker', 'image', 'ls', '--no-trunc', '--quiet', tag]).splitlines()
                if current != ([] if expected is None else [expected]):
                    raise ValueError('Archive alias no longer matches its restored original identity; preserved.')
        except Exception as error:
            result['status'] = 'blocked'
            result['errors'].append({'kind': 'archive-alias', 'error': str(error)})
            # A formerly complete restore can fail this final live readback.
            # Retain that uncertainty for the next variant or resumed attempt;
            # never overwrite malformed or already-blocked journal evidence.
            if cost.archive_cleanup_confirmed(archive_evidence):
                result['archive_tags'].append({'status': 'blocked',
                    'phase': 'post-measurement-cleanup', 'error': str(error)})
                write_json(archive_evidence, result['archive_tags'])
    def remove(kind, name, identity):
        try:
            owner = resources['owner']
            if not re.fullmatch(r'operator-measure-[0-9a-f]{32}', owner) or (kind == 'builder' and name != owner) or (kind == 'container' and not name.startswith(owner + '-extract-')) or (kind == 'image' and name not in {'quazonai-operator-measure:' + owner + '-full', 'quazonai-operator-measure:' + owner + '-base'}):
                raise ValueError('Resource is outside the recorded exclusive ownership namespace; preserved.')
            if kind == 'builder':
                current = json.loads(cost.run(['docker', 'container', 'inspect', 'buildx_buildkit_' + name + '0']))[0]
                if current['Id'] != identity or current['Config']['Image'] != resources['builder_image']:
                    raise ValueError('Builder container identity changed; preserved.')
                inspection = cost.run(['docker', 'buildx', 'inspect', name])
                if not re.search(r'^Name:\s+' + re.escape(name) + r'\s*$', inspection, re.MULTILINE) or not re.search(r'^Driver:\s+docker-container\s*$', inspection, re.MULTILINE):
                    raise ValueError('Builder name or driver changed; preserved.')
                cost.run(['docker', 'buildx', 'rm', name])
                if name in cost.run(['docker', 'buildx', 'ls', '--format', '{{.Name}}']).splitlines():
                    raise ValueError('Builder removal was not confirmed.')
            elif kind == 'container':
                current = json.loads(cost.run(['docker', 'container', 'inspect', name]))[0]
                if current['Id'] != identity or (current['Config'].get('Labels') or {}).get('quazonai.measurement') != resources['owner']:
                    raise ValueError('Extraction container identity or ownership changed; preserved.')
                cost.run(['docker', 'container', 'rm', name])
                if cost.run(['docker', 'container', 'ls', '--all', '--quiet', '--filter', 'id=' + identity]):
                    raise ValueError('Extraction container removal was not confirmed.')
            else:
                if name not in resources.get('diagnosed_images', []):
                    raise ValueError('Original ELF extraction/diagnostics did not complete; image preserved.')
                current = cost.run(['docker', 'image', 'inspect', '--format', '{{.Id}}', name])
                if current != identity:
                    raise ValueError('Image tag identity changed; preserved.')
                cost.run(['docker', 'image', 'rm', name])
                if cost.run(['docker', 'image', 'ls', '--quiet', name]):
                    raise ValueError('Image tag removal was not confirmed; current assignment preserved.')
            result['removed'].append({'kind': kind, 'name': name, 'id': identity})
        except Exception as error:
            result['errors'].append({'kind': kind, 'name': name, 'error': str(error)})
            result['status'] = 'blocked'
        write_json(directory / 'cleanup.json', result)
    for name, identity in resources['containers'].items():
        remove('container', name, identity)
    for name in resources.get('planned_images', []):
        if name not in resources['images']:
            try:
                if cost.run(['docker', 'image', 'ls', '--quiet', name]):
                    raise ValueError('Build left a tag without a recorded image identity; preserved.')
            except Exception as error:
                result['status'] = 'blocked'
                result['errors'].append({'kind': 'image', 'name': name, 'error': str(error)})
    if resources.get('builder_created') and not resources['builder_id']:
        result['status'] = 'blocked'
        result['errors'].append({'kind': 'builder', 'name': resources['owner'], 'error': 'Created builder has no confirmed container identity; preserved.'})
    if resources['builder_id']:
        remove('builder', resources['owner'], resources['builder_id'])
    for name, identity in resources['images'].items():
        remove('image', name, identity)
    write_json(directory / 'cleanup.json', result)
    return result


def pair_archive_evidence(directory):
    """Keep unresolved prior attempts, including pre-pair per-variant journals."""
    shared = directory.parent / 'archive-tags.json'
    for variant in ('old', 'candidate'):
        previous = directory.parent / variant / 'archive-tags.json'
        if previous.exists() and not cost.archive_cleanup_confirmed(previous):
            if cost.archive_cleanup_confirmed(shared):
                records = cost.archive_cleanup_records(shared)
                records.append({'status': 'blocked', 'phase': 'prior-variant-journal',
                                'evidence': str(previous)})
                write_json(shared, records)
    return shared


def measure(old, candidate, revision, variant, directory, version, builder_image, deadline=None):
    if os.environ.get('GITHUB_ACTIONS') != 'true' or os.environ.get('RUNNER_ENVIRONMENT') != 'github-hosted':
        raise ValueError('Run this expensive comparison only in its GitHub-hosted workflow.')
    if not re.fullmatch(r'moby/buildkit@sha256:[0-9a-f]{64}', builder_image):
        raise ValueError('Both builders require the same resolved official BuildKit image digest.')
    directory.mkdir(parents=True, exist_ok=True)
    # Both sequential variants and resumed attempts share the same archive tag.
    # Keep its pending/blocked state outside either variant's report directory.
    archive_evidence = pair_archive_evidence(directory)
    # The workflow fixes this absolute deadline before setup, leaving ten
    # minutes of its 95-minute job for comparison and artifact upload.
    deadline = deadline if deadline is not None else time.time() + 80 * 60
    previous_deadline, previous_cleanup_deadline = cost.COMMAND_DEADLINE, cost.CLEANUP_DEADLINE
    cost.CLEANUP_DEADLINE = deadline
    cost.COMMAND_DEADLINE = deadline - 180  # Reserve cleanup even after a build timeout.
    owner = 'operator-measure-' + uuid.uuid4().hex
    full_image = 'quazonai-operator-measure:' + owner + '-full'
    base_image = 'quazonai-operator-measure:' + owner + '-base'
    resources = {'owner': owner, 'builder_image': builder_image, 'builder_id': None, 'builder_created': False, 'planned_images': [], 'diagnosed_images': [], 'images': {}, 'containers': {}}
    detail = {'variant': variant, 'old_revision': OLD_REVISION, 'candidate_revision': revision}
    result = {'schema_version': REPORT_SCHEMA, 'revision': OLD_REVISION if variant == 'old' else revision,
              'version': version, 'measurement_status': 'running', 'hosted_comparison': detail}
    write_json(directory / 'report.json', result)
    write_json(directory / 'owned-resources.json', resources)
    try:
        if not math.isfinite(deadline):
            raise ValueError('Measurement deadline must be a finite absolute time.')
        precheck = cost.observe()
        docker_root = cost.run(['docker', 'info', '--format', '{{.DockerRootDir}}'])
        precheck['docker_root'] = docker_root
        precheck['docker_free_bytes'] = cost.shutil.disk_usage(docker_root).free
        precheck['required_free_bytes'] = MIN_FREE_BYTES
        write_json(directory / 'precheck.json', precheck)
        if min(precheck['disk_free_bytes'], precheck['docker_free_bytes']) < MIN_FREE_BYTES:
            raise ValueError('Blocked: at least 40 GB free is required before each variant; no global prune is permitted.')
        detail['harness'] = harness_identity()
        write_json(directory / 'harness.json', detail['harness'])
        if detail['harness']['revision'] != revision:
            raise ValueError('Final comparison harness must be the exact candidate revision.')
        recipes = verify_sources(old, candidate, revision)
        recipe = detail['recipe'] = recipes[0 if variant == 'old' else 1]
        write_json(directory / 'recipes.json', recipes)
        selected_revision = recipe['revision']
        source = old if variant == 'old' else candidate
        layout = 'standalone' if variant == 'old' else 'shared'
        environment = detail['environment'] = environment_identity(owner, builder_image)
        write_json(directory / 'environment.json', environment)
        for image in recipe['base_images']:
            cost.run(['docker', 'pull', '--platform', 'linux/amd64', image])
        environment['rustc'] = cost.run(['docker', 'run', '--rm', '--network', 'none', '--entrypoint',
                                        '/bin/sh', recipe['rust_image'], '-c', 'rustc -Vv && cargo -V'])
        # UUID names plus verified absence establish ownership before mutation.
        if owner in cost.run(['docker', 'buildx', 'ls', '--format', '{{.Name}}']).splitlines():
            raise ValueError('Disposable builder name already exists; preserved.')
        for image in (full_image, base_image):
            if cost.run(['docker', 'image', 'ls', '--quiet', image]):
                raise ValueError('Disposable image tag already exists; preserved.')
        resources['planned_images'] = [full_image, base_image]
        resources['builder_created'] = True
        write_json(directory / 'owned-resources.json', resources)
        cost.run(['docker', 'buildx', 'create', '--name', owner, '--driver', 'docker-container',
                  '--driver-opt', 'image=' + builder_image])
        environment['builder_inspect'] = cost.run(['docker', 'buildx', 'inspect', '--bootstrap', owner])
        container = json.loads(cost.run(['docker', 'container', 'inspect', 'buildx_buildkit_' + owner + '0']))[0]
        if not re.fullmatch(r'[0-9a-f]{64}', container['Id']) or container['Config']['Image'] != builder_image:
            raise ValueError('Disposable BuildKit container identity could not be established; preserved.')
        resources['builder_id'] = container['Id']
        write_json(directory / 'owned-resources.json', resources)
        environment['initial_cache'] = cost.run(['docker', 'buildx', 'du', '--builder', owner])
        environment['initial_cache_empty'] = cache_is_empty(environment['initial_cache'])
        write_json(directory / 'environment.json', environment)
        if environment['initial_cache_empty'] is not True:
            raise ValueError('Fresh builder did not prove an empty cache; blocked before building.')
        command = build_command(source, owner, full_image, selected_revision, version)
        write_json(directory / 'build-command.json', command)
        measured_build(directory, 'candidate', command)
        cold = cost.image_identity(full_image, selected_revision)
        resources['images'][full_image] = cold['id']
        write_json(directory / 'owned-resources.json', resources)
        detail['application_diagnostics'] = {'candidate': extract_application(directory / 'candidate-elf', cold['id'], owner, resources)}
        resources['diagnosed_images'].append(full_image)
        write_json(directory / 'owned-resources.json', resources)
        measured_build(directory, 'warm', command)
        if cost.image_identity(full_image, selected_revision)['id'] != cold['id']:
            raise ValueError('Warm replay changed the full image; original ELF evidence retained.')
        measured_build(directory, 'baseline', build_command(source, owner, base_image, selected_revision, version, 'application-base'))
        baseline = cost.image_identity(base_image, selected_revision)
        resources['images'][base_image] = baseline['id']
        write_json(directory / 'owned-resources.json', resources)
        detail['application_diagnostics']['baseline'] = extract_application(directory / 'baseline-elf', baseline['id'], owner, resources)
        resources['diagnosed_images'].append(base_image)
        write_json(directory / 'owned-resources.json', resources)
        result.update(cost.report(directory, base_image, full_image, selected_revision, version,
                                  source=source, layout=layout, emit=False, strict=False,
                                  cleanup_evidence=archive_evidence))
        result['schema_version'] = REPORT_SCHEMA
        result['hosted_comparison'] = detail
        for image_kind in ('baseline', 'candidate'):
            for binary, evidence in detail['application_diagnostics'][image_kind].items():
                if evidence['sha256'] != result[image_kind]['application_elf_sha256'][binary] or evidence['size_bytes'] != result[image_kind]['stripped_application_binary_bytes'][binary]:
                    raise ValueError('Retained ELF does not match the measured image: ' + image_kind + '/' + binary)
        detail.update({
            'independent_cold_full_build_seconds': elapsed(directory, 'candidate'),
            'same_builder_warm_full_build_seconds': elapsed(directory, 'warm'),
            'warm_observations': {phase: json.loads((directory / (phase + '.json')).read_text()) for phase in ('warm-start', 'warm-end')},
            'cold_scope': 'Sequential on one hosted runner, distinct empty docker-container builders; no cache imports. Timer includes builder base-image transfers. Shared host pulls and execution order remain timing caveats.',
            'warm_scope': 'Same revision and populated builder, layer-cache replay; retained producer duration is not a new compile duration.',
            'gha_layer_cache_only_replay': 'deferred to the separate cache and producer-identity batch; not measured',
            'actual_link_process_count': None,
            'link_count_scope': 'Not instrumented; no linker-count reduction claim.',
            'shipped_elf_count': len(result['payload']['stripped_binary_bytes']),
            'runtime_packages': cost.run(['docker', 'run', '--rm', '--network', 'none', '--read-only', '--entrypoint',
                                          '/usr/bin/dpkg-query', cold['id'], '-W', '-f', '${Package}\t${Version}\n'])})
        result['cache_conditions'] = [detail['cold_scope'], detail['warm_scope'],
            'Same-source no-operator baseline follows the full builds and reuses their layers.',
            'Disk observations are endpoint samples; disk and memory peaks are unknown. No speed claim.']
        result['measurement_status'] = 'blocked' if result['validation_errors'] else 'complete'
    except Exception as error:
        result['measurement_status'] = 'blocked'
        result['error'] = type(error).__name__ + ': ' + str(error)
    finally:
        for phase, field in (('candidate', 'independent_cold_full_build_seconds'), ('warm', 'same_builder_warm_full_build_seconds')):
            if all((directory / (phase + suffix + '.json')).exists() for suffix in ('-start', '-end')):
                detail[field] = elapsed(directory, phase)
        result['phase_observations'] = {path.stem: json.loads(path.read_text())
                                        for path in directory.glob('*-*.json')
                                        if path.stem.endswith(('-start', '-end'))}
        write_json(directory / 'report.json', result)
        cost.COMMAND_DEADLINE = deadline
        result['cleanup'] = cleanup_owned(directory, resources, archive_evidence)
        if result['cleanup']['status'] != 'complete':
            result['measurement_status'] = 'blocked'
        write_json(directory / 'report.json', result)
        cost.COMMAND_DEADLINE, cost.CLEANUP_DEADLINE = previous_deadline, previous_cleanup_deadline
    return result


def compare(old, candidate, revision):
    """Collect every failed gate before the caller writes evidence and exits."""
    reasons, observations = [], {}
    valid = {}
    reports = {'old': old, 'candidate': candidate}
    for side, report in reports.items():
        if isinstance(report, dict) and 'input_error' in report:
            reasons.append({'kind': 'input-error', 'field': side, 'detail': report['input_error'], 'path': report.get('path')})
    def value(side, path, predicate):
        key = side + '.' + path
        if key in valid:
            return observations[key] if valid[key] else None
        current = reports[side]
        try:
            for part in path.split('.'):
                current = current[part]
        except (KeyError, TypeError):
            current = None
            reasons.append({'kind': 'missing', 'field': key})
            valid[key] = False
        else:
            valid[key] = predicate(current)
            if not valid[key]:
                reasons.append({'kind': 'malformed', 'field': key, 'observed': current})
        observations[key] = current
        return current if valid[key] else None
    def text(value):
        return isinstance(value, str) and bool(value.strip())
    def sha(value):
        return isinstance(value, str) and re.fullmatch(r'[0-9a-f]{64}', value) is not None
    def commit(value):
        return isinstance(value, str) and re.fullmatch(r'[0-9a-f]{40}', value) is not None
    def integer(value):
        return type(value) is int and value > 0
    def seconds(value):
        try:
            return type(value) in (int, float) and math.isfinite(value) and value >= 0
        except OverflowError:
            # JSON integers may exceed float range; retain them as malformed
            # observations instead of losing the entire comparison report.
            return False
    def equal(path, predicate):
        left, right = [value(side, path, predicate) for side in reports]
        if left is not None and right is not None and left != right:
            reasons.append({'kind': 'mismatch', 'field': path, 'old': left, 'candidate': right})
        return left is not None and right is not None and left == right
    required_equal = {
        'version': text,
        'hosted_comparison.recipe.base_images': lambda x: isinstance(x, list) and bool(x) and all(isinstance(i, str) and re.fullmatch(r'[^\s@]+@sha256:[0-9a-f]{64}', i) for i in x),
        'hosted_comparison.recipe.rust_image': lambda x: isinstance(x, str) and re.fullmatch(r'rust:1.98.1-bookworm@sha256:[0-9a-f]{64}', x) is not None,
        'hosted_comparison.recipe.platform': lambda x: x == 'linux/amd64',
        'hosted_comparison.recipe.common_recipe_sha256': sha,
        'hosted_comparison.harness.revision': commit,
        'hosted_comparison.harness.tree': commit,
        'hosted_comparison.runtime_packages': text,
    }
    required_equal['hosted_comparison.recipe.locked_inputs_sha256'] = lambda x: isinstance(x, dict) and set(x) == set(LOCKED_INPUTS) and all(sha(v) for v in x.values())
    required_equal['hosted_comparison.recipe.common_inputs_sha256'] = lambda x: isinstance(x, dict) and set(x) == {'deploy/docker/native-inputs.mjs', '.dockerignore', 'deploy/docker/Dockerfile.dockerignore'} and all(sha(v) for v in x.values())
    required_equal['hosted_comparison.harness.file_sha256'] = lambda x: isinstance(x, dict) and set(x) == set(HARNESS_FILES) and all(sha(v) for v in x.values())
    env = 'hosted_comparison.environment.'
    for name in ('buildx_version', 'rustc', 'runner_os', 'runner_arch', 'memory', 'docker_version',
                 'runner_image', 'runner_image_version', 'runner_name', 'run_id', 'run_attempt'):
        required_equal[env + name] = text
    required_equal[env + 'runner_boot_id'] = lambda x: isinstance(x, str) and re.fullmatch(r'[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}', x) is not None
    required_equal[env + 'builder_image'] = lambda x: isinstance(x, str) and re.fullmatch(r'moby/buildkit@sha256:[0-9a-f]{64}', x) is not None
    required_equal[env + 'cpu_count'] = integer
    required_equal[env + 'cpu_models'] = lambda x: isinstance(x, list) and bool(x) and all(text(i) for i in x)
    required_equal[env + 'containerd'] = lambda x: isinstance(x, dict) and x.get('Name') == 'containerd' and text(x.get('Version')) and isinstance(x.get('Details'), dict) and text(x['Details'].get('GitCommit'))
    for path, predicate in required_equal.items():
        equal(path, predicate)
    if not commit(revision) or revision == OLD_REVISION:
        reasons.append({'kind': 'invalid-request', 'field': 'candidate_revision', 'observed': revision})
    for side, expected in (('old', OLD_REVISION), ('candidate', revision)):
        layout = 'standalone' if side == 'old' else 'shared'
        value(side, 'schema_version', lambda x: type(x) is int and x == REPORT_SCHEMA)
        value(side, 'validation_errors', lambda x: isinstance(x, list) and not x)
        value(side, 'hosted_comparison.recipe.tree', commit)
        value(side, 'hosted_comparison.recipe.native_build_sha256', sha)
        value(side, 'payload.layout', lambda x: x == layout)
        for path, wanted in (('revision', expected), ('hosted_comparison.variant', side),
                             ('hosted_comparison.recipe.revision', expected),
                             ('layout', layout), ('hosted_comparison.recipe.layout', layout),
                             ('hosted_comparison.harness.revision', revision),
                             ('hosted_comparison.old_revision', OLD_REVISION),
                             ('hosted_comparison.candidate_revision', revision),
                             ('measurement_status', 'complete'), ('cleanup.status', 'complete')):
            observed = value(side, path, text)
            if observed is not None and observed != wanted:
                reasons.append({'kind': 'unexpected', 'field': side + '.' + path, 'expected': wanted, 'observed': observed})
        if side == 'candidate':
            source_tree = value(side, 'hosted_comparison.recipe.tree', commit)
            harness_tree = value(side, 'hosted_comparison.harness.tree', commit)
            if source_tree is not None and harness_tree is not None and source_tree != harness_tree:
                reasons.append({'kind': 'harness-tree-mismatch', 'field': side + '.hosted_comparison.recipe.tree',
                                'source': source_tree, 'harness': harness_tree})
        bins = '--bin catalog-prepare --bin polymarket-history' if side == 'old' else '--bin source-tools'
        value(side, 'hosted_comparison.recipe.operator_build_command', lambda x: x == OPERATOR_RECIPE + ' ' + bins)
        value(side, env + 'initial_cache_empty', lambda x: x is True)
        value(side, env + 'initial_cache', lambda x: isinstance(x, str) and cache_is_empty(x))
        # Source identities are recomputed independently for B and C. Approved
        # relocation/recipe changes can alter both digests, but each producer and
        # measured source must match its own preflight, never the other variant.
        value(side, 'payload.native_build.schema_version', lambda x: type(x) is int and x == 2)
        for field, predicate in (('input_sha256', sha), ('recipe_sha256', sha),
                                 ('platform', lambda x: x == 'linux/amd64')):
            observed = [value(side, prefix + field, predicate) for prefix in (
                'hosted_comparison.recipe.native_identity.', 'source_native_identity.', 'payload.native_build.')]
            if all(item is not None for item in observed) and len(set(observed)) != 1:
                reasons.append({'kind': 'native-source-mismatch', 'field': side + '.' + field,
                                'preflight': observed[0], 'measured_source': observed[1], 'producer': observed[2]})
        names = {'server', 'runtime'} | ({'catalog-prepare', 'polymarket-history'} if side == 'old' else {'source-tools'})
        hashes = value(side, 'payload.elf_sha256', lambda x: isinstance(x, dict) and set(x) == names and all(sha(v) for v in x.values()))
        producer = value(side, 'payload.native_build.elf_sha256', lambda x: isinstance(x, dict) and set(x) == names and all(sha(v) for v in x.values()))
        if hashes is not None and producer is not None and hashes != producer:
            reasons.append({'kind': 'producer-elf-mismatch', 'field': side + '.payload.elf_sha256',
                            'measured': hashes, 'producer': producer})
        for binary in ('server', 'runtime'):
            actual = value(side, 'candidate.application_elf_sha256.' + binary, sha)
            if hashes is not None and actual is not None and hashes[binary] != actual:
                reasons.append({'kind': 'payload-application-mismatch', 'field': side + '.payload.elf_sha256.' + binary,
                                'payload': hashes[binary], 'application': actual})
        for field in ('original_native_build_elapsed_seconds', 'original_disk_before_bytes', 'original_disk_after_bytes'):
            value(side, 'payload.native_build.' + field, lambda x: type(x) is int and x >= 0)
        for image in ('baseline', 'candidate'):
            value(side, image + '.compressed_archive_bytes', lambda x: integer(x) and x < cost.MAX_ARCHIVE)
    builders = [value(side, env + 'builder_name', text) for side in reports]
    if None not in builders and builders[0] == builders[1]:
        reasons.append({'kind': 'shared-builder', 'field': env + 'builder_name', 'observed': builders[0]})
    identical = True
    for binary in ('server', 'runtime'):
        for measure, predicate in (('application_elf_sha256', sha), ('stripped_application_binary_bytes', integer)):
            identical = equal('candidate.' + measure + '.' + binary, predicate) and identical
            for side in reports:
                full = value(side, 'candidate.' + measure + '.' + binary, predicate)
                base = value(side, 'baseline.' + measure + '.' + binary, predicate)
                if full is not None and base is not None and full != base:
                    reasons.append({'kind': 'baseline-binary-mismatch', 'field': side + '.' + measure + '.' + binary, 'full': full, 'baseline': base})
                if full is None or base is None or full != base:
                    identical = False
    if not identical:
        reasons.append({'kind': 'application-identity', 'field': 'application_binaries_identical',
                        'detail': 'Missing or unequal original binary identity blocks admission pending causal diagnosis; hashes are not normalized.'})
    deltas = {}
    for name, path in (('full_image_bytes', 'candidate.size_bytes'),
                       ('full_compressed_archive_bytes', 'candidate.compressed_archive_bytes'),
                       ('operator_payload_bytes', 'payload.operator_payload_bytes')):
        left, right = [value(side, path, integer) for side in reports]
        deltas[name] = right - left if left is not None and right is not None else None
    for name, field in (('stripped_elf_bytes', 'stripped_binary_bytes'), ('launcher_bytes', 'launcher_bytes')):
        totals = []
        for side in reports:
            expected = ({'catalog-prepare', 'polymarket-history'} if side == 'old' else {'source-tools'}) if name == 'stripped_elf_bytes' else (set() if side == 'old' else {'catalog-prepare', 'polymarket-history'})
            values = value(side, 'payload.' + field, lambda x: isinstance(x, dict) and set(x) == expected and all(integer(i) for i in x.values()))
            totals.append(sum(values.values()) if values is not None else None)
        deltas[name] = totals[1] - totals[0] if None not in totals else None
    reductions = True
    for name in ('full_image_bytes', 'full_compressed_archive_bytes', 'operator_payload_bytes', 'stripped_elf_bytes'):
        if deltas[name] is None or deltas[name] >= 0:
            reductions = False
            reasons.append({'kind': 'size-gate', 'field': name, 'delta': deltas[name]})
    timings = {}
    for name in ('independent_cold_full_build_seconds', 'same_builder_warm_full_build_seconds'):
        left, right = [value(side, 'hosted_comparison.' + name, seconds) for side in reports]
        timings[name] = {'old': left, 'candidate': right, 'delta': right - left if None not in (left, right) else None}
    return {'schema_version': REPORT_SCHEMA, 'scope': 'pinned equivalent-capability two-ELF full image versus integrated shared-ELF full image; hosted actual bytes',
            'old_revision': OLD_REVISION, 'candidate_revision': revision, 'delta': deltas, 'build_seconds': timings,
            'application_binaries_identical': identical, 'required_size_reductions_observed': reductions,
            'admissible': not reasons, 'reasons': reasons, 'observations': observations,
            'required_equal_fields': list(required_equal) + ['candidate.application_elf_sha256.{server,runtime}', 'candidate.stripped_application_binary_bytes.{server,runtime}'],
            'diagnostic_only_fields': ['builder_inspect', 'ELF section/program-header diagnostics',
                                       'disk endpoint samples', 'build timings', 'dockerfile_sha256'],
            'timing_interpretation': 'Raw timings retained even when incomparable. Sequential order and host prerequisite cache remain caveats; no speed claim. Memory and disk peaks are unknown.',
            'old': old, 'candidate': candidate,
            'admission': 'Binary identity and size gates are required; behavior, scientific isolation, final-head CI and independent review remain separate requirements.'}


def read_report(path):
    try:
        raw = path.read_text()
    except (OSError, UnicodeError) as error:
        return {'input_error': str(error), 'path': str(path)}
    def finite_number(token):
        value = float(token)
        if not math.isfinite(value):
            raise ValueError('Non-finite JSON number: ' + token)
        return value
    try:
        return json.loads(raw, parse_float=finite_number, parse_constant=finite_number)
    except (ValueError, TypeError) as error:
        return {'input_error': str(error), 'path': str(path), 'original_text': raw}


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
    measurement.add_argument('--deadline', type=float, required=True)
    verification = subcommands.add_parser('verify-sources')
    verification.add_argument('--old-source', type=Path, required=True)
    verification.add_argument('--candidate-source', type=Path, required=True)
    verification.add_argument('--revision', required=True)
    verification.add_argument('--output', type=Path, required=True)
    comparison = subcommands.add_parser('compare')
    comparison.add_argument('--old', type=Path, required=True)
    comparison.add_argument('--candidate', type=Path, required=True)
    comparison.add_argument('--revision', required=True)
    comparison.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.command == 'verify-sources':
        result = {'schema_version': REPORT_SCHEMA, 'status': 'blocked'}
        try:
            result['harness'] = harness_identity()
            if result['harness']['revision'] != args.revision:
                raise ValueError('Final comparison harness must be the exact candidate revision.')
            result['recipes'] = verify_sources(args.old_source.resolve(), args.candidate_source.resolve(), args.revision)
            result['status'] = 'complete'
        except Exception as error:
            result['error'] = type(error).__name__ + ': ' + str(error)
        write_json(args.output, result)
        if result['status'] != 'complete':
            parser.exit(1, 'Source comparison blocked before building; see preflight report.\n')
    elif args.command == 'measure':
        result = measure(args.old_source.resolve(), args.candidate_source.resolve(), args.revision, args.variant,
                         args.directory.resolve(), args.version, args.builder_image, args.deadline)
        if result['measurement_status'] != 'complete':
            parser.exit(1, 'Measurement blocked; see retained report, original binaries, logs and cleanup evidence.\n')
    else:
        result = compare(read_report(args.old), read_report(args.candidate), args.revision)
        write_json(args.output, result)
        print(json.dumps({key: value for key, value in result.items() if key not in ('old', 'candidate', 'observations')}, indent=2))
        if not result['admissible']:
            parser.exit(1, 'Comparison invalid or inconclusive; all observations and reasons retained in comparison JSON.\n')


if __name__ == '__main__':
    main()
