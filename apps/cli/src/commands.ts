import { parseArgs } from 'node:util';
import { readFile } from 'node:fs/promises';
import { connect } from './client.ts';
import { CliError, forgetProfile, loadProfile, origin } from './profile.ts';
import { login } from './login.ts';
import { observe } from './watch.ts';
import { schemas } from './schema.ts';
import { researchCapital, researchSpecification } from '../../../packages/contracts/src/research.ts';
import type { ObservationStart, PortfolioStart, ResearchStart, SessionView } from '../../../packages/contracts/src/index.ts';

const help = `quazonai — HTTP client (Node 22.19+), JSON receipts
  login [--origin URL] [--name NAME] [--replace] [--ca-certificate FILE]
  identity | logout | forget
  schema list [--domain] | schema show NAME [--domain]
  datasets | usage
  maintenance status | enter --file ORIGINAL_REQUEST.json | resume --file ORIGINAL_REQUEST.json
  pm list | get ID | decision ID | history ID | events ID | capabilities
  pm start --file MANDATE.json | recompute ID --file ORIGINAL_COMMAND.json
  pm retire ID --file ORIGINAL_COMMAND.json | cycle ID | capacity ID
  pm targets ID --file ORIGINAL_COMMAND.json | refresh-data ID --file ORIGINAL_COMMAND.json
  pm clarify-intent ID --file ORIGINAL_COMMAND.json
  campaign start --file REQUEST.json | get ID | trials ID | compare ID
  campaign close ID --file COMMAND.json | pause ID --file COMMAND.json | resume ID --file COMMAND.json
  strategy versions ID | assessments ID | pause ID --file COMMAND.json | resume ID --file COMMAND.json
  research models | list [--cursor JSON] | get ID | stop ID
  research start --file REQUEST.json [--request-id ORIGINAL_ID]
  research start --goal TEXT --dataset ID --model provider:modelId --max-trials N --request-id ORIGINAL_ID
  research watch ID [--timeout SECONDS] | research wait ID [--timeout SECONDS]
  research diagnostics ID | research resume ID --submission-id PREVIOUS_TERMINAL_SUBMISSION
  factor list | get ID | capacity ID [--file ASSUMPTIONS.json]
  portfolio build --file REQUEST.json | list | get ID
  portfolio capacity ID --file ASSUMPTIONS.json
  observation start --file REQUEST.json | list | get ID | refresh ID | stop ID
  observation automatic ID --file POLICY.json
  capital status OBSERVATION_ID | preview OBSERVATION_ID --file REQUEST.json | command OBSERVATION_ID --file REQUEST.json
  settings get | set --file SETTINGS.json
  extensions list | enable NAME | disable NAME
Retain the original request and ID after uncertainty. No automatic write retries.
Watch/wait have no default deadline; detaching does not stop admitted work.
Paper/live bind an observed account, never launch trading. 'client' is a thin alias.
`;
const options = {
  help: { type: 'boolean' as const, short: 'h' }, version: { type: 'boolean' as const },
  origin: { type: 'string' as const }, name: { type: 'string' as const }, replace: { type: 'boolean' as const },
  'development-http': { type: 'boolean' as const }, 'ca-certificate': { type: 'string' as const },
  file: { type: 'string' as const }, goal: { type: 'string' as const }, dataset: { type: 'string' as const },
  model: { type: 'string' as const }, 'max-trials': { type: 'string' as const }, 'request-id': { type: 'string' as const },
  cursor: { type: 'string' as const }, timeout: { type: 'string' as const }, domain: { type: 'boolean' as const },
  'submission-id': { type: 'string' as const },
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
  if (positionals.length > 3) throw new CliError('unexpected_argument');
  const [group, action, id] = positionals;
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
      if (values.goal || values.dataset || values.model || values['max-trials']) throw new CliError('file_or_goal_required');
      const file = await fileBody(values.file); requestId(file.body, values['request-id']);
      submission = file.body as unknown as ResearchStart; body = file.bytes;
      const input = object('input' in submission ? submission.input : submission); const specification = input.specification;
      if (specification !== undefined) {
        try { const spec = researchSpecification(specification); if (input.goal !== spec.question) throw new Error('research_question_mismatch'); }
        catch (error) { throw new CliError(error instanceof Error ? error.message : 'invalid_research_specification'); }
        const maximum = 'input' in submission ? object(input.policy).max_trials : input.maxTrials;
        if (!Number.isSafeInteger(maximum) || Number(maximum) < 0) throw new CliError('research_trial_budget_required');
      }
    } else {
      const match = /^([^:]+):(.+)$/.exec(values.model ?? '');
      const maximum = Number(values['max-trials']);
      if (!values.goal || !values.dataset || !match || !Number.isSafeInteger(maximum) || maximum < 0 || !values['request-id']) throw new CliError('goal_dataset_model_max_trials_request_id_required');
      submission = { requestId: values['request-id'], goal: values.goal, datasetId: values.dataset, model: { provider: match[1]!, modelId: match[2]! }, maxTrials: maximum };
      body = JSON.stringify(submission);
    }
    requestId(submission as unknown as Record<string, unknown>, values['request-id']);
    method = 'POST'; path = '/research';
  } else if (group === 'pm') {
    if (action === 'list') path = '/mandates';
    else if (action === 'capabilities') path = '/capabilities';
    else if (action === 'start') {
      const file = await fileBody(values.file); requestId(file.body, values['request-id']);
      method = 'POST'; path = '/mandates'; body = file.bytes;
    } else if (['get', 'decision', 'history', 'events'].includes(action)) {
      path = `/mandates/${resourceId(id)}${action === 'get' ? '' : action === 'history' ? '/decisions' : `/${action}`}`;
    } else if (action === 'cycle') path = `/cycles/${resourceId(id)}`;
    else if (action === 'capacity') path = `/capacity-estimates/${resourceId(id)}`;
    else if (['recompute', 'retire', 'targets', 'refresh-data', 'clarify-intent'].includes(action)) {
      const file = await fileBody(values.file); requestId(file.body, values['request-id']); pmRevision(file.body);
      method = 'POST'; path = `/mandates/${resourceId(id)}/${action === 'clarify-intent' ? 'intent-clarifications' : action}`; body = file.bytes;
    } else throw new CliError('unknown_pm_command');
  } else if (group === 'campaign') {
    if (action === 'start') {
      const file = await fileBody(values.file); requestId(file.body, values['request-id']);
      method = 'POST'; path = '/campaigns'; body = file.bytes;
    } else if (['get', 'trials', 'compare'].includes(action)) {
      path = `/campaigns/${resourceId(id)}${action === 'get' ? '' : action === 'compare' ? '/decision' : '/trials'}`;
    } else if (['close', 'pause', 'resume'].includes(action)) {
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
    } else if (['get', 'watch', 'wait', 'stop', 'diagnostics', 'resume'].includes(action)) {
      path = `/research/${researchId(id)}`;
      if (action === 'stop') { method = 'POST'; path += '/stop'; }
      if (action === 'diagnostics') path += '/diagnostics';
      if (action === 'resume') {
        const submissionId = researchId(values['submission-id']);
        if (submissionId < 1) throw new CliError('original_failed_submission_id_required');
        method = 'POST'; path += '/resume'; body = JSON.stringify({ submissionId });
      }
    } else throw new CliError('unknown_research_command');
  } else if (group === 'maintenance') {
    path = '/maintenance';
    if (action === 'enter' || action === 'resume') {
      const file = await fileBody(values.file); requestId(file.body, values['request-id']);
      if (action === 'resume' && (typeof file.body.epoch !== 'string' || !file.body.epoch)) throw new CliError('maintenance_epoch_required');
      method = 'POST'; path += `/${action}`; body = file.bytes;
    } else if (action !== 'status') throw new CliError('unknown_maintenance_command');
  } else if (group === 'factor') {
    if (action === 'list') path = '/factors';
    else if (action === 'get' || action === 'capacity') path = `/factors/${resourceId(id)}${action === 'capacity' ? '/capacity' : ''}`;
    else throw new CliError('unknown_factor_command');
    if (action === 'capacity' && values.file) {
      const file = await fileBody(values.file);
      try { researchCapital(file.body); } catch (error) { throw new CliError(error instanceof Error ? error.message : 'invalid_capacity_assumptions'); }
      method = 'POST'; body = file.bytes;
    }
  } else if (group === 'portfolio' || group === 'backtest') {
    if (action === 'build') {
      const file = await fileBody(values.file); requestId(file.body, values['request-id']);
      const input = file.body as unknown as PortfolioStart;
      if (typeof input.capital !== 'string') throw new CliError('capital_decimal_string_required');
      body = file.bytes; method = 'POST'; path = '/portfolios';
    } else if (action === 'list') path = '/portfolios';
    else if (action === 'get') path = `/portfolios/${resourceId(id)}`;
    else if (action === 'capacity') {
      const file = await fileBody(values.file);
      try { researchCapital(file.body); } catch (error) { throw new CliError(error instanceof Error ? error.message : 'invalid_capacity_assumptions'); }
      method = 'POST'; path = `/portfolios/${resourceId(id)}/capacity`; body = file.bytes;
    } else throw new CliError('unknown_portfolio_command');
  } else if (group === 'observation') {
    if (action === 'start') {
      const file = await fileBody(values.file); requestId(file.body, values['request-id']);
      const input = file.body as unknown as ObservationStart;
      if (!['paper', 'live'].includes(input.mode)) throw new CliError('observed_mode_paper_or_live_required');
      body = file.bytes; method = 'POST'; path = '/observations';
    } else if (action === 'list' || action === 'get') {
      if (action === 'get') resourceId(id);
      path = '/observations'; // Get is optional server-side; select from the actual list.
    }
    else if (action === 'automatic') { method = 'POST'; path = `/observations/${resourceId(id)}/automatic`; body = (await fileBody(values.file)).bytes; }
    else if (action === 'refresh' || action === 'stop') { method = 'POST'; path = `/observations/${resourceId(id)}/${action}`; }
    else throw new CliError('unknown_observation_command');
  } else if (group === 'capital') {
    path = `/observations/${resourceId(id)}/capital-exits`;
    if (action === 'preview' || action === 'command') {
      const file = await fileBody(values.file); requestId(file.body, values['request-id']); body = file.bytes; method = 'POST'; path += action === 'preview' ? '/preview' : '/commands';
    } else if (action !== 'status') throw new CliError('unknown_capital_command');
  } else if (group === 'settings') {
    path = '/settings';
    if (action === 'set') { body = (await fileBody(values.file)).bytes; method = 'PATCH'; }
    else if (action !== 'get') throw new CliError('unknown_settings_command');
  } else if (group === 'extensions') {
    path = '/extensions';
    if (action === 'enable' || action === 'disable') { path += `/${resourceId(id)}`; method = 'PATCH'; body = JSON.stringify({ enabled: action === 'enable' }); }
    else if (action !== 'list') throw new CliError('unknown_extensions_command');
  } else if (group === 'identity') path = '/session';
  else if (group === 'logout') { method = 'DELETE'; path = '/devices/current'; }
  else if (group === 'datasets' || group === 'usage') path = `/${group}`;
  else throw new CliError('unknown_command');

  const profile = await loadProfile();
  if (values.origin && origin(values.origin) !== profile.origin) throw new CliError('login_replace_required');
  const client = await connect({ ...profile, ca_certificate: values['ca-certificate'] ?? profile.ca_certificate });
  try {
    if (group === 'research' && (action === 'watch' || action === 'wait')) {
      const seconds = values.timeout === undefined ? undefined : Number(values.timeout);
      if (seconds !== undefined && (!Number.isFinite(seconds) || seconds <= 0 || !Number.isSafeInteger(seconds * 1000))) throw new CliError('positive_timeout_seconds_required');
      const result = await observe(client, researchId(id), seconds, action === 'watch' ? emit : undefined, signal);
      emit(result);
      if (signal?.aborted) return 130;
      return result.settled ? (result.submissionStatus === 'done' ? 0 : 2) : action === 'wait' ? 124 : 0;
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
