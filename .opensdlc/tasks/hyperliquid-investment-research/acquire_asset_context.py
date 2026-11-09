"""Fetch only inventoried original daily contexts at the fixed publisher revision."""
import datetime
import fcntl
import json
import os
import sys
import http.client
import pathlib
import shutil
import time
import urllib.error
import urllib.request

TASK = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(TASK.parents[2] / 'runtimes' / 'data'))
from snapshot import acquire_cached_file
ROOT = pathlib.Path('/home/zzy/.local/share/quazonai-ts/data/hyperliquid-asset-ctxs-20250728-20261013')


def save(path, value):
    temp = path.with_suffix('.tmp')
    temp.write_text(json.dumps(value, indent=2) + '\n')
    temp.replace(path)


def main():
    inventory = json.loads((TASK / 'asset-context-inventory.json').read_text())
    ROOT.mkdir(parents=True, exist_ok=True)
    lock = (ROOT / 'writer.lock').open('a')
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    save(ROOT / 'manifest.json', inventory)
    state = {'pid': os.getpid(), 'status': 'RUNNING', 'filesTotal': len(inventory['files']),
             'bytesTotal': inventory['bytes'], 'filesCompleted': 0, 'bytesCompleted': 0,
             'missingPublishedDates': ['2026-07-08'], 'root': str(ROOT)}
    try:
        for item in inventory['files']:
            target = ROOT / item['path']
            target.parent.mkdir(exist_ok=True)
            state['currentFile'] = item['path']
            save(TASK / 'asset-context-progress.json', state)
            if target.exists():
                if target.stat().st_size != item['size']:
                    raise RuntimeError('Existing original file size mismatch: ' + item['path'])
            else:
                url = ('https://huggingface.co/datasets/ProbioticFarmer/hyperliquid-augmented-copytrading/resolve/'
                       + inventory['revision'] + '/' + item['path'])
                cache = ROOT / '.transport'
                entry = dict(item, url=url, format='csv-lz4')
                old_part = target.with_suffix(target.suffix + '.part')
                partial = cache / 'transfers' / item['path'] / 'data.partial'
                if old_part.exists() and not partial.exists():
                    partial.parent.mkdir(parents=True, exist_ok=True)
                    # Copy the interrupted prefix; preserve original failed bytes unchanged.
                    shutil.copyfile(old_part, partial)
                    save(partial.with_name('state.json'), {k: entry[k] for k in ('url', 'size', 'format')})
                def inspect(path, metadata):
                    if path.stat().st_size != metadata['size']:
                        raise ValueError('Context file size mismatch')
                    return {'size': path.stat().st_size}
                for attempt in range(5):
                    try:
                        result = acquire_cached_file(cache, entry, inspect)
                        os.link(result['local_path'], target)
                        break
                    except (urllib.error.URLError, TimeoutError, http.client.HTTPException, ValueError) as error:
                        code = getattr(error, 'code', None)
                        state['lastError'] = {'file': item['path'], 'attempt': attempt + 1,
                                              'type': type(error).__name__, 'httpStatus': code}
                        save(TASK / 'asset-context-progress.json', state)
                        transient = (isinstance(error, (urllib.error.URLError, TimeoutError, http.client.HTTPException))
                                     or str(error) == 'Hub file response was truncated; partial bytes retained')
                        if not transient or (code is not None and code not in (429, 500, 502, 503, 504)) or attempt == 4:
                            raise RuntimeError('Context download failed; see progress status') from None
                        time.sleep(5 * 2 ** attempt)
            state['filesCompleted'] += 1
            state['bytesCompleted'] += item['size']
            save(TASK / 'asset-context-progress.json', state)
        state['status'] = 'COMPLETED'
    except Exception as error:
        state.update(status='STOPPED_ERROR', error=str(error))
        raise
    finally:
        state['updatedAt'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        save(TASK / 'asset-context-progress.json', state)


if __name__ == '__main__':
    main()
