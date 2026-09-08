#!/usr/bin/env node
// TEST ONLY: run actual PostgreSQL DDL, then lose its client acknowledgement.
// A private test-owned directory provides settings; no production import uses it.
import fs from 'node:fs';
import { spawnSync } from 'node:child_process';
const settings = JSON.parse(fs.readFileSync(new URL('./settings.json', import.meta.url), 'utf8'));
const input = fs.readFileSync(0, 'utf8');
const result = spawnSync(settings.psql, process.argv.slice(2), {
  env: process.env, input, encoding: 'utf8', timeout: 35000, maxBuffer: 1024 * 1024,
});
const creation = /^CREATE (ROLE|DATABASE) "(web_(?:app|e2e)_[0-9a-f]{24})"/.exec(input);
if (result.status === 0 && creation) {
  fs.appendFileSync(settings.events, JSON.stringify({ kind: creation[1], name: creation[2], committed: true }) + '\n', { mode: 0o600 });
  if (creation[1] === settings.fault) {
    if (settings.signal) {
      process.kill(process.ppid, 'SIGTERM');
      await new Promise(resolve => setTimeout(resolve, 200));
    }
    process.exit(97); // PostgreSQL committed; no stdout/ack is delivered.
  }
}
process.stdout.write(result.stdout ?? '');
process.stderr.write(result.stderr ?? '');
process.exit(result.status ?? 98);
