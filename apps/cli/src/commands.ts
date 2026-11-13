import { accountStrategyCommand } from '../../../packages/contracts/src/account-strategy.ts';
import { parseArgs } from 'node:util';
import { readFile } from 'node:fs/promises';
import { connect } from './client.ts';
import { CliError, forgetProfile, loadProfile, origin } from './profile.ts';
import { login } from './login.ts';
import { observe, observeParameterScan, observePortfolio } from './watch.ts';
import { schemas } from './schema.ts';
import type { AccountObservationStart, ResearchStart, SessionView } from '../../../packages/contracts/src/index.ts';

const help = `quazonai — HTTP client (Node 22.19+), JSON receipts
  login [--origin URL] [--name NAME] [--replace] [--ca-certificate FILE]
  identity | logout | forget
  schema list [--domain] | schema show NAME [--domain]
  datasets | usage
  datasets coverage ID --file REQUEST.json (selectors [{data_type,identifier}] or bar_types; start_ns/end_ns as inclusive ts_init strings)
  maintenance status | enter --file ORIGINAL_REQUEST.json | resume --file ORIGINAL_REQUEST.json
  pm list | get ID | delete ID | decision ID | history ID | events ID | capabilities
  pm recompute ID --file ORIGINAL_COMMAND.json
  pm retire ID --file ORIGINAL_COMMAND.json | cycle ID | capacity ID
  pm targets ID --file ORIGINAL_COMMAND.json | refresh-data ID --file ORIGINAL_COMMAND.json
  campaign get ID | trials ID | compare ID
  campaign pause ID --file COMMAND.json
  strategy versions ID | assessments ID | pause ID --file COMMAND.json | resume ID --file COMMAND.json
  research models | list [--cursor JSON] | get ID | stop ID | delete ID
  research start --file REQUEST.json [--request-id ORIGINAL_ID]
  research start --goal TEXT --dataset ID --model provider:modelId --request-id ORIGINAL_ID
  research watch ID [--timeout SECONDS] | research wait ID [--timeout SECONDS]
  research diagnostics ID | research resume ID --submission-id PREVIOUS_TERMINAL_SUBMISSION [--instruction TEXT]
  parameter-scan list [--file QUERY.json] | get ID | stop ID
  parameter-scan trials ID [--file QUERY.json]
  parameter-scan watch ID [--timeout SECONDS] | parameter-scan wait ID [--timeout SECONDS]
    Historical scan records only; no parameter generation, ranking, or new scans.
  portfolio list | get ID | delete ID
  portfolio watch ID [--timeout SECONDS] | portfolio wait ID [--timeout SECONDS]
  observation start --file REQUEST.json | list | get ID | refresh ID | stop ID
  account-strategy status OBSERVATION_ID | receipt OBSERVATION_ID ORIGINAL_REQUEST_ID
  account-strategy start OBSERVATION_ID --request-id ORIGINAL_ID --account-source-id SOURCE --expected-revision REV
  account-strategy stop OBSERVATION_ID --request-id ORIGINAL_ID
    Read status first; copy its accountSourceId/strategyRevision unchanged. Stale START is rejected, never auto-refreshed.
    Explicit commands operate the configured native strategy (EMA_CROSS account wiring only); live START can place real orders.
  observation size ID --file SCENARIO.json | preview-order ID --file ORDER.json (read-only; never submits orders)
  capital status OBSERVATION_ID | context OBSERVATION_ID | get OBSERVATION_ID EXIT_ID
  observation cancel-protection OBSERVATION_ID
  capital preview OBSERVATION_ID --file REQUEST.json | command OBSERVATION_ID --file REQUEST.json
  settings get | set --file SETTINGS.json | set --tick-book-cores COUNT
  extensions list | enable NAME | disable NAME
Retain the original request and ID after uncertainty. No automatic write retries.
Watch/wait have no default deadline; detaching does not stop admitted work.
Delete hides an inactive object; original evidence and receipts remain. It never cancels remote work or liquidates positions.
Observation start only binds an account. Account-strategy start explicitly starts trading under the configured native strategy. 'client' is a thin alias.
`;
const options = {
  help: { type: 'boolean' as const, short: 'h' }, version: { type: 'boolean' as const },
  origin: { type: 'string' as const }, name: { type: 'string' as const }, replace: { type: 'boolean' as const },
  'development-http': { type: 'boolean' as const }, 'ca-certificate': { type: 'string' as const },
  file: { type: 'string' as const }, goal: { type: 'string' as const }, dataset: { type: 'string' as const },
  model: { type: 'string' as const }, 'request-id': { type: 'string' as const },
  cursor: { type: 'string' as const }, timeout: { type: 'string' as const }, domain: { type: 'boolean' as const },
  'submission-id': { type: 'string' as const },
  instruction: { type: 'string' as const },
  'account-source-id': { type: 'string' as const }, 'expected-revision': { type: 'string' as const },
  'tick-book-cores': { type: 'string' as const },
};
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new CliError('json_object_required');
  return value as Record<string, unknown>;
}
function exactNumbers(value: unknown): void {
  if (typeof value === 'number' && (!Number.isFinite(value) || (Number.isInteger(value) && !Number.isSafeInteger(value)))) throw new CliError('invalid_json_number_use_decimal_string');
  if (value && typeof value === 'object') Object.values(value).forEach(exactNumbers);
}
async function fileBody(path?: string) {
  if (!path) throw new CliError('file_required');
  let bytes: string;
  try { bytes = await readFile(path, 'utf8'); } catch { throw new CliError('input_file_unreadable'); }
  let body;
  try { body = object(JSON.parse(bytes)); } catch { throw new CliError('input_json_invalid'); }
  exactNumbers(body);
  return { bytes, body };
}
async function fileQuery(path?: string, jsonFields: readonly string[] = []): Promise<string> {
  if (!path) return '';
  const { body } = await fileBody(path);
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(body)) {
    if (jsonFields.includes(key) && value && typeof value === 'object' && !Array.isArray(value)) {
      query.set(key, JSON.stringify(value)); continue;
    }
    if (value === null || !['string', 'number', 'boolean'].includes(typeof value)) throw new CliError('query_scalar_required');
    query.set(key, String(value));
  }
  return query.size ? `?${query}` : '';
}
function requestId(body: Record<string, unknown>, expected?: string) {
  if (typeof body.requestId !== 'string' || !body.requestId) throw new CliError('original_request_id_required');
  if (expected !== undefined && body.requestId !== expected) throw new CliError('request_id_conflict');
}
function pmRevision(body: Record<string, unknown>) {
  if (typeof body.expectedRevision !== 'string' || !/^\d+$/.test(body.expectedRevision)) throw new CliError('expected_revision_required');
}
function researchId(value?: string): number {
  if (!value || !/^\d+$/.test(value) || !Number.isSafeInteger(Number(value))) throw new CliError('native_numeric_research_id_required');
  return Number(value);
}
function resourceId(value?: string): string {
  if (!value) throw new CliError('resource_id_required');
  return encodeURIComponent(value);
}

