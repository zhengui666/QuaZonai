import { Theme } from '@radix-ui/themes';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { ColumnDef } from '@tanstack/react-table';
import { describe, expect, it } from 'vitest';
import { DataTable } from '../components/ui/DataTable';
import { StateBadge } from '../components/ui/StateBadge';
import { I18nProvider, useI18n } from '../i18n';
import { formatCapitalAmount, formatCompactNumber, formatNumber, formatPercent, humanize } from '../lib/format';
import { renderApp } from './testUtils';

interface Row { name: string; state: string }
const columns: ColumnDef<Row, unknown>[] = [
  { accessorKey: 'name', header: 'Name' },
  { accessorKey: 'state', header: 'State', meta: { localizedSort: true }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
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
  { accessorKey: 'count', header: 'Count', cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as number)}</span> },
];

interface StringNumericRow { name: string; deployable: string }
const stringNumericColumns: ColumnDef<StringNumericRow, unknown>[] = [
  { accessorKey: 'name', header: 'Name' },
  { accessorKey: 'deployable', header: 'Deployable', cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as string)}</span> },
];

interface ExactStringNumericRow { name: string; deployable: string }
const exactStringNumericColumns: ColumnDef<ExactStringNumericRow, unknown>[] = [
  { accessorKey: 'name', header: 'Name' },
  { accessorKey: 'deployable', header: 'Deployable', cell: ({ getValue }) => <span className="qz-number">{formatCapitalAmount(getValue() as string)}</span> },
];

interface CapabilityRow { name: string; capabilities: string[] }
const capabilityColumns: ColumnDef<CapabilityRow, unknown>[] = [
  { id: 'name', header: 'Name', cell: ({ row }) => row.original.name },
  { id: 'capabilities', header: 'Capabilities', cell: ({ row }) => row.original.capabilities.join(', ') },
];

interface RuntimeRow { event: string }
const runtimeColumns: ColumnDef<RuntimeRow, unknown>[] = [
  { accessorKey: 'event', header: 'Event', meta: { localizedSort: true }, cell: ({ getValue }) => <span>{humanize(String(getValue()))}</span> },
];

interface AlphaRow { alpha: string }
const alphaColumns: ColumnDef<AlphaRow, unknown>[] = [
  { accessorKey: 'alpha', header: 'Alpha', meta: { messageKey: 'alpha.name' } },
];

function LocaleChangeButton() {
  const { setLocale } = useI18n();
  return <button type="button" onClick={() => setLocale('zh-CN')}>Switch locale</button>;
}

interface OperatorLabelRow { label: string }
const operatorLabelColumns: ColumnDef<OperatorLabelRow, unknown>[] = [
  { accessorKey: 'label', header: 'Source', cell: ({ getValue }) => <span>{String(getValue())}</span> },
];

