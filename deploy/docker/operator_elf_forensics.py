#!/usr/bin/env python3
"""Bounded, read-only evidence for the original production server link outputs."""
import hashlib
import base64
import itertools
import json
from pathlib import Path
import re
import resource
import stat
import subprocess

import operator_cost as cost


MIB = 1024 * 1024
MAX_EXTRA_BYTES = 1024 * MIB
MAX_SYMBOL_BYTES = 64 * MIB
MAX_REPORT_BYTES = 8 * MIB
MAX_REPETITION_BYTES = (MAX_EXTRA_BYTES - MAX_REPORT_BYTES) // 2
MAX_RAW_BYTES = MAX_REPETITION_BYTES - 2 * MAX_SYMBOL_BYTES - MAX_REPORT_BYTES
MAX_SYMBOL_PREVIEW_BYTES = 1024
MAX_SYMBOL_DIFF_BYTES = 512 * 1024
FILES = ('capture-status.txt', 'producer-tools.txt', 'server', 'runtime',
         'server-symbols.txt', 'runtime-symbols.txt')
# The original Dockerfile is an exact byte prefix. Nothing below changes its
# server RUN, native helper, source closure, compiler flags or environment.
SUFFIX = ('''
# DIAGNOSTIC_ONLY: observe the completed production server stage without compiling.
FROM server AS application-forensics
RUN --mount=type=cache,target=/build/target,readonly \\
    set -eu; mkdir /application-forensics; \\
    printf '%s\\n' running > /application-forensics/capture-status.txt; \\
    if test ! -f /build/target/release/server || test -L /build/target/release/server \\
       || test ! -f /build/target/release/runtime || test -L /build/target/release/runtime; then \\
      printf '%s\\n' blocked-missing-regular-originals > /application-forensics/capture-status.txt; \\
    else \\
      total=$(($(stat -c %s /build/target/release/server) + $(stat -c %s /build/target/release/runtime))); \\
      if test "$total" -gt RAW_LIMIT; then \\
        printf '%s\\n' blocked-original-byte-budget > /application-forensics/capture-status.txt; \\
      else \\
        cp /build/target/release/server /application-forensics/server; \\
        cp /build/target/release/runtime /application-forensics/runtime; \\
        printf '%s\\n' complete > /application-forensics/capture-status.txt; \\
        for binary in server runtime; do \\
          if ! (ulimit -c 0; ulimit -f 65536; readelf --wide --symbols /application-forensics/$binary > /application-forensics/$binary-symbols.txt); then \\
            printf '%s\\n' blocked-symbol-output-budget > /application-forensics/capture-status.txt; \\
          fi; \\
        done; \\
      fi; \\
    fi; \\
    (ulimit -c 0; ulimit -f 1024; rustc -Vv; cargo -V; strip --version; readelf --version; \\
      printf '%s\\n' 'Available system linker (actual linker argv unavailable):'; ld --version) \\
      > /application-forensics/producer-tools.txt; \\
    cd /application-forensics; \\
    for file in capture-status.txt producer-tools.txt server runtime server-symbols.txt runtime-symbols.txt; do \\
      if test -f "$file"; then printf '%s %s\\n' "$(stat -c %s "$file")" "$file"; fi; \\
    done > sizes.txt
'''.replace('RAW_LIMIT', str(MAX_RAW_BYTES))).encode()


def prepare_dockerfile(source, directory):
    directory.mkdir(parents=True, exist_ok=True)
    original = (source / 'deploy/docker/Dockerfile').read_bytes()
    ignore = (source / 'deploy/docker/Dockerfile.dockerignore').read_bytes()
    path = directory / 'forensic.Dockerfile'
    # Exclusive creation prevents accidental reuse of a changed diagnostic recipe.
    with path.open('xb') as output:
        output.write(original + SUFFIX)
    with Path(str(path) + '.dockerignore').open('xb') as output:
        output.write(ignore)
    digest = lambda body: hashlib.sha256(body).hexdigest()
    return path, {'qualification': 'DIAGNOSTIC_ONLY', 'production_prefix_bytes': len(original),
                  'production_dockerfile_sha256': digest(original), 'capture_suffix_sha256': digest(SUFFIX),
                  'executed_dockerfile_sha256': digest(original + SUFFIX),
                  'context_rules_sha256': digest(ignore), 'target': 'application-forensics',
                  'original_command': 'cargo build --locked --release -p server -p runtime',
                  'linker_argv': {'status': 'unavailable', 'reason': 'No instrumentation was added to the production compilation.'}}


