import { useEffect, useRef, useState } from 'react';
import { Button, Input, Select, Tag, type TableColumnType, type TableProps } from 'antd';
import { MagnifyingGlassIcon, SlidersHorizontalIcon } from '@phosphor-icons/react';
import { useQuery } from '@tanstack/react-query';
import type { ListPage } from '../../../packages/contracts/src/lists.ts';
import { api } from './api';
import { changeListParameters, filterEntries, listParameters, listQuery } from './list-values';
import { href, navigate, restoreListNavigation, type Route } from './router';

export function listPathFor(route: Route) {
  const params = listParameters(route.params);
  return `/${route.page}${params.size ? `?${params}` : ''}`;
}

export function useListWorkspace<T>(route: Route, collection: string, defaultSort?: string) {
  const path = collection.startsWith('/') ? collection : `/${collection}`;
  const params = route.params;
  const serialized = listQuery(params, defaultSort);
  const query = useQuery({ queryKey: [path, 'list', serialized], queryFn: ({ signal }) => api<ListPage<T>>(`${path}/list?${serialized}`, { signal }),
    enabled: !route.id && !route.creating, refetchInterval: 5000, refetchIntervalInBackground: false });
  const update = (patch: Record<string, string | number | undefined>, resetPage = true) => {
    const next = changeListParameters(params, patch, resetPage);
    navigate(`/${route.page}${next.size ? `?${next}` : ''}`, true);
  };
  const entries = filterEntries(params);
  const clear = () => update(Object.fromEntries([...entries.map(([key]) => [key, undefined]), ['q', undefined]]));
  const detailPath = (id: string | number, extra: Record<string, string> = {}) => {
    const next = listParameters(params);
    for (const [key, value] of Object.entries(extra)) next.set(key, value);
    return `/${route.page}/${encodeURIComponent(String(id))}${next.size ? `?${next}` : ''}`;
  };
  useEffect(() => { if (query.data) restoreListNavigation(route, true); }, [query.data, route.page, params.toString()]);
  useEffect(() => { if (query.data && query.data.page !== Number(params.get('page') ?? 1)) update({ page: query.data.page }, false); }, [query.data, params.toString()]);
  const sortColumn = (key: string): Pick<TableColumnType<T>, 'key' | 'sorter' | 'sortOrder' | 'sortDirections'> => ({ key, sorter: true,
    sortDirections: ['name', 'title', 'datasetName'].includes(key) ? ['ascend', 'descend'] : ['descend', 'ascend'],
    sortOrder: (params.get('sort') ?? defaultSort) === key ? params.get('order') === 'asc' ? 'ascend' : 'descend' : null });
  const tableProps: TableProps<T> = {
    dataSource: query.data?.items ?? [], loading: query.isPending, className: 'workspace-table',
    pagination: { current: query.data?.page ?? Number(params.get('page') ?? 1), pageSize: query.data?.pageSize ?? Number(params.get('pageSize') ?? 20),
      total: query.data?.total ?? 0, showSizeChanger: true, pageSizeOptions: [20, 50, 100], hideOnSinglePage: false,
      showTotal: total => `共 ${total} 条` },
    onChange: (pagination, _filters, sorter, extra) => {
      if (extra.action === 'sort') {
        const selected = Array.isArray(sorter) ? sorter[0] : sorter;
        update({ sort: selected?.order ? String(selected.columnKey) : undefined, order: selected?.order === 'ascend' ? 'asc' : selected?.order ? 'desc' : undefined });
      } else if (extra.action === 'paginate') update({ page: pagination.pageSize !== (query.data?.pageSize ?? 20) ? 1 : pagination.current ?? 1, pageSize: pagination.pageSize ?? 20 }, false);
    },
  };
  return { query, data: query.data, items: query.data?.items ?? [], params, value: (key: string) => params.get(key) ?? '', update, clear,
    activeFilters: entries, hasFilters: !!params.get('q') || entries.length > 0, listPath: listPathFor(route), detailPath,
    linkProps: (id: string | number, extra: Record<string, string> = {}) => ({ href: href(detailPath(id, extra)), 'data-record-id': String(id) }),
    sortColumn, tableProps };
}

