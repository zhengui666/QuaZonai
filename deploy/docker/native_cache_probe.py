#!/usr/bin/env python3
"""One bounded same-builder warm build; never substitute it for acceptance."""
import argparse
from datetime import datetime
import fnmatch
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import shlex
import shutil
import signal
import subprocess
import tarfile
import tempfile
import time
import uuid

import operator_cost

LOG_LIMIT = 2 * 1024 * 1024
OWNER = 'io.quazonai.native-cache-probe'


def command(args, deadline, *, log=None, limit=LOG_LIMIT):
    """Bound output and the entire process group, retaining raw build evidence."""
    if time.monotonic() >= deadline:
        raise TimeoutError('Probe command budget exhausted')
    output = bytearray()
    with subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                          start_new_session=True) as process:
        finished = False
        try:
            with selectors.DefaultSelector() as selector:
                selector.register(process.stdout, selectors.EVENT_READ)
                while selector.get_map():
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise TimeoutError('Probe command timed out')
                    for key, _ in selector.select(min(remaining, 0.25)):
                        chunk = os.read(key.fileobj.fileno(), 65536)
                        if not chunk:
                            selector.unregister(key.fileobj)
                        elif len(output) + len(chunk) > limit:
                            output.extend(chunk[:limit - len(output)])
                            raise ValueError('Probe output exceeded its bounded evidence limit')
                        else:
                            output.extend(chunk)
            status = process.wait(timeout=max(0.01, deadline - time.monotonic()))
            if status != 0:
                raise ValueError('Probe command failed with exit ' + str(status))
            result = bytes(output).decode()
            finished = True
            return result
        finally:
            try:
                if not finished or process.poll() is None:
                    # The group can outlive its leader while a descendant holds
                    # stdout. Both signals address only this new owned session.
                    try:
                        os.killpg(process.pid, signal.SIGINT)
                    except ProcessLookupError:
                        pass
                    try:
                        process.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        pass
                    # Always escalate any surviving group, including after its
                    # leader exited on SIGINT. This says nothing about BuildKit.
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    process.wait(timeout=2)
            finally:
                if log is not None:
                    log.write_bytes(output)


def cached_evidence(raw):
    vertices = {}
    for line in raw.splitlines():
        event = json.loads(line)
        if not isinstance(event, dict) or not set(event) <= {'vertexes', 'statuses', 'logs', 'warnings'}:
            raise ValueError('Malformed BuildKit rawjson event')
        if any(not isinstance(value, list) for value in event.values()):
            raise ValueError('Malformed BuildKit rawjson collections')
        for vertex in event.get('vertexes', []):
            if not isinstance(vertex, dict):
                raise ValueError('Malformed BuildKit vertex')
            digest = vertex.get('digest', '')
            if not re.fullmatch(r'sha256:[0-9a-f]{64}', digest):
                raise ValueError('Malformed BuildKit vertex identity')
            if digest in vertices and vertices[digest].get('name') != vertex.get('name'):
                raise ValueError('BuildKit vertex changed name')
            if vertex.get('error'):
                raise ValueError('BuildKit vertex reported an error')
            previous = vertices.get(digest)
            if (previous and previous.get('started') and not vertex.get('started')
                    and not vertex.get('completed') and vertex.get('cached', False) is False):
                # BuildKit can repeat an initial vertex after a later state.
                # Ignore only that initial record, never a new unfinished run.
                continue
            vertices[digest] = vertex
    matched = {}
    commands = {'native-inputs': 'node deploy/docker/native-inputs.mjs prepare ',
                'server': 'sh deploy/docker/native-build.sh server',
                'operator': 'sh deploy/docker/native-build.sh operator'}
    for stage, recipe in commands.items():
        selected = [value for value in vertices.values()
                    if re.match(r'^\[(?:linux/amd64 )?' + stage + r' \d+/\d+\] RUN ', value.get('name', ''))
                    and recipe in value['name']]
        if len(selected) != 1:
            raise ValueError('Missing or ambiguous BuildKit RUN evidence for ' + stage)
        vertex = selected[0]
        if (vertex.get('error') or not vertex.get('started') or not vertex.get('completed')
                or datetime.fromisoformat(vertex['completed']) < datetime.fromisoformat(vertex['started'])):
            raise ValueError('Incomplete BuildKit RUN evidence for ' + stage)
        cached = vertex.get('cached', False)
        if type(cached) is not bool or cached != (stage != 'native-inputs'):
            raise ValueError('Expected collector execution and both native RUN vertices CACHED: ' + stage)
        matched[stage] = vertex
    return matched


