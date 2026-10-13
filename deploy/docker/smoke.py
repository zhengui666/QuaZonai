#!/usr/bin/env python3
"""One disposable Node install/replay/restart/update/backup restore; no paid provider or orders."""
from __future__ import annotations
import argparse
import http.cookiejar
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import subprocess
import tarfile
import tempfile
import time
import urllib.request
import release

BUNDLE = Path(__file__).resolve().parent

def run(args, *, capture=False, env=None):
    result = subprocess.run([str(arg) for arg in args], env=env, check=True,
                            stdout=subprocess.PIPE if capture else None, text=True, timeout=180)
    return result.stdout.strip() if capture else ''

def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0)); return sock.getsockname()[1]

def image_id(image):
    return run(['docker', 'image', 'inspect', '--format', '{{.Id}}', image], capture=True)

def make_bundle(directory, version, revision, app, science):
    assets = directory / (version + '-assets')
    release.bundle(version, revision, app, assets, runtime_image=science)
    bundle = directory / version; bundle.mkdir()
    with tarfile.open(assets / 'quazonai-deploy.tar.gz') as archive:
        archive.extractall(bundle, filter='data')
    return bundle

def manage(bundle, root, command, *args):
    return run(['bash', bundle / 'manage.sh', command, *args, '--directory', root], capture=True)

def compose(config, *args):
    # Credentials remain in the private manifest; never appear in argv/logs.
    return run(['bash', '-c', 'source "$1/manage.sh"; qz_compose "$2" "${@:3}"', 'qz-smoke',
                config['bundle'], Path(config['root']) / 'installation.json', *args], capture=True)

def api(opener, origin, path, body=None, method=None):
    request = urllib.request.Request(origin + '/api/v3/' + path,
        data=json.dumps(body).encode() if body is not None else None,
        method=method, headers={'Content-Type': 'application/json', 'Origin': origin})
    with opener.open(request, timeout=15) as response:
        content = response.read(); return json.loads(content) if content else None

def model_settings(opener, origin, model=None):
    expected = {'defaultModel': model, 'thinkingLevel': None}
    assert api(opener, origin, 'settings', expected, method='PATCH') == expected
    assert api(opener, origin, 'settings') == expected
    return expected

def expanded_catalog(native, directory, fixture):
    # Extend the existing explicitly unqualified candle test, just as the Node
    # core scenario does. The actual native importer creates all catalog bytes.
    acquired = directory / 'acquired'
    manifest = json.loads((acquired / 'acquisition.json').read_text())
    row = json.loads((acquired / 'records.jsonl').read_text().splitlines()[0])
    rows = [{**row, 'selection_time_seconds': index * 60, 'event_time_seconds': (index + 1) * 60, 'source_row_index': index} for index in range(8)]
    records = ''.join(json.dumps(value) + '\n' for value in rows)
    raw = json.dumps([[value['selection_time_seconds'], *[float(value[name]) for name in ('low', 'high', 'open', 'close', 'volume')]] for value in rows])
    manifest['selection']['end_seconds'] = 480
    for request in (manifest['requests'][0], manifest['responses'][0]['request']):
        request['end_seconds'] = 480; request['url'] = request['url'].replace('00%3A03', '00%3A08')
    manifest['responses'][0]['counts']['source_rows'] = 8; manifest['responses'][0]['counts']['selected_rows'] = 8
    manifest['responses'][0]['file']['size'] = len(raw.encode()); manifest['raw_response_bytes'] = len(raw.encode())
    manifest['records']['size'] = len(records.encode()); manifest['record_count'] = 8
    (acquired / 'records.jsonl').write_text(records); (acquired / 'raw/0000.json').write_text(raw)
    (acquired / 'acquisition.json').write_text(json.dumps(manifest))
    output = directory / 'expanded'
    run([native / 'catalog-prepare', 'ingest-candles', '--acquisition', acquired / 'acquisition.json', '--instruments', directory / 'instruments.json', '--output', output], capture=True)
    fixture['catalog_root'] = str(output / 'catalog'); fixture['selection']['selection']['maximum_rows'] = 8

