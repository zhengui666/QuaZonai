import { Button, Table } from 'antd';
import { ArrowUpRightIcon, DatabaseIcon } from '@phosphor-icons/react';
import type { DatasetView } from '../../../packages/contracts/src/index.ts';
import { useResource } from './data-hooks';
import { navigate } from './router';
import { Empty, ErrorNotice, Loading, Status, Timestamp } from './ui';
export default function Data() {
  const query = useResource<DatasetView[]>('/datasets');
  return <div className="index-page"><header className="page-heading"><h1>数据</h1>{query.data && <span className="quiet">{query.data.length} 个已登记数据集</span>}</header><ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />
    {query.isPending ? <Loading /> : !query.data?.length ? !query.error && <Empty title="尚无已登记数据集" action={<a className="text-link" href="https://github.com/zhengui666/QuaZonai/blob/dev/apps/research/README.md" target="_blank" rel="noreferrer">配置数据 <ArrowUpRightIcon size={14} /></a>} /> : <Table<DatasetView> rowKey="id" pagination={false} scroll={{ x: 850 }} dataSource={query.data} columns={[
      { title: '数据集', width: 240, render: (_value, row) => <div className="dataset-name"><span className="dataset-symbol"><DatabaseIcon size={19} /></span><div><strong>{row.name}</strong><code>{row.id}</code></div></div> },
      { title: '来源', dataIndex: 'source' }, { title: '资产', dataIndex: 'assetClass' }, { title: '覆盖区间', width: 210, render: (_value, row) => <span className="coverage"><Timestamp value={row.start} /><span>至</span><Timestamp value={row.end} /></span> }, { title: '币种', render: (_value, row) => <code>{row.currency}</code> },
      { title: '状态', render: (_value, row) => <Status plain value={row.ready ? 'ready' : 'not_ready'} label={row.ready ? '已就绪' : '未就绪'} /> },
      { title: '操作', width: 130, render: (_value, row) => row.ready ? <Button onClick={() => navigate(`/research/new?dataset=${encodeURIComponent(row.id)}`)}>用于研究 <ArrowUpRightIcon size={13} /></Button> : <span className="quiet">请检查数据配置</span> },
    ]} />}
  </div>;
}