def add_noise(source, token):
    name = 'skills/QZ_NATIVE_CACHE_PROBE_' + token + '.md'
    # This directory is admitted by the reviewed Docker context rules, but it is
    # outside native inputs and every current final-image COPY source.
    # The existing Dockerfile uses simple line-based COPY instructions only.
    stage = None
    for line in (source / 'deploy/docker/Dockerfile').read_text().splitlines():
        if line.startswith('FROM '):
            stage = line.split()[-1]
        if line.startswith('COPY ') and stage != 'native-inputs' and '--from=' not in line:
            tokens = [word for word in shlex.split(line)[1:] if not word.startswith('--')]
            if len(tokens) < 2 or any(word.startswith('[') for word in tokens):
                raise ValueError('Review changed Docker COPY syntax before probing')
            patterns = [word.removeprefix('./').rstrip('/') for word in tokens[:-1]]
            if any(pattern in ('', '.') or fnmatch.fnmatchcase(name, pattern)
                   or name.startswith(pattern + '/') for pattern in patterns):
                raise ValueError('Probe noise would enter final image inputs')
    body = ('Same-builder native cache probe only; not product source.\n' + token + '\n').encode()
    with (source / name).open('xb') as stream:
        stream.write(body)
    return {'path': name, 'sha256': hashlib.sha256(body).hexdigest(),
            'reason': 'Unique admitted skills document: outside the native closure and every non-collector context COPY source.'}


SNAPSHOT = """import hashlib,json
from pathlib import Path
root=Path('/opt/quazonai/operator')
files={name:root/'bin'/name for name in ('catalog-prepare','polymarket-history')}
files.update({name:Path('/opt/quazonai/bin')/name for name in ('server','runtime')})
hashes={}
for name,path in files.items():
    with path.open('rb') as stream:
        if stream.read(4)!=bytes([127,69,76,70]): raise ValueError('Not ELF: '+name)
        stream.seek(0)
        hashes[name]=hashlib.file_digest(stream,'sha256').hexdigest()
print(json.dumps({'elf_sha256':hashes,'stripped_binary_bytes':{name:files[name].stat().st_size for name in ('catalog-prepare','polymarket-history')},'native_build':json.loads((root/'build-metrics.json').read_text())}))
"""


def image_info(image, revision, version, deadline):
    value = json.loads(command(['docker', 'image', 'inspect', image], deadline))[0]
    labels = value['Config']['Labels']
    if (labels.get('org.opencontainers.image.revision') != revision
            or labels.get('org.opencontainers.image.version') != version
            or (value.get('Os'), value.get('Architecture')) != ('linux', 'amd64')
            or not re.fullmatch(r'sha256:[0-9a-f]{64}', value['Id'])):
        raise ValueError('Probe image packaging identity does not match actual HEAD/version/platform')
    return value['Id']


def snapshot(image, owned, token, containers, deadline):
    cidfile = owned / ('container-' + str(len(containers)) + '.id')
    containers.append((cidfile, image))
    command(['docker', 'create', '--cidfile', str(cidfile), '--label', OWNER + '=' + token,
             '--network', 'none', '--read-only', '--no-healthcheck', '--cap-drop', 'ALL',
             '--security-opt', 'no-new-privileges:true', '--entrypoint', '/usr/bin/python3',
             image, '-E', '-s', '-B', '-c', SNAPSHOT], deadline)
    container = cidfile.read_text().strip()
    if not re.fullmatch(r'[0-9a-f]{64}', container):
        raise ValueError('Invalid owned container identity')
    result = json.loads(command(['docker', 'start', '--attach', container], deadline))
    state = json.loads(command(['docker', 'container', 'inspect', container], deadline))[0]['State']
    if state['Running'] or state['ExitCode'] != 0:
        raise ValueError('Probe measurement container did not complete successfully')
    return result


def compare_snapshots(before, after, expected):
    for snapshot in (before, after):
        operator_cost.validate_native_build(snapshot, expected,
            {name: snapshot['elf_sha256'][name] for name in ('server', 'runtime')})
    if before != after:
        raise ValueError('Warm probe changed actual ELF bytes, sizes or original native producer records')


