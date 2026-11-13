import assert from 'node:assert/strict';
import { test } from 'node:test';
import { writeFile, access } from 'node:fs/promises';
import { join } from 'node:path';
import { setup } from './support.ts';

test('one Rust environment saves without executing Cargo or connecting and retains legacy facts across restart', async () => {
  const state = await setup();
  try {
    const before = (await state.service.app.inject({url:'/api/v3/settings/instances',headers:state.headers})).json();
    const marker=join(state.directory,'unexpected-cargo-launch'); const cargo=join(state.directory,'cargo');
    await writeFile(cargo,'#!/bin/sh\nprintf invoked > '+JSON.stringify(marker)+'\nexit 1\n',{mode:0o755});
    const manifest=join(state.directory,'Cargo.toml'); await writeFile(manifest,'[package]\nname="owner_strategy"\nversion="0.1.0"\n');
    const environment = {kind:'RUST_PROCESS',id:'owner-rust',cargoBinary:cargo,manifestPath:manifest,packageName:'owner_strategy',features:[],backtestFactory:'backtests',accountFactory:'live'};
    const request = {expectedRevision:before.revision,environment};
    const saved = await state.service.app.inject({method:'PUT',url:'/api/v3/settings/instances',headers:state.headers,payload:request});
    assert.equal(saved.statusCode,200,saved.body); assert.deepEqual(saved.json().environment,environment);
    assert.deepEqual(saved.json().backtest,before.backtest); assert.deepEqual(saved.json().connections,before.connections);
    const replay = await state.service.app.inject({method:'PUT',url:'/api/v3/settings/instances',headers:state.headers,payload:request});
    assert.deepEqual(replay.json(),saved.json(),'Lost-response replay keeps original revision and source.');
    const legacy = await state.service.app.inject({method:'PUT',url:'/api/v3/settings/instances',headers:state.headers,payload:{expectedRevision:saved.json().revision,backtest:null,connections:[]}});
    assert.equal(legacy.statusCode,400,'New HTTP configuration is not a second transport.');
    await state.restart();
    const reopened = await state.service.app.inject({url:'/api/v3/settings/instances',headers:state.headers});
    assert.deepEqual(reopened.json(),saved.json(),'GET and reopening preserve unavailable source without launching it.');
    const conflict = await state.service.app.inject({method:'PUT',url:'/api/v3/settings/instances',headers:state.headers,payload:{...request,environment:{...environment,id:'different'}}});
    assert.equal(conflict.statusCode,409);
    await assert.rejects(access(marker), /ENOENT/, 'Save, GET and restart must not execute actual Cargo.');
    const inspect = await state.service.app.inject({method:'POST',url:'/api/v3/settings/instances/inspect',headers:state.headers,payload:{}});
    assert.notEqual(inspect.statusCode,200,'Only explicit inspection executes Cargo; its failure does not establish a connection.');
    await access(marker);
    assert.deepEqual((await state.service.app.inject({url:'/api/v3/settings/instances',headers:state.headers})).json(),saved.json());
  } finally { await state.cleanup(); }
});
