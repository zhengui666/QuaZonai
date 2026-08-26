import { DatabaseIcon, PlugsConnectedIcon } from '@phosphor-icons/react';
import { Button, Dialog, Select, Switch, Tabs, TextField } from '@radix-ui/themes';
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
import { Section } from '../components/ui/Section';
import { useI18n } from '../i18n';
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
  useRuntimeConfiguration,
  useUniverses,
} from '../lib/api/hooks';
import type { DataSource, DatasetRevision, DownstreamSystem, MarketUniverse, PluginRelease, PortfolioMandate } from '../lib/api/types';
import { formatCompactNumber, formatDateTime, formatNumber, humanize } from '../lib/format';

function ready(value: unknown) { return typeof value === 'boolean' ? value : Boolean((value as { ready?: boolean } | undefined)?.ready); }

const dataSourceColumns: ColumnDef<DataSource, unknown>[] = [
  { accessorKey: 'name', header: 'Source', meta: { messageKey: 'admin.source' } },
  { accessorKey: 'provider', header: 'Provider', meta: { messageKey: 'admin.provider' } },
  { accessorKey: 'state', header: 'State', meta: { messageKey: 'research.state' }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'preflight_state', header: 'Preflight', meta: { messageKey: 'admin.preflight' }, cell: ({ getValue }) => <StateBadge state={String(getValue() ?? 'UNKNOWN')} /> },
  { accessorKey: 'update_cadence', header: 'Cadence', meta: { messageKey: 'admin.cadence' } },
  { id: 'fields', header: 'Fields', meta: { messageKey: 'admin.fields' }, cell: ({ row }) => <span className="qz-list-subtitle">{row.original.fields?.slice(0, 6).join(', ') || '—'}</span> },
];
const datasetColumns: ColumnDef<DatasetRevision, unknown>[] = [
  { accessorKey: 'id', header: 'Revision', meta: { messageKey: 'admin.revision' }, cell: ({ getValue }) => <span className="qz-mono">{String(getValue()).slice(0, 12)}</span> },
  { accessorKey: 'partition', header: 'Partition', meta: { messageKey: 'admin.partition' }, cell: ({ getValue }) => <StateBadge state={String(getValue() ?? 'DISCOVERY')} /> },
  { accessorKey: 'universe_name', header: 'Universe', meta: { messageKey: 'alpha.universe' } },
  { accessorKey: 'row_count', header: 'Rows', meta: { messageKey: 'admin.rows', searchFormat: 'compact' }, cell: ({ getValue }) => <span className="qz-number">{formatCompactNumber(getValue() as number)}</span> },
  { accessorKey: 'quality_state', header: 'Quality', meta: { messageKey: 'admin.quality' }, cell: ({ getValue }) => <StateBadge state={String(getValue() ?? 'UNKNOWN')} /> },
  { accessorKey: 'point_in_time_state', header: 'PIT', meta: { messageKey: 'admin.pit' }, cell: ({ getValue }) => <StateBadge state={String(getValue() ?? 'UNKNOWN')} /> },
  { accessorKey: 'created_at', header: 'Registered', meta: { messageKey: 'admin.registered' }, cell: ({ getValue }) => formatDateTime(getValue() as string | undefined) },
];
const universeColumns: ColumnDef<MarketUniverse, unknown>[] = [
  { accessorKey: 'name', header: 'Universe', meta: { messageKey: 'alpha.universe' } },
  { accessorKey: 'universe_key', header: 'Key', meta: { messageKey: 'admin.key' }, cell: ({ getValue }) => <span className="qz-mono">{String(getValue() ?? '—')}</span> },
  { accessorKey: 'version_no', header: 'Version', meta: { messageKey: 'admin.version' }, cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as number | string | null | undefined)}</span> },
  { accessorKey: 'state', header: 'State', meta: { messageKey: 'research.state' }, cell: ({ getValue }) => <StateBadge state={String(getValue() ?? 'ACTIVE')} /> },
];
const downstreamColumns: ColumnDef<DownstreamSystem, unknown>[] = [
  { accessorKey: 'name', header: 'System', meta: { messageKey: 'admin.system' } },
  { accessorKey: 'environment_type', header: 'Environment', meta: { messageKey: 'admin.environment' }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'enabled', header: 'Enabled', meta: { messageKey: 'admin.enabled' }, cell: ({ getValue }) => <StateBadge state={getValue() ? 'ENABLED' : 'DISABLED'} /> },
  { accessorKey: 'preflight_state', header: 'Preflight', meta: { messageKey: 'admin.preflight' }, cell: ({ getValue }) => <StateBadge state={String(getValue() ?? 'UNKNOWN')} /> },
  { accessorKey: 'package_contract_version', header: 'Package contract', meta: { messageKey: 'admin.packageContract' } },
  { accessorKey: 'feedback_contract_version', header: 'Feedback contract', meta: { messageKey: 'admin.feedbackContract' } },
];
const pluginColumns: ColumnDef<PluginRelease, unknown>[] = [
  { accessorKey: 'plugin_id', header: 'Plugin', meta: { messageKey: 'admin.plugin' } },
  { accessorKey: 'version', header: 'Version', meta: { messageKey: 'admin.version' } },
  { accessorKey: 'state', header: 'State', meta: { messageKey: 'research.state' }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { id: 'capabilities', header: 'Capabilities', meta: { messageKey: 'admin.capabilities' }, cell: ({ row }) => <span className="qz-list-subtitle">{row.original.capabilities?.map(humanize).join(', ') || '—'}</span> },
  { accessorKey: 'created_at', header: 'Created', meta: { messageKey: 'admin.created' }, cell: ({ getValue }) => formatDateTime(getValue() as string | undefined) },
];

type CapitalContextRow = { id: string; purpose: string; candidate: string; currency: string; deployable: number | string; observed?: string; validUntil?: string };
const capitalColumns: ColumnDef<CapitalContextRow, unknown>[] = [
  { accessorKey: 'purpose', header: 'Purpose', meta: { messageKey: 'admin.purpose' }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'candidate', header: 'Candidate', meta: { messageKey: 'common.candidate' }, cell: ({ getValue }) => <span className="qz-mono">{String(getValue()).slice(0, 12)}</span> },
  { accessorKey: 'currency', header: 'Currency', meta: { messageKey: 'admin.currency' } },
  { accessorKey: 'deployable', header: 'Deployable capital', meta: { messageKey: 'admin.deployableCapital' }, cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as number | string | null | undefined)}</span> },
  { accessorKey: 'observed', header: 'Observed', meta: { messageKey: 'research.observed' }, cell: ({ getValue }) => formatDateTime(getValue() as string | undefined) },
  { accessorKey: 'validUntil', header: 'Valid until', meta: { messageKey: 'admin.validUntil' }, cell: ({ getValue }) => formatDateTime(getValue() as string | undefined) },
];

