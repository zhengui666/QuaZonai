import { DatabaseIcon, PlugsConnectedIcon } from '@phosphor-icons/react';
import { Button, Dialog, Select, TextArea, TextField } from '@radix-ui/themes';
import { useQueryClient } from '@tanstack/react-query';
import type { ColumnDef } from '@tanstack/react-table';
import { useState } from 'react';
import { RuntimeConfigurationPanel } from '../components/admin/RuntimeConfigurationPanel';
import { DataTable } from '../components/ui/DataTable';
import { EmptyState } from '../components/ui/EmptyState';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { KpiStrip } from '../components/ui/KpiStrip';
import { PageHeader } from '../components/ui/PageHeader';
import { PageSkeleton } from '../components/ui/Skeleton';
import { StateBadge } from '../components/ui/StateBadge';
import { ResponsiveDialogContent } from '../components/ui/ResponsiveDialogContent';
import { Section } from '../components/ui/Section';
import {
  useConfigurationCapitalContexts,
  useConfigurationDataSources,
  useConfigurationDatasets,
  useConfigurationDownstreams,
  useConfigurationEvaluationDatasetSelections,
  useConfigurationEvaluationDesignVersions,
  useConfigurationMandates,
  useConfigurationOperation,
  useConfigurationPromotionPolicyVersions,
  useConfigurationUniverses,
  useCreateConfigurationDataSource,
  useCreateConfigurationCapitalContext,
  useCreateConfigurationEvaluationDatasetSelection,
  useCreateConfigurationEvaluationDesignVersion,
  useCreateConfigurationMandate,
  useCreateConfigurationMandateVersion,
  useCreateConfigurationPromotionPolicyVersion,
  useCreateConfigurationUniverse,
  useCreateConfigurationUniverseVersion,
  useHealth,
  useReadiness,
  useRequestConfigurationDataSourcePreflight,
  useRequestConfigurationDatasetMaterialization,
  useRuntimeConfiguration,
} from '../lib/api/hooks';
import type {
  ConfigurationCapitalContext,
  ConfigurationDataSource,
  ConfigurationDataset,
  ConfigurationDownstream,
  ConfigurationDownstreamRegistration,
  ConfigurationEvaluationDatasetSelection,
  ConfigurationEvaluationDesignVersion,
  ConfigurationMandate,
  ConfigurationOperation,
  ConfigurationPromotionPolicyVersion,
  ConfigurationUniverse,
  UUID,
} from '../lib/api/types';
import { apiRequest, jsonBody } from '../lib/api/client';
import { formatCapitalAmount, formatCompactNumber, formatDateTime, formatNumber, humanize } from '../lib/format';

function ready(value: unknown) {
  return typeof value === 'boolean' ? value : Boolean((value as { ready?: boolean } | undefined)?.ready);
}

function parseJsonObject(value: string, label: string): Record<string, unknown> {
  try {
    const parsed: unknown = JSON.parse(value);
    if (!parsed || Array.isArray(parsed) || typeof parsed !== 'object') throw new Error('not an object');
    return parsed as Record<string, unknown>;
  } catch {
    throw new Error(`${label} must be a JSON object.`);
  }
}

function optionalJsonObject(value: string, label: string): Record<string, unknown> {
  return value.trim() ? parseJsonObject(value, label) : {};
}

function splitValues(value: string) {
  return value.split(',').map((item) => item.trim()).filter(Boolean);
}

export function CanonicalFieldList({ fields }: { fields?: string[] }) {
  const value = fields?.slice(0, 6).join(', ');
  return value ? <bdi dir="ltr">{value}</bdi> : <>—</>;
}

function JsonField({ label, value, onChange, placeholder, required = true }: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  required?: boolean;
}) {
  return <label className="qz-field"><span className="qz-label">{label}</span><TextArea aria-label={label} dir="ltr" value={value} onChange={(event) => onChange(event.target.value)} placeholder={placeholder} required={required} /></label>;
}

function FormError({ error }: { error: unknown }) {
  return error ? <ErrorPanel error={error} /> : null;
}

function positiveInteger(value: string, label: string) {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed < 1) throw new Error(`${label} must be a positive integer.`);
  return parsed;
}

const universeColumns: ColumnDef<ConfigurationUniverse, unknown>[] = [
  { accessorKey: 'name', header: 'Universe', meta: { messageKey: 'alpha.universe' } },
  { accessorKey: 'universe_key', header: 'Key', meta: { messageKey: 'admin.key' }, cell: ({ getValue }) => <span className="qz-mono">{String(getValue())}</span> },
  { accessorKey: 'version_no', header: 'Version', meta: { messageKey: 'admin.version' }, cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as number)}</span> },
  { accessorKey: 'state', header: 'State', meta: { messageKey: 'research.state', localizedSort: true }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'created_at', header: 'Created', meta: { messageKey: 'admin.created' }, cell: ({ getValue }) => formatDateTime(getValue() as string) },
];

const dataSourceColumns: ColumnDef<ConfigurationDataSource, unknown>[] = [
  { accessorKey: 'name', header: 'Source', meta: { messageKey: 'admin.source' } },
  { accessorKey: 'connector_key', header: 'Connector', meta: { messageKey: 'admin.plugin' }, cell: ({ getValue }) => <span className="qz-mono">{String(getValue())}</span> },
  { accessorKey: 'state', header: 'State', meta: { messageKey: 'research.state', localizedSort: true }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'preflight_state', header: 'Preflight', meta: { messageKey: 'admin.preflight', localizedSort: true }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { id: 'fields', header: 'Fields', meta: { messageKey: 'admin.fields' }, cell: ({ row }) => <span className="qz-list-subtitle"><CanonicalFieldList fields={Object.keys(row.original.field_schema)} /></span> },
];

const datasetColumns: ColumnDef<ConfigurationDataset, unknown>[] = [
  { accessorKey: 'id', header: 'Revision', meta: { messageKey: 'admin.revision' }, cell: ({ getValue }) => <span className="qz-mono">{String(getValue()).slice(0, 12)}</span> },
  { accessorKey: 'partition', header: 'Partition', meta: { messageKey: 'admin.partition', localizedSort: true }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'data_class', header: 'Class', meta: { messageKey: 'admin.dataClass' }, cell: ({ getValue }) => String(getValue() ?? '—') },
  { accessorKey: 'quality_state', header: 'Quality', meta: { messageKey: 'admin.quality', localizedSort: true }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'point_in_time_state', header: 'PIT', meta: { messageKey: 'admin.pit', localizedSort: true }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'promotability', header: 'Promotability', meta: { messageKey: 'admin.state', localizedSort: true }, cell: ({ getValue }) => <StateBadge state={String(getValue() ?? 'UNKNOWN')} /> },
  { accessorKey: 'row_count', header: 'Rows', meta: { messageKey: 'admin.rows', searchFormat: 'compact' }, cell: ({ getValue }) => <span className="qz-number">{formatCompactNumber(getValue() as number | null)}</span> },
];

const mandateColumns: ColumnDef<ConfigurationMandate, unknown>[] = [
  { accessorKey: 'name', header: 'Mandate', meta: { messageKey: 'portfolio.mandate' } },
  { accessorKey: 'key', header: 'Key', meta: { messageKey: 'admin.key' }, cell: ({ getValue }) => <span className="qz-mono">{String(getValue())}</span> },
  { id: 'version', header: 'Version', meta: { messageKey: 'admin.version' }, cell: ({ row }) => <span className="qz-number">{formatNumber(row.original.latest_version?.version_no)}</span> },
  { id: 'minimumAlphaCount', header: 'Minimum alphas', cell: ({ row }) => <span className="qz-number">{formatNumber(row.original.latest_version?.minimum_alpha_count)}</span> },
  { accessorKey: 'configuration_state', header: 'Configuration', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'state', header: 'State', meta: { messageKey: 'research.state', localizedSort: true }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
];

const capitalContextColumns: ColumnDef<ConfigurationCapitalContext, unknown>[] = [
  { accessorKey: 'base_currency', header: 'Currency', cell: ({ getValue }) => <span className="qz-mono">{String(getValue())}</span> },
  { accessorKey: 'deployable_capital', header: 'Deployable capital', cell: ({ getValue }) => <span className="qz-number">{formatCapitalAmount(getValue() as string)}</span> },
  { accessorKey: 'observed_at', header: 'Observed', cell: ({ getValue }) => formatDateTime(getValue() as string) },
  { accessorKey: 'valid_until', header: 'Valid until', cell: ({ getValue }) => formatDateTime(getValue() as string) },
  { accessorKey: 'configuration_state', header: 'Configuration', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
];

const evaluationDatasetSelectionColumns: ColumnDef<ConfigurationEvaluationDatasetSelection, unknown>[] = [
  { accessorKey: 'universe_version_id', header: 'Universe', cell: ({ getValue }) => <span className="qz-mono">{String(getValue()).slice(0, 12)}</span> },
  { accessorKey: 'version_no', header: 'Version', cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as number)}</span> },
  { accessorKey: 'discovery_dataset_revision_id', header: 'Discovery', cell: ({ getValue }) => <span className="qz-mono">{String(getValue()).slice(0, 12)}</span> },
  { accessorKey: 'validation_dataset_revision_id', header: 'Validation', cell: ({ getValue }) => <span className="qz-mono">{String(getValue()).slice(0, 12)}</span> },
  { accessorKey: 'sealed_dataset_revision_id', header: 'Sealed', cell: ({ getValue }) => <span className="qz-mono">{String(getValue()).slice(0, 12)}</span> },
  { accessorKey: 'state', header: 'State', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
];

const evaluationDesignVersionColumns: ColumnDef<ConfigurationEvaluationDesignVersion, unknown>[] = [
  { accessorKey: 'universe_version_id', header: 'Universe', cell: ({ getValue }) => <span className="qz-mono">{String(getValue()).slice(0, 12)}</span> },
  { accessorKey: 'version_no', header: 'Version', cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as number)}</span> },
  { accessorKey: 'allowed_model_mode', header: 'Model mode', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'qualification_role', header: 'Role', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'qualification_threshold', header: 'Threshold', cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as string)}</span> },
  { accessorKey: 'state', header: 'State', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
];

