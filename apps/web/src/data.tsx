import { useTranslation } from 'react-i18next';
import { useState } from 'react';
import { Button, Drawer, Table } from 'antd';
import { ArrowUpRightIcon, DatabaseIcon } from '@phosphor-icons/react';
import type { DatasetView } from '../../../packages/contracts/src/index.ts';
import { useResource } from './data-hooks';
import { navigate } from './router';
import { Copy, Empty, ErrorNotice, Facts, Loading, Status, Timestamp } from './ui';
import DatasetRegistration from './dataset-registration';
import DatasetSelection from './dataset-selection';
import { NativeInputFacts } from './native-input';
export default function Data() {
 const { t, i18n } = useTranslation('settings');

  const query = useResource<DatasetView[]>('/datasets?view=summary'); const [selectedId, setSelectedId] = useState<string>(); const summary = query.data?.find(item => item.id === selectedId);
  const detail = useResource<DatasetView>(`/datasets/${encodeURIComponent(selectedId ?? '')}`, { enabled: !!summary }); const selected = detail.data;
  const study = (id: string) => { setSelectedId(undefined); navigate(`/research/new?dataset=${encodeURIComponent(id)}&from=data`); };
  return <div className="settings-page"><header className="page-heading"><h2>{t("Data")}</h2>{query.data && <span className="quiet">{t("Registered datasets: {{countText}}", { countText: new Intl.NumberFormat(i18n.language).format(query.data.length) })}</span>}</header><ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />
    {query.isPending ? <Loading /> : !query.data?.length ? !query.error && <Empty title={t("No registered datasets yet")} /> : <Table<DatasetView> rowKey="id" pagination={false} scroll={{ x: 850 }} dataSource={query.data} columns={[
      { title: t("Dataset"), width: 240, render: (_value, row) => <div className="dataset-name"><span className="dataset-symbol"><DatabaseIcon size={19} /></span><div><Button type="text" className="dataset-open" onClick={() => setSelectedId(row.id)}>{row.name}</Button><code>{row.id}</code></div></div> },
      { title: t("Native input"), width: 115, render: (_value, row) => row.nativeDataTypes?.length ? row.nativeDataTypes.join(' / ') : row.nativeModality === 'TRADE_TICK' ? 'TradeTick' : row.nativeModality === 'BAR' ? 'BAR' : t("Not specified") },
      { title: t("Source"), dataIndex: 'source' }, { title: t("Asset class"), dataIndex: 'assetClass' }, { title: t("Coverage"), width: 210, render: (_value, row) => <span className="coverage"><Timestamp value={row.start} /><span>{t(" to ")}</span><Timestamp value={row.end} /></span> }, { title: t("Currency"), render: (_value, row) => <code>{row.currency}</code> },
      { title: t("Status"), render: (_value, row) => <Status plain value={row.ready ? 'ready' : 'not_ready'} label={row.ready ? t("Ready") : t("Not ready")} /> },
      { title: t("Actions"), width: 130, render: (_value, row) => row.ready ? <Button onClick={() => study(row.id)}>{t("Use for research")}<ArrowUpRightIcon size={13} /></Button> : <span className="quiet">{t("Check the data configuration")}</span> },
    ]} />}
    <DatasetSelection />
    <DatasetRegistration />
    <Drawer className="dataset-drawer" getContainer={false} title={summary?.name} open={!!summary} onClose={() => setSelectedId(undefined)} width="min(520px, 100vw)" footer={selected?.ready && <Button type="primary" onClick={() => study(selected.id)}>{t("Use for research")}<ArrowUpRightIcon size={13} /></Button>}>
      <ErrorNotice error={detail.error} retry={() => { void detail.refetch(); }} />{detail.isPending && summary && <Loading />}
      {selected && <><Facts items={[[t("Dataset ID"), <><code>{selected.id}</code><Copy value={selected.id} label={t("Copy dataset ID")} /></>], [t("Research admission"), <Status value={selected.ready ? 'ready' : 'not_ready'} label={selected.ready ? t("Prepared") : t("Preparation needed")} />], [t("Source"), selected.source], [t("Asset class"), selected.assetClass], [t("Currency"), selected.currency], [t("Start time"), <Timestamp value={selected.start} />], [t("End time"), <Timestamp value={selected.end} />]]} /><NativeInputFacts modality={selected.nativeModality} settings={selected.researchSettings} preparation={selected.preparation} coverage={selected.inputCoverage} /></>}
    </Drawer>
  </div>;
}
