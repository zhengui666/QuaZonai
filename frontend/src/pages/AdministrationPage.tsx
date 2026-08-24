import { DatabaseIcon, PlugsConnectedIcon } from '@phosphor-icons/react';
import { Button, Dialog, Select, Switch, Tabs, TextField } from '@radix-ui/themes';
import type { ColumnDef } from '@tanstack/react-table';
import { useState } from 'react';
import { DataTable } from '../components/ui/DataTable';
import { EmptyState } from '../components/ui/EmptyState';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { KpiStrip } from '../components/ui/KpiStrip';
import { PageHeader } from '../components/ui/PageHeader';
import { PageSkeleton } from '../components/ui/Skeleton';
import { StateBadge } from '../components/ui/StateBadge';
import { Section } from '../components/ui/Section';
import {
  useApprovals,
  useCreateDataSource,
  useCreateDownstream,
  useDataSources,
  useDatasets,
  useDownstreams,
  useHealth,
  useMandateToggle,
  useMandates,
  usePluginReleases,
  useReadiness,
  useUniverses,
} from '../lib/api/hooks';
import type { DataSource, DatasetRevision, DownstreamSystem, MarketUniverse, PluginRelease, PortfolioMandate } from '../lib/api/types';
import { formatCompactNumber, formatDateTime, humanize } from '../lib/format';

function ready(value: unknown) { return typeof value === 'boolean' ? value : Boolean((value as { ready?: boolean } | undefined)?.ready); }

