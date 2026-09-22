from pathlib import Path

path = Path('apps/runtime/src/engine.rs')
source = path.read_text()
old = '''                    if !matches!(class, "CurrencyPair" | "Equity")
                        && !(class == "BinaryOption"
                            && domain::prediction::instrument(definition).is_ok())'''
new = '''                    if !(matches!(class, "CurrencyPair" | "Equity")
                        || (class == "BinaryOption"
                            && domain::prediction::instrument(definition).is_ok()))'''
assert source.count(old) == 1, 'Expected original native capability predicate exactly once'
path.write_text(source.replace(old, new))
