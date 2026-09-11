import { Alert, App, Button, Card, DatePicker, Descriptions, Form, Input, Modal, Select, Space, Switch, Table, Tabs, Tag, Typography } from 'antd';
import type { Dayjs } from 'dayjs';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { ErrorNotice, NoData, Pager, QueryPanel, ResourceFacts, useGuard, useOnline } from './ui';
import { ResourceSelect } from './resource-select';
import { validateNativeCatalogKey } from './generated/responses.cjs';

type Source = Schema['DataSourceView'];
type Grant = Schema['DataGrantView'];
type Dataset = Schema['DatasetView'];
type Universe = Schema['UniverseView'];
const required = { required: true, message: '请填写此项。' };
const licenseNames: Record<Schema['DataLicenseState'], string> = {
  ACTIVE: '当前有效', NOT_YET_VALID: '尚未生效', EXPIRED: '已过期', REVOKED: '已撤销',
};
const uses = [
  { value: 'RESEARCH', label: '仅研究' },
  { value: 'RESEARCH_AND_PAPER', label: '研究与 Paper 目标交付' },
  { value: 'RESEARCH_PAPER_LIVE', label: '研究、Paper 与 Live 目标交付' },
] satisfies { value: Schema['DataUse']; label: string }[];
const originNames: Record<Schema['DataOrigin'], string> = {
  REAL: '真实来源', SYNTHETIC: '合成数据', FIXTURE: '测试数据', LEGACY_UNKNOWN: '历史来源未核验',
};

const registrationNames: Record<Schema['UniverseRegistrationState'], string> = {
  NATIVE_METADATA: '有原生登记证据', LEGACY_UNVERIFIED: '历史记录未核验',
};

function useDataRefresh() {
  const client = useQueryClient();
  return async () => { await client.invalidateQueries({ queryKey: ['data'] }); };
}
function LicenseTag({ value }: { value: Schema['DataLicenseState'] }) { return <Tag>{licenseNames[value]}</Tag>; }
function Identity({ value }: { value: string }) { return <Typography.Text className="break-word" copyable>{value}</Typography.Text>; }

function EvidenceSelect({ value, onChange }: { value?: string; onChange?: (id: string | undefined) => void }) {
  const [project, setProject] = useState<string>();
  return <Space orientation="vertical" className="full-width">
    <ResourceSelect label="许可证明所属研究项目" value={project} queryKey={['data','evidence-projects']}
      onChange={id => { setProject(id); onChange?.(undefined); }}
      load={async (cursor, signal) => {
        const page = dataOf(await api.GET('/api/v2/projects', { params: { query: { cursor, limit: 50 } }, signal }));
        return { items: page.items.map(item => ({ value: item.id, label: item.name })), next_cursor: page.next_cursor };
      }} />
    <ResourceSelect label="选择已发布的人工许可证明报告" value={value} onChange={onChange} disabled={!project}
      queryKey={['data','license-evidence',project]} load={async (cursor, signal) => {
        if (!project) return { items: [], next_cursor: null };
        const page = dataOf(await api.GET('/api/v2/artifacts', { params: { query: { project_id: project, cursor, limit: 50 } }, signal }));
        return { items: page.items.map(item => ({ value: item.id, label: `${item.kind} · ${item.id}`,
          disabled: item.kind !== 'REPORT' || item.created_by !== 'OPERATOR' || item.byte_count === '0',
        })), next_cursor: page.next_cursor };
      }} />
    <Typography.Text type="secondary">证明必须是人工发布的非空 REPORT。研究模型的自述或密封评估产物不能代替数据许可。</Typography.Text>
  </Space>;
}

