import { Alert, App, Button, Descriptions, Drawer, Form, Input, InputNumber, Select, Space, Table, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, dataOf, displayTime, Intent, isCounter, isDecimal } from './api';
import type { Schema } from './api';
import { uuidPattern } from './auth';
import { counterRules } from './budget-fields';
import { ErrorNotice, NoData, Pager, QueryPanel, useGuard, useOnline } from './ui';

type View = Schema['ExecutionAssumptionsViewV1'];
type Settings = Schema['NativeSimulationSettingsV1'];
type Fill = Extract<Schema['NativeModelRefV1'], { adapter_kind: 'NAUTILUS_DEFAULT_FILL' }>['parameters'];
type Latency = Extract<Schema['NativeModelRefV1'], { adapter_kind: 'NAUTILUS_STATIC_LATENCY' }>['parameters'];
type Fields = Omit<Schema['ExecutionAssumptionsCreateV1'], 'schema_version' | 'project_id' | 'settings'> & {
  settings: Omit<Settings, 'schema_version' | 'fee_model' | 'fill_model' | 'latency_model'>; fill: Fill; latency: Latency;
};
const required = { required: true, message: '请填写此项。' };
const ids = [required, { pattern: uuidPattern, message: '请输入现有记录的完整 UUIDv7。' }];
const decimals = [required, { validator: async (_: unknown, value: unknown) => { if (!isDecimal(value)) throw new Error('请输入精确十进制字符串。'); } }];
const counts = [required, { validator: async (_: unknown, value: unknown) => { if (typeof value !== 'string' || !isCounter(value)) throw new Error('请输入非负整数字符串。'); } }];

