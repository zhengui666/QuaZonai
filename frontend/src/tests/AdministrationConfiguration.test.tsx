import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { AdministrationPage } from '../pages/AdministrationPage';
import { jsonResponse, renderApp } from './testUtils';

vi.mock('../components/admin/RuntimeConfigurationPanel', () => ({
  RuntimeConfigurationPanel: () => <div>Runtime configuration</div>,
}));

beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, 'hasPointerCapture', { configurable: true, value: () => false });
  Object.defineProperty(HTMLElement.prototype, 'releasePointerCapture', { configurable: true, value: () => {} });
  Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { configurable: true, value: () => {} });
});

const runtime = {
  revision: 1,
  codex_model: null,
  codex_reasoning_effort: null,
  codex_fast_mode: false,
  codex_use_default_model_settings: true,
  codex_base_url: null,
  codex_api_key_configured: false,
  codex_login_configured: false,
  max_plugin_wheel_bytes: 1,
  plugin_validation_timeout_seconds: 1,
  bundle_build_timeout_seconds: 1,
  plugin_job_timeout_seconds: 1,
  mission_job_timeout_seconds: 1,
  job_poll_seconds: 1,
  job_lease_seconds: 1,
};

const dataSource = {
  id: 'source-1',
  name: 'PIT Data',
  connector_key: 'pit-data',
  provider: 'Approved',
  state: 'ACTIVE',
  universe_scope: ['universe-1'],
  field_schema: { event_time: 'timestamp', available_at: 'timestamp' },
  license_classification: 'LICENSED',
  availability_semantics: { available_at_field: 'available_at' },
  update_cadence: null,
  preflight_state: 'PENDING',
  public_config: {},
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
};

const preflightOperation = {
  id: 'operation-1',
  kind: 'DATA_SOURCE_PREFLIGHT',
  resource_type: 'governed_data_source',
  resource_id: 'source-1',
  state: 'READY',
  attempt: 1,
  last_error: null,
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
};

const universe = {
  id: 'universe-1',
  universe_key: 'US_EQUITIES',
  version_no: 1,
  name: 'US Equities',
  state: 'ACTIVE',
  spec: {},
  created_at: '2025-01-01T00:00:00Z',
};

const capitalContext = {
  id: 'capital-1',
  configuration_contract_version: null,
  configuration_state: 'LEGACY_UNAVAILABLE',
  source_type: 'ADMIN',
  source_downstream_system_id: null,
  base_currency: 'USD',
  deployable_capital: '100000',
  observed_at: '2025-01-01T00:00:00Z',
  valid_until: '2025-02-01T00:00:00Z',
  notes: null,
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
};

const datasets = ['DISCOVERY', 'VALIDATION', 'SEALED'].map((partition) => ({
  id: `dataset-${partition.toLowerCase()}`,
  data_source_id: 'source-1',
  universe_version_id: 'universe-1',
  universe_name: 'US Equities',
  revision_no: 1,
  partition,
  data_class: 'VENDOR',
  origin: 'PIT Data',
  promotability: 'PROMOTABLE',
  schema_version: 'bars-v1',
  event_start: '2025-01-01T00:00:00Z',
  event_end: '2025-01-02T00:00:00Z',
  available_start: '2025-01-01T00:05:00Z',
  available_end: '2025-01-02T00:05:00Z',
  row_count: 1,
  quality_state: 'VALID',
  point_in_time_state: 'VALID',
  created_at: '2025-01-01T00:00:00Z',
}));

const downstreams = [
  { id: 'paper-1', name: 'Paper Consumer', environment_type: 'PAPER', enabled: true },
  { id: 'live-1', name: 'Live Consumer', environment_type: 'LIVE', enabled: true },
].map((downstream) => ({
  ...downstream,
  package_contract_version: '1',
  feedback_contract_version: '1',
  compatibility: [],
  preflight_state: 'READY',
  public_config: {},
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
}));

async function selectOption(user: ReturnType<typeof userEvent.setup>, dialog: HTMLElement, name: string, option: string) {
  await user.click(within(dialog).getByRole('combobox', { name }));
  await user.click(await screen.findByRole('option', { name: option }));
}