function SourceDialog({ source, close }: { source?: Source; close: () => void }) {
  type Values = { name: string; runtime_id: string; native_catalog_ref: string; enabled: boolean };
  const [form] = Form.useForm<Values>(); const intent = useRef(new Intent());
  const online = useOnline(); const refresh = useDataRefresh(); const { modal } = App.useApp();
  const mutation = useMutation({ mutationFn: async (values: Values) => {
    if (source) {
      const body: Schema['DataSourceUpdate'] = { schema_version: 1, expected_revision: source.revision, name: values.name, enabled: values.enabled };
      return dataOf(await api.PATCH('/api/v2/data/sources/{id}', { params: { path: { id: source.id }, header: intent.current.headers('PATCH', `/api/v2/data/sources/${source.id}`, body) }, body }));
    }
    const body: Schema['DataSourceCreate'] = { schema_version: 1, provider_kind: 'NAUTILUS_CATALOG', ...values };
    return dataOf(await api.POST('/api/v2/data/sources', { body, params: { header: intent.current.headers('POST','/api/v2/data/sources',body) } }));
  }, onSuccess: async () => { await refresh(); close(); } });
  useGuard(true);
  function cancel() {
    if (mutation.isPending) return;
    if (form.isFieldsTouched()) modal.confirm({ title: '放弃未保存的数据源配置？', content: '已发送的请求不会因此撤销。', okText: '确认放弃', cancelText: '继续编辑', onOk: close });
    else close();
  }
  return <Modal open title={source ? '修改数据源显示与启用状态' : '登记数据源'} onCancel={cancel} destroyOnHidden
    okText={source ? '保存修改' : '登记'} cancelText="返回" confirmLoading={mutation.isPending} closable={!mutation.isPending} maskClosable={false}
    okButtonProps={{ disabled: !online, 'aria-label': source ? '保存修改' : '登记', 'aria-busy': mutation.isPending }} onOk={() => { if (online && !mutation.isPending) form.submit(); }}>
    <Form form={form} layout="vertical" initialValues={source ? { name: source.name, enabled: source.enabled } : { enabled: true }}
      disabled={mutation.isPending || !online} onFinish={values => mutation.mutate(values)}>
      {source && <><ResourceFacts id={source.id} revision={source.revision} updated={source.updated_at} /><Alert type="info" showIcon title="Runtime、原生登记键与 Provider 身份不可修改。停用仅阻止新消费，不删除历史证据。" /></>}
      <Form.Item name="name" label="数据源名称" rules={[required, { max: 120, whitespace: true }]}><Input maxLength={120} /></Form.Item>
      {!source && <>
        <Form.Item name="runtime_id" label="所属 Runtime" rules={[required]}><ResourceSelect label="选择已登记的 Runtime" queryKey={['data','runtime-options']}
          load={async (cursor, signal) => { const page = dataOf(await api.GET('/api/v2/integrations/runtimes', { params: { query: { cursor, limit: 50 } }, signal })); return { items: page.items.map(item => ({ value: item.id, label: item.configuration.name, disabled: !item.configuration.enabled })), next_cursor: page.next_cursor }; }} /></Form.Item>
        <Form.Item name="native_catalog_ref" label="Runtime 原生目录登记键" rules={[required, { validator: async (_, value: unknown) => {
          if (!validateNativeCatalogKey(value)) throw new Error('登记键不得包含 URL、空目录段、点路径、查询标记或首尾空白，最多 512 个字符。');
        } }]} extra="填写 Runtime 配置中的精确登记键，不是 URL 或宿主文件路径。"><Input /></Form.Item>
      </>}
      <Form.Item name="enabled" label="允许新消费" valuePropName="checked"><Switch /></Form.Item>
      <ErrorNotice error={mutation.error} />
    </Form>
  </Modal>;
}

