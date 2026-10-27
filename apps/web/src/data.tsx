import { useState } from 'react';
import { Button, Drawer, Table } from 'antd';
import { ArrowUpRightIcon, DatabaseIcon } from '@phosphor-icons/react';
import type { DatasetView } from '../../../packages/contracts/src/index.ts';
import { useResource } from './data-hooks';
import { href, navigate } from './router';
import { Copy, Empty, ErrorNotice, Facts, Loading, Status, Timestamp } from './ui';
import { NativeInputFacts } from './native-input';
export default function Data() {
  const query = useResource<DatasetView[]>('/datasets'); const [selectedId, setSelectedId] = useState<string>(); const selected = query.data?.find(item => item.id === selectedId);
  const study = (id: string) => { setSelectedId(undefined); navigate(`/research/new?dataset=${encodeURIComponent(id)}&from=data`); };
  return <div className="settings-page"><header className="page-heading"><h2>数据</h2>{query.data && <span className="quiet">{query.data.length} 个已登记数据集</span>}</header><ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />
    {query.isPending ? <Loading /> : !query.data?.length ? !query.error && <Empty title="尚无已登记数据集" action={<a className="text-link" href={href('/settings?section=data-plugins')}>配置数据 <ArrowUpRightIcon size={14} /></a>} /> : <Table<DatasetView> rowKey="id" pagination={false} scroll={{ x: 850 }} dataSource={query.data} columns={[
      { title: '数据集', width: 240, render: (_value, row) => <div className="dataset-name"><span className="dataset-symbol"><DatabaseIcon size={19} /></span><div><Button type="text" className="dataset-open" onClick={() => setSelectedId(row.id)}>{row.name}</Button><code>{row.id}</code></div></div> },
      { title: '原生输入', width: 115, render: (_value, row) => row.nativeModality === 'TRADE_TICK' ? 'TradeTick' : row.nativeModality === 'BAR' ? 'BAR' : '未登记' },
      { title: '来源', dataIndex: 'source' }, { title: '资产', dataIndex: 'assetClass' }, { title: '覆盖区间', width: 210, render: (_value, row) => <span className="coverage"><Timestamp value={row.start} /><span>至</span><Timestamp value={row.end} /></span> }, { title: '币种', render: (_value, row) => <code>{row.currency}</code> },
      { title: '状态', render: (_value, row) => <Status plain value={row.ready ? 'ready' : 'not_ready'} label={row.ready ? '已就绪' : '未就绪'} /> },
      { title: '操作', width: 130, render: (_value, row) => row.ready ? <Button onClick={() => study(row.id)}>用于研究 <ArrowUpRightIcon size={13} /></Button> : <span className="quiet">请检查数据配置</span> },
    ]} />}
    <Drawer className="dataset-drawer" getContainer={false} title={selected?.name} open={!!selected} onClose={() => setSelectedId(undefined)} width="min(520px, 100vw)" footer={selected?.ready && <Button type="primary" onClick={() => study(selected.id)}>用于研究 <ArrowUpRightIcon size={13} /></Button>}>
      {selected && <><Facts items={[["数据集身份", <><code>{selected.id}</code><Copy value={selected.id} label="复制数据集身份" /></>], ["研究准入", <Status value={selected.ready ? 'ready' : 'not_ready'} label={selected.ready ? '已准备' : '待准备'} />], ["来源", selected.source], ["资产", selected.assetClass], ["币种", selected.currency], ["起始时间", <Timestamp value={selected.start} />], ["结束时间", <Timestamp value={selected.end} />]]} /><NativeInputFacts modality={selected.nativeModality} settings={selected.researchSettings} preparation={selected.preparation} coverage={selected.inputCoverage} /></>}
    </Drawer>
  </div>;
}