function canonicalFetch(input: RequestInfo | URL, init?: RequestInit) {
  const path = String(input);
  if (init?.method === 'POST' && path === '/api/v1/universes') {
    return jsonResponse(universe, 201);
  }
  if (init?.method === 'POST' && path === '/api/v1/portfolio-mandates') return jsonResponse({}, 201);
  if (init?.method === 'POST' && path === '/api/v1/capital-contexts') return jsonResponse({ ...capitalContext, id: 'capital-2', configuration_contract_version: 'CAPITAL_CONTEXT_V1', configuration_state: 'V1_CONFIGURED' }, 201);
  if (init?.method === 'POST' && path === '/api/v1/evaluation-dataset-selections') return jsonResponse({}, 201);
  if (init?.method === 'POST' && path === '/api/v1/evaluation-design-versions') return jsonResponse({}, 201);
  if (init?.method === 'POST' && path === '/api/v1/promotion-policy-versions') return jsonResponse({}, 201);
  if (init?.method === 'POST' && path === '/api/v1/data-sources/source-1/preflight') return jsonResponse(preflightOperation, 202);
  if (path === '/api/v1/readiness') return jsonResponse({ SYSTEM_READY: false, RESEARCH_READY: false, PAPER_HANDOFF_READY: false, LIVE_HANDOFF_READY: false });
  if (path === '/api/v1/system/health') return jsonResponse({});
  if (path === '/api/v1/system/runtime-configuration') return jsonResponse(runtime);
  if (path === '/api/v1/operations/operation-1') return jsonResponse(preflightOperation);
  if (path === '/api/v1/data-sources') return jsonResponse({ items: [dataSource], next_cursor: null });
  if (path === '/api/v1/universes') return jsonResponse({ items: [universe], next_cursor: null });
  if (path === '/api/v1/capital-contexts') return jsonResponse({ items: [capitalContext], next_cursor: null });
  if (path === '/api/v1/datasets') return jsonResponse({ items: datasets, next_cursor: null });
  if (path === '/api/v1/evaluation-dataset-selections' || path === '/api/v1/evaluation-design-versions' || path === '/api/v1/promotion-policy-versions' || path === '/api/v1/portfolio-mandates') return jsonResponse({ items: [], next_cursor: null });
  if (path === '/api/v1/downstream-systems') return jsonResponse({ items: downstreams, next_cursor: null });
  throw new Error(`Unexpected request: ${path}`);
}

afterEach(() => vi.restoreAllMocks());