function GrantDialog({ source, close }: { source: Source; close: () => void }) {
  type Values = { license_reference: string; evidence_artifact_id: string; allowed_uses: Schema['DataUse']; valid_from: Dayjs | null | undefined; valid_until?: Dayjs | null | undefined };
  const [form] = Form.useForm<Values>(); const intent = useRef(new Intent()); const online = useOnline(); const refresh = useDataRefresh();
  const mutation = useMutation({ mutationFn: async (values: Values) => {
    const body: Schema['DataGrantCreate'] = { schema_version: 1, source_id: source.id, license_reference: values.license_reference,
      evidence_artifact_id: values.evidence_artifact_id, allowed_uses: values.allowed_uses,
      valid_from: values.valid_from!.toISOString(), valid_until: values.valid_until?.toISOString() ?? null };
    return dataOf(await api.POST('/api/v2/data/sources/{id}/grants', { body, params: { path: { id: source.id },
      header: intent.current.headers('POST',`/api/v2/data/sources/${source.id}/grants`,body) } }));
  }, onSuccess: async () => { await refresh(); close(); } });
  useGuard(true);
  return <Modal open title={`授权数据用途：${source.name}`} maskClosable={false} closable={!mutation.isPending}
    onCancel={() => { if (!mutation.isPending) close(); }} onOk={() => { if (online && !mutation.isPending) form.submit(); }}
    okText="确认登记不可变授权" cancelText="返回" confirmLoading={mutation.isPending} okButtonProps={{ disabled: !online }}>
    <Alert showIcon type="warning" title="登记授权不会证明历史 PIT 或计算可用性。" description="只允许选择已有许可真正覆盖的用途；授权不能在发行后扩大，撤销也不会抹掉已发生的读取。" />
    <Form form={form} layout="vertical" initialValues={{ allowed_uses: 'RESEARCH' }} disabled={mutation.isPending || !online} onFinish={values => mutation.mutate(values)}>
      <Form.Item name="license_reference" label="许可出处或合同编号" rules={[required, { max: 2000, whitespace: true }]}><Input.TextArea rows={3} maxLength={2000} /></Form.Item>
      <Form.Item name="evidence_artifact_id" label="已发布的许可证明" rules={[required]}><EvidenceSelect /></Form.Item>
      <Form.Item name="allowed_uses" label="允许用途" rules={[required]}><Select options={uses} /></Form.Item>
      <Form.Item name="valid_from" label="生效时间（本地时间显示，UTC 保存）" rules={[required]}><DatePicker showTime className="full-width" /></Form.Item>
      <Form.Item name="valid_until" label="到期时间（可不设）" dependencies={['valid_from']} rules={[({ getFieldValue }) => ({ validator: async (_, value: Dayjs | null | undefined) => {
        const start: Dayjs | null | undefined = getFieldValue('valid_from');
        if (value && start && value.valueOf() <= start.valueOf()) throw new Error('到期必须晚于生效时间。');
      } })]}><DatePicker showTime className="full-width" /></Form.Item>
      <ErrorNotice error={mutation.error} />
    </Form>
  </Modal>;
}

function RevokeDialog({ grant, close }: { grant: Grant; close: () => void }) {
  type Values = { reason_code: string; reason: string; effective_at?: Dayjs | null | undefined };
  const [form] = Form.useForm<Values>(); const intent = useRef(new Intent()); const online = useOnline(); const refresh = useDataRefresh();
  const mutation = useMutation({ mutationFn: async (values: Values) => {
    const body: Schema['DataGrantRevoke'] = { schema_version: 1, reason_code: values.reason_code, reason: values.reason, effective_at: values.effective_at?.toISOString() ?? null };
    return dataOf(await api.POST('/api/v2/data/grants/{id}/revoke', { body, params: { path: { id: grant.id },
      header: intent.current.headers('POST',`/api/v2/data/grants/${grant.id}/revoke`,body) } }));
  }, onSuccess: async () => { await refresh(); close(); } });
  useGuard(true);
  return <Modal open title="撤销这份数据授权？" maskClosable={false} closable={!mutation.isPending} confirmLoading={mutation.isPending}
    okText="确认追加撤销记录" cancelText="返回" okButtonProps={{ danger: true, disabled: !online }}
    onCancel={() => { if (!mutation.isPending) close(); }} onOk={() => { if (online && !mutation.isPending) form.submit(); }}>
    <Typography.Paragraph>授权版本 {grant.version} · {grant.license_reference}</Typography.Paragraph>
    <Alert type="warning" showIcon title="生效后阻止新的数据消费和交付授权；不删除已发生的研究、数据暴露或下游交易事实。" />
    <Form form={form} layout="vertical" initialValues={{ reason_code: 'OPERATOR_REVOKED' }} disabled={mutation.isPending || !online} onFinish={values => mutation.mutate(values)}>
      <Form.Item name="reason_code" label="原因代码" rules={[required, { max: 120 }]}><Input maxLength={120} /></Form.Item>
      <Form.Item name="reason" label="撤销说明" rules={[required, { max: 2000, whitespace: true }]}><Input.TextArea rows={4} maxLength={2000} /></Form.Item>
      <Form.Item name="effective_at" label="未来生效时间（留空则立即）"><DatePicker showTime className="full-width" /></Form.Item>
      <ErrorNotice error={mutation.error} />
    </Form>
  </Modal>;
}

