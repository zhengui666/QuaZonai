#!/usr/bin/env node
import { run } from './commands.ts';
import { CliError } from './profile.ts';

const controller = new AbortController();
const detach = () => controller.abort();
process.once('SIGINT', detach); process.once('SIGTERM', detach);
try { process.exitCode = await run(process.argv.slice(2), undefined, controller.signal); }
catch (error) {
  const failure = error instanceof CliError ? error : new CliError('client_input_or_operation_failed');
  process.stderr.write(`${JSON.stringify({ error: failure.code, ...(failure.status ? { status: failure.status } : {}) })}\n`);
  process.exitCode = 1;
} finally { process.off('SIGINT', detach); process.off('SIGTERM', detach); }