describe('canonical Administration configuration', () => {
  it('loads resource facts only from canonical root endpoints', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(canonicalFetch);
    renderApp(<AdministrationPage />);

    await screen.findByRole('button', { name: 'Create Universe' });
    const paths = fetchMock.mock.calls.map(([input]) => String(input));
    expect(paths).toEqual(expect.arrayContaining([
      '/api/v1/universes',
      '/api/v1/data-sources',
      '/api/v1/datasets',
      '/api/v1/evaluation-dataset-selections',
      '/api/v1/evaluation-design-versions',
      '/api/v1/promotion-policy-versions',
      '/api/v1/portfolio-mandates',
      '/api/v1/capital-contexts',
      '/api/v1/downstream-systems',
    ]));
    expect(paths).not.toContain('/api/v1/configuration/universes');
    expect(paths).not.toContain('/api/v1/configuration/data-sources');
    expect(paths).not.toContain('/api/v1/configuration/datasets');
    expect(paths).not.toContain('/api/v1/configuration/evaluation-dataset-selections');
    expect(paths).not.toContain('/api/v1/configuration/evaluation-design-versions');
    expect(paths).not.toContain('/api/v1/configuration/promotion-policy-versions');
    expect(paths).not.toContain('/api/v1/configuration/portfolio-mandates');
    expect(paths).not.toContain('/api/v1/configuration/capital-contexts');
    expect(paths).not.toContain('/api/v1/configuration/downstream-systems');
  });

  it('submits an exact Evaluation Dataset Selection without a latest-revision selector', async () => {
    const user = userEvent.setup();
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(canonicalFetch);
    renderApp(<AdministrationPage />);

    await user.click(await screen.findByRole('button', { name: 'Create Evaluation Dataset Selection' }));
    const dialog = screen.getByRole('dialog');
    await selectOption(user, dialog, 'Evaluation Dataset Selection Universe', 'US Equities · v1');
    await selectOption(user, dialog, 'Discovery Dataset revision', 'US Equities · DISCOVERY · r1 · dataset-discovery');
    await selectOption(user, dialog, 'Validation Dataset revision', 'US Equities · VALIDATION · r1 · dataset-validation');
    await selectOption(user, dialog, 'Sealed Dataset revision', 'US Equities · SEALED · r1 · dataset-sealed');
    await user.click(within(dialog).getByRole('button', { name: 'Create Evaluation Dataset Selection' }));

    await waitFor(() => {
      const call = fetchMock.mock.calls.find(([input, options]) => String(input) === '/api/v1/evaluation-dataset-selections' && (options as RequestInit | undefined)?.method === 'POST');
      expect(call).toBeDefined();
      expect(JSON.parse(String((call?.[1] as RequestInit).body))).toEqual({
        universe_version_id: 'universe-1',
        discovery_dataset_revision_id: 'dataset-discovery',
        validation_dataset_revision_id: 'dataset-validation',
        sealed_dataset_revision_id: 'dataset-sealed',
        state: 'ENABLED',
      });
    });
  });

  it('submits an explicit Evaluation Design Version with decimal strings', async () => {
    const user = userEvent.setup();
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(canonicalFetch);
    renderApp(<AdministrationPage />);

    await user.click(await screen.findByRole('button', { name: 'Create Evaluation Design Version' }));
    const dialog = screen.getByRole('dialog');
    await selectOption(user, dialog, 'Evaluation Design Universe', 'US Equities · v1');
    await user.type(within(dialog).getByLabelText('Contract version'), 'EVALUATION_DESIGN_V1');
    await selectOption(user, dialog, 'Allowed model mode', 'Relative Score');
    await selectOption(user, dialog, 'Qualification role', 'Primary Alpha');
    await user.type(within(dialog).getByLabelText('Walk-forward folds'), '5');
    await user.type(within(dialog).getByLabelText('Annualization factor'), '252');
    await selectOption(user, dialog, 'Multiple testing method', 'Bonferroni');
    await user.type(within(dialog).getByLabelText('Multiple testing maximum trials'), '10');
    await selectOption(user, dialog, 'Qualification metric code', 'Sharpe Ratio');
    await selectOption(user, dialog, 'Qualification comparator', 'Minimum');
    await user.type(within(dialog).getByLabelText('Qualification threshold'), '1.25');
    await user.type(within(dialog).getByLabelText('Pass disclosure code'), 'ALPHA_PASS');
    await user.type(within(dialog).getByLabelText('Failure disclosure code'), 'ALPHA_FAIL');
    await user.type(within(dialog).getByLabelText('Inconclusive disclosure code'), 'ALPHA_INCONCLUSIVE');
    await user.type(within(dialog).getByLabelText('Invalid disclosure code'), 'ALPHA_INVALID');
    await user.click(within(dialog).getByRole('button', { name: 'Create Evaluation Design Version' }));

    await waitFor(() => {
      const call = fetchMock.mock.calls.find(([input, options]) => String(input) === '/api/v1/evaluation-design-versions' && (options as RequestInit | undefined)?.method === 'POST');
      expect(call).toBeDefined();
      expect(JSON.parse(String((call?.[1] as RequestInit).body))).toEqual({
        universe_version_id: 'universe-1',
        contract_version: 'EVALUATION_DESIGN_V1',
        allowed_model_mode: 'RELATIVE_SCORE',
        qualification_role: 'PRIMARY_ALPHA',
        walk_forward_folds: 5,
        annualization_factor: '252',
        multiple_testing_method: 'BONFERRONI',
        multiple_testing_max_trials: 10,
        qualification_metric_code: 'SHARPE_RATIO',
        qualification_comparator: 'MINIMUM',
        qualification_threshold: '1.25',
        pass_disclosure_code: 'ALPHA_PASS',
        failure_disclosure_code: 'ALPHA_FAIL',
        inconclusive_disclosure_code: 'ALPHA_INCONCLUSIVE',
        invalid_disclosure_code: 'ALPHA_INVALID',
        state: 'ACTIVE',
      });
    });
  });

  it('submits an Alpha typed Promotion Policy without legacy downstream fields', async () => {
    const user = userEvent.setup();
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(canonicalFetch);
    renderApp(<AdministrationPage />);

    await user.click(await screen.findByRole('button', { name: 'Create Promotion Policy Version' }));
    const dialog = screen.getByRole('dialog');
    await selectOption(user, dialog, 'Promotion purpose', 'Sealed To Qualified');
    await selectOption(user, dialog, 'Promotion mode', 'Manual Approval');
    await user.click(within(dialog).getByRole('button', { name: 'Add promotion gate' }));
    await selectOption(user, dialog, 'Gate 1 metric code', 'Sharpe Ratio');
    await selectOption(user, dialog, 'Gate 1 comparator', 'Minimum');
    await user.type(within(dialog).getByLabelText('Gate 1 threshold'), '1.5');
    await user.type(within(dialog).getByLabelText('Gate 1 ordinal'), '1');
    await user.click(within(dialog).getByRole('button', { name: 'Create Promotion Policy Version' }));

    await waitFor(() => {
      const call = fetchMock.mock.calls.find(([input, options]) => String(input) === '/api/v1/promotion-policy-versions' && (options as RequestInit | undefined)?.method === 'POST');
      expect(call).toBeDefined();
      expect(JSON.parse(String((call?.[1] as RequestInit).body))).toEqual({
        purpose: 'SEALED_TO_QUALIFIED',
        mode: 'MANUAL_APPROVAL',
        gates: [{ metric_code: 'SHARPE_RATIO', comparator: 'MINIMUM', threshold: '1.5', ordinal: 1 }],
        state: 'ACTIVE',
      });
    });
  });

  it('uses only typed V1 mandate fields and creates a canonical Capital Context', async () => {
    const user = userEvent.setup();
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(canonicalFetch);
    renderApp(<AdministrationPage />);

    await user.click(await screen.findByRole('button', { name: 'Create Mandate/version' }));
    const mandateDialog = screen.getByRole('dialog');
    expect(within(mandateDialog).queryByLabelText('Portfolio inputs (JSON)')).not.toBeInTheDocument();
    expect(within(mandateDialog).getByLabelText('Minimum weight')).toBeInTheDocument();
    expect(within(mandateDialog).getByDisplayValue('LONG_ONLY_MEAN_VARIANCE_V1')).toBeInTheDocument();

    await user.click(within(mandateDialog).getByRole('combobox', { name: 'Action' }));
    await user.click(await screen.findByRole('option', { name: 'Create Mandate' }));
    await user.type(within(mandateDialog).getByLabelText('Mandate key'), 'core-growth');
    await user.type(within(mandateDialog).getByLabelText('Mandate name'), 'Core Growth');
    await user.click(within(mandateDialog).getByRole('combobox', { name: 'Enabled' }));
    await user.click(await screen.findByRole('option', { name: 'Enabled' }));
    await user.type(within(mandateDialog).getByLabelText('Base currency'), 'USD');
    await user.click(within(mandateDialog).getByRole('combobox', { name: 'Eligible Universe version' }));
    await user.click(await screen.findByRole('option', { name: 'US Equities · v1' }));
    await user.type(within(mandateDialog).getByLabelText('Minimum Alpha count'), '2');
    for (const [label, value] of Object.entries({
      'Minimum weight': '0.01',
      'Maximum weight': '1',
      'Gross exposure limit': '1',
      'Net exposure target': '1',
      'Cash reserve': '0',
      'Turnover limit': '1',
      'Variance limit': '0.01',
      'Risk aversion': '1',
      'Cost aversion': '1',
      'Uncertainty aversion': '1',
      'Commission rate': '0.001',
      'Half-spread rate': '0.001',
      'Slippage rate': '0.001',
      'Impact rate': '0.001',
      'Impact breakpoint': '0.1',
    })) await user.type(within(mandateDialog).getByLabelText(label), value);
    await user.click(within(mandateDialog).getByRole('combobox', { name: 'Version state' }));
    await user.click(await screen.findByRole('option', { name: 'Active' }));
    await user.click(within(mandateDialog).getByRole('button', { name: 'Create Mandate' }));

    await waitFor(() => {
      const call = fetchMock.mock.calls.find(([input, options]) => String(input) === '/api/v1/portfolio-mandates' && (options as RequestInit | undefined)?.method === 'POST');
      expect(call).toBeDefined();
      expect(JSON.parse(String((call?.[1] as RequestInit).body))).toEqual({
        key: 'core-growth',
        name: 'Core Growth',
        enabled: true,
        policy_family: 'LONG_ONLY_MEAN_VARIANCE_V1',
        base_currency: 'USD',
        objective: 'MAXIMIZE_NET_RETURN',
        eligible_alpha_role: 'PRIMARY_ALPHA',
        universe_version_id: 'universe-1',
        minimum_alpha_count: 2,
        minimum_weight: '0.01',
        maximum_weight: '1',
        gross_exposure_limit: '1',
        net_exposure_target: '1',
        cash_reserve: '0',
        turnover_limit: '1',
        variance_limit: '0.01',
        risk_aversion: '1',
        cost_aversion: '1',
        uncertainty_aversion: '1',
        commission_rate: '0.001',
        half_spread_rate: '0.001',
        slippage_rate: '0.001',
        impact_rate: '0.001',
        impact_breakpoint: '0.1',
        state: 'ACTIVE',
      });
    });

    await user.click(screen.getByRole('button', { name: 'Create Capital Context' }));
    const capitalDialog = screen.getByRole('dialog');
    await user.type(within(capitalDialog).getByLabelText('Currency'), 'USD');
    await user.type(within(capitalDialog).getByLabelText('Deployable capital'), '123456.78');
    await user.type(within(capitalDialog).getByLabelText('Observed at (UTC ISO 8601)'), '2026-09-03T00:00:00Z');
    await user.type(within(capitalDialog).getByLabelText('Valid until (UTC ISO 8601)'), '2026-09-04T00:00:00Z');
    await user.click(within(capitalDialog).getByRole('button', { name: 'Create Capital Context' }));

    await waitFor(() => {
      const call = fetchMock.mock.calls.find(([input, options]) => String(input) === '/api/v1/capital-contexts' && (options as RequestInit | undefined)?.method === 'POST');
      expect(call).toBeDefined();
      expect(JSON.parse(String((call?.[1] as RequestInit).body))).toEqual({
        base_currency: 'USD',
        deployable_capital: '123456.78',
        observed_at: '2026-09-03T00:00:00Z',
        valid_until: '2026-09-04T00:00:00Z',
      });
    });
    expect(screen.getAllByText(/Legacy unavailable/i).length).toBeGreaterThan(0);
  });

  it('submits an immutable Universe through the canonical endpoint', async () => {
    const user = userEvent.setup();
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(canonicalFetch);
    renderApp(<AdministrationPage />);

    await user.click(await screen.findByRole('button', { name: 'Create Universe' }));
    const dialog = screen.getByRole('dialog');
    await user.type(within(dialog).getByLabelText('Universe key'), 'US_EQUITIES');
    await user.type(within(dialog).getByLabelText('Name'), 'US Equities');
    fireEvent.change(within(dialog).getByLabelText('Universe specification (JSON)'), { target: { value: JSON.stringify({
      instrument_schema: { instrument_id: 'string' },
      membership_rules: { listing: 'NYSE|NASDAQ' },
      calendar_semantics: { timezone: 'America/New_York' },
      currency_semantics: { base_currency: 'USD' },
      data_requirements: { available_at: 'required' },
      risk_model_family: 'EWMA',
      cost_model_family: 'SPREAD',
      capacity_model_family: 'ADV',
    }) } });
    await user.click(within(dialog).getByRole('button', { name: 'Create Universe' }));

    await waitFor(() => {
      const call = fetchMock.mock.calls.find(([input, options]) => String(input) === '/api/v1/universes' && (options as RequestInit | undefined)?.method === 'POST');
      expect(call).toBeDefined();
      expect(JSON.parse(String((call?.[1] as RequestInit).body))).toMatchObject({
        universe_key: 'US_EQUITIES',
        name: 'US Equities',
        data_requirements: { available_at: 'required' },
        capacity_model_family: 'ADV',
      });
    });
  });

  it('requests source preflight through the canonical empty-body operation route', async () => {
    const user = userEvent.setup();
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(canonicalFetch);
    renderApp(<AdministrationPage />);

    await user.click(await screen.findByRole('button', { name: 'Preflight data source' }));
    const dialog = screen.getByRole('dialog');
    await user.click(within(dialog).getByRole('button', { name: 'Preflight PIT Data' }));

    await waitFor(() => {
      const call = fetchMock.mock.calls.find(([input, options]) => String(input) === '/api/v1/data-sources/source-1/preflight' && (options as RequestInit | undefined)?.method === 'POST');
      expect(call).toBeDefined();
      expect(JSON.parse(String((call?.[1] as RequestInit).body))).toEqual({});
    });
    expect(await screen.findByText(/Data Source Preflight operation/)).toBeInTheDocument();
    await waitFor(() => expect(fetchMock.mock.calls.map(([input]) => String(input))).toContain('/api/v1/operations/operation-1'));
  });
});