export interface ListFilter {
  key: string; label: string; options?: { value: string; label: string }[]; advanced?: boolean;
  kind?: 'select' | 'number' | 'date'; disabled?: boolean; min?: number; max?: number; step?: number;
}
type ListWorkspace<T> = ReturnType<typeof useListWorkspace<T>>;
export function ListTools<T>({ list, filters = [], searchLabel = '搜索记录', placeholder = '搜索名称、来源或原始编号' }: {
  list: ListWorkspace<T>; filters?: ListFilter[]; searchLabel?: string; placeholder?: string;
}) {
  const [search, setSearch] = useState(list.value('q'));
  const [advanced, setAdvanced] = useState(false);
  const composing = useRef(false); const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const latest = useRef(list); latest.current = list;
  const externalSearch = list.value('q');
  useEffect(() => { setSearch(externalSearch); clearTimeout(timer.current); }, [externalSearch]);
  useEffect(() => () => clearTimeout(timer.current), []);
  const commitSearch = (value: string) => {
    clearTimeout(timer.current);
    if (!composing.current) timer.current = setTimeout(() => { latest.current.update({ q: value.trim() || undefined }); }, 300);
  };
  const options = (filter: ListFilter) => filter.options ?? list.data?.facets[filter.key] ?? [];
  const control = (filter: ListFilter) => <div className="workspace-filter" key={filter.key}>
    <label htmlFor={`list-filter-${filter.key}`}>{filter.label}</label>
    {filter.kind === 'number' || filter.kind === 'date' ? <Input key={`${filter.key}:${list.value(filter.key)}`} id={`list-filter-${filter.key}`} aria-label={filter.label}
      type={filter.kind === 'date' ? 'date' : 'number'} defaultValue={list.value(filter.key)} disabled={filter.disabled}
      min={filter.min} max={filter.max} step={filter.step ?? 'any'} inputMode={filter.kind === 'number' ? 'decimal' : undefined}
      onBlur={event => { const value = event.target.value; if (value !== list.value(filter.key)) list.update({ [filter.key]: value || undefined }); }}
      onPressEnter={event => { const value = event.currentTarget.value; if (value !== list.value(filter.key)) list.update({ [filter.key]: value || undefined }); }} />
      : <Select id={`list-filter-${filter.key}`} aria-label={filter.label} mode="multiple" allowClear showSearch optionFilterProp="label" placeholder={`全部${filter.label}`}
        disabled={filter.disabled} value={list.value(filter.key).split(',').filter(Boolean)} options={options(filter)} maxTagCount="responsive"
        onChange={(values: string[]) => list.update({ [filter.key]: values.join(',') || undefined })} />}
  </div>;
  const active = filters.flatMap(filter => list.value(filter.key).split(',').filter(Boolean).map(value => ({ filter, value, label: options(filter).find(option => option.value === value)?.label ?? value })));
  const unknown = list.activeFilters.filter(([key]) => !filters.some(filter => filter.key === key));
  return <div className="workspace-tools">
    <div className="workspace-primary-tools"><Input className="workspace-search" aria-label={searchLabel} prefix={<MagnifyingGlassIcon size={16} />} value={search} allowClear placeholder={placeholder}
      onChange={event => { setSearch(event.target.value); commitSearch(event.target.value); }}
      onCompositionStart={() => { composing.current = true; clearTimeout(timer.current); }}
      onCompositionEnd={event => { composing.current = false; commitSearch(event.currentTarget.value); }}
      onPressEnter={() => { if (!composing.current) { clearTimeout(timer.current); list.update({ q: search.trim() || undefined }); } }} />
      {filters.filter(filter => !filter.advanced).map(control)}
      {filters.some(filter => filter.advanced) && <Button icon={<SlidersHorizontalIcon size={16} />} aria-expanded={advanced} onClick={() => setAdvanced(value => !value)}>更多筛选</Button>}
    </div>
    {advanced && <div className="workspace-advanced-tools">{filters.filter(filter => filter.advanced).map(control)}</div>}
    <div className="workspace-list-scope"><span role="status">{list.data ? `匹配 ${list.data.total} 条` : list.query.isPending ? '正在读取记录' : '记录暂不可用'}</span>
      {list.hasFilters && <Button type="text" size="small" onClick={() => { setSearch(''); clearTimeout(timer.current); list.clear(); }}>清除全部</Button>}</div>
    {(active.length > 0 || unknown.length > 0 || list.value('q')) && <div className="workspace-filter-tags" aria-label="当前筛选">
      {list.value('q') && <Tag closable onClose={() => { setSearch(''); clearTimeout(timer.current); list.update({ q: undefined }); }}>搜索：{list.value('q')}</Tag>}
      {active.map(({ filter, value, label }) => <Tag key={`${filter.key}:${value}`} closable onClose={() => list.update({ [filter.key]: list.value(filter.key).split(',').filter(item => item !== value).join(',') || undefined })}>{filter.label}：{label}</Tag>)}
      {unknown.map(([key, value]) => <Tag key={key} closable onClose={() => list.update({ [key]: undefined })}>{value}</Tag>)}
    </div>}
  </div>;
}
