import { Theme } from '@radix-ui/themes';
import { fireEvent, render, screen } from '@testing-library/react';
import type { ColumnDef } from '@tanstack/react-table';
import { describe, expect, it } from 'vitest';
import { DataTable } from '../components/ui/DataTable';
import { StateBadge } from '../components/ui/StateBadge';
import { I18nProvider } from '../i18n';
import { renderApp } from './testUtils';

interface Row { name: string; state: string }
const columns: ColumnDef<Row, unknown>[] = [
  { accessorKey: 'name', header: 'Name' },
  { accessorKey: 'state', header: 'State', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
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
});
