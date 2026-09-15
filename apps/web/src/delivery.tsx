import { Alert, Button, Collapse, Descriptions, Drawer, Space, Table, Tabs, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { useContext, useState } from 'react';
import { api, dataOf, displayTime } from './api';
import type { Schema } from './api';
import { GuardContext, NoData, Pager, QueryPanel } from './ui';
import { ResourceSelect } from './resource-select';
import { ReleaseApprove } from './release-approve';
import { HandoffOffer } from './handoff-offer';
import { ApprovalRevoke } from './approval-revoke';
import { ReleaseDecision } from './release-decision';
import { AutomationPolicies } from './automation-policies';

export function Delivery() {
  const [project, setProject] = useState<string>();
  const { blocked } = useContext(GuardContext);
  return <Space orientation="vertical" size="large" className="full-width">
    <Typography.Title level={1}>交付</Typography.Title>
    <Alert showIcon type="info" title="目标包与交付授权分开" description="Release 保存不可变目标包。请从目标包详情查看审批、人工决定及原审批历史；登记 Offer 仍需独立审批，审批不代表下游领取或真实交易。" />
    <ResourceSelect label="选择交付所属项目" value={project} onChange={setProject} disabled={blocked} queryKey={['delivery-projects']} load={async (cursor, signal) => {
      const page = dataOf(await api.GET('/api/v2/projects', { params: { query: { cursor, limit: 50 } }, signal }));
      return { next_cursor: page.next_cursor, items: page.items.map(item => ({ value: item.id, label: `${item.name} · ${item.id}` })) };
    }} />
    {project ? <Tabs key={project} items={[{ key: 'releases', label: '目标包', children: <Releases project={project} /> }, { key: 'handoffs', label: '交付记录', children: <Handoffs project={project} /> }, { key: 'policies', label: '自动化政策', children: <AutomationPolicies project={project} /> }]} /> : <NoData text="请选择项目查看已冻结的目标包。" />}
  </Space>;
}

function Releases({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<string>();
  const query = useQuery({ queryKey: ['releases', project, history.at(-1)], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/projects/{id}/releases', { params: { path: { id: project }, query: { cursor: history.at(-1), limit: 25 } }, signal }));
    if (page.items.some(item => item.project_id !== project)) throw new Error('目标包记录不属于当前项目。');
    return page;
  } });
  return <Space orientation="vertical" className="full-width">
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新目标包</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['ReleaseViewV1']> rowKey="id" dataSource={query.data?.items} pagination={false} onHeaderRow={() => ({ tabIndex: 0 })} scroll={{ x: 800 }} locale={{ emptyText: <NoData text="尚无冻结目标包。可从组合候选的独立评估请求冻结，不会自动批准或交付。" /> }} columns={[
        { title: '目标包版本', key: 'id', render: (_, item) => <Button type="link" disabled={query.isError} onClick={() => setSelected(item.id)}>Release {item.id.slice(-8)}</Button> },
        { title: '候选', dataIndex: 'candidate_id' }, { title: '来源（非交付环境）', dataIndex: 'environment' },
        { title: '目标时点', dataIndex: 'asof', render: displayTime }, { title: '原有效期', dataIndex: 'valid_until', render: displayTime },
      ]} />
    </QueryPanel>
    <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    {selected && <ReleaseDetail key={selected} id={selected} project={project} close={() => setSelected(undefined)} />}
  </Space>;
}

export function ReleaseDetail({ id, project, close }: { id: string; project: string; close: () => void }) {
  const [approving, setApproving] = useState(false);
  const [deciding, setDeciding] = useState(false);
  const [offering, setOffering] = useState<Schema['ApprovalViewV1']>();
  const [revoking, setRevoking] = useState<Schema['ApprovalViewV1']>();
  const query = useQuery({ queryKey: ['release', project, id], queryFn: async ({ signal }) => {
    const item = dataOf(await api.GET('/api/v2/releases/{id}', { params: { path: { id } }, signal }));
    if (item.id !== id || item.project_id !== project) throw new Error('服务器返回了其他目标包版本。');
    return item;
  } });
  const item = query.data;
  const deliverableOrigin = item?.environment === 'REAL';
  return <Drawer title="原始目标包版本" open onClose={approving || offering || revoking || deciding ? undefined : close} closable={!approving && !offering && !revoking && !deciding} maskClosable={!approving && !offering && !revoking && !deciding} width={760}>
    <Alert showIcon type="info" title="历史有效期不是当前审批资格" description="读取不会延长期限或重判数据、Alpha 资格与下游兼容性。REAL 是包来源，不代表已批准 Live。" />
    <QueryPanel pending={query.isPending} error={query.error} stale={!!item} reload={() => { void query.refetch(); }}>
      {item && <Descriptions column={1} className="break-word" items={[
        { key: 'id', label: 'Release 编号', children: item.id }, { key: 'project', label: '项目', children: item.project_id },
        { key: 'candidate', label: '原候选', children: item.candidate_id }, { key: 'mandate', label: '原组合配置', children: item.mandate_id },
        { key: 'evaluation', label: '原独立评估', children: item.evaluation_id }, { key: 'artifact', label: '不可变 Package 产物', children: item.package_artifact_id },
        { key: 'schema', label: 'Package 协议版本', children: item.package_schema_version }, { key: 'market', label: '市场合同版本', children: item.market_capability_version },
        { key: 'origin', label: '来源', children: item.environment }, { key: 'asof', label: '目标时点', children: displayTime(item.asof) },
        { key: 'start', label: '原有效起点', children: displayTime(item.valid_from) }, { key: 'end', label: '原有效终点', children: displayTime(item.valid_until) },
        { key: 'created', label: '冻结于', children: displayTime(item.created_at) },
      ]} />}
    </QueryPanel>
    {item?.environment === 'DEMO' && <Alert showIcon type="warning" title="DEMO 目标包不能用于 Paper 或 Live 审批及交付。" />}
    {item && !query.isError && <Button disabled={!deliverableOrigin} onClick={() => setApproving(true)}>审批此目标包</Button>}
    {item && !query.isError && <Button onClick={() => setDeciding(true)}>人工拒绝与重新考虑</Button>}
    {deciding && item && <ReleaseDecision release={item} close={() => setDeciding(false)} />}
    {revoking && <ApprovalRevoke approval={revoking} close={() => setRevoking(undefined)} />}
    {offering && item && deliverableOrigin && <HandoffOffer release={item} approval={offering} close={() => setOffering(undefined)} />}
    {approving && item && deliverableOrigin && <ReleaseApprove release={item} close={() => setApproving(false)} />}
    {item && !query.isError && <Collapse items={[{ key: 'approvals', label: '原审批历史', children: <ReleaseApprovals release={item} offer={setOffering} revoke={setRevoking} /> }]} />}
  </Drawer>;
}

function Handoffs({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<string>();
  const query = useQuery({ queryKey: ['handoffs', project, history.at(-1)], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/projects/{id}/handoffs', { params: { path: { id: project }, query: { cursor: history.at(-1), limit: 25 } }, signal }));
    if (page.items.some(item => item.project_id !== project)) throw new Error('交付记录不属于当前项目。');
    return page;
  } });
  return <Space orientation="vertical" className="full-width">
    <Alert showIcon type="info" title="下游领取与确认不代表真实成交" description="这里保留原 Offer、Claim 和 ACK 事实。读取不会续期，撤销不代表撤单或平仓。" />
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新交付记录</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['HandoffViewV1']> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 800 }} onHeaderRow={() => ({ tabIndex: 0 })} locale={{ emptyText: <NoData text="尚无交付记录。冻结目标包不会自动创建 Offer。" /> }} columns={[
        { title: '交付编号', key: 'id', render: (_, item) => <Button type="link" disabled={query.isError} onClick={() => setSelected(item.id)}>Handoff {item.id.slice(-8)}</Button> },
        { title: '下游', dataIndex: 'downstream_id' }, { title: '环境', dataIndex: 'environment' }, { title: '原状态', dataIndex: 'state' },
        { title: '序号', dataIndex: 'delivery_sequence' }, { title: '原期限', dataIndex: 'expires_at', render: displayTime },
      ]} />
    </QueryPanel>
    <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    {selected && <HandoffDetail key={selected} id={selected} project={project} close={() => setSelected(undefined)} />}
  </Space>;
}