def extract_originals(container, directory):
    """Check the producer's fixed allowlist and total before any large copy."""
    directory.mkdir(parents=True, exist_ok=True)
    inventory = directory / 'sizes.txt'
    cost.run(['docker', 'cp', container + ':/application-forensics/sizes.txt', str(inventory)])
    if inventory.stat().st_size > 4096:
        raise ValueError('Forensic inventory exceeds its fixed bound.')
    sizes = {}
    for line in inventory.read_text().splitlines():
        match = re.fullmatch(r'([0-9]+) ([a-z-]+(?:\.txt)?)', line)
        if not match or match[2] not in FILES or match[2] in sizes:
            raise ValueError('Malformed or unexpected forensic inventory member.')
        sizes[match[2]] = int(match[1])
    limits = {'server': MAX_RAW_BYTES, 'runtime': MAX_RAW_BYTES,
              'server-symbols.txt': MAX_SYMBOL_BYTES, 'runtime-symbols.txt': MAX_SYMBOL_BYTES,
              'capture-status.txt': 1024, 'producer-tools.txt': MIB}
    if sum(sizes.values()) + 4096 > MAX_REPETITION_BYTES or any(size > limits[name] for name, size in sizes.items()):
        raise ValueError('Forensic evidence exceeds the per-repetition bound before copying.')
    for name, size in sizes.items():
        path = directory / name
        cost.run(['docker', 'cp', container + ':/application-forensics/' + name, str(path)])
        if not stat.S_ISREG(path.lstat().st_mode) or path.stat().st_size != size:
            raise ValueError('Forensic evidence is not an exact regular original: ' + name)
    status = (directory / 'capture-status.txt').read_text().strip()
    if status != 'complete' or set(sizes) != set(FILES):
        raise ValueError('Forensic capture blocked; retained available originals: ' + status)
    return {'status': 'complete', 'extra_bytes': sum(sizes.values()) + inventory.stat().st_size,
            'maximum_extra_bytes': MAX_REPETITION_BYTES, 'original_sizes': sizes}


def bounded_readelf(binary, output):
    # Enforce the bound in the child before GNU readelf can grow its output.
    def limit_output():
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        resource.setrlimit(resource.RLIMIT_FSIZE, (MIB, MIB))
    with output.open('xb') as stream:
        subprocess.run(['readelf', '--wide', '--file-header', '--program-headers', '--section-headers',
                        '--notes', '--string-dump=.comment', str(binary)], stdout=stream,
                       stderr=subprocess.STDOUT, check=True, timeout=cost.command_timeout(), preexec_fn=limit_output)
    return output.read_text()


