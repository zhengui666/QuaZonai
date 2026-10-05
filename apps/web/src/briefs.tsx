import { Button, Drawer, Space, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api, dataOf } from './api';
import type { Schema } from './api';
import { BriefExecutionContext } from './cycles';
import { Pager, QueryPanel, ResourceFacts, StateTag } from './ui';

type Brief = Schema['BriefView'];
export function Briefs({ projectId, currentBriefId }: { projectId: string; projectState?: Schema['ProjectState']; currentBriefId?: string | null }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<Brief>();
  const cursor = history.at(-1);
  const query = useQuery({ queryKey: ['briefs', projectId, cursor], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{id}/briefs', { params: { path: { id: projectId }, query: { cursor, limit: 25 } }, signal })) });
  return <Space orientation="vertical" className="full-width" size="middle">
    <div className="section-toolbar"><div><h2>研究 Brief</h2><p>查看假设与边界，保留每个版本的研究依据。</p></div><Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新 Brief</Button></div>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      {query.data?.items.length === 0 ? <div className="research-empty"><h3>尚无 Brief</h3><p>这里展示外部 Agent 通过 CLI / Skill 保存的 Brief、数据边界与预算。</p></div> : <div className="brief-list">{query.data?.items.map(item => <article className="brief-record" key={item.id}>
        <div className="brief-record-main"><div className="record-label"><span>BRIEF · 版本 {item.version}</span><StateTag value={item.state} />{item.id === currentBriefId && <span className="current-record">当前版本</span>}</div>
          <h3>{item.content.hypothesis}</h3><p className="brief-rationale">{item.content.economic_rationale}</p>
          <div className="record-meta"><span>预测单位：{item.content.target_kind}</span><span>基础币种：{item.content.base_currency}</span><span>数据绑定：{item.bindings.length}</span></div>
          <details className="record-details"><summary>版本记录与修订</summary><ResourceFacts id={item.id} revision={item.revision} updated={item.updated_at} /></details></div>
        <div className="record-actions"><p>{item.state === 'DRAFT' ? '尚未冻结的原研究草稿' : '查看原冻结版本与执行上下文'}</p>
          <Button disabled={query.isError} onClick={() => setSelected(item)}>{item.state === 'DRAFT' ? '查看草稿' : '查看冻结版本'}</Button></div>
      </article>)}</div>}
      {(history.length > 1 || query.data?.next_cursor) && <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />}
    </QueryPanel>
    {selected && <BriefDetail brief={selected} close={() => setSelected(undefined)} />}
  </Space>;
}

export function BriefDetail({ brief, close }: { brief: Brief; close: () => void }) {
  return <Drawer title={`Brief · 版本 ${brief.version}`} open width={840} onClose={close}>
    <Space orientation="vertical" className="full-width" size="middle">
      <ResourceFacts id={brief.id} revision={brief.revision} updated={brief.updated_at} />
      <StateTag value={brief.state} />
      <Typography.Title level={3}>服务器保存的完整 Brief</Typography.Title>
      <pre tabIndex={0} aria-label="原完整 Brief" className="break-word" style={{ whiteSpace: 'pre-wrap' }}>{JSON.stringify(brief, null, 2)}</pre>
      {brief.state === 'FROZEN' && <BriefExecutionContext briefId={brief.id} />}
    </Space>
  </Drawer>;
}