const dataSourceColumns: ColumnDef<DataSource, unknown>[] = [
  { accessorKey: 'name', header: 'Source' },
  { accessorKey: 'provider', header: 'Provider' },
  { accessorKey: 'state', header: 'State', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'preflight_state', header: 'Preflight', cell: ({ getValue }) => <StateBadge state={String(getValue() ?? 'UNKNOWN')} /> },
  { accessorKey: 'update_cadence', header: 'Cadence' },
  { id: 'fields', header: 'Fields', cell: ({ row }) => <span className="qz-list-subtitle">{row.original.fields?.slice(0, 6).join(', ') || '—'}</span> },
];
const datasetColumns: ColumnDef<DatasetRevision, unknown>[] = [
  { accessorKey: 'id', header: 'Revision', cell: ({ getValue }) => <span className="qz-mono">{String(getValue()).slice(0, 12)}</span> },
  { accessorKey: 'partition', header: 'Partition', cell: ({ getValue }) => <StateBadge state={String(getValue() ?? 'DISCOVERY')} /> },
  { accessorKey: 'universe_name', header: 'Universe' },
  { accessorKey: 'row_count', header: 'Rows', cell: ({ getValue }) => <span className="qz-number">{formatCompactNumber(getValue() as number)}</span> },
  { accessorKey: 'quality_state', header: 'Quality', cell: ({ getValue }) => <StateBadge state={String(getValue() ?? 'UNKNOWN')} /> },
  { accessorKey: 'point_in_time_state', header: 'PIT', cell: ({ getValue }) => <StateBadge state={String(getValue() ?? 'UNKNOWN')} /> },
  { accessorKey: 'created_at', header: 'Registered', cell: ({ getValue }) => formatDateTime(getValue() as string | undefined) },
];
const universeColumns: ColumnDef<MarketUniverse, unknown>[] = [
  { accessorKey: 'name', header: 'Universe' },
  { accessorKey: 'universe_key', header: 'Key', cell: ({ getValue }) => <span className="qz-mono">{String(getValue() ?? '—')}</span> },
  { accessorKey: 'version_no', header: 'Version', cell: ({ getValue }) => <span className="qz-number">{String(getValue() ?? '—')}</span> },
  { accessorKey: 'state', header: 'State', cell: ({ getValue }) => <StateBadge state={String(getValue() ?? 'ACTIVE')} /> },
];
const downstreamColumns: ColumnDef<DownstreamSystem, unknown>[] = [
  { accessorKey: 'name', header: 'System' },
  { accessorKey: 'environment_type', header: 'Environment', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'enabled', header: 'Enabled', cell: ({ getValue }) => <StateBadge state={getValue() ? 'ENABLED' : 'DISABLED'} /> },
  { accessorKey: 'preflight_state', header: 'Preflight', cell: ({ getValue }) => <StateBadge state={String(getValue() ?? 'UNKNOWN')} /> },
  { accessorKey: 'package_contract_version', header: 'Package contract' },
  { accessorKey: 'feedback_contract_version', header: 'Feedback contract' },
];
const pluginColumns: ColumnDef<PluginRelease, unknown>[] = [
  { accessorKey: 'plugin_id', header: 'Plugin' },
  { accessorKey: 'version', header: 'Version' },
  { accessorKey: 'state', header: 'State', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { id: 'capabilities', header: 'Capabilities', cell: ({ row }) => <span className="qz-list-subtitle">{row.original.capabilities?.map(humanize).join(', ') || '—'}</span> },
  { accessorKey: 'created_at', header: 'Created', cell: ({ getValue }) => formatDateTime(getValue() as string | undefined) },
];

type CapitalContextRow = { id: string; purpose: string; candidate: string; currency: string; deployable: string; observed?: string; validUntil?: string };
const capitalColumns: ColumnDef<CapitalContextRow, unknown>[] = [
  { accessorKey: 'purpose', header: 'Purpose', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'candidate', header: 'Candidate', cell: ({ getValue }) => <span className="qz-mono">{String(getValue()).slice(0, 12)}</span> },
  { accessorKey: 'currency', header: 'Currency' },
  { accessorKey: 'deployable', header: 'Deployable capital', cell: ({ getValue }) => <span className="qz-number">{String(getValue())}</span> },
  { accessorKey: 'observed', header: 'Observed', cell: ({ getValue }) => formatDateTime(getValue() as string | undefined) },
  { accessorKey: 'validUntil', header: 'Valid until', cell: ({ getValue }) => formatDateTime(getValue() as string | undefined) },
];

function DataSourceDialog() {
  const mutation = useCreateDataSource();
  const [name, setName] = useState('');
  const [provider, setProvider] = useState('');
  const [fields, setFields] = useState('');
  return <Dialog.Root><Dialog.Trigger><Button size="1" variant="soft"><DatabaseIcon size={14} />Register data source</Button></Dialog.Trigger><Dialog.Content maxWidth="500px"><Dialog.Title>Register governed data source</Dialog.Title><Dialog.Description size="2">This creates public connector configuration only. Credentials remain write-only and are never echoed into research UI.</Dialog.Description><div className="qz-form-grid" style={{ marginTop: 18 }}><label className="qz-field"><span className="qz-label">Name</span><TextField.Root value={name} onChange={(event) => setName(event.target.value)} /></label><label className="qz-field"><span className="qz-label">Provider</span><TextField.Root value={provider} onChange={(event) => setProvider(event.target.value)} /></label><label className="qz-field"><span className="qz-label">Canonical fields</span><TextField.Root value={fields} onChange={(event) => setFields(event.target.value)} placeholder="event_time, available_time, close, volume" /></label><Button disabled={!name.trim() || mutation.isPending} onClick={() => mutation.mutate({ name: name.trim(), provider: provider.trim(), fields: fields.split(',').map((value) => value.trim()).filter(Boolean), state: 'STAGED' })}>{mutation.isPending ? 'Registering…' : 'Register'}</Button></div></Dialog.Content></Dialog.Root>;
}

function DownstreamDialog() {
  const mutation = useCreateDownstream();
  const [name, setName] = useState('');
  const [environment, setEnvironment] = useState('PAPER');
  return <Dialog.Root><Dialog.Trigger><Button size="1" variant="soft"><PlugsConnectedIcon size={14} />Register downstream</Button></Dialog.Trigger><Dialog.Content maxWidth="500px"><Dialog.Title>Register logical downstream</Dialog.Title><Dialog.Description size="2">QuaZonai publishes packages to logical consumers; it does not own their runtime, accounts or execution.</Dialog.Description><div className="qz-form-grid" style={{ marginTop: 18 }}><label className="qz-field"><span className="qz-label">Name</span><TextField.Root value={name} onChange={(event) => setName(event.target.value)} /></label><label className="qz-field"><span className="qz-label">Environment</span><Select.Root value={environment} onValueChange={setEnvironment}><Select.Trigger /><Select.Content>{['PAPER', 'LIVE', 'EXTERNAL_BACKTEST'].map((value) => <Select.Item key={value} value={value}>{humanize(value)}</Select.Item>)}</Select.Content></Select.Root></label><Button disabled={!name.trim() || mutation.isPending} onClick={() => mutation.mutate({ name: name.trim(), environment_type: environment, enabled: true })}>{mutation.isPending ? 'Registering…' : 'Register'}</Button></div></Dialog.Content></Dialog.Root>;
}

function MandateRow({ mandate }: { mandate: PortfolioMandate }) {
  const toggle = useMandateToggle(mandate.id, mandate.enabled);
  return <div className="qz-list-row"><div className="qz-list-main"><div className="qz-list-title">{mandate.name}</div><div className="qz-list-subtitle">{String(mandate.spec_json?.objective ?? mandate.spec_json?.description ?? 'Versioned capital objective')}</div></div><div style={{ display: 'flex', gap: 10, alignItems: 'center' }}><StateBadge state={mandate.enabled ? 'ENABLED' : 'DISABLED'} /><Switch checked={mandate.enabled} disabled={toggle.isPending} onCheckedChange={() => toggle.mutate()} aria-label={`Toggle ${mandate.name}`} /></div></div>;
}

export function AdministrationPage() {
  const readiness = useReadiness();
  const health = useHealth();
  const sources = useDataSources();
  const datasets = useDatasets();
  const universes = useUniverses();
  const downstreams = useDownstreams();
  const plugins = usePluginReleases();
  const mandates = useMandates();
  const approvals = useApprovals();
  const queries = [readiness, health, sources, datasets, universes, downstreams, plugins, mandates, approvals];
  if (queries.some((query) => query.isLoading)) return <PageSkeleton />;
  const error = queries.find((query) => query.error)?.error;
  if (error) return <ErrorPanel error={error} />;

  const capitalRows: CapitalContextRow[] = (approvals.data ?? []).flatMap((approval) => approval.capital_context ? [{
    id: approval.id,
    purpose: approval.purpose,
    candidate: approval.candidate_id,
    currency: approval.capital_context.base_currency ?? '—',
    deployable: String(approval.capital_context.deployable_capital ?? '—'),
    observed: approval.capital_context.observed_at,
    validUntil: approval.capital_context.valid_until,
  }] : []);

  return (
    <>
      <PageHeader title="Administration" description="Low-frequency capability configuration and operational health. These controls prepare research and handoff capabilities; they never expose broker credentials or trading execution." actions={<><DataSourceDialog /><DownstreamDialog /></>} />
      <KpiStrip items={[{ label: 'System ready', value: ready(readiness.data?.SYSTEM_READY) ? 'YES' : 'NO' }, { label: 'Research ready', value: ready(readiness.data?.RESEARCH_READY) ? 'YES' : 'NO' }, { label: 'Paper handoff', value: ready(readiness.data?.PAPER_HANDOFF_READY) ? 'READY' : 'NOT READY' }, { label: 'Live handoff', value: ready(readiness.data?.LIVE_HANDOFF_READY) ? 'READY' : 'NOT READY' }]} />
      <Section title="Runtime health" meta="Authoritative service readiness, not UI guesses"><div className="qz-panel qz-panel-pad qz-grid-4">{Object.entries(health.data ?? {}).filter(([key]) => ['database', 'worker', 'agent_worker', 'evaluator', 'storage', 'codex'].includes(key)).map(([key, value]) => <div key={key}><div className="qz-label">{humanize(key)}</div><div style={{ marginTop: 6 }}><StateBadge state={typeof value === 'object' && value && 'state' in value ? String((value as { state: unknown }).state) : value ? 'READY' : 'UNKNOWN'} /></div></div>)}</div></Section>
      <Section title="Mandate templates" meta="Enable only the capital objectives you want researched"><div className="qz-panel qz-panel-pad qz-list">{(mandates.data ?? []).map((mandate) => <MandateRow key={mandate.id} mandate={mandate} />)}</div></Section>
      <Section title="Capability registry">
        <Tabs.Root defaultValue="data">
          <Tabs.List>
            <Tabs.Trigger value="data">Data sources</Tabs.Trigger>
            <Tabs.Trigger value="datasets">Datasets</Tabs.Trigger>
            <Tabs.Trigger value="universes">Universes</Tabs.Trigger>
            <Tabs.Trigger value="capital">Capital context</Tabs.Trigger>
            <Tabs.Trigger value="downstreams">Downstreams</Tabs.Trigger>
            <Tabs.Trigger value="plugins">Plugins</Tabs.Trigger>
          </Tabs.List>
          <div style={{ marginTop: 12 }}>
            <Tabs.Content value="data"><DataTable data={sources.data ?? []} columns={dataSourceColumns} emptyTitle="No data sources" emptyDescription="Register an approved data connector before autonomous acquisition can run." /></Tabs.Content>
            <Tabs.Content value="datasets"><DataTable data={datasets.data ?? []} columns={datasetColumns} emptyTitle="No dataset revisions" emptyDescription="Discovery and Sealed dataset revisions appear after governed ingestion." /></Tabs.Content>
            <Tabs.Content value="universes"><DataTable data={universes.data ?? []} columns={universeColumns} emptyTitle="No universes" emptyDescription="Define Market Universe resources through the backend-supported administration contract before scoped research." /></Tabs.Content>
            <Tabs.Content value="capital"><div className="qz-resource-note">Capital Context is immutable once frozen into a Candidate/Approval. The current public API has no Capital Context mutation endpoint, so the frontend deliberately exposes observed snapshots without inventing unsupported editing.</div><div style={{ marginTop: 10 }}><DataTable data={capitalRows} columns={capitalColumns} emptyTitle="No capital context snapshots" emptyDescription="Capital context appears when the backend includes a frozen context in an Approval snapshot." /></div></Tabs.Content>
            <Tabs.Content value="downstreams"><DataTable data={downstreams.data ?? []} columns={downstreamColumns} emptyTitle="No downstream systems" emptyDescription="Research can run without a downstream. Configure logical Paper or Live consumers only when handoff is needed." /></Tabs.Content>
            <Tabs.Content value="plugins">{(plugins.data ?? []).length ? <DataTable data={plugins.data ?? []} columns={pluginColumns} /> : <EmptyState title="No runtime plugins" description="Approved DATA/RESEARCH/HANDOFF plugin releases appear here. Execution plugins are outside QuaZonai." />}</Tabs.Content>
          </div>
        </Tabs.Root>
      </Section>
    </>
  );
}