function DataSourceDialog() {
  const { t } = useI18n();
  const mutation = useCreateDataSource();
  const [name, setName] = useState('');
  const [provider, setProvider] = useState('');
  const [fields, setFields] = useState('');
  return <Dialog.Root><Dialog.Trigger><Button size="1" variant="soft"><DatabaseIcon size={14} />{t('admin.registerDataSource')}</Button></Dialog.Trigger><Dialog.Content maxWidth="500px"><Dialog.Title>{t('admin.registerGoverned')}</Dialog.Title><Dialog.Description size="2">{t('admin.dataSourceDesc')}</Dialog.Description><div className="qz-form-grid" style={{ marginTop: 18 }}><label className="qz-field"><span className="qz-label">{t('admin.name')}</span><TextField.Root value={name} onChange={(event) => setName(event.target.value)} /></label><label className="qz-field"><span className="qz-label">{t('admin.provider')}</span><TextField.Root value={provider} onChange={(event) => setProvider(event.target.value)} /></label><label className="qz-field"><span className="qz-label">{t('admin.canonicalFields')}</span><TextField.Root value={fields} onChange={(event) => setFields(event.target.value)} placeholder="event_time, available_time, close, volume" /></label><Button disabled={!name.trim() || mutation.isPending} onClick={() => mutation.mutate({ name: name.trim(), provider: provider.trim(), fields: fields.split(',').map((value) => value.trim()).filter(Boolean), state: 'STAGED' })}>{mutation.isPending ? t('common.registering') : t('admin.register')}</Button></div></Dialog.Content></Dialog.Root>;
}

