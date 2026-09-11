import { Alert, Button, Select, Space, Typography } from 'antd';
import { useInfiniteQuery } from '@tanstack/react-query';
import { useMemo } from 'react';
import { ErrorNotice, useOnline } from './ui';

export type ResourceOption = { value: string; label: string; disabled?: boolean };
type Page = { items: ResourceOption[]; next_cursor: string | null | undefined };

/** Paged business-reference selection. It never creates a reference from a label. */
export function ResourceSelect({ value, onChange, label, queryKey, load, disabled = false, allowClear = false }: {
  value?: string | null;
  onChange?: (value: string | undefined) => void;
  label: string;
  queryKey: readonly unknown[];
  load: (cursor: string | undefined, signal: AbortSignal) => Promise<Page>;
  disabled?: boolean;
  allowClear?: boolean;
}) {
  const online = useOnline();
  const query = useInfiniteQuery({
    queryKey, enabled: !disabled, initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam, signal }) => load(pageParam, signal),
    getNextPageParam: last => last.next_cursor ?? undefined,
  });
  const options = useMemo(() => {
    const unique = new Map<string, ResourceOption>();
    for (const page of query.data?.pages ?? []) for (const option of page.items) unique.set(option.value, option);
    return [...unique.values()];
  }, [query.data]);
  const missing = value !== undefined && value !== null && !options.some(option => option.value === value);
  return <Space orientation="vertical" size="small" className="full-width resource-select">
    <Select aria-label={label} value={value ?? undefined} onChange={onChange} allowClear={allowClear}
      className="full-width" getPopupContainer={trigger => trigger.parentElement ?? document.body}
      disabled={disabled || !online || query.isError} loading={query.isPending || query.isFetching}
      showSearch={{ optionFilterProp: 'label' }} options={options} placeholder={label}
      notFoundContent={query.isPending ? '正在载入' : query.isError ? '读取失败，不视为空列表' : '当前已载入的记录中没有匹配项'} />
    {!disabled && <Space wrap>
      <Typography.Text type="secondary">已载入 {options.length} 条；搜索只匹配已载入记录。</Typography.Text>
      {query.hasNextPage && <Button size="small" disabled={!online || query.isFetching} onClick={() => { void query.fetchNextPage(); }}>载入更多选项</Button>}
      <Button size="small" disabled={!online || query.isFetching} onClick={() => { void query.refetch(); }}>刷新选项</Button>
    </Space>}
    {missing && <Alert type="warning" showIcon title="当前引用尚未出现在已载入记录中。请载入更多或刷新后核对，不会擅自替换它。" />}
    <ErrorNotice error={query.error} />
  </Space>;
}