export function ExecutionAssumptions({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<string>(); const [creating, setCreating] = useState(false); const online = useOnline();
  const query = useQuery({ queryKey: ['execution-assumptions', project, history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{id}/execution-assumptions', { params: { path: { id: project }, query: { cursor: history.at(-1), limit: 25 } }, signal })) });
  return <Space orientation="vertical" size="middle" className="full-width">
    <Alert showIcon type="info" title="不可变执行假设，不是真实账户或成交记录" description="仅保存保守 BAR 假设。费用须匹配原登记资产，保存不会启动模拟、证明 DATA_BACKED 或授予交付资格。" />
    <Space wrap><Button type="primary" disabled={!online} onClick={() => setCreating(true)}>新建执行假设</Button><Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新执行假设</Button></Space>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<View> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 650 }} onHeaderRow={() => ({ tabIndex: 0 })} locale={{ emptyText: <NoData text="尚无原生来源绑定的执行假设；不代表评估完成。" /> }} columns={[
        { title: '假设编号', key: 'id', render: (_, item) => <Button type="link" disabled={query.isError} onClick={() => setSelected(item.id)}>查看假设 {item.id}</Button> },
        { title: '资本假设', key: 'capital', render: (_, item) => `${item.settings.starting_capital} ${item.settings.base_currency}` },
        { title: '创建于', key: 'created', render: (_, item) => displayTime(item.created_at) },
      ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {selected && <Detail id={selected} close={() => setSelected(undefined)} />}
    {creating && <Editor project={project} close={() => setCreating(false)} />}
  </Space>;
}

function Detail({ id, close }: { id: string; close: () => void }) {
  const query = useQuery({ queryKey: ['execution-assumption', id], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/execution-assumptions/{id}', { params: { path: { id } }, signal })) });
  return <Drawer title="不可变执行假设" open width={760} onClose={close}>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      {query.data && <><Descriptions column={1} items={([
        ['id', '假设编号'], ['input_set_id', '冻结输入'], ['dataset_revision_id', '数据版本'], ['runtime_id', 'Runtime'],
        ['capability_snapshot_artifact_id', '能力探测证据'], ['fee_schedule_artifact_id', '原配置产物'], ['engine_image_ref', '原生镜像'],
        ['venue_capability_ref', '市场'], ['calendar_version', '日历版本'], ['settlement_rule_ref', '结算规则'],
      ] as const).map(([key, label]) => ({ key, label, children: <Typography.Text className="break-word" copyable>{query.data![key]}</Typography.Text> }))} />
      <Alert type="info" showIcon title="CONSERVATIVE_ASSUMPTION · 保守假设，不是数据支持成本证明" />
      <Typography.Title level={2}>服务器保存的原生模型配置</Typography.Title><pre className="break-word" style={{ whiteSpace: 'pre-wrap' }}>{JSON.stringify(query.data.settings, null, 2)}</pre></>}
    </QueryPanel>
  </Drawer>;
}

function Editor({ project, close }: { project: string; close: () => void }) {
  const [form] = Form.useForm<Fields>(); const [dirty, setDirty] = useState(false); const intent = useRef(new Intent());
  const client = useQueryClient(); const online = useOnline(); const { modal, message } = App.useApp();
  const mutation = useMutation({ mutationFn: async (values: Fields) => {
    const { fill, latency, ...source } = values;
    const body: Schema['ExecutionAssumptionsCreateV1'] = { ...source, schema_version: 1, project_id: project, settings: { ...source.settings, schema_version: 1,
      fee_model: { schema_version: 1, adapter_kind: 'NAUTILUS_MAKER_TAKER', upstream_class: 'nautilus_execution::models::fee::MakerTakerFeeModel', upstream_version: '0.63.0', parameters: {} },
      fill_model: { schema_version: 1, adapter_kind: 'NAUTILUS_DEFAULT_FILL', upstream_class: 'nautilus_execution::models::fill::DefaultFillModel', upstream_version: '0.63.0', parameters: fill },
      latency_model: { schema_version: 1, adapter_kind: 'NAUTILUS_STATIC_LATENCY', upstream_class: 'nautilus_execution::models::latency::StaticLatencyModel', upstream_version: '0.63.0', parameters: latency },
    } };
    return dataOf(await api.POST('/api/v2/execution-assumptions', { body, params: { header: intent.current.headers('POST', '/api/v2/execution-assumptions', body) } }));
  }, onSuccess: async result => {
    intent.current.clear(); setDirty(false); await client.invalidateQueries({ queryKey: ['execution-assumptions', project] });
    await message.success(result.replayed ? '已读取原假设回执，没有重复创建。' : '已保存不可变执行假设，未启动模拟。'); close();
  } });
  useGuard(dirty || mutation.isPending);
  function dismiss() {
    if (mutation.isPending) return;
    if (!dirty) return close();
    modal.confirm({ title: '放弃未保存的执行假设？', content: '不会撤销已发送请求；结果未知时保留原输入重试。', okText: '放弃修改', cancelText: '继续编辑', onOk: close });
  }
  return <Drawer title="新建不可变执行假设" open width={800} onClose={dismiss} closable={!mutation.isPending} maskClosable={!mutation.isPending}>
    <Alert showIcon type="info" title="使用原登记数据与当前 Runtime 版本" description="冻结 Nautilus 0.63.0 费用、填充/滑点和延迟模型。参数由操作者明确填写，不从账户凭据或钱包读取。插入延迟合计必须大于零；现金模型杠杆必须为 1。" />
    <ErrorNotice error={mutation.error} />
    <Form form={form} layout="vertical" onValuesChange={() => setDirty(true)} onFinish={values => mutation.mutate(values)} disabled={!online || mutation.isPending} initialValues={{ settings: { fee_rates: [{}] } }}>
      {([['runtime_id', 'Runtime 编号'], ['input_set_id', '冻结输入编号'], ['dataset_revision_id', '数据版本编号']] as const).map(([name, label]) => <Form.Item key={name} name={name} label={label} rules={ids}><Input /></Form.Item>)}
      <Form.Item name="expected_runtime_revision" label="Runtime 配置版本" rules={counterRules}><Input inputMode="numeric" /></Form.Item>
      <Form.Item name="settlement_rule_ref" label="结算规则引用" rules={[required, { max: 200, whitespace: true }]}><Input /></Form.Item>
      <Form.Item name={['settings', 'base_currency']} label="基础币种" rules={[required, { pattern: /^[A-Z]{3}$/, message: '请输入 ISO 币种代码。' }]}><Input maxLength={3} /></Form.Item>
      <Form.Item name={['settings', 'account_kind']} label="模拟账户模型" rules={[required]}><Select options={[{ value: 'CASH', label: '现金' }, { value: 'MARGIN', label: '保证金' }]} /></Form.Item>
      {([['starting_capital', '资本假设'], ['leverage', '杠杆上限'], ['exposure_tolerance', '敞口容差']] as const).map(([name, label]) => <Form.Item key={name} name={['settings', name]} label={label} rules={decimals}><Input inputMode="decimal" /></Form.Item>)}
      <Form.Item name={['settings', 'snapshot_interval_ms']} label="快照间隔（毫秒）" rules={[required, { type: 'integer', min: 1, max: 86400000 }]}><InputNumber min={1} max={86400000} precision={0} /></Form.Item>
      <Typography.Title level={2}>填充、滑点和延迟</Typography.Title>
      {([['prob_fill_on_limit', '限价成交概率'], ['prob_slippage', '滑点概率']] as const).map(([name, label]) => <Form.Item key={name} name={['fill', name]} label={`${label}（0 至 1）`} rules={decimals}><Input inputMode="decimal" /></Form.Item>)}
      <Form.Item name={['fill', 'random_seed']} label="随机种子" rules={counts}><Input inputMode="numeric" /></Form.Item>
      {([['base_latency_ns', '基础延迟'], ['insert_latency_ns', '插入附加延迟'], ['update_latency_ns', '更新附加延迟'], ['cancel_latency_ns', '取消附加延迟']] as const).map(([name, label]) => <Form.Item key={name} name={['latency', name]} label={`${label}（纳秒）`} rules={counts}><Input inputMode="numeric" /></Form.Item>)}
      <Typography.Title level={2}>逐资产原生费用</Typography.Title>
      <Typography.Paragraph>与原始资产定义精确核对，不接受以手填费率替换来源。</Typography.Paragraph>
      <Form.List name={['settings', 'fee_rates']}>{(fields, { add, remove }) => <>
        {fields.map(field => <Space key={field.key} orientation="vertical" className="full-width">
          <Form.Item name={[field.name, 'instrument_id']} label={`资产 ${field.name + 1} 标识`} rules={[required, { max: 200, whitespace: true }]}><Input /></Form.Item>
          {(['maker', 'taker'] as const).map(name => <Form.Item key={name} name={[field.name, name]} label={`资产 ${field.name + 1} ${name} 费率`} rules={decimals}><Input inputMode="decimal" /></Form.Item>)}
          <Button disabled={fields.length <= 1} onClick={() => remove(field.name)}>删除资产 {field.name + 1}</Button>
        </Space>)}
        <Button disabled={fields.length >= 256} onClick={() => add()}>添加费用资产</Button>
      </>}</Form.List>
      <Space wrap><Button type="primary" htmlType="submit" loading={mutation.isPending}>保存不可变执行假设</Button><Button onClick={dismiss}>取消</Button></Space>
    </Form>
  </Drawer>;
}