const promotionPolicyVersionColumns: ColumnDef<ConfigurationPromotionPolicyVersion, unknown>[] = [
  { accessorKey: 'purpose', header: 'Purpose', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'version_no', header: 'Version', cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as number)}</span> },
  { accessorKey: 'mode', header: 'Mode', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { id: 'gates', header: 'Gates', cell: ({ row }) => <span className="qz-number">{formatNumber(row.original.gates.length)}</span> },
  { accessorKey: 'state', header: 'State', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
];

const evaluationModelModes = ['RELATIVE_SCORE'] as const;
const alphaMetricCodes = ['OBSERVATION_COUNT', 'COVERAGE', 'IC_MEAN', 'RANK_IC_MEAN', 'HIT_RATE', 'NET_RETURN', 'ANNUALIZED_VOLATILITY', 'SHARPE_RATIO', 'MAX_DRAWDOWN', 'TRIAL_ADJUSTED_SHARPE'] as const;
const qualificationRoles = ['PRIMARY_ALPHA', 'DIVERSIFIER_ALPHA', 'HEDGE_ALPHA', 'REGIME_SIGNAL', 'RISK_MODULATOR', 'SHADOW_ALPHA'] as const;
const multipleTestingMethods = ['BONFERRONI', 'BENJAMINI_HOCHBERG'] as const;
const promotionPurposes = ['ALPHA_DISCOVERY_TO_SEALED', 'SEALED_TO_QUALIFIED'] as const;
const promotionModes = ['MANUAL_APPROVAL', 'AUTO_HANDOFF'] as const;
const comparators = ['MINIMUM', 'MAXIMUM'] as const;

type PromotionGateForm = { metric_code: string; comparator: string; threshold: string; ordinal: string };

const v1NumericFields = [
  ['minimum_weight', 'Minimum weight'],
  ['maximum_weight', 'Maximum weight'],
  ['gross_exposure_limit', 'Gross exposure limit'],
  ['net_exposure_target', 'Net exposure target'],
  ['cash_reserve', 'Cash reserve'],
  ['turnover_limit', 'Turnover limit'],
  ['variance_limit', 'Variance limit'],
  ['risk_aversion', 'Risk aversion'],
  ['cost_aversion', 'Cost aversion'],
  ['uncertainty_aversion', 'Uncertainty aversion'],
  ['commission_rate', 'Commission rate'],
  ['half_spread_rate', 'Half-spread rate'],
  ['slippage_rate', 'Slippage rate'],
  ['impact_rate', 'Impact rate'],
  ['impact_breakpoint', 'Impact breakpoint'],
] as const;

type V1NumericField = (typeof v1NumericFields)[number][0];

function RotateDownstreamTokenDialog({ downstreamId, onTokenIssued }: { downstreamId: UUID; onTokenIssued: (token: string) => void }) {
  const client = useQueryClient();
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>();
  async function rotate() {
    try {
      setPending(true);
      setError(undefined);
      const response = await apiRequest<{ service_token: string | null }>(`/api/v1/downstream-systems/${downstreamId}/rotate-service-token`, { method: 'POST', body: jsonBody({}), idempotent: true });
      await client.invalidateQueries({ queryKey: ['downstreams'] });
      if (!response.service_token) throw new Error('The replacement token is unavailable. Retry rotation from this registry entry.');
      onTokenIssued(response.service_token);
      setOpen(false);
    } catch (requestError) { setError(requestError); }
    finally { setPending(false); }
  }
  return <Dialog.Root open={open} onOpenChange={setOpen}><Dialog.Trigger><Button size="1" variant="ghost">Rotate token</Button></Dialog.Trigger><ResponsiveDialogContent maxWidth="460px"><Dialog.Title>Rotate downstream token</Dialog.Title><Dialog.Description size="2">This invalidates the prior downstream credential. QuaZonai will reveal the replacement once only after rotation.</Dialog.Description><div style={{ display: 'flex', gap: 8, marginTop: 18 }}><Dialog.Close><Button variant="soft" color="gray">Cancel</Button></Dialog.Close><Button color="red" disabled={pending} onClick={() => void rotate()}>{pending ? 'Rotating…' : 'Rotate token'}</Button></div><FormError error={error} /></ResponsiveDialogContent></Dialog.Root>;
}

function downstreamColumns(onTokenIssued: (token: string) => void): ColumnDef<ConfigurationDownstream, unknown>[] {
  return [
    { accessorKey: 'name', header: 'System', meta: { messageKey: 'admin.system' } },
    { accessorKey: 'environment_type', header: 'Environment', meta: { messageKey: 'admin.environment', localizedSort: true }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
    { accessorKey: 'enabled', header: 'Enabled', meta: { messageKey: 'admin.enabled', localizedSort: true }, cell: ({ getValue }) => <StateBadge state={getValue() ? 'ENABLED' : 'DISABLED'} /> },
    { accessorKey: 'preflight_state', header: 'Preflight', meta: { messageKey: 'admin.preflight', localizedSort: true }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
    { accessorKey: 'package_contract_version', header: 'Package contract', meta: { messageKey: 'admin.packageContract' } },
    { id: 'token', header: 'Credential', cell: ({ row }) => <RotateDownstreamTokenDialog downstreamId={row.original.id} onTokenIssued={onTokenIssued} /> },
  ];
}

function UniverseDialog() {
  const mutation = useCreateConfigurationUniverse();
  const [open, setOpen] = useState(false);
  const [key, setKey] = useState('');
  const [name, setName] = useState('');
  const [specification, setSpecification] = useState('');
  const [error, setError] = useState<unknown>();
  async function submit() {
    try {
      setError(undefined);
      await mutation.mutateAsync({ universe_key: key.trim(), name: name.trim(), ...parseJsonObject(specification, 'Universe specification') });
      setOpen(false);
    } catch (requestError) { setError(requestError); }
  }
  return <Dialog.Root open={open} onOpenChange={setOpen}><Dialog.Trigger><Button size="1" variant="soft"><DatabaseIcon size={14} />Create Universe</Button></Dialog.Trigger><ResponsiveDialogContent maxWidth="560px"><Dialog.Title>Create immutable Universe version</Dialog.Title><Dialog.Description size="2">Describe the real market boundary. The specification must include instrument, membership, calendar, currency, data, risk, cost, and capacity semantics.</Dialog.Description><div className="qz-form-grid" style={{ marginTop: 18 }}><label className="qz-field"><span className="qz-label">Universe key</span><TextField.Root dir="ltr" value={key} onChange={(event) => setKey(event.target.value)} placeholder="US_EQUITIES" /></label><label className="qz-field"><span className="qz-label">Name</span><TextField.Root dir="auto" value={name} onChange={(event) => setName(event.target.value)} /></label><JsonField label="Universe specification (JSON)" value={specification} onChange={setSpecification} placeholder={'{"instrument_schema":{"instrument_id":"string"},"membership_rules":{"listing":"NYSE|NASDAQ"},"calendar_semantics":{"timezone":"America/New_York"},"currency_semantics":{"base_currency":"USD"},"data_requirements":{"available_at":"required"},"risk_model_family":"EWMA","cost_model_family":"SPREAD","capacity_model_family":"ADV"}'} /><Button disabled={!key.trim() || !name.trim() || !specification.trim() || mutation.isPending} onClick={() => void submit()}>{mutation.isPending ? 'Creating…' : 'Create Universe'}</Button><FormError error={error} /></div></ResponsiveDialogContent></Dialog.Root>;
}

function UniverseVersionDialog({ universes }: { universes: ConfigurationUniverse[] }) {
  const mutation = useCreateConfigurationUniverseVersion();
  const [open, setOpen] = useState(false);
  const [universeId, setUniverseId] = useState('');
  const [name, setName] = useState('');
  const [specification, setSpecification] = useState('');
  const [error, setError] = useState<unknown>();
  async function submit() {
    try {
      setError(undefined);
      await mutation.mutateAsync({ universeId, payload: { name: name.trim(), ...parseJsonObject(specification, 'Universe specification') } });
      setOpen(false);
    } catch (requestError) { setError(requestError); }
  }
  return <Dialog.Root open={open} onOpenChange={setOpen}><Dialog.Trigger><Button size="1" variant="soft" disabled={!universes.length}>New Universe version</Button></Dialog.Trigger><ResponsiveDialogContent maxWidth="560px"><Dialog.Title>Create next immutable Universe version</Dialog.Title><Dialog.Description size="2">The prior Universe remains historical. This creates a new version with the selected key.</Dialog.Description><div className="qz-form-grid" style={{ marginTop: 18 }}><label className="qz-field"><span className="qz-label">Existing Universe</span><Select.Root value={universeId} onValueChange={setUniverseId}><Select.Trigger placeholder="Select Universe" /><Select.Content>{universes.map((universe) => <Select.Item key={universe.id} value={universe.id}>{universe.name} · v{universe.version_no}</Select.Item>)}</Select.Content></Select.Root></label><label className="qz-field"><span className="qz-label">New name</span><TextField.Root dir="auto" value={name} onChange={(event) => setName(event.target.value)} /></label><JsonField label="Universe specification (JSON)" value={specification} onChange={setSpecification} placeholder="Complete replacement specification" /><Button disabled={!universeId || !name.trim() || !specification.trim() || mutation.isPending} onClick={() => void submit()}>{mutation.isPending ? 'Creating…' : 'Create version'}</Button><FormError error={error} /></div></ResponsiveDialogContent></Dialog.Root>;
}

function DataSourceDialog({ universes }: { universes: ConfigurationUniverse[] }) {
  const mutation = useCreateConfigurationDataSource();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState('');
  const [connectorKey, setConnectorKey] = useState('');
  const [provider, setProvider] = useState('');
  const [universeId, setUniverseId] = useState('');
  const [license, setLicense] = useState('');
  const [fieldSchema, setFieldSchema] = useState('');
  const [availability, setAvailability] = useState('');
  const [cadence, setCadence] = useState('');
  const [publicConfig, setPublicConfig] = useState('');
  const [error, setError] = useState<unknown>();
  async function submit() {
    try {
      setError(undefined);
      await mutation.mutateAsync({ name: name.trim(), connector_key: connectorKey.trim(), provider: provider.trim(), universe_scope: [universeId], license_classification: license.trim(), field_schema: parseJsonObject(fieldSchema, 'Field schema'), availability_semantics: parseJsonObject(availability, 'Availability semantics'), ...(cadence.trim() ? { update_cadence: cadence.trim() } : {}), public_config: optionalJsonObject(publicConfig, 'Public configuration') });
      setOpen(false);
    } catch (requestError) { setError(requestError); }
  }
  return <Dialog.Root open={open} onOpenChange={setOpen}><Dialog.Trigger><Button size="1" variant="soft" disabled={!universes.length}><DatabaseIcon size={14} />Register data source</Button></Dialog.Trigger><ResponsiveDialogContent maxWidth="560px"><Dialog.Title>Register governed Data Source</Dialog.Title><Dialog.Description size="2">Registration records governance only. It does not claim connector preflight, data quality, or research readiness.</Dialog.Description><div className="qz-form-grid" style={{ marginTop: 18 }}><label className="qz-field"><span className="qz-label">Name</span><TextField.Root dir="auto" value={name} onChange={(event) => setName(event.target.value)} /></label><label className="qz-field"><span className="qz-label">Connector key</span><TextField.Root dir="ltr" value={connectorKey} onChange={(event) => setConnectorKey(event.target.value)} placeholder="licensed-bars" /></label><label className="qz-field"><span className="qz-label">Provider</span><TextField.Root dir="auto" value={provider} onChange={(event) => setProvider(event.target.value)} /></label><label className="qz-field"><span className="qz-label">Universe version</span><Select.Root value={universeId} onValueChange={setUniverseId}><Select.Trigger placeholder="Select Universe" /><Select.Content>{universes.map((universe) => <Select.Item key={universe.id} value={universe.id}>{universe.name} · v{universe.version_no}</Select.Item>)}</Select.Content></Select.Root></label><label className="qz-field"><span className="qz-label">License classification</span><TextField.Root dir="auto" value={license} onChange={(event) => setLicense(event.target.value)} placeholder="LICENSED" /></label><JsonField label="Field schema (JSON)" value={fieldSchema} onChange={setFieldSchema} placeholder={'{"event_time":"timestamp","available_at":"timestamp","close":"decimal"}'} /><JsonField label="Availability semantics (JSON)" value={availability} onChange={setAvailability} placeholder={'{"available_at_field":"received_at"}'} /><label className="qz-field"><span className="qz-label">Update cadence (optional)</span><TextField.Root dir="auto" value={cadence} onChange={(event) => setCadence(event.target.value)} placeholder="daily" /></label><JsonField label="Public connector configuration (JSON, optional)" value={publicConfig} onChange={setPublicConfig} placeholder={'{"dataset":"daily-bars"}'} required={false} /><Button disabled={!name.trim() || !connectorKey.trim() || !provider.trim() || !universeId || !license.trim() || !fieldSchema.trim() || !availability.trim() || mutation.isPending} onClick={() => void submit()}>{mutation.isPending ? 'Registering…' : 'Register data source'}</Button><FormError error={error} /></div></ResponsiveDialogContent></Dialog.Root>;
}

function DataSourcePreflightDialog({ sources, onRequested }: { sources: ConfigurationDataSource[]; onRequested: (id: UUID) => void }) {
  const mutation = useRequestConfigurationDataSourcePreflight();
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<unknown>();
  const pendingSources = sources.filter((source) => source.preflight_state === 'PENDING');
  async function submit(sourceId: UUID) {
    try {
      setError(undefined);
      const operation = await mutation.mutateAsync(sourceId);
      onRequested(operation.id);
      setOpen(false);
    } catch (requestError) { setError(requestError); }
  }
  return <Dialog.Root open={open} onOpenChange={setOpen}><Dialog.Trigger><Button size="1" variant="soft" disabled={!pendingSources.length}>Preflight data source</Button></Dialog.Trigger><ResponsiveDialogContent maxWidth="520px"><Dialog.Title>Preflight governed Data Source</Dialog.Title><Dialog.Description size="2">Preflight only uses registered source facts and creates an asynchronous operation. It never accepts a URL, endpoint, plugin path, or credential here.</Dialog.Description><div className="qz-form-grid" style={{ marginTop: 18 }}>{pendingSources.map((source) => <Button key={source.id} disabled={mutation.isPending} onClick={() => void submit(source.id)}>{mutation.isPending ? 'Requesting…' : `Preflight ${source.name}`}</Button>)}<FormError error={error} /></div></ResponsiveDialogContent></Dialog.Root>;
}

function DatasetMaterializationDialog({ sources, universes, onRequested }: { sources: ConfigurationDataSource[]; universes: ConfigurationUniverse[]; onRequested: (id: UUID) => void }) {
  const mutation = useRequestConfigurationDatasetMaterialization();
  const [open, setOpen] = useState(false);
  const [sourceId, setSourceId] = useState('');
  const [universeId, setUniverseId] = useState('');
  const [partition, setPartition] = useState('DISCOVERY');
  const [dataClass, setDataClass] = useState('VENDOR');
  const [origin, setOrigin] = useState('');
  const [schemaVersion, setSchemaVersion] = useState('');
  const [dataType, setDataType] = useState('');
  const [instrumentScope, setInstrumentScope] = useState('');
  const [eventStart, setEventStart] = useState('');
  const [eventEnd, setEventEnd] = useState('');
  const [availableStart, setAvailableStart] = useState('');
  const [availableEnd, setAvailableEnd] = useState('');
  const [quality, setQuality] = useState('');
  const [pointInTime, setPointInTime] = useState('');
  const [error, setError] = useState<unknown>();
  async function submit() {
    try {
      setError(undefined);
      const operation = await mutation.mutateAsync({ data_source_id: sourceId, universe_version_id: universeId, partition, data_class: dataClass, origin: origin.trim(), schema_version: schemaVersion.trim(), data_type: dataType.trim(), instrument_scope: splitValues(instrumentScope), event_start: eventStart.trim(), event_end: eventEnd.trim(), available_start: availableStart.trim(), available_end: availableEnd.trim(), quality_requirements: parseJsonObject(quality, 'Quality requirements'), point_in_time_requirements: parseJsonObject(pointInTime, 'Point-in-time requirements') });
      onRequested(operation.id);
      setOpen(false);
    } catch (requestError) { setError(requestError); }
  }
  const readyToRequest = sourceId && universeId && origin.trim() && schemaVersion.trim() && dataType.trim() && instrumentScope.trim() && eventStart.trim() && eventEnd.trim() && availableStart.trim() && availableEnd.trim() && quality.trim() && pointInTime.trim();
  return <Dialog.Root open={open} onOpenChange={setOpen}><Dialog.Trigger><Button size="1" variant="soft" disabled={!sources.length || !universes.length}>Request materialization</Button></Dialog.Trigger><ResponsiveDialogContent maxWidth="560px"><Dialog.Title>Request Dataset materialization</Dialog.Title><Dialog.Description size="2">This queues a governed operation. The resulting revision remains non-promotable until independent quality and point-in-time checks complete.</Dialog.Description><div className="qz-form-grid" style={{ marginTop: 18 }}><label className="qz-field"><span className="qz-label">Data Source</span><Select.Root value={sourceId} onValueChange={setSourceId}><Select.Trigger placeholder="Select Data Source" /><Select.Content>{sources.map((source) => <Select.Item key={source.id} value={source.id}>{source.name}</Select.Item>)}</Select.Content></Select.Root></label><label className="qz-field"><span className="qz-label">Universe version</span><Select.Root value={universeId} onValueChange={setUniverseId}><Select.Trigger placeholder="Select Universe" /><Select.Content>{universes.map((universe) => <Select.Item key={universe.id} value={universe.id}>{universe.name} · v{universe.version_no}</Select.Item>)}</Select.Content></Select.Root></label><label className="qz-field"><span className="qz-label">Partition</span><Select.Root value={partition} onValueChange={setPartition}><Select.Trigger /><Select.Content>{['DISCOVERY', 'VALIDATION', 'SEALED', 'FORWARD'].map((value) => <Select.Item key={value} value={value}>{humanize(value)}</Select.Item>)}</Select.Content></Select.Root></label><label className="qz-field"><span className="qz-label">Data class</span><Select.Root value={dataClass} onValueChange={setDataClass}><Select.Trigger /><Select.Content>{['VENDOR', 'PRODUCTION', 'SYNTHETIC', 'FIXTURE'].map((value) => <Select.Item key={value} value={value}>{humanize(value)}</Select.Item>)}</Select.Content></Select.Root></label><label className="qz-field"><span className="qz-label">Origin</span><TextField.Root dir="auto" value={origin} onChange={(event) => setOrigin(event.target.value)} /></label><label className="qz-field"><span className="qz-label">Schema version</span><TextField.Root dir="ltr" value={schemaVersion} onChange={(event) => setSchemaVersion(event.target.value)} placeholder="bars-v1" /></label><label className="qz-field"><span className="qz-label">Data type</span><TextField.Root dir="auto" value={dataType} onChange={(event) => setDataType(event.target.value)} placeholder="BAR" /></label><label className="qz-field"><span className="qz-label">Instrument scope (comma-separated)</span><TextField.Root dir="ltr" value={instrumentScope} onChange={(event) => setInstrumentScope(event.target.value)} placeholder="AAPL.XNAS, MSFT.XNAS" /></label><label className="qz-field"><span className="qz-label">Event start (ISO 8601 with offset)</span><TextField.Root dir="ltr" value={eventStart} onChange={(event) => setEventStart(event.target.value)} placeholder="2025-01-01T00:00:00Z" /></label><label className="qz-field"><span className="qz-label">Event end (ISO 8601 with offset)</span><TextField.Root dir="ltr" value={eventEnd} onChange={(event) => setEventEnd(event.target.value)} placeholder="2025-01-02T00:00:00Z" /></label><label className="qz-field"><span className="qz-label">Available start (ISO 8601 with offset)</span><TextField.Root dir="ltr" value={availableStart} onChange={(event) => setAvailableStart(event.target.value)} placeholder="2025-01-01T00:05:00Z" /></label><label className="qz-field"><span className="qz-label">Available end (ISO 8601 with offset)</span><TextField.Root dir="ltr" value={availableEnd} onChange={(event) => setAvailableEnd(event.target.value)} placeholder="2025-01-02T00:05:00Z" /></label><JsonField label="Quality requirements (JSON)" value={quality} onChange={setQuality} placeholder={'{"minimum_coverage":1.0}'} /><JsonField label="Point-in-time requirements (JSON)" value={pointInTime} onChange={setPointInTime} placeholder={'{"available_at":"required"}'} /><Button disabled={!readyToRequest || mutation.isPending} onClick={() => void submit()}>{mutation.isPending ? 'Requesting…' : 'Request materialization'}</Button><FormError error={error} /></div></ResponsiveDialogContent></Dialog.Root>;
}

function EvaluationDatasetSelectionDialog({ universes, datasets }: { universes: ConfigurationUniverse[]; datasets: ConfigurationDataset[] }) {
  const mutation = useCreateConfigurationEvaluationDatasetSelection();
  const [open, setOpen] = useState(false);
  const [universeId, setUniverseId] = useState('');
  const [discoveryId, setDiscoveryId] = useState('');
  const [validationId, setValidationId] = useState('');
  const [sealedId, setSealedId] = useState('');
  const [error, setError] = useState<unknown>();
  const phaseDatasets = (partition: string) => datasets.filter((dataset) => dataset.universe_version_id === universeId && dataset.partition === partition);
  const datasetLabel = (dataset: ConfigurationDataset) => `${dataset.universe_name ?? 'Unknown Universe'} · ${dataset.partition} · r${dataset.revision_no} · ${dataset.id}`;
  const selectUniverse = (value: string) => { setUniverseId(value); setDiscoveryId(''); setValidationId(''); setSealedId(''); };
  async function submit() {
    try {
      setError(undefined);
      await mutation.mutateAsync({
        universe_version_id: universeId,
        discovery_dataset_revision_id: discoveryId,
        validation_dataset_revision_id: validationId,
        sealed_dataset_revision_id: sealedId,
        state: 'ENABLED',
      });
      setOpen(false);
    } catch (requestError) { setError(requestError); }
  }
  const complete = universeId && discoveryId && validationId && sealedId;
  return <Dialog.Root open={open} onOpenChange={setOpen}>
    <Dialog.Trigger><Button size="1" variant="soft" disabled={!universes.length || !datasets.length}>Create Evaluation Dataset Selection</Button></Dialog.Trigger>
    <ResponsiveDialogContent maxWidth="680px">
      <Dialog.Title>Create immutable Evaluation Dataset Selection</Dialog.Title>
      <Dialog.Description size="2">Select three exact dataset revisions. The canonical API verifies their trust state; this screen never infers a latest revision or makes an unvalidated dataset promotable.</Dialog.Description>
      <div className="qz-form-grid" style={{ marginTop: 18 }}>
        <label className="qz-field"><span className="qz-label">Universe version</span><Select.Root value={universeId} onValueChange={selectUniverse}><Select.Trigger aria-label="Evaluation Dataset Selection Universe" placeholder="Select Universe" /><Select.Content>{universes.map((universe) => <Select.Item key={universe.id} value={universe.id}>{universe.name} · v{universe.version_no}</Select.Item>)}</Select.Content></Select.Root></label>
        <label className="qz-field"><span className="qz-label">Discovery Dataset revision</span><Select.Root value={discoveryId} onValueChange={setDiscoveryId} disabled={!universeId}><Select.Trigger aria-label="Discovery Dataset revision" placeholder="Select Discovery revision" /><Select.Content>{phaseDatasets('DISCOVERY').map((dataset) => <Select.Item key={dataset.id} value={dataset.id}>{datasetLabel(dataset)}</Select.Item>)}</Select.Content></Select.Root></label>
        <label className="qz-field"><span className="qz-label">Validation Dataset revision</span><Select.Root value={validationId} onValueChange={setValidationId} disabled={!universeId}><Select.Trigger aria-label="Validation Dataset revision" placeholder="Select Validation revision" /><Select.Content>{phaseDatasets('VALIDATION').map((dataset) => <Select.Item key={dataset.id} value={dataset.id}>{datasetLabel(dataset)}</Select.Item>)}</Select.Content></Select.Root></label>
        <label className="qz-field"><span className="qz-label">Sealed Dataset revision</span><Select.Root value={sealedId} onValueChange={setSealedId} disabled={!universeId}><Select.Trigger aria-label="Sealed Dataset revision" placeholder="Select Sealed revision" /><Select.Content>{phaseDatasets('SEALED').map((dataset) => <Select.Item key={dataset.id} value={dataset.id}>{datasetLabel(dataset)}</Select.Item>)}</Select.Content></Select.Root></label>
        <label className="qz-field"><span className="qz-label">State</span><TextField.Root dir="ltr" readOnly value="ENABLED" /></label>
        <Button disabled={!complete || mutation.isPending} onClick={() => void submit()}>{mutation.isPending ? 'Creating…' : 'Create Evaluation Dataset Selection'}</Button>
        <FormError error={error} />
      </div>
    </ResponsiveDialogContent>
  </Dialog.Root>;
}

function EvaluationDesignVersionDialog({ universes }: { universes: ConfigurationUniverse[] }) {
  const mutation = useCreateConfigurationEvaluationDesignVersion();
  const [open, setOpen] = useState(false);
  const [universeId, setUniverseId] = useState('');
  const [contractVersion, setContractVersion] = useState('');
  const [modelMode, setModelMode] = useState('');
  const [role, setRole] = useState('');
  const [walkForwardFolds, setWalkForwardFolds] = useState('');
  const [annualizationFactor, setAnnualizationFactor] = useState('');
  const [multipleTestingMethod, setMultipleTestingMethod] = useState('');
  const [multipleTestingMaxTrials, setMultipleTestingMaxTrials] = useState('');
  const [metricCode, setMetricCode] = useState('');
  const [comparator, setComparator] = useState('');
  const [threshold, setThreshold] = useState('');
  const [passDisclosureCode, setPassDisclosureCode] = useState('');
  const [failureDisclosureCode, setFailureDisclosureCode] = useState('');
  const [inconclusiveDisclosureCode, setInconclusiveDisclosureCode] = useState('');
  const [invalidDisclosureCode, setInvalidDisclosureCode] = useState('');
  const [error, setError] = useState<unknown>();
  async function submit() {
    try {
      setError(undefined);
      if (!evaluationModelModes.includes(modelMode as (typeof evaluationModelModes)[number])) throw new Error('Select an allowed model mode.');
      if (!qualificationRoles.includes(role as (typeof qualificationRoles)[number])) throw new Error('Select a qualification role.');
      if (!multipleTestingMethods.includes(multipleTestingMethod as (typeof multipleTestingMethods)[number])) throw new Error('Select a multiple testing method.');
      if (!alphaMetricCodes.includes(metricCode as (typeof alphaMetricCodes)[number])) throw new Error('Select a supported sealed Alpha metric.');
      if (!comparators.includes(comparator as (typeof comparators)[number])) throw new Error('Select a qualification comparator.');
      await mutation.mutateAsync({
        universe_version_id: universeId,
        contract_version: contractVersion.trim(),
        allowed_model_mode: modelMode,
        qualification_role: role,
        walk_forward_folds: positiveInteger(walkForwardFolds, 'Walk-forward folds'),
        annualization_factor: annualizationFactor.trim(),
        multiple_testing_method: multipleTestingMethod,
        multiple_testing_max_trials: positiveInteger(multipleTestingMaxTrials, 'Maximum trials'),
        qualification_metric_code: metricCode.trim(),
        qualification_comparator: comparator,
        qualification_threshold: threshold.trim(),
        pass_disclosure_code: passDisclosureCode.trim(),
        failure_disclosure_code: failureDisclosureCode.trim(),
        inconclusive_disclosure_code: inconclusiveDisclosureCode.trim(),
        invalid_disclosure_code: invalidDisclosureCode.trim(),
        state: 'ACTIVE',
      });
      setOpen(false);
    } catch (requestError) { setError(requestError); }
  }
  const complete = universeId && contractVersion.trim() && modelMode && role && walkForwardFolds.trim() && annualizationFactor.trim() && multipleTestingMethod && multipleTestingMaxTrials.trim() && metricCode.trim() && comparator && threshold.trim() && passDisclosureCode.trim() && failureDisclosureCode.trim() && inconclusiveDisclosureCode.trim() && invalidDisclosureCode.trim();
  return <Dialog.Root open={open} onOpenChange={setOpen}>
    <Dialog.Trigger><Button size="1" variant="soft" disabled={!universes.length}>Create Evaluation Design Version</Button></Dialog.Trigger>
    <ResponsiveDialogContent maxWidth="760px">
      <Dialog.Title>Create immutable Evaluation Design Version</Dialog.Title>
      <Dialog.Description size="2">Every qualification statistic, disclosure code, and test method is explicit. Decimal inputs are transported as strings without browser number coercion.</Dialog.Description>
      <div className="qz-form-grid" style={{ marginTop: 18 }}>
        <label className="qz-field"><span className="qz-label">Universe version</span><Select.Root value={universeId} onValueChange={setUniverseId}><Select.Trigger aria-label="Evaluation Design Universe" placeholder="Select Universe" /><Select.Content>{universes.map((universe) => <Select.Item key={universe.id} value={universe.id}>{universe.name} · v{universe.version_no}</Select.Item>)}</Select.Content></Select.Root></label>
        <label className="qz-field"><span className="qz-label">Contract version</span><TextField.Root dir="ltr" value={contractVersion} onChange={(event) => setContractVersion(event.target.value)} /></label>
        <label className="qz-field"><span className="qz-label">Allowed model mode</span><Select.Root value={modelMode} onValueChange={setModelMode}><Select.Trigger aria-label="Allowed model mode" placeholder="Select model mode" /><Select.Content>{evaluationModelModes.map((value) => <Select.Item key={value} value={value}>{humanize(value)}</Select.Item>)}</Select.Content></Select.Root></label>
        <label className="qz-field"><span className="qz-label">Qualification role</span><Select.Root value={role} onValueChange={setRole}><Select.Trigger aria-label="Qualification role" placeholder="Select qualification role" /><Select.Content>{qualificationRoles.map((value) => <Select.Item key={value} value={value}>{humanize(value)}</Select.Item>)}</Select.Content></Select.Root></label>
        <label className="qz-field"><span className="qz-label">Walk-forward folds</span><TextField.Root dir="ltr" inputMode="numeric" value={walkForwardFolds} onChange={(event) => setWalkForwardFolds(event.target.value)} /></label>
        <label className="qz-field"><span className="qz-label">Annualization factor</span><TextField.Root dir="ltr" inputMode="decimal" value={annualizationFactor} onChange={(event) => setAnnualizationFactor(event.target.value)} placeholder="Decimal string" /></label>
        <label className="qz-field"><span className="qz-label">Multiple testing method</span><Select.Root value={multipleTestingMethod} onValueChange={setMultipleTestingMethod}><Select.Trigger aria-label="Multiple testing method" placeholder="Select test method" /><Select.Content>{multipleTestingMethods.map((value) => <Select.Item key={value} value={value}>{humanize(value)}</Select.Item>)}</Select.Content></Select.Root></label>
        <label className="qz-field"><span className="qz-label">Multiple testing maximum trials</span><TextField.Root dir="ltr" inputMode="numeric" value={multipleTestingMaxTrials} onChange={(event) => setMultipleTestingMaxTrials(event.target.value)} /></label>
        <label className="qz-field"><span className="qz-label">Qualification metric code</span><Select.Root value={metricCode} onValueChange={setMetricCode}><Select.Trigger aria-label="Qualification metric code" placeholder="Select sealed Alpha metric" /><Select.Content>{alphaMetricCodes.map((value) => <Select.Item key={value} value={value}>{humanize(value)}</Select.Item>)}</Select.Content></Select.Root></label>
        <label className="qz-field"><span className="qz-label">Qualification comparator</span><Select.Root value={comparator} onValueChange={setComparator}><Select.Trigger aria-label="Qualification comparator" placeholder="Select comparator" /><Select.Content>{comparators.map((value) => <Select.Item key={value} value={value}>{humanize(value)}</Select.Item>)}</Select.Content></Select.Root></label>
        <label className="qz-field"><span className="qz-label">Qualification threshold</span><TextField.Root dir="ltr" inputMode="decimal" value={threshold} onChange={(event) => setThreshold(event.target.value)} placeholder="Decimal string" /></label>
        <label className="qz-field"><span className="qz-label">Pass disclosure code</span><TextField.Root dir="ltr" value={passDisclosureCode} onChange={(event) => setPassDisclosureCode(event.target.value)} /></label>
        <label className="qz-field"><span className="qz-label">Failure disclosure code</span><TextField.Root dir="ltr" value={failureDisclosureCode} onChange={(event) => setFailureDisclosureCode(event.target.value)} /></label>
        <label className="qz-field"><span className="qz-label">Inconclusive disclosure code</span><TextField.Root dir="ltr" value={inconclusiveDisclosureCode} onChange={(event) => setInconclusiveDisclosureCode(event.target.value)} /></label>
        <label className="qz-field"><span className="qz-label">Invalid disclosure code</span><TextField.Root dir="ltr" value={invalidDisclosureCode} onChange={(event) => setInvalidDisclosureCode(event.target.value)} /></label>
        <label className="qz-field"><span className="qz-label">State</span><TextField.Root dir="ltr" readOnly value="ACTIVE" /></label>
        <Button disabled={!complete || mutation.isPending} onClick={() => void submit()}>{mutation.isPending ? 'Creating…' : 'Create Evaluation Design Version'}</Button>
        <FormError error={error} />
      </div>
    </ResponsiveDialogContent>
  </Dialog.Root>;
}

function PromotionPolicyVersionDialog() {
  const mutation = useCreateConfigurationPromotionPolicyVersion();
  const [open, setOpen] = useState(false);
  const [purpose, setPurpose] = useState('');
  const [mode, setMode] = useState('');
  const [gates, setGates] = useState<PromotionGateForm[]>([]);
  const [error, setError] = useState<unknown>();
  const updateGate = (index: number, field: keyof PromotionGateForm, value: string) => setGates((current) => current.map((gate, gateIndex) => gateIndex === index ? { ...gate, [field]: value } : gate));
  async function submit() {
    try {
      setError(undefined);
      if (!promotionPurposes.includes(purpose as (typeof promotionPurposes)[number])) throw new Error('Select a promotion purpose.');
      if (!promotionModes.includes(mode as (typeof promotionModes)[number])) throw new Error('Select a promotion mode.');
      const gatePayload = gates.map((gate, index) => {
        if (!comparators.includes(gate.comparator as (typeof comparators)[number])) throw new Error(`Select a comparator for gate ${index + 1}.`);
        return { metric_code: gate.metric_code.trim(), comparator: gate.comparator, threshold: gate.threshold.trim(), ordinal: positiveInteger(gate.ordinal, `Gate ${index + 1} ordinal`) };
      });
      await mutation.mutateAsync({
        purpose,
        mode,
        gates: gatePayload,
        state: 'ACTIVE',
      });
      setOpen(false);
    } catch (requestError) { setError(requestError); }
  }
  const gatesComplete = gates.length > 0 && gates.every((gate) => gate.metric_code.trim() && gate.comparator && gate.threshold.trim() && gate.ordinal.trim());
  const complete = purpose && mode && gatesComplete;
  return <Dialog.Root open={open} onOpenChange={setOpen}>
    <Dialog.Trigger><Button size="1" variant="soft">Create Promotion Policy Version</Button></Dialog.Trigger>
    <ResponsiveDialogContent maxWidth="760px">
      <Dialog.Title>Create immutable Promotion Policy Version</Dialog.Title>
      <Dialog.Description size="2">Each Alpha gate is explicit and ordered. Paper/Live policy creation remains unavailable until complete typed connection, feedback-contract, and preflight writers exist.</Dialog.Description>
      <div className="qz-form-grid" style={{ marginTop: 18 }}>
        <label className="qz-field"><span className="qz-label">Purpose</span><Select.Root value={purpose} onValueChange={setPurpose}><Select.Trigger aria-label="Promotion purpose" placeholder="Select purpose" /><Select.Content>{promotionPurposes.map((value) => <Select.Item key={value} value={value}>{humanize(value)}</Select.Item>)}</Select.Content></Select.Root></label>
        <label className="qz-field"><span className="qz-label">Mode</span><Select.Root value={mode} onValueChange={setMode}><Select.Trigger aria-label="Promotion mode" placeholder="Select mode" /><Select.Content>{promotionModes.map((value) => <Select.Item key={value} value={value}>{humanize(value)}</Select.Item>)}</Select.Content></Select.Root></label>
        <label className="qz-field"><span className="qz-label">State</span><TextField.Root dir="ltr" readOnly value="ACTIVE" /></label>
        {gates.map((gate, index) => <div className="qz-panel qz-panel-pad" key={index} style={{ gridColumn: '1 / -1' }}><div className="qz-list-title">Promotion gate {index + 1}</div><div className="qz-form-grid" style={{ marginTop: 12 }}><label className="qz-field"><span className="qz-label">Metric code</span><Select.Root value={gate.metric_code} onValueChange={(value) => updateGate(index, 'metric_code', value)}><Select.Trigger aria-label={`Gate ${index + 1} metric code`} placeholder="Select sealed Alpha metric" /><Select.Content>{alphaMetricCodes.map((value) => <Select.Item key={value} value={value}>{humanize(value)}</Select.Item>)}</Select.Content></Select.Root></label><label className="qz-field"><span className="qz-label">Comparator</span><Select.Root value={gate.comparator} onValueChange={(value) => updateGate(index, 'comparator', value)}><Select.Trigger aria-label={`Gate ${index + 1} comparator`} placeholder="Select comparator" /><Select.Content>{comparators.map((value) => <Select.Item key={value} value={value}>{humanize(value)}</Select.Item>)}</Select.Content></Select.Root></label><label className="qz-field"><span className="qz-label">Threshold</span><TextField.Root aria-label={`Gate ${index + 1} threshold`} dir="ltr" inputMode="decimal" value={gate.threshold} onChange={(event) => updateGate(index, 'threshold', event.target.value)} placeholder="Decimal string" /></label><label className="qz-field"><span className="qz-label">Ordinal</span><TextField.Root aria-label={`Gate ${index + 1} ordinal`} dir="ltr" inputMode="numeric" value={gate.ordinal} onChange={(event) => updateGate(index, 'ordinal', event.target.value)} /></label><Button size="1" color="red" variant="soft" onClick={() => setGates((current) => current.filter((_, gateIndex) => gateIndex !== index))}>Remove gate</Button></div></div>)}
        <Button size="1" variant="soft" onClick={() => setGates((current) => [...current, { metric_code: '', comparator: '', threshold: '', ordinal: '' }])}>Add promotion gate</Button>
        <Button disabled={!complete || mutation.isPending} onClick={() => void submit()}>{mutation.isPending ? 'Creating…' : 'Create Promotion Policy Version'}</Button>
        <FormError error={error} />
      </div>
    </ResponsiveDialogContent>
  </Dialog.Root>;
}

function MandateDialog({ mandates, universes }: { mandates: ConfigurationMandate[]; universes: ConfigurationUniverse[] }) {
  const create = useCreateConfigurationMandate();
  const createVersion = useCreateConfigurationMandateVersion();
  const [open, setOpen] = useState(false);
  const [mode, setMode] = useState<'CREATE' | 'VERSION' | ''>('');
  const [mandateId, setMandateId] = useState('');
  const [key, setKey] = useState('');
  const [name, setName] = useState('');
  const [enabled, setEnabled] = useState('');
  const [baseCurrency, setBaseCurrency] = useState('');
  const [universeId, setUniverseId] = useState('');
  const [minimumAlphaCount, setMinimumAlphaCount] = useState('');
  const [state, setState] = useState('');
  const [numbers, setNumbers] = useState<Partial<Record<V1NumericField, string>>>({});
  const [error, setError] = useState<unknown>();
  const pending = create.isPending || createVersion.isPending;
  const updateNumber = (field: V1NumericField, value: string) => setNumbers((current) => ({ ...current, [field]: value }));
  async function submit() {
    try {
      setError(undefined);
      const minimum = Number(minimumAlphaCount);
      if (!Number.isInteger(minimum) || minimum < 2) throw new Error('Minimum alpha count must be an integer of at least 2.');
      const minimumWeight = Number(numbers.minimum_weight);
      if (!Number.isFinite(minimumWeight) || minimumWeight <= 0) throw new Error('Minimum weight must be a positive decimal.');
      if (state !== 'ACTIVE' && state !== 'RETIRED') throw new Error('Select the immutable version state.');
      const payload = {
        policy_family: 'LONG_ONLY_MEAN_VARIANCE_V1',
        base_currency: baseCurrency.trim(),
        objective: 'MAXIMIZE_NET_RETURN',
        eligible_alpha_role: 'PRIMARY_ALPHA',
        universe_version_id: universeId,
        minimum_alpha_count: minimum,
        ...Object.fromEntries(v1NumericFields.map(([field]) => [field, numbers[field]?.trim()])),
        state,
      };
      if (mode === 'CREATE') await create.mutateAsync({ key: key.trim(), name: name.trim(), enabled: enabled === 'true', ...payload });
      else await createVersion.mutateAsync({ mandateId, payload });
      setOpen(false);
    } catch (requestError) { setError(requestError); }
  }
  const complete = mode && baseCurrency.trim() && universeId && minimumAlphaCount.trim() && state && v1NumericFields.every(([field]) => numbers[field]?.trim()) && (mode === 'CREATE' ? key.trim() && name.trim() && enabled : mandateId);
  return <Dialog.Root open={open} onOpenChange={setOpen}>
    <Dialog.Trigger><Button size="1" variant="soft" disabled={!universes.length}>Create Mandate/version</Button></Dialog.Trigger>
    <ResponsiveDialogContent maxWidth="760px">
      <Dialog.Title>Create immutable Portfolio Mandate version</Dialog.Title>
      <Dialog.Description size="2">V1 is a single-universe, long-only mean-variance policy. Every constraint, cost rate, and aversion is explicitly frozen; it never creates manual weights or execution controls.</Dialog.Description>
      <div className="qz-form-grid" style={{ marginTop: 18 }}>
        <label className="qz-field"><span className="qz-label">Action</span><Select.Root value={mode} onValueChange={(value) => setMode(value as 'CREATE' | 'VERSION')}><Select.Trigger aria-label="Action" placeholder="Select action" /><Select.Content><Select.Item value="CREATE">Create Mandate</Select.Item><Select.Item value="VERSION">Create next version</Select.Item></Select.Content></Select.Root></label>
        {mode === 'CREATE' ? <>
          <label className="qz-field"><span className="qz-label">Mandate key</span><TextField.Root dir="ltr" value={key} onChange={(event) => setKey(event.target.value)} placeholder="core-growth" /></label>
          <label className="qz-field"><span className="qz-label">Mandate name</span><TextField.Root dir="auto" value={name} onChange={(event) => setName(event.target.value)} /></label>
          <label className="qz-field"><span className="qz-label">Enabled</span><Select.Root value={enabled} onValueChange={setEnabled}><Select.Trigger aria-label="Enabled" placeholder="Select enabled state" /><Select.Content><Select.Item value="true">Enabled</Select.Item><Select.Item value="false">Disabled</Select.Item></Select.Content></Select.Root></label>
        </> : mode === 'VERSION' ? <label className="qz-field"><span className="qz-label">Existing Mandate</span><Select.Root value={mandateId} onValueChange={setMandateId}><Select.Trigger aria-label="Existing Mandate" placeholder="Select Mandate" /><Select.Content>{mandates.map((mandate) => <Select.Item key={mandate.id} value={mandate.id}>{mandate.name} · {mandate.configuration_state === 'V1_CONFIGURED' ? `v${mandate.latest_version?.version_no}` : 'legacy unavailable'}</Select.Item>)}</Select.Content></Select.Root></label> : null}
        <label className="qz-field"><span className="qz-label">Policy family</span><TextField.Root dir="ltr" readOnly value="LONG_ONLY_MEAN_VARIANCE_V1" /></label>
        <label className="qz-field"><span className="qz-label">Objective</span><TextField.Root dir="ltr" readOnly value="MAXIMIZE_NET_RETURN" /></label>
        <label className="qz-field"><span className="qz-label">Eligible Alpha role</span><TextField.Root dir="ltr" readOnly value="PRIMARY_ALPHA" /></label>
        <label className="qz-field"><span className="qz-label">Base currency</span><TextField.Root dir="ltr" value={baseCurrency} onChange={(event) => setBaseCurrency(event.target.value)} placeholder="USD" /></label>
        <label className="qz-field"><span className="qz-label">Eligible Universe version</span><Select.Root value={universeId} onValueChange={setUniverseId}><Select.Trigger aria-label="Eligible Universe version" placeholder="Select Universe" /><Select.Content>{universes.map((universe) => <Select.Item key={universe.id} value={universe.id}>{universe.name} · v{universe.version_no}</Select.Item>)}</Select.Content></Select.Root></label>
        <label className="qz-field"><span className="qz-label">Minimum Alpha count</span><TextField.Root dir="ltr" inputMode="numeric" value={minimumAlphaCount} onChange={(event) => setMinimumAlphaCount(event.target.value)} /></label>
        {v1NumericFields.map(([field, label]) => <label className="qz-field" key={field}><span className="qz-label">{label}</span><TextField.Root aria-label={label} dir="ltr" inputMode="decimal" value={numbers[field] ?? ''} onChange={(event) => updateNumber(field, event.target.value)} placeholder="Decimal string" /></label>)}
        <label className="qz-field"><span className="qz-label">Version state</span><Select.Root value={state} onValueChange={setState}><Select.Trigger aria-label="Version state" placeholder="Select version state" /><Select.Content><Select.Item value="ACTIVE">Active</Select.Item><Select.Item value="RETIRED">Retired</Select.Item></Select.Content></Select.Root></label>
        <Button disabled={!complete || pending} onClick={() => void submit()}>{pending ? 'Creating…' : mode === 'CREATE' ? 'Create Mandate' : 'Create version'}</Button>
        <FormError error={error} />
      </div>
    </ResponsiveDialogContent>
  </Dialog.Root>;
}

function CapitalContextDialog() {
  const mutation = useCreateConfigurationCapitalContext();
  const [open, setOpen] = useState(false);
  const [baseCurrency, setBaseCurrency] = useState('');
  const [deployableCapital, setDeployableCapital] = useState('');
  const [observedAt, setObservedAt] = useState('');
  const [validUntil, setValidUntil] = useState('');
  const [notes, setNotes] = useState('');
  const [error, setError] = useState<unknown>();
  async function submit() {
    try {
      setError(undefined);
      await mutation.mutateAsync({
        base_currency: baseCurrency.trim(),
        deployable_capital: deployableCapital.trim(),
        observed_at: observedAt.trim(),
        valid_until: validUntil.trim(),
        ...(notes.trim() ? { notes: notes.trim() } : {}),
      });
      setOpen(false);
    } catch (requestError) { setError(requestError); }
  }
  const complete = baseCurrency.trim() && deployableCapital.trim() && observedAt.trim() && validUntil.trim();
  return <Dialog.Root open={open} onOpenChange={setOpen}>
    <Dialog.Trigger><Button size="1" variant="soft">Create Capital Context</Button></Dialog.Trigger>
    <ResponsiveDialogContent maxWidth="560px">
      <Dialog.Title>Create immutable Capital Context</Dialog.Title>
      <Dialog.Description size="2">This records an operator-supplied research input snapshot. It is not a downstream account, position, or deployable-cash lookup.</Dialog.Description>
      <div className="qz-form-grid" style={{ marginTop: 18 }}>
        <label className="qz-field"><span className="qz-label">Currency</span><TextField.Root dir="ltr" value={baseCurrency} onChange={(event) => setBaseCurrency(event.target.value)} placeholder="USD" /></label>
        <label className="qz-field"><span className="qz-label">Deployable capital</span><TextField.Root dir="ltr" inputMode="decimal" value={deployableCapital} onChange={(event) => setDeployableCapital(event.target.value)} placeholder="Decimal string" /></label>
        <label className="qz-field"><span className="qz-label">Observed at (UTC ISO 8601)</span><TextField.Root dir="ltr" value={observedAt} onChange={(event) => setObservedAt(event.target.value)} placeholder="2026-09-03T00:00:00Z" /></label>
        <label className="qz-field"><span className="qz-label">Valid until (UTC ISO 8601)</span><TextField.Root dir="ltr" value={validUntil} onChange={(event) => setValidUntil(event.target.value)} placeholder="2026-09-04T00:00:00Z" /></label>
        <label className="qz-field"><span className="qz-label">Notes (optional)</span><TextArea aria-label="Notes (optional)" dir="auto" value={notes} onChange={(event) => setNotes(event.target.value)} /></label>
        <Button disabled={!complete || mutation.isPending} onClick={() => void submit()}>{mutation.isPending ? 'Creating…' : 'Create Capital Context'}</Button>
        <FormError error={error} />
      </div>
    </ResponsiveDialogContent>
  </Dialog.Root>;
}

function DownstreamDialog({ onTokenIssued }: { onTokenIssued: (token: string) => void }) {
  const client = useQueryClient();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState('');
  const [environment, setEnvironment] = useState('PAPER');
  const [publicConfig, setPublicConfig] = useState('');
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>();
  async function submit() {
    try {
      setPending(true);
      setError(undefined);
      const registration = await apiRequest<ConfigurationDownstreamRegistration>('/api/v1/downstream-systems', { method: 'POST', body: jsonBody({ name: name.trim(), environment_type: environment, public_config: optionalJsonObject(publicConfig, 'Public configuration') }), idempotent: true });
      await client.invalidateQueries({ queryKey: ['downstreams'] });
      if (!registration.service_token) throw new Error('The downstream was registered, but its one-time token is unavailable. Rotate it from the registry.');
      setOpen(false);
      onTokenIssued(registration.service_token);
    } catch (requestError) { setError(requestError); }
    finally { setPending(false); }
  }
  return <Dialog.Root open={open} onOpenChange={setOpen}><Dialog.Trigger><Button size="1" variant="soft"><PlugsConnectedIcon size={14} />Register Paper/Live Downstream</Button></Dialog.Trigger><ResponsiveDialogContent maxWidth="520px"><Dialog.Title>Register logical Downstream</Dialog.Title><Dialog.Description size="2">QuaZonai registers a target-portfolio consumer only. It cannot start, stop, or control the independent Paper or Live runtime.</Dialog.Description><div className="qz-form-grid" style={{ marginTop: 18 }}><label className="qz-field"><span className="qz-label">Name</span><TextField.Root dir="auto" value={name} onChange={(event) => setName(event.target.value)} /></label><label className="qz-field"><span className="qz-label">Environment</span><Select.Root value={environment} onValueChange={setEnvironment}><Select.Trigger /><Select.Content>{['PAPER', 'LIVE'].map((value) => <Select.Item key={value} value={value}>{humanize(value)}</Select.Item>)}</Select.Content></Select.Root></label><JsonField label="Public downstream configuration (JSON, optional)" value={publicConfig} onChange={setPublicConfig} placeholder={'{"endpoint":"https://paper.example.invalid"}'} required={false} /><Button disabled={!name.trim() || pending} onClick={() => void submit()}>{pending ? 'Registering…' : 'Register Downstream'}</Button><FormError error={error} /></div></ResponsiveDialogContent></Dialog.Root>;
}

function OneTimeDownstreamTokenDialog({ token, onRecorded }: { token: string | null; onRecorded: () => void }) {
  return <Dialog.Root open={token !== null} onOpenChange={(nextOpen) => { if (!nextOpen) onRecorded(); }}><ResponsiveDialogContent maxWidth="520px"><Dialog.Title>Record the one-time downstream token</Dialog.Title><Dialog.Description size="2">Copy this token into the independent downstream now. It is held only in this open screen and cannot be shown again by this screen.</Dialog.Description><TextArea aria-label="One-time downstream token" dir="ltr" readOnly value={token ?? ''} style={{ marginTop: 18 }} /><Dialog.Close><Button style={{ marginTop: 18 }}>I have recorded it</Button></Dialog.Close></ResponsiveDialogContent></Dialog.Root>;
}

function ConfigurationOperationStatus({ operationId }: { operationId?: UUID }) {
  const operation = useConfigurationOperation(operationId);
  if (!operationId) return <div className="qz-resource-note">No configuration operation has been requested in this browser session. A request is not a readiness signal.</div>;
  if (operation.isLoading) return <div className="qz-resource-note">Loading configuration operation status…</div>;
  if (operation.error) return <ErrorPanel error={operation.error} />;
  const item = operation.data as ConfigurationOperation;
  return <div className="qz-panel qz-panel-pad qz-list"><div className="qz-list-row"><div className="qz-list-main"><div className="qz-list-title">{humanize(item.kind)} operation <span className="qz-mono">{item.id.slice(0, 12)}</span></div><div className="qz-list-subtitle">{item.kind} · attempt {item.attempt}{item.last_error ? ` · ${item.last_error}` : ''}</div></div><StateBadge state={item.state} /></div></div>;
}

export function AdministrationPage() {
  const readiness = useReadiness();
  const health = useHealth();
  const runtimeConfiguration = useRuntimeConfiguration();
  const universes = useConfigurationUniverses();
  const sources = useConfigurationDataSources();
  const datasets = useConfigurationDatasets();
  const mandates = useConfigurationMandates();
  const capitalContexts = useConfigurationCapitalContexts();
  const evaluationDatasetSelections = useConfigurationEvaluationDatasetSelections();
  const evaluationDesignVersions = useConfigurationEvaluationDesignVersions();
  const promotionPolicyVersions = useConfigurationPromotionPolicyVersions();
  const downstreams = useConfigurationDownstreams();
  const [operationId, setOperationId] = useState<UUID>();
  const [serviceToken, setServiceToken] = useState<string | null>(null);
  const queries = [readiness, health, runtimeConfiguration, universes, sources, datasets, mandates, capitalContexts, evaluationDatasetSelections, evaluationDesignVersions, promotionPolicyVersions, downstreams];
  if (queries.some((query) => query.isLoading)) return <PageSkeleton />;
  const error = queries.find((query) => query.error)?.error;
  if (error) return <ErrorPanel error={error} />;
  const universeItems = universes.data ?? [];
  const sourceItems = sources.data ?? [];
  const mandateItems = mandates.data ?? [];
  const capitalContextItems = capitalContexts.data ?? [];
  const evaluationDatasetSelectionItems = evaluationDatasetSelections.data ?? [];
  const evaluationDesignVersionItems = evaluationDesignVersions.data ?? [];
  const promotionPolicyVersionItems = promotionPolicyVersions.data ?? [];

  return <>
    <PageHeader title="Administration" description="Configure real low-frequency capabilities. Resource facts below use only the canonical configuration API; runtime health is shown separately and never substitutes for data validation." actions={<><UniverseDialog /><UniverseVersionDialog universes={universeItems} /><DataSourceDialog universes={universeItems} /><DataSourcePreflightDialog sources={sourceItems} onRequested={setOperationId} /><DatasetMaterializationDialog sources={sourceItems} universes={universeItems} onRequested={setOperationId} /><EvaluationDatasetSelectionDialog universes={universeItems} datasets={datasets.data ?? []} /><EvaluationDesignVersionDialog universes={universeItems} /><PromotionPolicyVersionDialog /><MandateDialog mandates={mandateItems} universes={universeItems} /><CapitalContextDialog /><DownstreamDialog onTokenIssued={setServiceToken} /></>} />
    <KpiStrip items={[{ label: 'System ready', value: ready(readiness.data?.SYSTEM_READY) ? 'YES' : 'NO' }, { label: 'Research ready', value: ready(readiness.data?.RESEARCH_READY) ? 'YES' : 'NO' }, { label: 'Paper handoff', value: ready(readiness.data?.PAPER_HANDOFF_READY) ? 'READY' : 'NOT READY' }, { label: 'Live handoff', value: ready(readiness.data?.LIVE_HANDOFF_READY) ? 'READY' : 'NOT READY' }]} />
    <Section title="System runtime" meta="Service readiness is operational state, not a claim that registered data is validated."><div className="qz-panel qz-panel-pad qz-grid-4">{Object.entries(health.data ?? {}).filter(([key]) => ['database', 'worker', 'agent_worker', 'evaluator', 'storage', 'codex'].includes(key)).map(([key, value]) => <div key={key}><div className="qz-label">{humanize(key)}</div><div style={{ marginTop: 6 }}><StateBadge state={typeof value === 'object' && value && 'state' in value ? String((value as { state: unknown }).state) : value ? 'READY' : 'UNKNOWN'} /></div></div>)}</div></Section>
    <Section title="Codex runtime" meta="Separate system configuration; it does not create research facts."><RuntimeConfigurationPanel configuration={runtimeConfiguration.data!} /></Section>
    <Section title="Configuration operations" meta="An accepted preflight or materialization request is a real operation, not a readiness signal or completed quality check."><ConfigurationOperationStatus operationId={operationId} /></Section>
    <Section title="Canonical capability registry" meta="These read-only records are loaded only from canonical /api/v1 resource endpoints. Legacy configuration is explicitly unavailable, not inferred."><DataTable data={universeItems} columns={universeColumns} searchPlaceholder="Filter Universes…" emptyTitle="No Universes" emptyDescription="Create an immutable Universe version before registering Data Sources, Dataset requests, or Mandates." getRowId={(row) => row.id} /><div style={{ marginTop: 18 }}><DataTable data={sourceItems} columns={dataSourceColumns} searchPlaceholder="Filter Data Sources…" emptyTitle="No governed Data Sources" emptyDescription="Register a governed source with actual field and availability semantics." getRowId={(row) => row.id} /></div><div style={{ marginTop: 18 }}><DataTable data={datasets.data ?? []} columns={datasetColumns} searchPlaceholder="Filter Dataset revisions…" emptyTitle="No Dataset revisions" emptyDescription="Materialization requests create pending revisions; quality and PIT results remain authoritative." getRowId={(row) => row.id} /></div><div style={{ marginTop: 18 }}><DataTable data={evaluationDatasetSelectionItems} columns={evaluationDatasetSelectionColumns} searchPlaceholder="Filter Evaluation Dataset Selections…" emptyTitle="No Evaluation Dataset Selections" emptyDescription="Select exact Discovery, Validation, and Sealed revisions; there is no latest-revision fallback." getRowId={(row) => row.id} /></div><div style={{ marginTop: 18 }}><DataTable data={evaluationDesignVersionItems} columns={evaluationDesignVersionColumns} searchPlaceholder="Filter Evaluation Designs…" emptyTitle="No Evaluation Design Versions" emptyDescription="Create a complete typed evaluation design before trusted evaluation can use one." getRowId={(row) => row.id} /></div><div style={{ marginTop: 18 }}><DataTable data={promotionPolicyVersionItems} columns={promotionPolicyVersionColumns} searchPlaceholder="Filter Promotion Policies…" emptyTitle="No Promotion Policy Versions" emptyDescription="Create fixed-metric ordered Alpha gates. Paper/Live policy configuration remains unavailable until typed downstream binding writers exist." getRowId={(row) => row.id} /></div><div style={{ marginTop: 18 }}><DataTable data={mandateItems} columns={mandateColumns} searchPlaceholder="Filter Mandates…" emptyTitle="No Portfolio Mandates" emptyDescription="Only complete V1 typed mandate versions are configured; legacy versions are unavailable until an operator creates a new V1 version." getRowId={(row) => row.id} /></div><div style={{ marginTop: 18 }}><DataTable data={capitalContextItems} columns={capitalContextColumns} searchPlaceholder="Filter Capital Contexts…" emptyTitle="No Capital Contexts" emptyDescription="Create a typed operator capital snapshot before portfolio promotion can use one." getRowId={(row) => row.id} /></div><div style={{ marginTop: 18 }}>{(downstreams.data ?? []).length ? <DataTable data={downstreams.data ?? []} columns={downstreamColumns(setServiceToken)} searchPlaceholder="Filter Downstreams…" getRowId={(row) => row.id} /> : <EmptyState title="No Paper or Live Downstreams" description="Register logical target-portfolio consumers only when handoff is needed." />}</div></Section>
    <OneTimeDownstreamTokenDialog token={serviceToken} onRecorded={() => setServiceToken(null)} />
  </>;
}