describe('DataTable', () => {
  it('filters raw values using TanStack Table', () => {
    renderApp(<DataTable data={[{ name: 'Beta', state: 'ACTIVE' }, { name: 'Alpha', state: 'COOLING' }]} columns={columns} />);
    expect(screen.getByText('Beta')).toBeInTheDocument();
    expect(screen.getByText('Beta').closest('td')).toHaveAttribute('dir', 'auto');
    const filterInput = screen.getByRole('textbox', { name: 'Filter rows…' });
    fireEvent.change(filterInput, { target: { value: 'Alpha' } });
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

  it('clears localized filters when the locale changes', async () => {
    render(
      <I18nProvider initialLocale="es">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <LocaleChangeButton />
          <DataTable data={[{ name: 'Beta', state: 'ACTIVE' }, { name: 'Alpha', state: 'COOLING' }]} columns={columns} />
        </Theme>
      </I18nProvider>,
    );
    fireEvent.change(screen.getByPlaceholderText('Filtrar filas…'), { target: { value: 'Activo' } });
    expect(screen.getByText('Beta')).toBeInTheDocument();
    expect(screen.queryByText('Alpha')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Switch locale' }));
    await waitFor(() => {
      expect(screen.getByPlaceholderText('筛选行…')).toHaveValue('');
      expect(screen.getByText('Alpha')).toBeInTheDocument();
      expect(screen.getByText('Beta')).toBeInTheDocument();
    });
  });

  it('clears localized sorting when the locale changes', async () => {
    render(
      <I18nProvider initialLocale="es">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <LocaleChangeButton />
          <DataTable data={[{ name: 'Blocked row', state: 'BLOCKED' }, { name: 'Active row', state: 'ACTIVE' }]} columns={columns} />
        </Theme>
      </I18nProvider>,
    );
    const stateSort = screen.getAllByRole('button').find((button) => /^(State|Estado)/.test(button.textContent ?? ''));
    expect(stateSort).toBeDefined();
    fireEvent.click(stateSort!);
    expect(screen.getAllByText(/^(Activo|Bloqueado)$/).map((element) => element.textContent)).toEqual(['Activo', 'Bloqueado']);

    fireEvent.click(screen.getByRole('button', { name: 'Switch locale' }));
    await waitFor(() => {
      expect(screen.getAllByText(/^(活跃|阻塞)$/).map((element) => element.textContent)).toEqual(['阻塞', '活跃']);
    });
  });

  it('sorts translated state labels in their displayed locale', () => {
    render(
      <I18nProvider initialLocale="es">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <DataTable
            data={[
              { name: 'Active row', state: 'ACTIVE' },
              { name: 'Blocked row', state: 'BLOCKED' },
              { name: 'Available row', state: 'AVAILABLE' },
            ]}
            columns={columns}
          />
        </Theme>
      </I18nProvider>,
    );
    const stateSort = screen.getAllByRole('button').find((button) => /^(State|Estado)/.test(button.textContent ?? ''));
    expect(stateSort).toBeDefined();
    fireEvent.click(stateSort!);
    expect(screen.getAllByText(/^(Activo|Bloqueado|Disponible)$/).map((element) => element.textContent)).toEqual([
      'Activo',
      'Bloqueado',
      'Disponible',
    ]);
  });

  it('sorts raw operator labels by the text that is actually displayed', () => {
    render(
      <I18nProvider initialLocale="es">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <DataTable data={[{ label: 'BLOCKED' }, { label: 'AVAILABLE' }]} columns={operatorLabelColumns} />
        </Theme>
      </I18nProvider>,
    );
    const sourceSort = screen.getAllByRole('button').find((button) => /^(Source|Fuente)/.test(button.textContent ?? ''));
    expect(sourceSort).toBeDefined();
    fireEvent.click(sourceSort!);
    expect(screen.getAllByText(/^(AVAILABLE|BLOCKED)$/).map((element) => element.textContent)).toEqual(['AVAILABLE', 'BLOCKED']);
  });

  it('does not filter raw operator labels by hidden localized aliases', () => {
    render(
      <I18nProvider initialLocale="es">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <DataTable data={[{ label: 'AVAILABLE' }, { label: 'BLOCKED' }]} columns={operatorLabelColumns} />
        </Theme>
      </I18nProvider>,
    );

    fireEvent.change(screen.getByPlaceholderText('Filtrar filas…'), { target: { value: 'Disponible' } });
    expect(screen.queryByText('AVAILABLE')).not.toBeInTheDocument();
    expect(screen.queryByText('BLOCKED')).not.toBeInTheDocument();
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

  it('renders qz-number cells with the active locale at the cell boundary', () => {
    render(
      <I18nProvider initialLocale="ar">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <DataTable data={[{ name: 'Arabic count', count: 1234 }]} columns={countColumns} />
        </Theme>
      </I18nProvider>,
    );
    expect(screen.getByText(new Intl.NumberFormat('ar').format(1234))).toBeInTheDocument();
    expect(screen.queryByText('1234')).not.toBeInTheDocument();
    expect(screen.getByRole('textbox')).toHaveAttribute('dir', 'auto');
  });

  it('indexes localized display aliases for numeric strings', () => {
    render(
      <I18nProvider initialLocale="es">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <DataTable data={[{ name: 'String capital', deployable: '123456' }]} columns={stringNumericColumns} />
        </Theme>
      </I18nProvider>,
    );
    const localized = new Intl.NumberFormat('es').format(123456);
    expect(screen.getByText(localized)).toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText('Filtrar filas…'), { target: { value: localized } });
    expect(screen.getByText('String capital')).toBeInTheDocument();
  });

  it('sorts arbitrary-precision decimal strings exactly and indexes their exact localized display', () => {
    const larger = '9007199254740993';
    const smaller = '9007199254740992';
    render(
      <I18nProvider initialLocale="es">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <DataTable
            data={[{ name: 'Larger capital', deployable: larger }, { name: 'Smaller capital', deployable: smaller }]}
            columns={exactStringNumericColumns}
          />
        </Theme>
      </I18nProvider>,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Deployable' }));
    expect(screen.getAllByText(/^(Larger|Smaller) capital$/).map((element) => element.textContent)).toEqual([
      'Smaller capital',
      'Larger capital',
    ]);

    const localizedLarger = formatCapitalAmount(larger, 'es');
    expect(screen.getByText(localizedLarger)).toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText('Filtrar filas…'), { target: { value: localizedLarger } });
    expect(screen.getByText('Larger capital')).toBeInTheDocument();
    expect(screen.queryByText('Smaller capital')).not.toBeInTheDocument();
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
  it('indexes localized runtime labels', () => {
    render(
      <I18nProvider initialLocale="es">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <DataTable data={[{ event: 'MISSION_STARTED' }]} columns={runtimeColumns} />
        </Theme>
      </I18nProvider>,
    );
    expect(screen.getByText('Misión iniciada')).toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText('Filtrar filas…'), { target: { value: 'Misión iniciada' } });
    expect(screen.getByText('Misión iniciada')).toBeInTheDocument();
  });


  it('uses semantic keys for headers whose source text has multiple presentation meanings', () => {
    render(
      <I18nProvider initialLocale="ar">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <DataTable data={[{ alpha: 'Momentum' }]} columns={alphaColumns} />
        </Theme>
      </I18nProvider>,
    );
    expect(screen.getByText('ألفا')).toBeInTheDocument();
  });
  it('sorts numeric-string cells by their numeric value', () => {
    renderApp(
      <DataTable
        data={[{ name: '2.2 row', deployable: '2.2' }, { name: '2.10 row', deployable: '2.10' }]}
        columns={stringNumericColumns}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Deployable' }));
    expect(screen.getAllByText(/2\.(2|10) row/).map((element) => element.textContent)).toEqual(['2.10 row', '2.2 row']);
  });
});
