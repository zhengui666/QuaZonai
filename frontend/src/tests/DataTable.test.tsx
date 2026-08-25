import { Theme } from '@radix-ui/themes';
import { fireEvent, render, screen } from '@testing-library/react';
import type { ColumnDef } from '@tanstack/react-table';
import { describe, expect, it } from 'vitest';
import { DataTable } from '../components/ui/DataTable';
import { StateBadge } from '../components/ui/StateBadge';
import { I18nProvider } from '../i18n';
import { formatCompactNumber, formatPercent } from '../lib/format';
import { renderApp } from './testUtils';

interface Row { name: string; state: string }
const columns: ColumnDef<Row, unknown>[] = [
  { accessorKey: 'name', header: 'Name' },
  { accessorKey: 'state', header: 'State', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
];

interface NumericRow { name: string; amount: number; weight: number }
const numericColumns: ColumnDef<NumericRow, unknown>[] = [
  { accessorKey: 'name', header: 'Name' },
  { accessorKey: 'amount', header: 'Amount', meta: { searchFormat: 'compact' }, cell: ({ getValue }) => <span>{formatCompactNumber(getValue() as number)}</span> },
  { accessorKey: 'weight', header: 'Weight', meta: { searchFormat: 'percent', searchDecimals: 1 }, cell: ({ getValue }) => <span>{formatPercent(getValue() as number)}</span> },
];

interface CountRow { name: string; count: number }
const countColumns: ColumnDef<CountRow, unknown>[] = [
  { accessorKey: 'name', header: 'Name' },
  { accessorKey: 'count', header: 'Count', cell: ({ getValue }) => <span className="qz-number">{String(getValue())}</span> },
];

interface CapabilityRow { name: string; capabilities: string[] }
const capabilityColumns: ColumnDef<CapabilityRow, unknown>[] = [
  { accessorKey: 'name', header: 'Name' },
  { id: 'capabilities', header: 'Capabilities', cell: ({ row }) => row.original.capabilities.join(', ') },
];

describe('DataTable', () => {
  it('filters raw values using TanStack Table', () => {
    renderApp(<DataTable data={[{ name: 'Beta', state: 'ACTIVE' }, { name: 'Alpha', state: 'COOLING' }]} columns={columns} />);
    expect(screen.getByText('Beta')).toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText('Filter rows…'), { target: { value: 'Alpha' } });
    expect(screen.getByText('Alpha')).toBeInTheDocument();
    expect(screen.queryByText('Beta')).not.toBeInTheDocument();
  });

  it('filters by localized domain labels', () => {
    render(
      <I18nProvider initialLocale="zh-CN">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <DataTable data={[{ name: 'Beta', state: 'ACTIVE' }, { name: 'Alpha', state: 'COOLING' }]} columns={columns} />
        </Theme>
      </I18nProvider>,
    );
    expect(screen.getByText('活跃')).toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText('筛选行…'), { target: { value: '活跃' } });
    expect(screen.getByText('Beta')).toBeInTheDocument();
    expect(screen.queryByText('Alpha')).not.toBeInTheDocument();
  });

  it('filters numeric columns by the declared compact and percentage display formats', () => {
    const data = [
      { name: 'Large', amount: 1234, weight: 0.25 },
      { name: 'Small', amount: 12, weight: 0.05 },
    ];
    renderApp(<DataTable data={data} columns={numericColumns} />);
    const input = screen.getByPlaceholderText('Filter rows…');
    const compact = formatCompactNumber(1234);
    const percent = formatPercent(0.25);
    expect(screen.getByText(compact)).toBeInTheDocument();
    expect(screen.getByText(percent)).toBeInTheDocument();

    fireEvent.change(input, { target: { value: compact } });
    expect(screen.getByText('Large')).toBeInTheDocument();
    expect(screen.queryByText('Small')).not.toBeInTheDocument();

    fireEvent.change(input, { target: { value: percent } });
    expect(screen.getByText('Large')).toBeInTheDocument();
    expect(screen.queryByText('Small')).not.toBeInTheDocument();
  });

  it('formats direct qz-number cells with the active locale', () => {
    render(
      <I18nProvider initialLocale="ar">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <DataTable data={[{ name: 'Arabic count', count: 1234 }]} columns={countColumns} />
        </Theme>
      </I18nProvider>,
    );
    expect(screen.getByText(new Intl.NumberFormat('ar').format(1234))).toBeInTheDocument();
    expect(screen.queryByText('1234')).not.toBeInTheDocument();
  });

  it('does not generate percentage aliases for ordinary numeric columns', () => {
    renderApp(<DataTable data={[{ name: 'One row', count: 1 }]} columns={countColumns} />);
    fireEvent.change(screen.getByPlaceholderText('Filter rows…'), { target: { value: '100%' } });
    expect(screen.queryByText('One row')).not.toBeInTheDocument();
  });

  it('indexes property-backed id columns without an explicit accessor', () => {
    render(
      <I18nProvider initialLocale="zh-CN">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <DataTable
            data={[
              { name: 'Importer', capabilities: ['HISTORICAL_IMPORT'] },
              { name: 'Live feed', capabilities: ['LIVE_DATA'] },
            ]}
            columns={capabilityColumns}
          />
        </Theme>
      </I18nProvider>,
    );
    fireEvent.change(screen.getByPlaceholderText('筛选行…'), { target: { value: '历史导入' } });
    expect(screen.getByText('Importer')).toBeInTheDocument();
    expect(screen.queryByText('Live feed')).not.toBeInTheDocument();
  });
});