function DownstreamDialog() {
  const { t } = useI18n();
  const mutation = useCreateDownstream();
  const [name, setName] = useState('');
  const [environment, setEnvironment] = useState('PAPER');
  return <Dialog.Root><Dialog.Trigger><Button size="1" variant="soft"><PlugsConnectedIcon size={14} />{t('admin.registerDownstream')}</Button></Dialog.Trigger><Dialog.Content maxWidth="500px"><Dialog.Title>{t('admin.registerLogical')}</Dialog.Title><Dialog.Description size="2">{t('admin.downstreamDesc')}</Dialog.Description><div className="qz-form-grid" style={{ marginTop: 18 }}><label className="qz-field"><span className="qz-label">{t('admin.name')}</span><TextField.Root value={name} onChange={(event) => setName(event.target.value)} /></label><label className="qz-field"><span className="qz-label">{t('admin.environment')}</span><Select.Root value={environment} onValueChange={setEnvironment}><Select.Trigger /><Select.Content>{['PAPER', 'LIVE', 'EXTERNAL_BACKTEST'].map((value) => <Select.Item key={value} value={value}>{humanize(value)}</Select.Item>)}</Select.Content></Select.Root></label><Button disabled={!name.trim() || mutation.isPending} onClick={() => mutation.mutate({ name: name.trim(), environment_type: environment, enabled: true })}>{mutation.isPending ? t('common.registering') : t('admin.register')}</Button></div></Dialog.Content></Dialog.Root>;
}

function MandateRow({ mandate }: { mandate: PortfolioMandate }) {
  const { t } = useI18n();
  const toggle = useMandateToggle(mandate.id, mandate.enabled);
  return <div className="qz-list-row"><div className="qz-list-main"><div className="qz-list-title">{mandate.name}</div><div className="qz-list-subtitle">{String(mandate.spec_json?.objective ?? mandate.spec_json?.description ?? t('admin.versionedObjective'))}</div></div><div style={{ display: 'flex', gap: 10, alignItems: 'center' }}><StateBadge state={mandate.enabled ? 'ENABLED' : 'DISABLED'} /><Switch checked={mandate.enabled} disabled={toggle.isPending} onCheckedChange={() => toggle.mutate()} aria-label={`${t('admin.enabled')}: ${mandate.name}`} /></div></div>;
}

