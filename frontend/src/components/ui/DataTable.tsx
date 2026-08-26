import { CaretDownIcon, CaretUpDownIcon, CaretUpIcon, ColumnsIcon, MagnifyingGlassIcon } from '@phosphor-icons/react';
import { Button, DropdownMenu, Select, TextField } from '@radix-ui/themes';
import {
  flexRender,
  getCoreRowModel,
  getFilteredRowModel,
  getPaginationRowModel,
  getSortedRowModel,
  type ColumnDef,
  type FilterFn,
  type SortingFn,
  type SortingState,
  type VisibilityState,
  useReactTable,
} from '@tanstack/react-table';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { localeLabels, useI18n, type Locale, type MessageKey } from '../../i18n';
import { translateDomainLabel } from '../../i18n/domain';
import { translateRuntimeLabel } from '../../i18n/runtime';
import { comparePlainDecimalStrings, formatDateTime, formatPlainDecimalString } from '../../lib/format';
import { EmptyState } from './EmptyState';

type SearchFormat = 'compact' | 'percent';
type LocalizedColumnMeta = { messageKey?: MessageKey; searchFormat?: SearchFormat; searchDecimals?: number; localizedSort?: boolean };

function columnMeta(meta: unknown): LocalizedColumnMeta | undefined {
  return meta && typeof meta === 'object' ? meta as LocalizedColumnMeta : undefined;
}

function messageKeyFromMeta(meta: unknown): MessageKey | undefined {
  return columnMeta(meta)?.messageKey;
}

function humanizeCanonical(value: string): string {
  return value.replaceAll('_', ' ').toLowerCase().replace(/(^|\s)\S/g, (character) => character.toUpperCase());
}

function finiteNumericValue(value: unknown): number | undefined {
  if (typeof value === 'number') return Number.isFinite(value) ? value : undefined;
  if (typeof value !== 'string' || !value.trim()) return undefined;
  const numeric = Number(value);
  return Number.isFinite(numeric) ? numeric : undefined;
}
function numericSearchValues(value: number, locale: Locale, meta?: LocalizedColumnMeta): string[] {
  const values = [
    String(value),
    new Intl.NumberFormat(locale).format(value),
    new Intl.NumberFormat(locale, { maximumSignificantDigits: 15 }).format(value),
  ];
  if (meta?.searchFormat === 'compact') {
    values.push(new Intl.NumberFormat(locale, { notation: 'compact', maximumFractionDigits: 2 }).format(value));
  }
  if (meta?.searchFormat === 'percent') {
    const decimals = meta.searchDecimals ?? 1;
    values.push(new Intl.NumberFormat(locale, {
      style: 'percent',
      minimumFractionDigits: decimals,
      maximumFractionDigits: decimals,
    }).format(value));
  }
  return values;
}

function searchableValues(value: unknown, locale: Locale, meta?: LocalizedColumnMeta): string[] {
  if (value === null || value === undefined) return [];
  if (Array.isArray(value)) return value.flatMap((item) => searchableValues(item, locale, meta));
  if (typeof value === 'boolean') {
    const source = value ? 'Enabled' : 'Disabled';
    return [String(value), translateDomainLabel(locale, source) ?? source];
  }
  if (typeof value === 'number') return numericSearchValues(value, locale, meta);
  if (typeof value === 'string') {
    const values = [value];
    const exactLocalizedValue = formatPlainDecimalString(value.trim(), locale);
    if (exactLocalizedValue !== null) values.push(exactLocalizedValue);
    else {
      const numericValue = finiteNumericValue(value);
      if (numericValue !== undefined) values.push(...numericSearchValues(numericValue, locale, meta));
    }
    const sourceLabel = humanizeCanonical(value);
    const domainLabel = translateDomainLabel(locale, sourceLabel);
    if (domainLabel) values.push(domainLabel);
    const runtimeLabel = translateRuntimeLabel(locale, sourceLabel);
    if (runtimeLabel) values.push(runtimeLabel);
    if (/^\d{4}-\d{2}-\d{2}(?:T|$)/.test(value)) values.push(formatDateTime(value));
    return values;
  }
  if (typeof value === 'object') return Object.values(value as Record<string, unknown>).flatMap((item) => searchableValues(item, locale, meta));
  return [String(value)];
}

function objectField(value: unknown, key: string): unknown {
  if (value === null || typeof value !== 'object') return undefined;
  return (value as Record<string, unknown>)[key];
}

