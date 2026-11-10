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
    expected = {'defaultModel': model, 'thinkingLevel': None, 'serviceTier': None}
    assert api(opener, origin, 'settings', expected, method='PATCH') == expected
    assert api(opener, origin, 'settings') == expected
    return expected

def completed(opener, origin, id):
    for _ in range(90):
        detail = api(opener, origin, f'research/{id}')
        if detail['final'] is not None and detail['final']['status'] != 'STARTED':
            assert detail['trialRecords'] == [] and detail['final']['status'] == 'STOPPED'
            return detail
        if detail['trialRecords'] and detail['trialRecords'][0]['status'] == 'FAILED':
            raise AssertionError('Packaged native trial failed: ' + str(detail['trialRecords'][0]['error']))
        time.sleep(1)
    raise TimeoutError('Packaged controlled research did not complete')

def scientific_smoke(image, directory):
    run(['docker', 'run', '--rm', '--network=none', '--read-only', '--entrypoint', '/opt/quazonai/bin/job', image, '--version'])
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
        port = free_port()
        config = None
        try:
            manage(bundle, root, 'deploy', '--port', str(port))
            config = json.loads((root / 'installation.json').read_text())
            origin = f'http://localhost:{port}'
            jar = http.cookiejar.CookieJar(); opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
            password = secrets.token_urlsafe(24)
            api(opener, origin, 'session/setup', {'password': password})
            api(opener, origin, 'session/login', {'password': password})
            initial_settings = model_settings(opener, origin)
            assert api(opener, origin, 'session')['authenticated']
            if args.published_only:
                # Registry + shipped archive installation, not another whole core suite.
                assert api(opener, origin, 'settings') == initial_settings
                print('Published Node image setup/login/static installation passed.')
                return
            manage(bundle, root, 'stop')
            provider = root / 'data/research/smoke-provider.mjs'
            shutil.copy2(BUNDLE / 'smoke-provider.mjs', provider)
            path = root / 'data/service.json'; service = json.loads(path.read_text())
            service['research']['providerModules'] = [str(provider)]
            service['datasets'] = [{'id': 'image-control', 'name': 'Controlled image protocol, unqualified',
                'assetClass': 'TEST', 'currency': 'USD', 'source': 'FIXTURE', 'start': None, 'end': None, 'ready': True, 'data': {}}]
            with (root / 'data/research/provider.env').open('a') as env:
                env.write('QZ_TEST_IMAGE_SERVICE_ONLY=1\n')
            path.write_text(json.dumps(service, indent=2) + '\n')
            manage(bundle, root, 'start')
            models = api(opener, origin, 'research/models')
            model = next({key: item[key] for key in ('provider', 'modelId')} for item in models if item['provider'] == 'faux')
            original_settings = model_settings(opener, origin, model)
            command = {'requestId': 'original-image-idea', 'goal': 'Controlled image protocol; no market qualification',
                'model': model, 'datasetId': 'image-control', 'maxTrials': 0}
            receipt = api(opener, origin, 'research', command)
            detail = completed(opener, origin, receipt['conversationId'])
            assert detail['trialRecords'] == []
            assert api(opener, origin, 'research', command) == receipt
            manage(bundle, root, 'restart')
            assert api(opener, origin, 'research', command) == receipt
            assert api(opener, origin, f"research/{receipt['conversationId']}")['trialRecords'] == detail['trialRecords']
            previous_config = path.read_bytes()
            target = make_bundle(directory, 'v0.0.0-ci.2', revision, app, science)
            manage(target, root, 'apply-update')
            updated = json.loads((root / 'installation.json').read_text())
            for key in ('root', 'uid', 'gid', 'project', 'port'):
                assert updated[key] == config[key]
            assert path.read_bytes() == previous_config
            assert api(opener, origin, 'research', command) == receipt
            manage(target, root, 'backup')
            backup = max((root / 'backups').iterdir(), key=lambda item: item.stat().st_mtime_ns)
            assert not (backup / 'database.dump').exists()
            if (backup / 'data').is_dir():
                assert (backup / 'data/service.json').read_bytes() == path.read_bytes()
            else:
                assert (backup / 'data.tar.gz').is_file()
            manage(target, root, 'start')
            model_settings(opener, origin)
            manage(target, root, 'restore', str(backup))
            assert api(opener, origin, 'settings') == original_settings
            assert api(opener, origin, 'research', command) == receipt
            scientific_smoke(science, directory)
            run(['docker', 'run', '--rm', '--network=none', '--entrypoint', 'node', app, '-e',
                 "const fs = require('node:fs'); for (const path of ['/opt/quazonai/bin/job', '/opt/quazonai/operator/bin/source-tools']) if (fs.existsSync(path)) throw new Error('Application contains native execution: ' + path);"])
            inventory = json.loads(run(['docker', 'run', '--rm', '--network=none', '--entrypoint', '/usr/bin/python3', science,
                 '-E', '-s', '-B', '/opt/quazonai/operator/source_plugins.py', 'plugins'], capture=True))
            assert [item['id'] for item in inventory] == ['cryptohftdata-hyperliquid']
            run(['docker', 'run', '--rm', '--network=none', '--entrypoint', '/usr/bin/python3', science,
                 '-E', '-s', '-B', '/opt/quazonai/operator/source_plugins.py', 'convert', 'cryptohftdata-hyperliquid', '--help'], capture=True)
            scope = 'Service-only controlled receipt (NO native trial)'
            print(scope + ': Node setup/login, original receipt replay, restart/update, stopped SQLite backup restore and isolated compiler passed. No quality/remote-stop/order claim.')
        finally:
            if (root / 'installation.json').exists():
                config = json.loads((root / 'installation.json').read_text())
                compose(config, 'down', '--volumes', '--remove-orphans')

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image'); parser.add_argument('--runtime-image'); parser.add_argument('--revision')
    parser.add_argument('--manifest', type=Path)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--published-only', action='store_true')
    mode.add_argument('--service-only', action='store_true', help='Local packaging diagnosis only; never used by CI/release acceptance')
    args = parser.parse_args()
    smoke(args)