export function AdministrationPage() {
  const { t } = useI18n();
  const readiness = useReadiness();
  const health = useHealth();
  const runtimeConfiguration = useRuntimeConfiguration();
  const sources = useDataSources();
  const datasets = useDatasets();
  const universes = useUniverses();
  const downstreams = useDownstreams();
  const plugins = usePluginReleases();
  const mandates = useMandates();
  const approvals = useApprovals();
  const queries = [readiness, health, runtimeConfiguration, sources, datasets, universes, downstreams, plugins, mandates, approvals];
  if (queries.some((query) => query.isLoading)) return <PageSkeleton />;
  const error = queries.find((query) => query.error)?.error;
  if (error) return <ErrorPanel error={error} />;

  const capitalRows: CapitalContextRow[] = (approvals.data ?? []).flatMap((approval) => approval.capital_context ? [{
    id: approval.id,
    purpose: approval.purpose,
    candidate: approval.candidate_id,
    currency: approval.capital_context.base_currency ?? '—',
    deployable: approval.capital_context.deployable_capital ?? '—',
    observed: approval.capital_context.observed_at,
    validUntil: approval.capital_context.valid_until,
  }] : []);

  return (
    <>
      <PageHeader title="Administration" description="Low-frequency capability configuration and operational health. These controls prepare research and handoff capabilities; they never expose broker credentials or trading execution." actions={<><DataSourceDialog /><DownstreamDialog /></>} />
      <KpiStrip items={[{ label: 'System ready', value: ready(readiness.data?.SYSTEM_READY) ? 'YES' : 'NO' }, { label: 'Research ready', value: ready(readiness.data?.RESEARCH_READY) ? 'YES' : 'NO' }, { label: 'Paper handoff', value: ready(readiness.data?.PAPER_HANDOFF_READY) ? 'READY' : 'NOT READY' }, { label: 'Live handoff', value: ready(readiness.data?.LIVE_HANDOFF_READY) ? 'READY' : 'NOT READY' }]} />
      <Section title="Runtime health" meta="Authoritative service readiness, not UI guesses"><div className="qz-panel qz-panel-pad qz-grid-4">{Object.entries(health.data ?? {}).filter(([key]) => ['database', 'worker', 'agent_worker', 'evaluator', 'storage', 'codex'].includes(key)).map(([key, value]) => <div key={key}><div className="qz-label">{humanize(key)}</div><div style={{ marginTop: 6 }}><StateBadge state={typeof value === 'object' && value && 'state' in value ? String((value as { state: unknown }).state) : value ? 'READY' : 'UNKNOWN'} /></div></div>)}</div></Section>
      <RuntimeConfigurationPanel configuration={runtimeConfiguration.data!} />
      <Section title="Mandate templates" meta="Enable only the capital objectives you want researched"><div className="qz-panel qz-panel-pad qz-list">{(mandates.data ?? []).map((mandate) => <MandateRow key={mandate.id} mandate={mandate} />)}</div></Section>
      <Section title="Capability registry">
        <Tabs.Root defaultValue="data">
          <Tabs.List>
            <Tabs.Trigger value="data">{t('admin.dataSources')}</Tabs.Trigger>
            <Tabs.Trigger value="datasets">{t('admin.datasets')}</Tabs.Trigger>
            <Tabs.Trigger value="universes">{t('admin.universes')}</Tabs.Trigger>
            <Tabs.Trigger value="capital">{t('admin.capitalContextTab')}</Tabs.Trigger>
            <Tabs.Trigger value="downstreams">{t('admin.downstreams')}</Tabs.Trigger>
            <Tabs.Trigger value="plugins">{t('admin.plugins')}</Tabs.Trigger>
          </Tabs.List>
          <div style={{ marginTop: 12 }}>
            <Tabs.Content value="data"><DataTable data={sources.data ?? []} columns={dataSourceColumns} emptyTitle="No data sources" emptyDescription="Register an approved data connector before autonomous acquisition can run." /></Tabs.Content>
            <Tabs.Content value="datasets"><DataTable data={datasets.data ?? []} columns={datasetColumns} emptyTitle="No dataset revisions" emptyDescription="Discovery and Sealed dataset revisions appear after governed ingestion." /></Tabs.Content>
            <Tabs.Content value="universes"><DataTable data={universes.data ?? []} columns={universeColumns} emptyTitle="No universes" emptyDescription="Define Market Universe resources through the backend-supported administration contract before scoped research." /></Tabs.Content>
            <Tabs.Content value="capital"><div className="qz-resource-note">{t('admin.capitalNote')}</div><div style={{ marginTop: 10 }}><DataTable data={capitalRows} columns={capitalColumns} emptyTitle="No capital context snapshots" emptyDescription="Capital context appears when the backend includes a frozen context in an Approval snapshot." /></div></Tabs.Content>
            <Tabs.Content value="downstreams"><DataTable data={downstreams.data ?? []} columns={downstreamColumns} emptyTitle="No downstream systems" emptyDescription="Research can run without a downstream. Configure logical Paper or Live consumers only when handoff is needed." /></Tabs.Content>
            <Tabs.Content value="plugins">{(plugins.data ?? []).length ? <DataTable data={plugins.data ?? []} columns={pluginColumns} /> : <EmptyState title="No runtime plugins" description="Approved DATA/RESEARCH/HANDOFF plugin releases appear here. Execution plugins are outside QuaZonai." />}</Tabs.Content>
          </div>
        </Tabs.Root>
      </Section>
    </>
  );
}