function localizedSortLabel(value: unknown, locale: Locale): string {
  if (value === null || value === undefined) return '';
  if (Array.isArray(value)) return value.map((item) => localizedSortLabel(item, locale)).join(', ');
  if (typeof value === 'boolean') {
    const source = value ? 'Enabled' : 'Disabled';
    return translateDomainLabel(locale, source) ?? source;
  }
  if (typeof value === 'string') {
    const source = humanizeCanonical(value);
    return translateRuntimeLabel(locale, source) ?? translateDomainLabel(locale, source) ?? value;
  }
  return String(value);
}

function withPropertyAccessors<T>(defs: ColumnDef<T, unknown>[], data: T[]): ColumnDef<T, unknown>[] {
  return defs.map((def) => {
    if ('columns' in def) return def;
    const id = 'id' in def ? def.id : undefined;
    const hasAccessor = ('accessorKey' in def && def.accessorKey !== undefined) || ('accessorFn' in def && def.accessorFn !== undefined);
    if (!id || hasAccessor || !data.some((row) => objectField(row, id) !== undefined)) return def;
    return {
      ...def,
      accessorFn: (row: T) => objectField(row, id),
      enableSorting: def.enableSorting ?? false,
    } as ColumnDef<T, unknown>;
  });
}

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
  const { locale, t, text, plural } = useI18n();
  const [sorting, setSorting] = useState<SortingState>([]);
  const [globalFilter, setGlobalFilter] = useState('');
  useEffect(() => {
    setGlobalFilter('');
    setSorting([]);
  }, [locale]);
  const [columnVisibility, setColumnVisibility] = useState<VisibilityState>({});
  const viewportRef = useRef<HTMLDivElement>(null);
  const stableColumns = useMemo(() => withPropertyAccessors(columns, data), [columns, data]);
  const sortCollator = useMemo(() => new Intl.Collator(locale, { numeric: true, sensitivity: 'base' }), [locale]);
  const localizedSorting = useCallback<SortingFn<T>>((rowA, rowB, columnId) => {
    const rawLeft = rowA.getValue(columnId);
    const rawRight = rowB.getValue(columnId);
    const left = rawLeft === undefined ? objectField(rowA.original, columnId) : rawLeft;
    const right = rawRight === undefined ? objectField(rowB.original, columnId) : rawRight;
    const exactDecimalComparison = typeof left === 'string' && typeof right === 'string'
      ? comparePlainDecimalStrings(left.trim(), right.trim())
      : undefined;
    if (exactDecimalComparison !== undefined) return exactDecimalComparison;
    const leftNumber = finiteNumericValue(left);
    const rightNumber = finiteNumericValue(right);
    if (leftNumber !== undefined && rightNumber !== undefined) return leftNumber - rightNumber;
    const meta = columnMeta(rowA.getAllCells().find((cell) => cell.column.id === columnId)?.column.columnDef.meta);
    const leftLabel = meta?.localizedSort ? localizedSortLabel(left, locale) : left === null || left === undefined ? '' : String(left);
    const rightLabel = meta?.localizedSort ? localizedSortLabel(right, locale) : right === null || right === undefined ? '' : String(right);
    return sortCollator.compare(leftLabel, rightLabel);
  }, [locale, sortCollator]);
  const localizedGlobalFilter = useCallback<FilterFn<T>>((row, columnId, filterValue) => {
    const query = String(filterValue ?? '').trim().toLocaleLowerCase(locale);
    if (!query) return true;
    const meta = columnMeta(row.getAllCells().find((cell) => cell.column.id === columnId)?.column.columnDef.meta);
    const accessorValue = row.getValue(columnId);
    const value = accessorValue === undefined ? objectField(row.original, columnId) : accessorValue;
    return searchableValues(value, locale, meta).some((candidate) => candidate.toLocaleLowerCase(locale).includes(query));
  }, [locale]);
  const table = useReactTable({
    data,
    columns: stableColumns,
    defaultColumn: { sortingFn: localizedSorting },
    state: { sorting, globalFilter, columnVisibility },
    onSortingChange: setSorting,
    onGlobalFilterChange: setGlobalFilter,
    onColumnVisibilityChange: setColumnVisibility,
    globalFilterFn: localizedGlobalFilter,
    getColumnCanGlobalFilter: (column) => Boolean(column.accessorFn),
    getCoreRowModel: getCoreRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    initialState: { pagination: { pageSize: initialPageSize, pageIndex: 0 } },
    getRowId,
  });
  const filteredRowCount = table.getFilteredRowModel().rows.length;
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
        <TextField.Root className="qz-table-search" dir="auto" size="1" value={globalFilter} onChange={(event) => { setGlobalFilter(event.target.value); table.setPageIndex(0); }} aria-label={text(searchPlaceholder)} placeholder={text(searchPlaceholder)}>
          <TextField.Slot side={localeLabels[locale].dir === 'rtl' ? 'right' : 'left'}><MagnifyingGlassIcon size={14} /></TextField.Slot>
        </TextField.Root>
        <div className="qz-table-tools">
          <span className="qz-section-meta qz-number">{plural({
            zero: 'table.rows.zero',
            one: 'table.rows.one',
            two: 'table.rows.two',
            few: 'table.rows.few',
            many: 'table.rows.many',
            other: 'table.rows.other',
          }, filteredRowCount)}</span>
          <DropdownMenu.Root>
            <DropdownMenu.Trigger><Button size="1" variant="soft"><ColumnsIcon size={13} />{t('table.columns')}</Button></DropdownMenu.Trigger>
            <DropdownMenu.Content align="end">
              {table.getAllLeafColumns().filter((column) => column.getCanHide()).map((column) => {
                const messageKey = messageKeyFromMeta(column.columnDef.meta);
                return (
                  <DropdownMenu.CheckboxItem key={column.id} checked={column.getIsVisible()} onCheckedChange={(checked) => column.toggleVisibility(Boolean(checked))}>
                    {messageKey ? t(messageKey) : text(String(column.columnDef.header ?? column.id))}
                  </DropdownMenu.CheckboxItem>
                );
              })}
            </DropdownMenu.Content>
          </DropdownMenu.Root>
        </div>
      </div>
      <div ref={viewportRef} className="qz-table-viewport" data-virtualized={virtualEnabled ? 'true' : 'false'}>
        <table className="qz-table" aria-label={text(ariaLabel)}>
          <thead>
            {table.getHeaderGroups().map((group) => (
              <tr key={group.id}>
                {group.headers.map((header) => {
                  const sorted = header.column.getIsSorted();
                  const ariaSort = sorted === 'asc' ? 'ascending' : sorted === 'desc' ? 'descending' : header.column.getCanSort() ? 'none' : undefined;
                  const messageKey = messageKeyFromMeta(header.column.columnDef.meta);
                  const headerContent = messageKey
                    ? t(messageKey)
                    : typeof header.column.columnDef.header === 'string'
                      ? text(header.column.columnDef.header)
                      : flexRender(header.column.columnDef.header, header.getContext());
                  return (
                    <th key={header.id} colSpan={header.colSpan} aria-sort={ariaSort}>
                      {header.isPlaceholder ? null : header.column.getCanSort() ? (
                        <Button className="qz-sort-button" type="button" size="1" variant="ghost" onClick={header.column.getToggleSortingHandler()}>
                          {headerContent}
                          {sorted === 'asc' ? <CaretUpIcon size={11} /> : sorted === 'desc' ? <CaretDownIcon size={11} /> : <CaretUpDownIcon size={11} />}
                        </Button>
                      ) : headerContent}
                    </th>
                  );
                })}
              </tr>
            ))}
          </thead>
          <tbody>
            {paddingTop > 0 ? <tr aria-hidden="true"><td colSpan={table.getVisibleLeafColumns().length} style={{ height: paddingTop, padding: 0, border: 0 }} /></tr> : null}
            {visibleRows.map((row) => (
              <tr key={row.id}>{row.getVisibleCells().map((cell) => (
                <td key={cell.id} dir="auto">{flexRender(cell.column.columnDef.cell, cell.getContext())}</td>
              ))}</tr>
            ))}
            {paddingBottom > 0 ? <tr aria-hidden="true"><td colSpan={table.getVisibleLeafColumns().length} style={{ height: paddingBottom, padding: 0, border: 0 }} /></tr> : null}
          </tbody>
        </table>
      </div>
      <div className="qz-table-pagination">
        <span className="qz-number">{t('table.page', { page: table.getState().pagination.pageIndex + 1, pages: Math.max(1, table.getPageCount()) })}</span>
        <div className="qz-table-tools">
          <Select.Root value={String(table.getState().pagination.pageSize)} onValueChange={(value) => table.setPageSize(Number(value))}>
            <Select.Trigger aria-label={t('table.rowsPerPage')} />
            <Select.Content>{[20, 50, 100].map((size) => <Select.Item value={String(size)} key={size}>{t('table.perPage', { count: size })}</Select.Item>)}</Select.Content>
          </Select.Root>
          <Button size="1" variant="soft" disabled={!table.getCanPreviousPage()} onClick={() => table.previousPage()}>{t('table.previous')}</Button>
          <Button size="1" variant="soft" disabled={!table.getCanNextPage()} onClick={() => table.nextPage()}>{t('table.next')}</Button>
        </div>
      </div>
    </div>
  );
}