function HandoffDetail({ id, project, close }: { id: string; project: string; close: () => void }) {
  const query = useQuery({ queryKey: ['handoff', project, id], queryFn: async ({ signal }) => {
    const item = dataOf(await api.GET('/api/v2/handoffs/{id}', { params: { path: { id } }, signal }));
    if (item.id !== id || item.project_id !== project) throw new Error('服务器返回了其他交付记录。');
    return item;
  } });
  const item = query.data;
  return <Drawer title="原交付记录" open onClose={close} width={760}>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!item} reload={() => { void query.refetch(); }}>
      {item && <Descriptions column={1} className="break-word" items={[
        { key: 'id', label: 'Handoff 编号', children: item.id }, { key: 'project', label: '项目', children: item.project_id },
        { key: 'candidate', label: '原候选', children: item.candidate_id }, { key: 'mandate', label: '原组合配置', children: item.mandate_id },
        { key: 'release', label: '原 Release', children: item.release_id }, { key: 'approval', label: '原审批', children: item.approval_id },
        { key: 'downstream', label: '下游', children: item.downstream_id }, { key: 'environment', label: '交付环境', children: item.environment },
        { key: 'state', label: '原状态', children: item.state }, { key: 'sequence', label: '原交付序号', children: item.delivery_sequence },
        { key: 'revision', label: '原修订', children: item.revision }, { key: 'previous', label: '前版交付', children: item.supersedes_handoff_id ?? '无前版' },
        { key: 'offered', label: 'Offer 时间', children: displayTime(item.offered_at) }, { key: 'expires', label: '原期限', children: displayTime(item.expires_at) },
        { key: 'claimed', label: 'Claim 时间', children: item.claimed_at ? displayTime(item.claimed_at) : '尚未领取' },
        { key: 'external', label: '原下游领取编号', children: item.external_claim_id ?? '无领取编号' },
        { key: 'ack', label: 'ACK 时间', children: item.acknowledged_at ? displayTime(item.acknowledged_at) : '尚未确认' },
      ]} />}
    </QueryPanel>
  </Drawer>;
}