export async function run(args: string[], emit: (value: unknown) => void = (value) => process.stdout.write(`${JSON.stringify(value)}\n`), signal?: AbortSignal): Promise<number> {
  const { values, positionals } = parseArgs({ args, options, allowPositionals: true });
  if (positionals[0] === 'client') positionals.shift();
  if (values.version) { process.stdout.write('quazonai 3.0.0-dev.1\n'); return 0; }
  if (values.help || positionals.length === 0) { process.stdout.write(help); return 0; }
  if (positionals.length > 3 && !((positionals[0] === 'capital' && positionals[1] === 'get' || positionals[0] === 'account-strategy' && positionals[1] === 'receipt') && positionals.length === 4)) throw new CliError('unexpected_argument');
  const [group, action, id, exitId] = positionals;
  if (group === 'schema') {
    if (!['list', 'show'].includes(action) || (action === 'show' && !id)) throw new CliError('schema_command_required');
    emit(await schemas(action === 'show' ? id : undefined, values.domain)); return 0;
  }
  if (group === 'login') {
    emit(await login({ origin: values.origin, name: values.name, replace: values.replace, developmentHttp: values['development-http'], caCertificate: values['ca-certificate'], signal })); return 0;
  }
  if (group === 'forget') { await forgetProfile(); emit({ forgotten: true, remoteRevocationRequested: false }); return 0; }

  // Prepare the complete immutable request before connecting. Never create keys
  // or refresh a body/revision automatically after an unknown outcome.
  let method = 'GET'; let path: string; let body: string | undefined;
  if (group === 'research' && action === 'start') {
    let submission: ResearchStart;
    if (values.file) {
      if (values.goal || values.dataset || values.model) throw new CliError('file_or_goal_required');
      const file = await fileBody(values.file); requestId(file.body, values['request-id']);
      submission = file.body as unknown as ResearchStart; body = file.bytes;

    } else {
      const match = /^([^:]+):(.+)$/.exec(values.model ?? '');
      if (!values.goal || !values.dataset || !match || !values['request-id']) throw new CliError('goal_dataset_model_request_id_required');
      submission = { requestId: values['request-id'], goal: values.goal, datasetId: values.dataset, model: { provider: match[1]!, modelId: match[2]! } };
      body = JSON.stringify(submission);
    }
    requestId(submission as unknown as Record<string, unknown>, values['request-id']);
    method = 'POST'; path = '/research';
  } else if (group === 'parameter-scan') {
    if (action === 'list') path = `/parameter-scans${await fileQuery(values.file)}`;
    else if (action === 'trials') {
      path = `/parameter-scans/${resourceId(id)}/trials${await fileQuery(values.file)}`;
    } else if (['get', 'stop', 'watch', 'wait'].includes(action)) {
      path = `/parameter-scans/${resourceId(id)}`;
      if (action === 'stop') { method = 'POST'; path += '/stop'; }
    } else throw new CliError('unknown_parameter_scan_command');
  } else if (group === 'pm') {
    if (action === 'list') path = '/mandates';
    else if (action === 'capabilities') path = '/capabilities';
    else if (action === 'delete') { method = 'DELETE'; path = `/mandates/${resourceId(id)}`; }
    else if (['get', 'decision', 'history', 'events'].includes(action)) {
      path = `/mandates/${resourceId(id)}${action === 'get' ? '' : action === 'history' ? '/decisions' : `/${action}`}`;
    } else if (action === 'cycle') path = `/cycles/${resourceId(id)}`;
    else if (action === 'capacity') path = `/capacity-estimates/${resourceId(id)}`;
    else if (['recompute', 'retire', 'targets', 'refresh-data'].includes(action)) {
      const file = await fileBody(values.file); requestId(file.body, values['request-id']); pmRevision(file.body);
      method = 'POST'; path = `/mandates/${resourceId(id)}/${action}`; body = file.bytes;
    } else throw new CliError('unknown_pm_command');
  } else if (group === 'campaign') {
    if (['get', 'trials', 'compare'].includes(action)) {
      path = `/campaigns/${resourceId(id)}${action === 'get' ? '' : action === 'compare' ? '/decision' : '/trials'}`;
    } else if (action === 'pause') {
      const file = await fileBody(values.file); requestId(file.body, values['request-id']); pmRevision(file.body);
      method = 'POST'; path = `/campaigns/${resourceId(id)}/${action}`; body = file.bytes;
    } else throw new CliError('unknown_campaign_command');
  } else if (group === 'strategy') {
    if (action === 'versions' || action === 'assessments') path = `/strategies/${resourceId(id)}/${action}`;
    else if (action === 'pause' || action === 'resume') {
      const file = await fileBody(values.file); requestId(file.body, values['request-id']); pmRevision(file.body);
      method = 'POST'; path = `/strategies/${resourceId(id)}/${action}`; body = file.bytes;
    } else throw new CliError('unknown_strategy_command');
  } else if (group === 'research') {
    if (action === 'models') path = '/research/models';
    else if (action === 'list') {
      if (values.cursor) object(JSON.parse(values.cursor));
      path = `/research${values.cursor ? `?cursor=${encodeURIComponent(values.cursor)}` : ''}`;
    } else if (['get', 'watch', 'wait', 'stop', 'diagnostics', 'resume', 'delete'].includes(action)) {
      path = `/research/${researchId(id)}`;
      if (action === 'stop') { method = 'POST'; path += '/stop'; }
      if (action === 'delete') method = 'DELETE';
      if (action === 'diagnostics') path += '/diagnostics';
      if (action === 'resume') {
        const submissionId = researchId(values['submission-id']);
        if (submissionId < 1) throw new CliError('original_failed_submission_id_required');
        method = 'POST'; path += '/resume'; body = JSON.stringify({ submissionId, instruction: values.instruction });
      }
    } else throw new CliError('unknown_research_command');
  } else if (group === 'maintenance') {
    path = '/maintenance';
    if (action === 'enter' || action === 'resume') {
      const file = await fileBody(values.file); requestId(file.body, values['request-id']);
      if (action === 'resume' && (typeof file.body.epoch !== 'string' || !file.body.epoch)) throw new CliError('maintenance_epoch_required');
      method = 'POST'; path += `/${action}`; body = file.bytes;
    } else if (action !== 'status') throw new CliError('unknown_maintenance_command');
  } else if (group === 'portfolio' || group === 'backtest') {
    if (action === 'list') path = '/portfolios';
    else if (['get', 'delete', 'watch', 'wait'].includes(action)) { path = `/portfolios/${resourceId(id)}`; if (action === 'delete') method = 'DELETE'; }
    else throw new CliError('unknown_portfolio_command');
  } else if (group === 'observation') {
    if (action === 'start') {
      const file = await fileBody(values.file); requestId(file.body, values['request-id']);
      const input = file.body as unknown as AccountObservationStart;
      if (input.purpose !== 'ACCOUNT_ONLY') throw new CliError('native_strategy_only_account_binding_required');
      if (!['paper', 'live'].includes(input.mode)) throw new CliError('observed_mode_paper_or_live_required');
      body = file.bytes; method = 'POST'; path = '/observations';
    } else if (action === 'list' || action === 'get') {
      if (action === 'get') resourceId(id);
      path = '/observations'; // Get is optional server-side; select from the actual list.
    }
    else if (action === 'size' || action === 'preview-order') { method = 'POST'; path = `/observations/${resourceId(id)}/${action === 'size' ? 'sizing/fixed-risk' : 'orders/preview'}`; body = (await fileBody(values.file)).bytes; }
    else if (action === 'refresh' || action === 'stop' || action === 'cancel-protection') { method = 'POST'; path = `/observations/${resourceId(id)}/${action}`; }
    else throw new CliError('unknown_observation_command');
  } else if (group === 'account-strategy') {
    path = `/observations/${resourceId(id)}/strategy`;
    if (action === 'receipt') path += `/commands/${resourceId(exitId)}`;
    else if (action === 'start' || action === 'stop') {
      if (!values['request-id']?.trim()) throw new CliError('original_request_id_required');
      method = 'POST'; path += '/commands';
      body = JSON.stringify(accountStrategyCommand({ requestId: values['request-id'], action: action.toUpperCase(), ...(action === 'start' ? { expectedAccountSourceId: values['account-source-id'], expectedStrategyRevision: values['expected-revision'] } : {}) }));
    } else if (action !== 'status') throw new CliError('unknown_account_strategy_command');
  } else if (group === 'capital') {
    path = `/observations/${resourceId(id)}/capital-exits`;
    if (action === 'context') path += '/planning-context';
    else if (action === 'get') path += `/${resourceId(exitId)}`;
    else if (action === 'preview' || action === 'command') {
      const file = await fileBody(values.file); requestId(file.body, values['request-id']); body = file.bytes; method = 'POST'; path += action === 'preview' ? '/preview' : '/commands';
    } else if (action !== 'status') throw new CliError('unknown_capital_command');
  } else if (group === 'settings') {
    path = '/settings';
    if (action === 'set') {
      if (values['tick-book-cores'] !== undefined) {
        if (values.file || !/^[1-9]\d*$/.test(values['tick-book-cores']) || !Number.isSafeInteger(Number(values['tick-book-cores']))) throw new CliError('tick_book_cores_invalid');
        body = JSON.stringify({ tickBookCores: Number(values['tick-book-cores']) });
      } else body = (await fileBody(values.file)).bytes;
      method = 'PATCH';
    }
    else if (action !== 'get') throw new CliError('unknown_settings_command');
  } else if (group === 'extensions') {
    path = '/extensions';
    if (action === 'enable' || action === 'disable') { path += `/${resourceId(id)}`; method = 'PATCH'; body = JSON.stringify({ enabled: action === 'enable' }); }
    else if (action !== 'list') throw new CliError('unknown_extensions_command');
  } else if (group === 'identity') path = '/session';
  else if (group === 'logout') { method = 'DELETE'; path = '/devices/current'; }
  else if (group === 'datasets' && action === 'coverage') {
    path = `/datasets/${resourceId(id)}/coverage`; method = 'POST'; body = (await fileBody(values.file)).bytes;
  } else if (group === 'datasets' || group === 'usage') path = `/${group}`;
  else throw new CliError('unknown_command');

  const profile = await loadProfile();
  if (values.origin && origin(values.origin) !== profile.origin) throw new CliError('login_replace_required');
  const client = await connect({ ...profile, ca_certificate: values['ca-certificate'] ?? profile.ca_certificate });
  try {
    if (group === 'parameter-scan' && (action === 'watch' || action === 'wait')) {
      const seconds = values.timeout === undefined ? undefined : Number(values.timeout);
      if (seconds !== undefined && (!Number.isFinite(seconds) || seconds <= 0 || !Number.isSafeInteger(seconds * 1000))) throw new CliError('positive_timeout_seconds_required');
      const result = await observeParameterScan(client, id!, seconds, action === 'watch' ? emit : undefined, signal);
      emit(result);
      if (signal?.aborted) return 130;
      return result.settled ? (result.scan?.status === 'COMPLETED' ? 0 : 2) : action === 'wait' ? 124 : 0;
    }
    if (group === 'research' && (action === 'watch' || action === 'wait')) {
      const seconds = values.timeout === undefined ? undefined : Number(values.timeout);
      if (seconds !== undefined && (!Number.isFinite(seconds) || seconds <= 0 || !Number.isSafeInteger(seconds * 1000))) throw new CliError('positive_timeout_seconds_required');
      const result = await observe(client, researchId(id), seconds, action === 'watch' ? emit : undefined, signal);
      emit(result);
      if (signal?.aborted) return 130;
      return result.settled ? (result.submissionStatus === 'done' ? 0 : 2) : action === 'wait' ? 124 : 0;
    }
    if ((group === 'portfolio' || group === 'backtest') && (action === 'watch' || action === 'wait')) {
      const seconds = values.timeout === undefined ? undefined : Number(values.timeout);
      if (seconds !== undefined && (!Number.isFinite(seconds) || seconds <= 0 || !Number.isSafeInteger(seconds * 1000))) throw new CliError('positive_timeout_seconds_required');
      const result = await observePortfolio(client, id!, seconds, action === 'watch' ? emit : undefined, signal);
      emit(result);
      if (signal?.aborted) return 130;
      return result.settled ? (result.portfolio?.status === 'COMPUTED' ? 0 : 2) : action === 'wait' ? 124 : 0;
    }
    const result = await client.request(method, path, body, signal);
    if (group === 'observation' && action === 'get') {
      const item = (result as { id: string }[]).find((value) => value.id === id);
      if (!item) throw new CliError('observation_not_found', 404);
      emit(item);
    } else if (group === 'logout') { await forgetProfile(); emit({ revoked: true, forgotten: true }); }
    else emit(result);
    return group === 'identity' && !(result as SessionView).authenticated ? 1 : 0;
  } finally { await client.close(); }
}