function RegisterDialog({ source, runtimeRevision, close }: { source: Source; runtimeRevision: string; close: () => void }) {
  type Values = { grant_id: string; native_storage_version: string; existing_universe_version_id?: string };
  const [form] = Form.useForm<Values>(); const intent = useRef(new Intent()); const online = useOnline(); const refresh = useDataRefresh();
  const mutation = useMutation({ mutationFn: async (values: Values) => {
    const body: Schema['DatasetRegister'] = { schema_version: 1, source_id: source.id, grant_id: values.grant_id,
      expected_source_revision: source.revision, expected_runtime_revision: runtimeRevision,
      native_storage_version: values.native_storage_version, existing_universe_version_id: values.existing_universe_version_id ?? null };
    return dataOf(await api.POST('/api/v2/data/revisions', { body, params: { header: intent.current.headers('POST','/api/v2/data/revisions',body) } }));
  }, onSuccess: async () => { await refresh(); close(); } });
  useGuard(true);
  return <Modal open title={`读取并登记原生数据：${source.name}`} maskClosable={false} closable={!mutation.isPending}
    okText="读取真实元数据并登记" cancelText="返回" confirmLoading={mutation.isPending} okButtonProps={{ disabled: !online }}
    onCancel={() => { if (!mutation.isPending) close(); }} onOk={() => { if (online && !mutation.isPending) form.submit(); }}>
    <Alert type="info" showIcon title="来源、PIT、样本数、Universe 和质量报告从 Runtime 原生接口读取，不由表单填写。" description={`本次绑定数据源版本 ${source.revision}、Runtime 版本 ${runtimeRevision}。冲突时请核对后重新打开表单；不会自动覆盖。`} />
    <Form form={form} layout="vertical" disabled={mutation.isPending || !online} onFinish={values => mutation.mutate(values)}>
      <Form.Item name="grant_id" label="适用数据授权" rules={[required]}><ResourceSelect label="选择当前有效的授权" queryKey={['data','grant-options',source.id]}
        load={async (cursor, signal) => { const page = dataOf(await api.GET('/api/v2/data/sources/{id}/grants', { params: { path: { id: source.id }, query: { cursor, limit: 50 } }, signal })); return { items: page.items.map(item => ({ value: item.id, label: `版本 ${item.version} · ${item.license_reference} · ${licenseNames[item.license_state]}`, disabled: item.license_state !== 'ACTIVE' })), next_cursor: page.next_cursor }; }} /></Form.Item>
      <Form.Item name="native_storage_version" label="原生存储版本" rules={[required, { max: 120, pattern: /^[!-~]+$/, message: '填写不含空白的原生版本。' }]}><Input maxLength={120} /></Form.Item>
      <Form.Item name="existing_universe_version_id" label="复用已登记 Universe（可选）"><ResourceSelect allowClear label="不选择则按真实元数据建立 Universe" queryKey={['data','universe-options']}
        load={async (cursor, signal) => { const page = dataOf(await api.GET('/api/v2/data/universes', { params: { query: { cursor, limit: 50 } }, signal })); return { items: page.items.map(item => ({ value: item.id, label: `${item.name} · ${item.calendar_version} · ${registrationNames[item.registration_state]} · ${displayTime(item.selection_asof)}`, disabled: item.registration_state !== 'NATIVE_METADATA' })), next_cursor: page.next_cursor }; }} /></Form.Item>
      <ErrorNotice error={mutation.error} />
    </Form>
  </Modal>;
}

