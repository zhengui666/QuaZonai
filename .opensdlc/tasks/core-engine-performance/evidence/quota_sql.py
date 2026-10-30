import sqlite3, pathlib, re, time, statistics, json, platform
repo = pathlib.Path(__file__).resolve().parents[4]
source=(repo/'apps/runtime/src/journal.rs').read_text()
query=re.search(r'let occupied: i64 = sqlx::query_scalar\("([^"]+)"\)', source).group(1)
records=[]
for n in [1000,10000,100000]:
    db=sqlite3.connect(':memory:')
    db.execute('PRAGMA foreign_keys=ON')
    for m in sorted((repo/'apps/runtime/migrations').glob('*.sql')): db.executescript(m.read_text())
    db.executemany('INSERT INTO input_objects VALUES (?, ?, ?, ?, ?)', ((str(i),'1',b'x',1,i) for i in range(n)))
    db.commit()
    assert db.execute(query).fetchone()[0]==n
    samples=[]
    for _ in range(31):
        t=time.perf_counter_ns(); result=db.execute(query).fetchone()[0]; samples.append((time.perf_counter_ns()-t)/1e6)
        assert result==n
    plan=[row[3] for row in db.execute('EXPLAIN QUERY PLAN '+query)]
    records.append({'input_objects':n,'payload_bytes_each':1,'median_ms':statistics.median(samples),'p95_ms':sorted(samples)[29],'min_ms':min(samples),'max_ms':max(samples),'query_plan':plan})
    db.close()
report={'type':'isolated_SQL_microbenchmark_NOT_native_runtime_or_production_RT','sqlite':sqlite3.sqlite_version,'python':platform.python_version(),'machine':platform.machine(),'platform':platform.platform(),'source':'apps/runtime/src/journal.rs:182','repeats':31,'database':':memory:','query':query,'results':records}
pathlib.Path(__file__).with_name('quota_sql_results.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