def completed(opener, origin, id, *, native=True):
    for _ in range(90):
        detail = api(opener, origin, f'research/{id}')
        if detail['final'] is not None:
            if native:
                assert detail['trialRecords'][0]['status'] == 'COMPUTED', detail['trialRecords'][0]['error']
                assert detail['final']['status'] == 'COMPUTED', detail['final']['reason']
            else:
                assert detail['trialRecords'] == [] and detail['final']['status'] == 'STOPPED'
            return detail
        if detail['trialRecords'] and detail['trialRecords'][0]['status'] == 'FAILED':
            raise AssertionError('Packaged native trial failed: ' + str(detail['trialRecords'][0]['error']))
        time.sleep(1)
    raise TimeoutError('Packaged controlled research did not complete')

def scientific_smoke(image, directory):
    run(['docker', 'run', '--rm', '--network=none', '--read-only', image, '--version'])
    run(['docker', 'run', '--rm', '--network=none', '--read-only', '--entrypoint', '/opt/rust/bin/rustc', image, '--version'])
    source = directory / 'compiler-input'; output = directory / 'compiler-output'
    source.mkdir(); output.mkdir()
    # Actually compile one supplied Wasm input in the isolated pinned compiler.
    (source / 'factor.rs').write_text('#![no_std]\n#[panic_handler] fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }\n#[unsafe(no_mangle)] pub extern "C" fn predict(_:f64,_:f64,_:f64,_:f64,_:f64,_:f64,_:f64,_:f64)->f64 {1.0}\n')
    run(['docker', 'run', '--rm', '--network=none', '--read-only', '--cap-drop=ALL',
         '--user', f'{os.getuid()}:{os.getgid()}', '--tmpfs', '/tmp:rw,size=128m',
         '--mount', f'type=bind,src={source},dst=/input,readonly',
         '--mount', f'type=bind,src={output},dst=/output', '--entrypoint', '/opt/rust/bin/rustc', image,
         '--edition=2024', '--target=wasm32-unknown-unknown', '--crate-type=cdylib', '-C', 'panic=abort',
         '/input/factor.rs', '-o', '/output/factor.wasm'])
    assert (output / 'factor.wasm').read_bytes()[:4] == b'\0asm'