def coverage(binary, headers):
    """Interpret GNU readelf offsets for the expected ELF64 little-endian files."""
    if not stat.S_ISREG(binary.lstat().st_mode):
        raise ValueError('Coverage requires a regular original ELF.')
    length = binary.stat().st_size
    if not 0 < length <= MAX_RAW_BYTES:
        raise ValueError('Original ELF exceeds the bounded coverage input.')
    with binary.open('rb') as stream:
        if stream.read(6) != b'\x7fELF\x02\x01':
            raise ValueError('Expected ELF64 little-endian production output.')
    def field(name):
        values = re.findall(r'^\s*' + re.escape(name) + r':\s*(\d+)(?:\s|$)', headers, re.MULTILINE)
        if len(values) != 1:
            raise ValueError('Missing or ambiguous GNU ELF header field: ' + name)
        return int(values[0])
    count = field('Number of section headers')
    if not 0 < count <= 4096 or field('Size of this header') != 64 or \
            field('Size of section headers') != 64 or field('Size of program headers') != 56 or \
            not 0 < field('Number of program headers') <= 4096:
        raise ValueError('Unsupported or missing section table.')
    ranges = [('elf-header', 0, field('Size of this header')),
              ('program-headers', field('Start of program headers'),
               field('Size of program headers') * field('Number of program headers')),
              ('section-headers', field('Start of section headers'), field('Size of section headers') * count)]
    sections = []
    if headers.count('Section Headers:') != 1 or headers.count('Key to Flags:') != 1:
        raise ValueError('Missing or ambiguous GNU section header table.')
    section_table = headers.split('Section Headers:', 1)[1].split('Key to Flags:', 1)[0]
    for line in section_table.splitlines():
        match = re.match(r'^\s*\[\s*(\d+)\]\s+(.*)$', line)
        if not match:
            continue
        index, parts = int(match[1]), match[2].split()
        if index == 0:
            if len(parts) < 4 or parts[0] != 'NULL':
                raise ValueError('Malformed null section.')
            name, kind, offset, size = '', parts[0], parts[2], parts[3]
        else:
            if len(parts) < 5:
                raise ValueError('Malformed GNU section row.')
            name, kind, offset, size = parts[0], parts[1], parts[3], parts[4]
        if not re.fullmatch(r'[0-9a-fA-F]+', offset) or not re.fullmatch(r'[0-9a-fA-F]+', size):
            raise ValueError('Invalid GNU section offset or size.')
        offset, size = int(offset, 16), int(size, 16)
        sections.append({'index': index, 'name': name, 'type': kind, 'offset': offset, 'size_bytes': size,
                         'file_backed': kind != 'NOBITS'})
        if kind != 'NOBITS' and size:
            ranges.append(('section:' + str(index) + ':' + name, offset, size))
    if [section['index'] for section in sections] != list(range(count)) or \
            sum(section['name'] == '.shstrtab' for section in sections) != 1:
        raise ValueError('Section coverage is missing, duplicated or incomplete, including .shstrtab.')
    covered, position = [], 0
    for name, offset, size in sorted(ranges, key=lambda item: item[1]):
        if not size:
            continue
        if offset < position or offset + size > length:
            raise ValueError('ELF coverage ranges overlap or exceed the original file.')
        if offset > position:
            covered.append({'name': 'gap:' + str(position), 'offset': position, 'size_bytes': offset - position})
        covered.append({'name': name, 'offset': offset, 'size_bytes': size})
        position = offset + size
    if position < length:
        covered.append({'name': 'trailer', 'offset': position, 'size_bytes': length - position})
    whole = hashlib.sha256()
    with binary.open('rb') as stream:
        for interval in covered:
            digest, remaining = hashlib.sha256(), interval['size_bytes']
            if stream.tell() != interval['offset']:
                raise ValueError('Coverage does not partition the original file.')
            while remaining:
                cost.command_timeout()
                body = stream.read(min(MIB, remaining))
                if not body:
                    raise ValueError('Original changed during coverage.')
                digest.update(body); whole.update(body); remaining -= len(body)
            interval['sha256'] = digest.hexdigest()
        if stream.tell() != length or stream.read(1):
            raise ValueError('Original length changed during coverage.')
    return {'size_bytes': length, 'sha256': whole.hexdigest(), 'coverage_bytes': sum(r['size_bytes'] for r in covered),
            'all_file_bytes_covered': True, 'sections': sections, 'ranges': covered}


def observe(directory, stripped):
    result = {'qualification': 'DIAGNOSTIC_ONLY', 'linker_argv': {'status': 'unavailable'}, 'binaries': {}}
    for name in ('server', 'runtime'):
        for kind, path in (('prestrip', directory / name), ('stripped', stripped / name)):
            headers = bounded_readelf(path, directory / (name + '-' + kind + '-readelf.txt'))
            result['binaries'][name + '-' + kind] = coverage(path, headers)
    body = (json.dumps(result, indent=2, allow_nan=False) + '\n').encode()
    if len(body) > MIB:
        raise ValueError('Forensic coverage report exceeds its 1 MiB reserved bound.')
    (directory / 'coverage.json').write_bytes(body)
    return result


