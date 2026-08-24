import type { ColumnDef } from '@tanstack/react-table';
import { Link } from 'react-router-dom';
import { DataTable } from '../components/ui/DataTable';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { PageHeader } from '../components/ui/PageHeader';
import { PageSkeleton } from '../components/ui/Skeleton';
import { StateBadge } from '../components/ui/StateBadge';
import { useAlphaLibrary } from '../lib/api/hooks';
import type { AlphaQualification } from '../lib/api/types';
import { formatDateTime, humanize, readMetric } from '../lib/format';

const columns: ColumnDef<AlphaQualification, unknown>[] = [
  { accessorKey: 'name', header: 'Alpha', cell: ({ row }) => <div><div className="qz-list-title"><Link to={`/alpha/${row.original.id}`}>{row.original.name ?? `Alpha ${row.original.id.slice(0, 8)}`}</Link></div><div className="qz-list-subtitle qz-mono">{row.original.id}</div></div> },
  { accessorKey: 'universe', header: 'Universe', cell: ({ row }) => row.original.universe ?? row.original.universe_version_id?.slice(0, 8) ?? '—' },
  { accessorKey: 'horizon', header: 'Horizon', cell: ({ getValue }) => String(getValue() ?? '—') },
  { accessorKey: 'role', header: 'Role', cell: ({ getValue }) => humanize(String(getValue())) },
  { accessorKey: 'state', header: 'Qualification', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'degradation_state', header: 'Health', cell: ({ getValue }) => <StateBadge state={String(getValue() ?? 'HEALTHY')} /> },
  { id: 'evidence', header: 'Evidence', cell: ({ row }) => <span className="qz-number">{String(readMetric(row.original.metrics, ['search_adjusted_quality', 'edge', 'ic']) ?? '—')}</span> },
  { accessorKey: 'created_at', header: 'Qualified', cell: ({ getValue }) => formatDateTime(getValue() as string | undefined) },
];

export function AlphaLibraryPage() {
  const query = useAlphaLibrary();
  if (query.isLoading) return <PageSkeleton />;
  if (query.error) return <ErrorPanel error={query.error} />;
  return <><PageHeader title="Alpha Library" description="Immutable, scope-specific Alpha qualifications. Standalone predictors, diversifiers, hedges, regime signals and risk modulators remain explicit rather than collapsing into one score." /><DataTable data={query.data ?? []} columns={columns} searchPlaceholder="Filter by alpha, role, universe or state…" emptyTitle="Alpha Library is empty" emptyDescription="Qualified research assets appear after independent promotion evaluation." getRowId={(row) => row.id} ariaLabel="Alpha Library" /></>;
}