def smoke(args):
    app = args.image; science = args.runtime_image; revision = args.revision
    if args.manifest:
        manifest = json.loads(args.manifest.read_text())
        app, science, revision = manifest['image'], manifest['runtime_image'], manifest['revision']
        run(['docker', 'pull', app]); run(['docker', 'pull', science])
    app, science = image_id(app), image_id(science)
    scratch = os.environ.get('PI_SCRATCH_DIR') or os.environ.get('RUNNER_TEMP') or os.environ.get('TMPDIR')
    with tempfile.TemporaryDirectory(prefix='quazonai-node-smoke-', dir=scratch) as temporary:
        directory = Path(temporary); root = directory / 'installation'
        bundle = make_bundle(directory, 'v0.0.0-ci.1', revision, app, science)
        port, database_port = free_port(), free_port()
        while port == database_port: database_port = free_port()
        config = None
        try:
            manage(bundle, root, 'deploy', '--port', str(port), '--database-port', str(database_port))
            config = json.loads((root / 'installation.json').read_text())
            origin = f'http://localhost:{port}'
            jar = http.cookiejar.CookieJar(); opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
            password = secrets.token_urlsafe(24)
            api(opener, origin, 'session/setup', {'password': password})
            api(opener, origin, 'session/login', {'password': password})
            model_settings(opener, origin)
            assert api(opener, origin, 'session')['authenticated']
            if args.published_only:
                # Registry + shipped archive installation, not another whole core suite.
                assert api(opener, origin, 'settings') == {'defaultModel': None, 'thinkingLevel': None}
                print('Published Node image setup/login/static installation passed.')
                return
            manage(bundle, root, 'stop')
            fixture = None
            if not args.service_only:
                native = args.native_directory.resolve()
                catalog = root / 'data/native-smoke'
                run([native / 'browser_catalog_fixture', catalog, native / 'catalog-prepare'])
                fixture = json.loads((catalog / 'prepared.json').read_text())
                expanded_catalog(native, catalog, fixture)
            data = {}
            if fixture:
                selection = {**fixture['selection']['selection'], 'event_end_ns': str(int(fixture['original_receipt_ns']) + 1),
                             'decision_cutoff_ns': str(int(fixture['original_receipt_ns']) + 1)}
                request = {'schema_version': 1, 'forecast': {'schema_version': 1, 'selection': selection,
                    'parameters': {'schema_version': 1, 'fast_period': 1, 'slow_period': 2, 'label_horizon_observations': 1, 'total_fuel': '1000000'}},
                    'train_end_ns': '180000000000', 'validation_end_ns': '300000000000'}
                original = {'catalog': fixture['catalog_root'], 'request': request}
                data = {**original, 'sealed': original}
            provider = root / 'data/research/smoke-provider.mjs'
            shutil.copy2(BUNDLE / 'smoke-provider.mjs', provider)
            path = root / 'data/service.json'; service = json.loads(path.read_text())
            service['research']['providerModules'] = [str(provider)]
            service['datasets'] = [{'id': 'image-control', 'name': 'Controlled image protocol, unqualified',
                'assetClass': 'TEST', 'currency': 'USD', 'source': 'FIXTURE', 'start': None, 'end': None, 'ready': True, 'data': data}]
            path.write_text(json.dumps(service, indent=2) + '\n')
            # Native input is an explicit immutable mount; all writer state remains
            # in the original persistent data bind. Test provider is private sidecar.
            if fixture:
                (root / 'compose.override.yaml').write_text('services:\n  app:\n    volumes:\n      - type: bind\n        source: ' + json.dumps(str(catalog)) + '\n        target: ' + json.dumps(str(catalog)) + '\n        read_only: true\n')
            else:
                with (root / 'data/research/provider.env').open('a') as env:
                    env.write('QZ_TEST_IMAGE_SERVICE_ONLY=1\n')
            manage(bundle, root, 'start')
            models = api(opener, origin, 'research/models')
            model = next({key: item[key] for key in ('provider', 'modelId')} for item in models if item['provider'] == 'faux')
            original_settings = model_settings(opener, origin, model)
            command = {'requestId': 'original-image-idea', 'goal': 'Controlled image protocol; no market qualification',
                'model': model, 'datasetId': 'image-control', 'maxTrials': 0 if args.service_only else 1}
            receipt = api(opener, origin, 'research', command)
            detail = completed(opener, origin, receipt['conversationId'], native=not args.service_only)
            assert len(detail['trialRecords']) == (0 if args.service_only else 1)
            assert api(opener, origin, 'research', command) == receipt
            manage(bundle, root, 'restart')
            assert api(opener, origin, 'research', command) == receipt
            assert api(opener, origin, f"research/{receipt['conversationId']}")['trialRecords'] == detail['trialRecords']
            previous_config = path.read_bytes()
            target = make_bundle(directory, 'v0.0.0-ci.2', revision, app, science)
            manage(target, root, 'apply-update')
            updated = json.loads((root / 'installation.json').read_text())
            for key in ('root', 'uid', 'gid', 'project', 'password', 'port', 'database_port'):
                assert updated[key] == config[key]
            assert path.read_bytes() == previous_config
            assert api(opener, origin, 'research', command) == receipt
            manage(target, root, 'backup')
            backup = max((root / 'backups').iterdir(), key=lambda item: item.stat().st_mtime_ns)
            assert (backup / 'data.tar.gz').is_file() and (backup / 'database.dump').is_file()
            manage(target, root, 'start')
            model_settings(opener, origin)
            manage(target, root, 'restore', str(backup))
            assert api(opener, origin, 'settings') == original_settings
            assert api(opener, origin, 'research', command) == receipt
            scientific_smoke(science, directory)
            run(['docker', 'run', '--rm', '--network=none', '--entrypoint', '/usr/bin/python3', app,
                 '-E', '-s', '-B', '/opt/quazonai/operator/source_plugins.py', 'plugins'], capture=True)
            scope = 'Service-only controlled receipt (NO native trial)' if args.service_only else 'Real controlled native factor/heldout'
            print(scope + ': Node setup/login, original receipt replay, restart/update, stopped SQLite/PG backup restore and isolated compiler passed. No quality/remote-stop/order claim.')
        finally:
            if (root / 'installation.json').exists():
                config = json.loads((root / 'installation.json').read_text())
                compose(config, 'down', '--volumes', '--remove-orphans')

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image'); parser.add_argument('--runtime-image'); parser.add_argument('--revision')
    parser.add_argument('--native-directory', type=Path)
    parser.add_argument('--manifest', type=Path); parser.add_argument('--published-only', action='store_true')
    parser.add_argument('--service-only', action='store_true', help='Local packaging diagnosis only; never used by CI/release acceptance')
    args = parser.parse_args()
    if not args.published_only and not args.service_only and not args.native_directory: parser.error('--native-directory is required for real core image acceptance')
    smoke(args)
