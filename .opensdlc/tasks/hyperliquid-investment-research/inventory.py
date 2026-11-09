"""Inspect existing Parquet footers; never read wallet or order values."""
import collections
import datetime
import json
import pathlib
import sys

import pyarrow.parquet as pq


def inventory(root):
    manifest = json.loads((root / 'market-data.json').read_text())
    result = {'sourceRevision': manifest['revision'], 'method': 'PARQUET_FOOTERS_ONLY',
              'files': [], 'streams': {}, 'limitations': [
                  'Row counts include maker and taker fills; not unique market trades.',
                  'Footer ranges do not prove within-day continuity or per-market coverage.',
                  'No receive timestamp or executable quotes are established by this inventory.']}
    streams = collections.defaultdict(lambda: {'files': 0, 'rows': 0, 'bytes': 0})
    for item in manifest['files']:
        if not item['path'].endswith('.parquet'):
            continue
        path = root / item['path']
        if path.stat().st_size != item['size']:
            raise ValueError(f'file_size_mismatch:{item["path"]}')
        file = pq.ParquetFile(path)
        fields = {field.name: str(field.type) for field in file.schema_arrow}
        times = []
        missing_stats = 0
        for group in range(file.metadata.num_row_groups):
            metadata = file.metadata.row_group(group)
            column = next((metadata.column(i) for i in range(metadata.num_columns)
                           if metadata.column(i).path_in_schema == 'timestamp'), None)
            stats = column.statistics if column else None
            if stats is None or not stats.has_min_max:
                missing_stats += 1
            else:
                times.extend([stats.min, stats.max])
        row = {'path': item['path'], 'rows': file.metadata.num_rows,
               'rowGroups': file.metadata.num_row_groups, 'fields': fields,
               'minimumTimestamp': min(times).isoformat() if times else None,
               'maximumTimestamp': max(times).isoformat() if times else None,
               'groupsWithoutTimeStatistics': missing_stats}
        result['files'].append(row)
        stream = streams[item['path'].split('/')[0]]
        stream['files'] += 1
        stream['rows'] += row['rows']
        stream['bytes'] += item['size']
    result['streams'] = dict(streams)
    result['observedAt'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    assert sum(s['files'] for s in streams.values()) == len(result['files'])
    assert sum(s['rows'] for s in streams.values()) == sum(f['rows'] for f in result['files'])
    return result


if __name__ == '__main__':
    root, output = map(pathlib.Path, sys.argv[1:])
    result = inventory(root)
    with output.open('x') as handle:
        json.dump(result, handle, ensure_ascii=False, indent=2)
        handle.write('\n')
    print(json.dumps(result['streams']))