function SourceDetails({ sourceId }: { sourceId: string }) {
  const online = useOnline(); const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [editing, setEditing] = useState<Source>(); const [granting, setGranting] = useState<Source>();
  const [revoking, setRevoking] = useState<Grant>(); const [registering, setRegistering] = useState<{ source: Source; revision: string }>();
  const [historyGrant, setHistoryGrant] = useState<Grant>();
  const source = useQuery({ queryKey: ['data','source',sourceId], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/data/sources/{id}', { params: { path: { id: sourceId } }, signal })) });
  const grants = useQuery({ queryKey: ['data','grants',sourceId,history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/data/sources/{id}/grants', { params: { path: { id: sourceId }, query: { cursor: history.at(-1), limit: 50 } }, signal })) });
  const runtimeId = source.data?.runtime_id;
  const runtime = useQuery({ queryKey: ['data','source-runtime',runtimeId], enabled: runtimeId !== undefined, queryFn: async ({ signal }) => {
    if (!runtimeId) throw new Error('Source binding missing');
    return dataOf(await api.GET('/api/v2/integrations/runtimes/{id}', { params: { path: { id: runtimeId } }, signal }));
  } });
  const current = source.data;
  return <Card title={current?.name ?? '数据源详情'}>
    <QueryPanel pending={source.isPending} error={source.error} stale={!!current} reload={() => { void source.refetch(); }}>
      {current && <>
        <ResourceFacts id={current.id} revision={current.revision} updated={current.updated_at} />
        <Descriptions column={1} items={[
          { key: 'runtime', label: 'Runtime', children: runtime.data?.configuration.name ?? current.runtime_id },
          { key: 'catalog', label: '原生登记键', children: current.native_catalog_ref },
          { key: 'provider', label: 'Provider', children: current.provider_kind },
          { key: 'enabled', label: '新消费', children: current.enabled ? '允许（仍须检查许可及运行能力）' : '已停用' },
        ]} />
        <Space wrap>
          <Button disabled={!online || source.isError} onClick={() => setEditing(current)}>修改数据源</Button>
          <Button disabled={!online || source.isError || !current.enabled} onClick={() => setGranting(current)}>登记许可授权</Button>
          <Button type="primary" disabled={!online || source.isError || runtime.isError || !current.enabled || !runtime.data?.configuration.enabled}
            onClick={() => { if (runtime.data) setRegistering({ source: current, revision: runtime.data.revision }); }}>登记原生数据版本</Button>
        </Space>
        <ErrorNotice error={runtime.error} retry={() => { void runtime.refetch(); }} />
      </>}
    </QueryPanel>
    <Typography.Title level={4}>许可与用途</Typography.Title>
    <QueryPanel pending={grants.isPending} error={grants.error} stale={!!grants.data} reload={() => { void grants.refetch(); }}>
      <Table<Grant> rowKey="id" dataSource={grants.data?.items} pagination={false} scroll={{ x: 760 }} locale={{ emptyText: <NoData text="尚未登记数据授权。没有授权的数据不能进入研究。" /> }} columns={[
        { title: '版本', dataIndex: 'version' }, { title: '许可', dataIndex: 'license_reference' },
        { title: '用途', key: 'use', render: (_, item) => uses.find(option => option.value === item.allowed_uses)?.label ?? item.allowed_uses },
        { title: '读取时状态', key: 'state', render: (_, item) => <LicenseTag value={item.license_state} /> },
        { title: '有效期', key: 'period', render: (_, item) => `${displayTime(item.valid_from)} — ${item.valid_until ? displayTime(item.valid_until) : '未设到期'}` },
        { title: '操作', key: 'actions', render: (_, item) => <Space wrap><Button onClick={() => setHistoryGrant(item)}>撤销历史</Button><Button danger disabled={!online || grants.isError || item.license_state === 'REVOKED'} onClick={() => setRevoking(item)}>撤销授权</Button></Space> },
      ]} />
      <Pager history={history} next={grants.data?.next_cursor} loading={grants.isFetching} move={setHistory} />
    </QueryPanel>
    {editing && <SourceDialog source={editing} close={() => setEditing(undefined)} />}
    {granting && <GrantDialog source={granting} close={() => setGranting(undefined)} />}
    {revoking && <RevokeDialog grant={revoking} close={() => setRevoking(undefined)} />}
    {registering && <RegisterDialog source={registering.source} runtimeRevision={registering.revision} close={() => setRegistering(undefined)} />}
    {historyGrant && <RevocationHistory grant={historyGrant} close={() => setHistoryGrant(undefined)} />}
  </Card>;
}

function RevocationHistory({ grant, close }: { grant: Grant; close: () => void }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const query = useQuery({ queryKey: ['data','revocations',grant.id,history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/data/grants/{id}/revocations', { params: { path: { id: grant.id }, query: { cursor: history.at(-1), limit: 50 } }, signal })) });
  return <Modal open title={`授权 ${grant.version} 的撤销历史`} onCancel={close} footer={<Button onClick={close}>关闭</Button>}>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['DataGrantRevocationView']> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 500 }} locale={{ emptyText: <NoData text="没有撤销记录。许可仍可能尚未生效或已经到期。" /> }} columns={[
        { title: '生效时间', key: 'time', render: (_, item) => displayTime(item.effective_at) }, { title: '原因代码', dataIndex: 'reason_code' }, { title: '说明', dataIndex: 'reason' },
      ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
  </Modal>;
}

function Sources() {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]); const [selected, setSelected] = useState<string>(); const [creating, setCreating] = useState(false); const online = useOnline();
  const query = useQuery({ queryKey: ['data','sources',history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/data/sources', { params: { query: { cursor: history.at(-1), limit: 50 } }, signal })) });
  return <Space orientation="vertical" className="full-width" size="large">
    <Button type="primary" disabled={!online} onClick={() => setCreating(true)}>登记数据源</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Source> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 680 }} locale={{ emptyText: <NoData text="还没有数据源。先在集成设置登记 Runtime，再使用其目录登记键创建数据源。" /> }} columns={[
        { title: '名称', dataIndex: 'name' }, { title: '原生登记键', dataIndex: 'native_catalog_ref' }, { title: '版本', dataIndex: 'revision' },
        { title: '状态', key: 'state', render: (_, source) => source.enabled ? '允许新消费' : '已停用' },
        { title: '操作', key: 'show', render: (_, source) => <Button onClick={() => setSelected(source.id)}>查看许可与版本登记</Button> },
      ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {selected && <SourceDetails key={selected} sourceId={selected} />}
    {creating && <SourceDialog close={() => setCreating(false)} />}
  </Space>;
}

function DatasetDetails({ id, close }: { id: string; close: () => void }) {
  const query = useQuery({ queryKey: ['data','revision',id], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/data/revisions/{id}', { params: { path: { id } }, signal })) });
  const item = query.data;
  return <Modal open title="原生数据版本与证据" width={760} onCancel={close} footer={<Button onClick={close}>关闭</Button>}>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!item} reload={() => { void query.refetch(); }}>
      {item && <><Alert type={item.origin === 'REAL' && item.pit_status === 'VERIFIED' ? 'info' : 'warning'} showIcon title={`${originNames[item.origin]} · PIT ${item.pit_status}`} description="元数据登记、时间排序或运行成功都不能代替独立科学评估。每次消费还会重新检查许可。" />
        <Descriptions column={1} items={[
          { key: 'id', label: '数据版本编号', children: <Identity value={item.id} /> },
          { key: 'native', label: '原生快照', children: item.native_snapshot_ref },
          { key: 'version', label: '原生存储版本', children: item.storage_version },
          { key: 'partition', label: '数据分区', children: item.partition },
          { key: 'rows', label: '原生行数', children: item.row_count },
          { key: 'period', label: '事件区间（左闭右开）', children: `${displayTime(item.event_start)} — ${displayTime(item.event_end)}` },
          { key: 'available', label: '已可得至', children: displayTime(item.available_through) },
          { key: 'license', label: '当前许可', children: <LicenseTag value={item.license_state} /> },
          { key: 'checked', label: '许可核对时间', children: displayTime(item.checked_at) },
          { key: 'enabled', label: '新消费开关', children: `数据源：${item.source_enabled ? '开启' : '关闭'}；Runtime：${item.runtime_enabled ? '开启' : '关闭'}` },
          { key: 'universe', label: 'Universe 版本', children: <Identity value={item.universe_version_id} /> },
          { key: 'quality', label: '质量证据', children: <Identity value={item.quality_artifact_id} /> },
          { key: 'metadata', label: '原生登记证据', children: item.native_metadata_artifact_id ? <Identity value={item.native_metadata_artifact_id} /> : '历史记录缺少正式登记证据，不能推断补齐' },
          { key: 'observed', label: '登记时真实观测', children: displayTime(item.registration_observed_at) },
        ]} /></>}
    </QueryPanel>
  </Modal>;
}

function Datasets() {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]); const [partition, setPartition] = useState<Schema['DataPartition']>(); const [selected, setSelected] = useState<string>();
  const query = useQuery({ queryKey: ['data','revisions',partition,history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/data/revisions', { params: { query: { cursor: history.at(-1), partition, limit: 50 } }, signal })) });
  return <Space orientation="vertical" className="full-width">
    <Select<Schema['DataPartition']> aria-label="筛选数据分区" allowClear placeholder="全部分区" value={partition} style={{ width: 240 }}
      options={['DISCOVERY','VALIDATION','SEALED','FORWARD'].map(value => ({ value, label: value }))}
      onChange={value => { setPartition(value); setHistory([undefined]); }} />
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Dataset> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 900 }} locale={{ emptyText: <NoData text="没有符合筛选条件的已登记版本。在数据源详情中读取真实 Runtime 元数据后登记。" /> }} columns={[
        { title: '原生快照', dataIndex: 'native_snapshot_ref' }, { title: '原生版本', dataIndex: 'storage_version' }, { title: '分区', dataIndex: 'partition' },
        { title: '来源', key: 'origin', render: (_, item) => originNames[item.origin] }, { title: 'PIT', dataIndex: 'pit_status' },
        { title: '行数', dataIndex: 'row_count' }, { title: '当前许可', key: 'license', render: (_, item) => <LicenseTag value={item.license_state} /> },
        { title: '操作', key: 'show', render: (_, item) => <Button onClick={() => setSelected(item.id)}>查看版本证据</Button> },
      ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {selected && <DatasetDetails id={selected} close={() => setSelected(undefined)} />}
  </Space>;
}

function Universes() {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const query = useQuery({ queryKey: ['data','universes',history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/data/universes', { params: { query: { cursor: history.at(-1), limit: 50 } }, signal })) });
  return <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
    <Alert type="info" showIcon title="原生登记与历史记录分开展示；不能覆盖已冻结的成员或资产定义。" description="有原生登记证据只表示版本来源可追溯，不等于真实市场数据、PIT 已验证或研究合格。历史记录未核验时不会补造证据。" />
    <Table<Universe> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 960 }} locale={{ emptyText: <NoData text="尚无 Universe 版本。" /> }} columns={[
      { title: '名称', dataIndex: 'name' }, { title: '登记证据', key: 'registration', render: (_, item) => <Tag>{registrationNames[item.registration_state]}</Tag> },
      { title: '日历版本', key: 'calendar', render: (_, item) => `${item.calendar_ref} / ${item.calendar_version}` },
      { title: '选择时点', key: 'asof', render: (_, item) => displayTime(item.selection_asof) },
      { title: '历史成员声明', key: 'history', render: (_, item) => item.has_historical_membership ? '已声明（仍以登记证据为准）' : '未声明历史成员' },
      { title: '覆盖区间', key: 'coverage', render: (_, item) => `${displayTime(item.coverage_start)} — ${displayTime(item.coverage_end)}` },
    ]} expandable={{ expandedRowRender: item => <Descriptions column={1} items={[
      { key: 'id', label: 'Universe 编号', children: <Identity value={item.id} /> },
      { key: 'members', label: '成员证据', children: <Identity value={item.membership_artifact_id} /> },
      { key: 'instruments', label: '原生资产定义证据', children: <Identity value={item.instrument_definitions_artifact_id} /> },
    ]} /> }} />
    <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
  </QueryPanel>;
}

export function DataManagement() {
  const [tab, setTab] = useState('sources');
  return <Space orientation="vertical" className="full-width" size="large">
    <Typography.Title level={2}>数据、许可与 Universe</Typography.Title>
    <Typography.Paragraph>先配置原生数据源，再登记用途许可和不可变版本。数据源配置、原生观测、独立评估是三个不同步骤。</Typography.Paragraph>
    <Tabs activeKey={tab} onChange={setTab} items={[
      { key: 'sources', label: '数据源与许可' }, { key: 'revisions', label: '已登记数据版本' }, { key: 'universes', label: 'Universe 版本' },
    ]} />
    {tab === 'sources' ? <Sources /> : tab === 'revisions' ? <Datasets /> : <Universes />}
  </Space>;
}
