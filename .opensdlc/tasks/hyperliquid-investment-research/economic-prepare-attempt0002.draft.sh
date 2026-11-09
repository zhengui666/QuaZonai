#!/usr/bin/env bash
# Draft only: invoke after the public funding acquisition has finished.
set -euo pipefail
catalog_prepare=/home/zzy/.cache/quazonai-release-v3.0.0-dev.20261028072856.40650351540/operator/catalog-prepare-high-precision
ops=/home/zzy/projects/QuaZonai/.opensdlc/tasks/hyperliquid-investment-research
data=/home/zzy/.local/share/quazonai-ts/data
attempt="$data/catalogs/hyperliquid-flow-prepared-20261028/attempt-0002"
python3 "$ops/normalize_funding.py" "$attempt/source/catalog"
python3 - "$data/catalogs/hyperliquid-flow-composed-20261028/attempt-0001/catalog" "$attempt/source/catalog" <<'PY'
import json, os, pathlib, sys
source, target = map(pathlib.Path, sys.argv[1:])
manifest = json.loads((source / 'cohort-cache.json').read_text())
os.link(source / 'cohort-cache.json', target / 'cohort-cache.json')
for row in manifest['files']:
    output = target / row['path']
    output.parent.mkdir(parents=True, exist_ok=True)
    os.link(source / row['path'], output)
PY
"$catalog_prepare" prepare-ticks \
  --catalog "$attempt/source/catalog" \
  --request "$ops/economic-prepare-native-request-attempt0002.json" \
  --output "$attempt/prepared" --maximum-output-bytes 51539607552
python3 - "$ops/economic-dataset-attempt0002.draft.json" "$attempt/prepared/catalog-metadata.json" "$data/catalogs/hyperliquid-cohort-dataset-attempt-0002.json" <<'PY'
import json, pathlib, sys
config = json.loads(pathlib.Path(sys.argv[1]).read_text())
metadata = json.loads(pathlib.Path(sys.argv[2]).read_text())
assert metadata['request'] == config['data']['sealed']['nativeInput']['request']
assert metadata['research_request'] == config['data']['nativeInput']['request']
config['inputCoverage'] = metadata['coverage']
with pathlib.Path(sys.argv[3]).open('x') as target:
    json.dump(config, target, indent=2, ensure_ascii=False)
    target.write('\n')
PY