function ReleaseApprovals({ release, offer, revoke }: { release: Schema['ReleaseViewV1']; offer: (approval: Schema['ApprovalViewV1']) => void; revoke: (approval: Schema['ApprovalViewV1']) => void }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const query = useQuery({ queryKey: ['release-approvals', release.id, history.at(-1)], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/releases/{id}/approvals', { params: { path: { id: release.id }, query: { cursor: history.at(-1), limit: 25 } }, signal }));
    if (page.items.some(item => item.project_id !== release.project_id || item.release_id !== release.id || item.candidate_id !== release.candidate_id)) throw new Error('审批记录不属于原目标包。');
    return page;
  } });
  return <Space orientation="vertical" className="full-width">
    <Alert showIcon type="info" title="历史审批不代表当前可发送" description="这里保留原授权与证据引用。有效期、撤销、决定及下游配置仍须在实际交付时由服务端复核。" />
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新审批历史</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['ApprovalViewV1']> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 720 }} onHeaderRow={() => ({ tabIndex: 0 })} locale={{ emptyText: <NoData text="原目标包尚无审批记录。" /> }} columns={[
        { title: '原审批', dataIndex: 'id' }, { title: '操作', key: 'offer', render: (_, item) => <Space><Button disabled={release.environment !== 'REAL' || query.isError || query.isFetching || item.authority_kind !== 'OPERATOR'} onClick={() => offer(item)}>登记 Offer</Button><Button danger disabled={query.isError || query.isFetching} onClick={() => revoke(item)}>撤销审批</Button></Space> }, { title: '下游', dataIndex: 'downstream_id' }, { title: '环境', dataIndex: 'environment' },
        { title: '授权来源', dataIndex: 'authority_kind' }, { title: '原期限', dataIndex: 'valid_until', render: displayTime },
      ]} expandable={{ expandedRowRender: item => <Descriptions column={1} className="break-word" items={[
        { key: 'evidence', label: '原证据集合', children: item.evidence_set_id }, { key: 'policy', label: '原自动化政策', children: item.automation_policy_id ?? '无自动化政策' },
        { key: 'revision', label: '原下游配置版本', children: item.downstream_revision ?? '历史未记录' },
        { key: 'decision', label: '原决定序号', children: item.decision_ordinal ?? '历史未记录' },
        { key: 'observation', label: '原就绪观察', children: item.readiness_observation_id ?? '历史未记录' },
        { key: 'granted', label: '原授权时间', children: displayTime(item.granted_at) }, { key: 'created', label: '记录时间', children: displayTime(item.created_at) },
      ]} /> }} />
    </QueryPanel>
    <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
  </Space>;
}