def tag_identity(tag, deadline):
    value = command(['docker', 'image', 'ls', '--no-trunc', '--quiet', tag], deadline).splitlines()
    if len(value) > 1 or any(not re.fullmatch(r'sha256:[0-9a-f]{64}', item) for item in value):
        raise ValueError('Ambiguous probe tag identity')
    return value[0] if value else None


def cleanup(owned, token, containers, tag, image, build_state, deadline):
    errors = []
    for cidfile, expected_image in containers:
        try:
            container = cidfile.read_text().strip()
            if not re.fullmatch(r'[0-9a-f]{64}', container):
                raise ValueError('Unconfirmed probe container identity')
            value = json.loads(command(['docker', 'container', 'inspect', container], deadline))[0]
            if value['Config']['Labels'].get(OWNER) != token or value['Image'] != expected_image:
                raise ValueError('Container ownership changed; preserved')
            command(['docker', 'container', 'rm', '--force', container], deadline)
        except Exception as error:
            errors.append(type(error).__name__ + ': ' + str(error))
    if image:
        try:
            current = tag_identity(tag, deadline)
            if current != image:
                raise ValueError('Probe tag changed; current assignment preserved')
            command(['docker', 'image', 'rm', tag], deadline)
            if tag_identity(tag, deadline) is not None:
                raise ValueError('Probe tag cleanup not confirmed')
        except Exception as error:
            errors.append(type(error).__name__ + ': ' + str(error))
    if build_state == 'completed' and image is None:
        errors.append('Completed build has no verified output identity; possible probe tag preserved')
    if build_state == 'uncertain':
        errors.append('Owned BuildKit solve termination is unconfirmed after client failure; builder was not stopped or pruned. Context/tag retained for diagnosis.')
    if not errors:
        try:
            shutil.rmtree(owned)
        except Exception as error:
            errors.append(type(error).__name__ + ': ' + str(error))
    return errors