def compare(left, right):
    observations = {}
    for name in ('server-prestrip', 'runtime-prestrip', 'server-stripped', 'runtime-stripped'):
        a, b = left['binaries'][name], right['binaries'][name]
        ranges = [{item['name']: item for item in report['ranges']} for report in (a, b)]
        observations[name] = {'original_sha256': [a['sha256'], b['sha256']],
            'original_size_bytes': [a['size_bytes'], b['size_bytes']],
            'whole_equal': a['sha256'] == b['sha256'] and a['size_bytes'] == b['size_bytes'],
            'changed_ranges': {key: {'a1': ranges[0].get(key), 'a2': ranges[1].get(key)}
                               for key in sorted(ranges[0].keys() | ranges[1].keys())
                               if ranges[0].get(key) != ranges[1].get(key)}}
    return {'qualification': 'DIAGNOSTIC_ONLY', 'status': 'observed', 'binaries': observations,
            'interpretation': 'Original byte differences only; linker argv is unavailable and no cause is established.'}


def compare_symbol_text(left, right):
    """Compare original ordered lines without parsing, sorting or rewriting symbols."""
    result = {'status': 'unavailable', 'files': {'a1': {}, 'a2': {}},
              'sample_limit': 32, 'preview_byte_limit': MAX_SYMBOL_PREVIEW_BYTES,
              'output_byte_limit': MAX_SYMBOL_DIFF_BYTES, 'differing_lines': [],
              'total_differing_lines': None, 'sample_truncated': False, 'output_truncated': False}
    def lines(path, state):
        if not stat.S_ISREG(path.lstat().st_mode) or path.stat().st_size > MAX_SYMBOL_BYTES:
            raise ValueError('Symbol evidence must be an original bounded regular file.')
        expected = path.stat().st_size
        whole, count, length = hashlib.sha256(), 0, 0
        state.update(status='incomplete', size_bytes=expected)
        with path.open('rb') as stream:
            while True:
                chunk = stream.readline(MAX_SYMBOL_PREVIEW_BYTES + 1)
                if not chunk:
                    break
                digest, size, preview = hashlib.sha256(), 0, chunk[:MAX_SYMBOL_PREVIEW_BYTES]
                while chunk:
                    cost.command_timeout()
                    whole.update(chunk); digest.update(chunk)
                    size += len(chunk); length += len(chunk)
                    if length > expected:
                        raise ValueError('Original symbol evidence changed during comparison.')
                    if chunk.endswith(b'\n'):
                        break
                    chunk = stream.readline(64 * 1024)
                count += 1
                yield {'size_bytes': size, 'sha256': digest.hexdigest(),
                       'original_prefix_base64': base64.b64encode(preview).decode('ascii'),
                       'text_preview': preview.decode('utf-8', errors='backslashreplace'),
                       'line_truncated': size > len(preview)}
        if length != expected:
            raise ValueError('Original symbol evidence length changed during comparison.')
        state.update(status='complete', size_bytes=length, line_count=count, sha256=whole.hexdigest())
    changed, sampled_bytes = 0, 0
    try:
        for number, (a, b) in enumerate(itertools.zip_longest(lines(left, result['files']['a1']),
                                                            lines(right, result['files']['a2'])), 1):
            if a is not None and b is not None and (a['size_bytes'], a['sha256']) == (b['size_bytes'], b['sha256']):
                continue
            changed += 1
            if len(result['differing_lines']) < result['sample_limit'] and not result['output_truncated']:
                original = {'line_number': number, 'a1': a, 'a2': b}
                size = len(json.dumps(original, ensure_ascii=True).encode())
                if sampled_bytes + size <= MAX_SYMBOL_DIFF_BYTES - 16 * 1024:
                    result['differing_lines'].append(original)
                    sampled_bytes += size
                else:
                    result['output_truncated'] = True
        result.update(status='complete', total_differing_lines=changed,
                      sample_truncated=changed > len(result['differing_lines']))
    except (OSError, ValueError, TimeoutError) as error:
        result['reason'] = str(error)
        result['observed_differing_lines_before_failure'] = changed
    return result
