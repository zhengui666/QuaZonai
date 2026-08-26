import type { ColumnDef } from '@tanstack/react-table';
import { Link } from 'react-router-dom';
import { DataTable } from '../components/ui/DataTable';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { PageHeader } from '../components/ui/PageHeader';
import { PageSkeleton } from '../components/ui/Skeleton';
import { StateBadge } from '../components/ui/StateBadge';
import { useI18n } from '../i18n';
import { useAlphaLibrary } from '../lib/api/hooks';
import type { AlphaQualification } from '../lib/api/types';
import { formatDateTime, formatNumber, humanize, readMetric } from '../lib/format';

function AlphaName({ alpha }: { alpha: AlphaQualification }) {
  const { t } = useI18n();
  if (alpha.name) return <span dir="auto">{alpha.name}</span>;
  return <><span>{t('alpha.name')}</span>{' '}<bdi dir="ltr">{alpha.id.slice(0, 8)}</bdi></>;
}

const columns: ColumnDef<AlphaQualification, unknown>[] = [  { accessorKey: 'name', header: 'Alpha', meta: { messageKey: 'alpha.name' }, cell: ({ row }) => <div><div className="qz-list-title"><Link to={`/alpha/${row.original.id}`}><AlphaName alpha={row.original} /></Link></div><div className="qz-list-subtitle qz-mono"><bdi dir="ltr">{row.original.id}</bdi></div></div> },
  { accessorKey: 'universe', header: 'Universe', cell: ({ row }) => row.original.universe ? <span dir="auto">{row.original.universe}</span> : <bdi dir="ltr">{row.original.universe_version_id?.slice(0, 8) ?? '—'}</bdi> },
  { accessorKey: 'horizon', header: 'Horizon', cell: ({ getValue }) => String(getValue() ?? '—') },
  { accessorKey: 'role', header: 'Role', meta: { localizedSort: true }, cell: ({ getValue }) => humanize(String(getValue())) },
  { accessorKey: 'state', header: 'Qualification', meta: { localizedSort: true }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'degradation_state', header: 'Health', meta: { localizedSort: true }, cell: ({ getValue }) => <StateBadge state={String(getValue() ?? 'HEALTHY')} /> },
  { id: 'evidence', accessorFn: (row) => readMetric(row.metrics, ['search_adjusted_quality', 'edge', 'ic']), header: 'Evidence', cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as number | string | null | undefined, { maximumSignificantDigits: 15 })}</span> },
  { accessorKey: 'created_at', header: 'Qualified', cell: ({ getValue }) => formatDateTime(getValue() as string | undefined) },
];

export function AlphaLibraryPage() {
  const query = useAlphaLibrary();
  if (query.isLoading) return <PageSkeleton />;
  if (query.error) return <ErrorPanel error={query.error} />;
  return <><PageHeader title="Alpha Library" description="Immutable, scope-specific Alpha qualifications. Standalone predictors, diversifiers, hedges, regime signals and risk modulators remain explicit rather than collapsing into one score." /><DataTable data={query.data ?? []} columns={columns} searchPlaceholder="Filter by alpha, role, universe or state…" emptyTitle="Alpha Library is empty" emptyDescription="Qualified research assets appear after independent promotion evaluation." getRowId={(row) => row.id} ariaLabel="Alpha Library" /></>;
}