def probe(args):
    args.directory.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    deadline = started + max(0, min(236, args.deadline - time.time() - 4))
    work_deadline = deadline - 30  # Dedicated cleanup reserve; never extend the job timeout.
    token = uuid.uuid4().hex
    tag = 'quazonai-native-cache-probe:' + token
    owned = Path(tempfile.mkdtemp(prefix='quazonai-native-cache-probe-'))
    containers, probe_image, build_state = [], None, 'not-started'
    result = {'schema_version': 1, 'revision': args.revision, 'version': args.version,
              'builder': args.builder, 'tag': tag, 'status': 'running', 'execution_status': 'running',
              'scope': 'same-builder warm reuse only; not new-runner GHA restoration or whole-PR five-minute proof',
              'build_limit_seconds': 180, 'total_limit_seconds': 240,
              'cleanup_reserve_seconds': 30, 'job_admission_deadline': args.deadline}
    save = lambda: (args.directory / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
    save()
    try:
        if work_deadline - time.monotonic() < 45:
            result['execution_status'] = 'blocked'
            raise TimeoutError('Insufficient job budget for probe plus owned cleanup; no build attempted')
        source = args.source.resolve()
        if command(['git', '-C', str(source), 'rev-parse', 'HEAD'], work_deadline).strip() != args.revision:
            raise ValueError('Probe requires the actual exact checked-out HEAD')
        command(['git', '-C', str(source), 'diff', '--quiet', 'HEAD', '--'], work_deadline)
        if tag_identity(tag, work_deadline) is not None:
            raise ValueError('Probe tag already exists; preserved without building')
        nodes = json.loads(args.builder_nodes)
        if args.builder_driver != 'docker-container' or not isinstance(nodes, list) or len(nodes) != 1:
            raise ValueError('Probe requires this job’s single-node setup-buildx docker-container builder')
        builder_info = command(['docker', 'buildx', 'inspect', args.builder], work_deadline)
        names = re.findall(r'^Name:\s+(\S+)\s*$', builder_info, re.MULTILINE)
        if names != [args.builder, nodes[0]['name']] or not re.search(r'^Driver:\s+docker-container\s*$', builder_info, re.MULTILINE):
            raise ValueError('Actual builder identity differs from this job’s setup-buildx outputs')
        result['setup_builder'] = {'name': args.builder, 'driver': args.builder_driver, 'nodes': nodes}
        (args.directory / 'builder-inspect.txt').write_text(builder_info)
        (args.directory / 'buildx-version.txt').write_text(command(['docker', 'buildx', 'version'], work_deadline))
        original_image = image_info(args.candidate, args.revision, args.version, work_deadline)
        result['original_image'] = original_image
        archive, context = owned / 'source.tar', owned / 'source'
        context.mkdir()
        command(['git', '-C', str(source), 'archive', '--format=tar', '--output=' + str(archive), args.revision], work_deadline)
        with tarfile.open(archive) as contents:
            for member in contents:
                if time.monotonic() >= work_deadline:
                    raise TimeoutError('Exact-HEAD archive extraction exceeded probe budget')
                contents.extract(member, context, filter='data')
        def identity(root):
            return json.loads(command(['node', str(root / 'deploy/docker/native-inputs.mjs'),
                                       'identity', str(root), 'linux/amd64'], work_deadline))
        expected = identity(source)
        if identity(context) != expected:
            raise ValueError('Exact HEAD archive differs from actual candidate native inputs')
        result['noise'] = add_noise(context, token)
        if identity(context) != expected:
            raise ValueError('Documentation noise changed native inputs')
        result['native_identity'] = {key: expected[key] for key in ('input_sha256', 'recipe_sha256', 'platform')}
        result['before'] = snapshot(original_image, owned, token, containers, work_deadline)
        save()
        build_deadline = min(work_deadline - 20, time.monotonic() + 180)
        if build_deadline - time.monotonic() < 15:
            result['execution_status'] = 'blocked'
            raise TimeoutError('Insufficient build/verification budget; no probe build attempted')
        build_state = 'uncertain'
        build_started = time.monotonic()
        command(['docker', 'buildx', 'build', '--builder', args.builder, '--progress=rawjson',
                 '--platform', 'linux/amd64', '--load', '--tag', tag,
                 '--metadata-file', str(args.directory / 'build-metadata.json'),
                 '--file', str(context / 'deploy/docker/Dockerfile'),
                 '--build-arg', 'VERSION=' + args.version, '--build-arg', 'REVISION=' + args.revision,
                 str(context)], build_deadline, log=args.directory / 'build-progress.jsonl')
        build_state = 'completed'
        result['build_elapsed_seconds'] = time.monotonic() - build_started
        metadata = json.loads((args.directory / 'build-metadata.json').read_text())
        probe_image = metadata['containerimage.config.digest']
        if not re.fullmatch(r'sha256:[0-9a-f]{64}', probe_image):
            probe_image = None
            raise ValueError('Invalid probe output image identity')
        result['probe_image'] = image_info(tag, args.revision, args.version, work_deadline)
        if result['probe_image'] != probe_image:
            raise ValueError('Probe tag differs from the build output; preserved')
        result['vertices'] = cached_evidence((args.directory / 'build-progress.jsonl').read_text())
        result['after'] = snapshot(probe_image, owned, token, containers, work_deadline)
        compare_snapshots(result['before'], result['after'], expected)
        if image_info(args.candidate, args.revision, args.version, work_deadline) != original_image:
            raise ValueError('Original candidate tag changed during probe')
        result['execution_status'] = 'passed'
    except Exception as error:
        if result['execution_status'] != 'blocked':
            result['execution_status'] = 'failed'
        result['error'] = type(error).__name__ + ': ' + str(error)
    finally:
        result['build_state'] = build_state
        result['execution_elapsed_seconds'] = time.monotonic() - started
        save()
        result['cleanup_errors'] = cleanup(owned, token, containers, tag, probe_image, build_state, deadline)
        result['cleanup_status'] = 'failed' if result['cleanup_errors'] else 'passed'
        result['status'] = 'passed' if result['execution_status'] == 'passed' and not result['cleanup_errors'] else 'failed'
        result['total_elapsed_seconds'] = time.monotonic() - started
        if owned.exists():
            result['retained_owned_context'] = str(owned)
        save()
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=Path.cwd())
    for name in ('candidate', 'revision', 'version', 'builder', 'builder-driver', 'builder-nodes'):
        parser.add_argument('--' + name, required=True)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--deadline', type=float, required=True, help='Probe-only epoch deadline; real acceptance is never gated by it')
    args = parser.parse_args()
    result = probe(args)
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if result['status'] == 'passed' else 1)


if __name__ == '__main__':
    main()
