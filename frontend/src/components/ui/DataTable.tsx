import { CaretDownIcon, CaretUpDownIcon, CaretUpIcon, ColumnsIcon, MagnifyingGlassIcon } from '@phosphor-icons/react';
import { Button, DropdownMenu, Select, TextField } from '@radix-ui/themes';
import {
  flexRender,
  getCoreRowModel,
  getFilteredRowModel,
  getPaginationRowModel,
  getSortedRowModel,
  type ColumnDef,
  type SortingState,
  type VisibilityState,
  useReactTable,
} from '@tanstack/react-table';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useMemo, useRef, useState } from 'react';
import { EmptyState } from './EmptyState';

interface DataTableProps<T> {
  data: T[];
  columns: ColumnDef<T, unknown>[];
  searchPlaceholder?: string;
  emptyTitle?: string;
  emptyDescription?: string;
  initialPageSize?: number;
  getRowId?: (row: T) => string;
  ariaLabel?: string;
  enableVirtualization?: boolean;
}

export function DataTable<T>({
  data,
  columns,
  searchPlaceholder = 'Filter rows…',
  emptyTitle = 'No records',
  emptyDescription = 'No data matched the current filters.',
  initialPageSize = 50,
  getRowId,
  ariaLabel = 'Data table',
  enableVirtualization = true,
}: DataTableProps<T>) {
  const [sorting, setSorting] = useState<SortingState>([]);
  const [globalFilter, setGlobalFilter] = useState('');
  const [columnVisibility, setColumnVisibility] = useState<VisibilityState>({});
  const viewportRef = useRef<HTMLDivElement>(null);
  const stableColumns = useMemo(() => columns, [columns]);
  const table = useReactTable({
    data,
    columns: stableColumns,
    state: { sorting, globalFilter, columnVisibility },
    onSortingChange: setSorting,
    onGlobalFilterChange: setGlobalFilter,
    onColumnVisibilityChange: setColumnVisibility,
    getCoreRowModel: getCoreRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    initialState: { pagination: { pageSize: initialPageSize, pageIndex: 0 } },
    getRowId,
  });
  const rows = table.getRowModel().rows;
  const virtualEnabled = enableVirtualization && data.length > 100 && rows.length > 30;
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => viewportRef.current,
    estimateSize: () => 48,
    overscan: 8,
    enabled: virtualEnabled,
  });
  const virtualRows = virtualEnabled ? virtualizer.getVirtualItems() : [];
  const paddingTop = virtualEnabled && virtualRows.length ? virtualRows[0].start : 0;
  const paddingBottom = virtualEnabled && virtualRows.length ? virtualizer.getTotalSize() - virtualRows[virtualRows.length - 1].end : 0;
  const visibleRows = virtualEnabled ? virtualRows.map((item) => rows[item.index]) : rows;

  if (data.length === 0) return <EmptyState title={emptyTitle} description={emptyDescription} />;

  return (
    <div className="qz-table-shell">
      <div className="qz-table-toolbar">
        <TextField.Root className="qz-table-search" size="1" value={globalFilter} onChange={(event) => { setGlobalFilter(event.target.value); table.setPageIndex(0); }} placeholder={searchPlaceholder}>
          <TextField.Slot><MagnifyingGlassIcon size={14} /></TextField.Slot>
        </TextField.Root>
        <div className="qz-table-tools">
          <span className="qz-section-meta qz-number">{table.getFilteredRowModel().rows.length} rows</span>
          <DropdownMenu.Root>
            <DropdownMenu.Trigger><Button size="1" variant="soft"><ColumnsIcon size={13} />Columns</Button></DropdownMenu.Trigger>
            <DropdownMenu.Content align="end">
              {table.getAllLeafColumns().filter((column) => column.getCanHide()).map((column) => (
                <DropdownMenu.CheckboxItem key={column.id} checked={column.getIsVisible()} onCheckedChange={(checked) => column.toggleVisibility(Boolean(checked))}>
                  {String(column.columnDef.header ?? column.id)}
                </DropdownMenu.CheckboxItem>
              ))}
            </DropdownMenu.Content>
          </DropdownMenu.Root>
        </div>
      </div>
      <div ref={viewportRef} className="qz-table-viewport" data-virtualized={virtualEnabled ? 'true' : 'false'}>
        <table className="qz-table" aria-label={ariaLabel}>
          <thead>
            {table.getHeaderGroups().map((group) => (
              <tr key={group.id}>
                {group.headers.map((header) => {
                  const sorted = header.column.getIsSorted();
                  const ariaSort = sorted === 'asc' ? 'ascending' : sorted === 'desc' ? 'descending' : header.column.getCanSort() ? 'none' : undefined;
                  return (
                    <th key={header.id} colSpan={header.colSpan} aria-sort={ariaSort}>
                      {header.isPlaceholder ? null : header.column.getCanSort() ? (
                        <Button className="qz-sort-button" type="button" size="1" variant="ghost" onClick={header.column.getToggleSortingHandler()}>
                          {flexRender(header.column.columnDef.header, header.getContext())}
                          {sorted === 'asc' ? <CaretUpIcon size={11} /> : sorted === 'desc' ? <CaretDownIcon size={11} /> : <CaretUpDownIcon size={11} />}
                        </Button>
                      ) : flexRender(header.column.columnDef.header, header.getContext())}
                    </th>
                  );
                })}
              </tr>
            ))}
          </thead>
          <tbody>
            {paddingTop > 0 ? <tr aria-hidden="true"><td colSpan={table.getVisibleLeafColumns().length} style={{ height: paddingTop, padding: 0, border: 0 }} /></tr> : null}
            {visibleRows.map((row) => (
              <tr key={row.id}>{row.getVisibleCells().map((cell) => <td key={cell.id}>{flexRender(cell.column.columnDef.cell, cell.getContext())}</td>)}</tr>
            ))}
            {paddingBottom > 0 ? <tr aria-hidden="true"><td colSpan={table.getVisibleLeafColumns().length} style={{ height: paddingBottom, padding: 0, border: 0 }} /></tr> : null}
          </tbody>
        </table>
      </div>
      <div className="qz-table-pagination">
        <span className="qz-number">Page {table.getState().pagination.pageIndex + 1} / {Math.max(1, table.getPageCount())}</span>
        <div className="qz-table-tools">
          <Select.Root value={String(table.getState().pagination.pageSize)} onValueChange={(value) => table.setPageSize(Number(value))}>
            <Select.Trigger aria-label="Rows per page" />
            <Select.Content>{[20, 50, 100].map((size) => <Select.Item value={String(size)} key={size}>{size} / page</Select.Item>)}</Select.Content>
          </Select.Root>
          <Button size="1" variant="soft" disabled={!table.getCanPreviousPage()} onClick={() => table.previousPage()}>Previous</Button>
          <Button size="1" variant="soft" disabled={!table.getCanNextPage()} onClick={() => table.nextPage()}>Next</Button>
        </div>
      </div>
    </div>
  );
}
