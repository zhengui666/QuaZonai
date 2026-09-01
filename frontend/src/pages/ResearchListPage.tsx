import { FlaskIcon } from '@phosphor-icons/react';
import { Button } from '@radix-ui/themes';
import type { ColumnDef } from '@tanstack/react-table';
import { Link } from 'react-router-dom';
import { DataTable } from '../components/ui/DataTable';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { ForwardArrowIcon } from '../components/ui/ForwardArrowIcon';
import { PageHeader } from '../components/ui/PageHeader';
import { PageSkeleton } from '../components/ui/Skeleton';
import { StateBadge } from '../components/ui/StateBadge';
import { Translated } from '../i18n';
import { usePrograms } from '../lib/api/hooks';
import type { ResearchProgram } from '../lib/api/types';
import { formatDateTime, formatNumber } from '../lib/format';

const columns: ColumnDef<ResearchProgram, unknown>[] = [
  { accessorKey: 'title', header: 'Program', meta: { mobile: { placement: 'title' } }, cell: ({ row }) => <div><div className="qz-list-title" dir="auto">{row.original.title ?? row.original.charter?.research_question ?? row.original.id.slice(0, 8)}</div><div className="qz-list-subtitle qz-mono"><bdi dir="ltr">{row.original.id}</bdi></div></div> },
  { accessorKey: 'state', header: 'State', meta: { localizedSort: true, mobile: { placement: 'badge' } }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'mission_count', header: 'Missions', cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as number | undefined)}</span> },
  { accessorKey: 'alpha_count', header: 'Alphas', cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as number | undefined)}</span> },
  { accessorKey: 'updated_at', header: 'Updated', cell: ({ getValue }) => formatDateTime(getValue() as string | undefined) },
  { id: 'open', header: '', meta: { mobile: { placement: 'action' } }, cell: ({ row }) => <Button asChild size="1" variant="ghost"><Link to={`/research/${row.original.id}`}><Translated source="Open" /> <ForwardArrowIcon size={12} /></Link></Button>, enableSorting: false },
];

export function ResearchListPage() {
  const query = usePrograms();
  if (query.isLoading) return <PageSkeleton />;
  if (query.error) return <ErrorPanel error={query.error} />;
  return <><PageHeader title="Research Observatory" description="Long-lived Programs stay autonomous. Inspect lineage, evidence, Mission activity, and why a research line is progressing, cooling, or blocked." actions={<Button asChild><Link to="/ideas"><FlaskIcon size={15} /><Translated source="New idea" /></Link></Button>} /><DataTable data={query.data ?? []} columns={columns} searchPlaceholder="Filter programs…" emptyTitle="No research programs" emptyDescription="Start with an investment idea. QuaZonai will create and manage the Mission graph automatically." getRowId={(row) => row.id} /></>;
}
