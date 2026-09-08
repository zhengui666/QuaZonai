import { App, Alert, Button, Card, Divider, Drawer, Form, Input, Select, Space, Table, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, ApiFailure, dataOf, Intent } from './api';
import type { Schema } from './api';
import { uuidPattern } from './auth';
import { briefContent, initialBudget, initialStop } from './brief-fields';
import { BudgetFields, counterRules } from './budget-fields';
import { ErrorNotice, NoData, Pager, QueryPanel, ResourceFacts, StateTag, useGuard, useOnline } from './ui';

type Brief = Schema['BriefView'];
type Content = Schema['BriefContentV1'];
type Fields = { content: Content; bindings: Schema['BriefBindingV1'][] };
const uuidRules = [{ required: true, pattern: uuidPattern, message: '需要现有记录的完整 UUIDv7 编号。' }];
export function Briefs({ projectId }: { projectId: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [editing, setEditing] = useState<Brief | 'new'>();
  const cursor = history.at(-1); const online = useOnline();
  const query = useQuery({ queryKey: ['briefs', projectId, cursor], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{id}/briefs', { params: { path: { id: projectId }, query: { cursor, limit: 25 } }, signal })) });
  return <Space orientation="vertical" className="full-width" size="middle">
    <Alert showIcon type="info" title="Brief 是研究假设、数据边界和预算的版本化记录。"
      description="当前可保存和修改草稿；冻结与启动不在本版 HTTP 合同中。不会把保存成功显示为研究开始、资格通过或可交付。" />
    <Button type="primary" disabled={!online} onClick={() => setEditing('new')}>新建 Brief 草稿</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Brief> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 600 }} locale={{ emptyText: <NoData text="尚无 Brief。请先填写可检验假设和真实数据引用。" /> }} columns={[
        { title: '版本', dataIndex: 'version' }, { title: '假设', key: 'hypothesis', render: (_, item) => <Typography.Paragraph ellipsis={{ rows: 2, expandable: true }}>{item.content.hypothesis}</Typography.Paragraph> },
        { title: '状态', key: 'state', render: (_, item) => <StateTag value={item.state} /> },
        { title: '操作', key: 'open', render: (_, item) => <Button disabled={query.isError} onClick={() => setEditing(item)}>{item.state === 'DRAFT' ? '查看 / 编辑' : '查看冻结版本'}</Button> },
      ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {editing && <BriefEditor projectId={projectId} brief={editing === 'new' ? undefined : editing} close={() => setEditing(undefined)} />}
  </Space>;
}
function BriefEditor({ projectId, brief, close }: { projectId: string; brief?: Brief; close: () => void }) {
  const [form] = Form.useForm<Fields>(); const [dirty, setDirty] = useState(false);
  const [fork, setFork] = useState(false);
  const online = useOnline(); const client = useQueryClient(); const { modal, message } = App.useApp();
  const intent = useRef(new Intent());
  const horizon: Content['horizon_kind'] | undefined = Form.useWatch(['content', 'horizon_kind'], form);
  const readOnly = brief?.state === 'FROZEN' && !fork;
  const mutation = useMutation({ mutationFn: async (value: Fields) => {
    const content = briefContent(value.content);
    if (brief && !fork) {
      const body: Schema['BriefUpdate'] = { schema_version: 1, expected_revision: brief.revision, content, bindings: value.bindings };
      return dataOf(await api.PATCH('/api/v2/briefs/{id}', { params: { path: { id: brief.id }, header: intent.current.headers('PATCH', `/api/v2/briefs/${brief.id}`, body) }, body }));
    }
    const body: Schema['BriefCreate'] = { schema_version: 1, content, bindings: value.bindings, supersedes_id: brief?.id ?? null };
    return dataOf(await api.POST('/api/v2/projects/{id}/briefs', { params: { path: { id: projectId }, header: intent.current.headers('POST', `/api/v2/projects/${projectId}/briefs`, body) }, body }));
  }, onSuccess: async result => {
    intent.current.clear(); setDirty(false); await client.invalidateQueries({ queryKey: ['briefs', projectId] });
    void message.success(result.replayed ? '已确认上次保存的结果。' : 'Brief 草稿已保存，尚未启动研究。'); close();
  } });
  useGuard(dirty || mutation.isPending);
  const conflict = mutation.error instanceof ApiFailure && mutation.error.code === 'REVISION_CONFLICT';
  const disabled = !online || mutation.isPending || readOnly || conflict;
  function dismiss() {
    if (mutation.isPending) return;
    if (!dirty) { close(); return; }
    modal.confirm({ title: '放弃未保存的 Brief 修改？', okText: '放弃修改', cancelText: '继续编辑', onOk: close });
  }
  return <Drawer title={brief ? `Brief · 版本 ${brief.version}${fork ? ' 的新草稿' : ''}` : '新建 Brief 草稿'} open width={840} onClose={dismiss} closable={!mutation.isPending} maskClosable={!mutation.isPending}>
    <Space orientation="vertical" className="full-width" size="middle">
      {brief && <ResourceFacts id={brief.id} revision={brief.revision} updated={brief.updated_at} />}
      {readOnly && <Alert showIcon type="info" title="冻结版本不可修改。" action={<Button disabled={!online} onClick={() => { setFork(true); setDirty(true); }}>以此创建新版本</Button>} />}
      <ErrorNotice error={mutation.error} />
      {conflict && <Button onClick={() => { void client.invalidateQueries({ queryKey: ['briefs', projectId] }); dismiss(); }}>关闭并重载服务器版本</Button>}
      <Form form={form} layout="vertical" disabled={disabled} scrollToFirstError initialValues={brief ? { content: brief.content, bindings: brief.bindings } : {
        content: { target_kind: 'SCORE', horizon_kind: 'FIXED_BARS', horizon_value: '1', base_currency: 'USD', budget: initialBudget, stop_rule: initialStop }, bindings: [],
      }} onValuesChange={() => setDirty(true)} onFinish={value => { if (!disabled) mutation.mutate(value); }}>
        <Typography.Title level={3}>假设与预测目标</Typography.Title>
        <Form.Item name={['content', 'hypothesis']} label="可检验的假设" rules={[{ required: true, whitespace: true, max: 8000 }]}><Input.TextArea rows={3} maxLength={8000} /></Form.Item>
        <Form.Item name={['content', 'economic_rationale']} label="经济依据" rules={[{ required: true, whitespace: true, max: 8000 }]}><Input.TextArea rows={3} maxLength={8000} /></Form.Item>
        <div className="field-grid">
          <Form.Item name={['content', 'target_kind']} label="预测单位" rules={[{ required: true }]}><Select options={[{ value: 'SCORE', label: '无量纲分数' }, { value: 'EXPECTED_RETURN', label: '预期收益' }]} /></Form.Item>
          <Form.Item name={['content', 'base_currency']} label="基础币种（ISO 4217）" rules={[{ required: true, pattern: /^[A-Z]{3}$/ }]}><Input maxLength={3} /></Form.Item>
          <Form.Item name={['content', 'horizon_kind']} label="预测周期" rules={[{ required: true }]}><Select options={[{ value: 'FIXED_BARS', label: '固定 K 线数' }, { value: 'FIXED_DURATION', label: '固定时长' }, { value: 'VARIABLE_INTERVAL', label: '可变区间' }]} /></Form.Item>
          {horizon !== 'VARIABLE_INTERVAL' && <Form.Item name={['content', 'horizon_value']} label="固定周期值（整数）" rules={counterRules}><Input inputMode="numeric" maxLength={19} /></Form.Item>}
        </div>
        <Typography.Title level={3}>真实记录引用</Typography.Title>
        <Alert type="info" showIcon title="以下编号必须来自服务器已有记录。输入编号不代表数据许可、时间点正确性或评估能力已通过验证。" />
        <div className="field-grid">
          {([['universe_version_id', '投资域版本'], ['evaluation_policy_id', '评估策略'], ['execution_assumptions_id', '执行假设']] as const).map(([field, label]) => <Form.Item key={field} name={['content', field]} label={label} rules={uuidRules}><Input /></Form.Item>)}
          <Form.Item name={['content', 'benchmark_ref']} label="基准引用（可选）" rules={[{ pattern: uuidPattern }]}><Input /></Form.Item>
        </div>
        <Form.List name="bindings" rules={[{ validator: (_, values: unknown[]) => Array.isArray(values) && values.length >= 1 && values.length <= 64 ? Promise.resolve() : Promise.reject(new Error('需要 1 至 64 项真实数据绑定。')) }]}>
          {(fields, { add, remove }, { errors }) => <Space orientation="vertical" className="full-width">
            {fields.map(field => <Card key={field.key} size="small" title={`数据绑定 ${field.name + 1}`} extra={<Button danger disabled={disabled} onClick={() => remove(field.name)}>删除绑定 {field.name + 1}</Button>}>
              <Form.Item name={[field.name, 'dataset_revision_id']} label="数据集版本" rules={uuidRules}><Input /></Form.Item>
              <div className="field-grid">
                <Form.Item name={[field.name, 'role']} label="数据角色" rules={[{ required: true }]}><Select options={['DISCOVERY', 'VALIDATION', 'SEALED', 'FORWARD'].map(value => ({ value, label: value }))} /></Form.Item>
                <Form.Item name={[field.name, 'access_policy']} label="访问边界" rules={[{ required: true }]}><Select options={['METADATA_ONLY', 'RESEARCH_READ', 'EVALUATOR_ONLY'].map(value => ({ value, label: value }))} /></Form.Item>
              </div>
            </Card>)}
            <Form.ErrorList errors={errors} /><Button disabled={disabled || fields.length >= 64} onClick={() => add({ role: 'DISCOVERY', access_policy: 'METADATA_ONLY' })}>添加数据绑定</Button>
          </Space>}
        </Form.List>
        <Divider /><BudgetFields />
        {!readOnly && <Button type="primary" htmlType="submit" loading={mutation.isPending} disabled={disabled}>保存 Brief 草稿</Button>}
      </Form>
    </Space>
  </Drawer>;
}
