from pathlib import Path

path = Path('apps/job/tests/polymarket.rs')
text = path.read_text()
old = '''    for (asset, id) in request.assets.iter_mut().zip(IDS) {
        asset.instrument_id = id.into();
    }'''
assert text.count(old) == 1
text = text.replace(old, '''    for (asset, id) in request.assets.iter_mut().zip(IDS) {
        asset.instrument_id = id.into();
        asset.currency = "pUSD".into();
    }''')
# Fixtures may disclose their detailed field errors; production CLI errors stay bounded.
text = text.replace('format!("{error:#}")', 'format!("{error:?}")')
path.write_text(text)
