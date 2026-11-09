"""Run the four original primary economic comparisons once, sequentially."""
import json
import pathlib
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parent
NAMES = ('C01', 'C03', 'F01', 'F02')
CLI = '/home/zzy/.local/bin/quazonai'
STEMS = {name: f'economic-primary-{name}-context-v3' for name in NAMES}

def call(*args):
    return json.loads(subprocess.check_output([CLI, 'portfolio', *args], text=True))

def run():
    requests = [json.loads((ROOT / f'{STEMS[name]}.json').read_text()) for name in NAMES]
    assert len({r['requestId'] for r in requests}) == len(NAMES)
    for name, request in zip(NAMES, requests):
        receipt = ROOT / f'{STEMS[name]}-receipt.json'
        if receipt.exists():
            admitted = json.loads(receipt.read_text())
            assert admitted['requestId'] == request['requestId']
        else:
            # Keep the original body/ID on uncertainty; never automatically repeat POST.
            raw = subprocess.run([CLI, 'portfolio', 'replay-ticks', '--file', str(ROOT / f'{STEMS[name]}.json')], capture_output=True, text=True)
            receipt.write_text(raw.stdout)
            raw.check_returncode()
            admitted = json.loads(raw.stdout)
        print(name, admitted['id'], flush=True)
        while True:
            view = call('tick-replay', admitted['id'])
            (ROOT / f'{STEMS[name]}-result.json').write_text(json.dumps(view, indent=2) + '\n')
            if view['status'] == 'COMPUTED':
                print(name, 'COMPUTED', flush=True)
                break
            if view['status'] not in ('QUEUED', 'RUNNING'):
                raise RuntimeError(f"{name} {view['status']}: {view.get('error')}")
            time.sleep(30)
    print('FOUR_PRIMARY_COMPARISONS_COMPUTED', flush=True)

if __name__ == '__main__':
    run()
