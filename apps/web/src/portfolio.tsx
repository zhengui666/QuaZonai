import { Alert, App, Button, Card, Descriptions, Drawer, Form, Input, InputNumber, Select, Space, Switch, Table, Tabs, Typography } from 'antd';
import { ExecutionAssumptions } from './execution-assumptions';
import { Candidates } from './portfolio-candidates';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useContext, useRef, useState } from 'react';
import { api, dataOf, displayTime, Intent, isDecimal } from './api';
import type { Schema } from './api';
import { uuidPattern } from './auth';
import { counterRules } from './budget-fields';
import { ResourceSelect } from './resource-select';
import { ErrorNotice, GuardContext, NoData, Pager, QueryPanel, useGuard, useOnline } from './ui';
import { validateBaseCurrency } from './generated/responses.cjs';

type Mandate = Schema['MandateViewV1'];
type Content = Schema['MandateContentV1'];
type Parameters = Extract<Schema['NativeModelRefV1'], { adapter_kind: 'CLARABEL_QP' }>['parameters'];
type Fields = { runtime_id: string; expected_runtime_revision: string; content: Omit<Content, 'optimizer' | 'alpha_ensemble' | 'covariance_estimator'>; parameters: Parameters };
const required = { required: true, message: '请填写此项。' };
const uuidRules = [required, { pattern: uuidPattern, message: '需要现有记录的完整 UUIDv7 编号。' }];
const decimalRules = [required, { validator: async (_: unknown, value: unknown) => { if (!isDecimal(value)) throw new Error('请输入可精确保存的十进制字符串，不使用指数格式。'); } }];
const optionalDecimal = [{ validator: async (_: unknown, value: unknown) => { if (value != null && value !== '' && !isDecimal(value)) throw new Error('请输入十进制字符串，或留空。'); } }];
const exposures = [['min_cash_weight', '现金下限'], ['max_cash_weight', '现金上限'], ['min_asset_weight', '资产默认下限'], ['max_asset_weight', '资产默认上限'], ['max_gross_exposure', '总敞口上限'], ['min_net_exposure', '净敞口下限'], ['max_net_exposure', '净敞口上限'], ['max_turnover_per_rebalance', '单次换手上限']] as const;
const blank = (value: string | null | undefined) => value === '' || value == null ? null : value;

function mandateRequest(project: string, values: Fields): Schema['MandateCreateV1'] {
  const c = values.content; const schedule = c.rebalance_schedule;
  return { schema_version: 1, project_id: project, runtime_id: values.runtime_id, expected_runtime_revision: values.expected_runtime_revision,
    content: { ...c,
      covariance_estimator: { schema_version: 1, adapter_kind: 'SAMPLE_COVARIANCE', upstream_class: 'ndarray_stats::CorrelationExt::cov', upstream_version: '0.7.0', parameters: { ddof: 1 } },
      alpha_ensemble: { schema_version: 1, adapter_kind: 'FIXED_WEIGHTED_FORECAST', upstream_class: 'ndarray::ArrayBase::dot', upstream_version: '0.17.1', parameters: {} },
      optimizer: { schema_version: 1, adapter_kind: 'CLARABEL_QP', upstream_class: 'clarabel::solver::DefaultSolver', upstream_version: '0.11.1', parameters: { ...values.parameters, schema_version: 1 } },
      constraints: { ...c.constraints, schema_version: 1, max_ex_ante_risk: null, max_participation: blank(c.constraints.max_participation), liquidity_ref: blank(c.constraints.liquidity_ref) },
      rebalance_schedule: { ...schedule, schema_version: 1, interval_seconds: schedule.kind === 'FIXED_INTERVAL' ? schedule.interval_seconds : null,
        calendar_ref: schedule.kind === 'CALENDAR_SESSION' ? schedule.calendar_ref : null, session_offset_seconds: schedule.kind === 'CALENDAR_SESSION' ? schedule.session_offset_seconds : null },
    } };
}

export function Portfolios() {
  const [project, setProject] = useState<string>();
  const { blocked } = useContext(GuardContext);
  return <Space orientation="vertical" size="large" className="full-width">
    <Typography.Title level={1}>组合</Typography.Title>
    <Alert showIcon type="info" title="不可变组合配置与原始候选快照" description="配置保存和候选查询不是 Alpha 资格、科学 PASS 或交付授权。网页构建、独立回测与 Release 交付尚未接通，不会填充示例收益。" />
    <ResourceSelect label="选择组合所属项目" value={project} onChange={setProject} disabled={blocked} queryKey={['portfolio-projects']} load={async (cursor, signal) => {
      const page = dataOf(await api.GET('/api/v2/projects', { params: { query: { cursor, limit: 50 } }, signal }));
      return { next_cursor: page.next_cursor, items: page.items.map(item => ({ value: item.id, label: `${item.name} · ${item.id}` })) };
    }} />
    {project ? <Tabs key={project} items={[{ key: 'mandates', label: '组合配置', children: <Mandates project={project} /> }, { key: 'assumptions', label: '执行假设', children: <ExecutionAssumptions project={project} /> }, { key: 'candidates', label: '候选快照', children: <Candidates project={project} /> }]} /> : <NoData text="请选择项目后查看配置，不会自动创建或启动组合。" />}
  </Space>;
}

function Mandates({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<string>(); const [creating, setCreating] = useState(false); const online = useOnline();
  const query = useQuery({ queryKey: ['mandates', project, history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{id}/portfolio-mandates', { params: { path: { id: project }, query: { cursor: history.at(-1), limit: 25 } }, signal })) });
  return <Space orientation="vertical" size="middle" className="full-width">
    <Space wrap><Button type="primary" disabled={!online} onClick={() => setCreating(true)}>新建组合配置</Button><Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新配置</Button></Space>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Mandate> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 650 }} locale={{ emptyText: <NoData text="本项目尚无组合配置。这不表示组合评估已完成。" /> }} columns={[
        { title: '版本', key: 'version', render: (_, item) => <Button type="link" disabled={query.isError} onClick={() => setSelected(item.id)}>配置 v{item.version}</Button> },
        { title: '目标', key: 'objective', render: (_, item) => item.content.objective },
        { title: '资本假设', key: 'capital', render: (_, item) => `${item.content.capital_assumption} ${item.content.base_currency}` },
        { title: '创建于', key: 'created', render: (_, item) => displayTime(item.created_at) },
      ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {selected && <MandateDetail id={selected} close={() => setSelected(undefined)} />}
    {creating && <MandateEditor project={project} close={() => setCreating(false)} />}
  </Space>;
}

function MandateDetail({ id, close }: { id: string; close: () => void }) {
  const query = useQuery({ queryKey: ['mandate', id], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/portfolio-mandates/{id}', { params: { path: { id } }, signal })) });
  return <Drawer title="不可变组合配置" open onClose={close} width={760}>
    <Alert showIcon type="info" title="此版本不可修改。变更需新建配置，不会覆盖原目标依赖。" />
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      {query.data && <><Descriptions column={1} items={[
        { key: 'id', label: '配置编号', children: <Typography.Text className="break-word" copyable>{query.data.id}</Typography.Text> },
        { key: 'version', label: '版本', children: query.data.version },
        { key: 'created', label: '创建于', children: displayTime(query.data.created_at) },
        { key: 'capital', label: '资本假设（非真实账户）', children: `${query.data.content.capital_assumption} ${query.data.content.base_currency}` },
      ]} /><Typography.Title level={2}>服务器保存的完整配置</Typography.Title><pre className="break-word" style={{ whiteSpace: 'pre-wrap' }}>{JSON.stringify(query.data.content, null, 2)}</pre></>}
    </QueryPanel>
  </Drawer>;
}

function MandateEditor({ project, close }: { project: string; close: () => void }) {
  const [form] = Form.useForm<Fields>(); const [dirty, setDirty] = useState(false); const intent = useRef(new Intent());
  const client = useQueryClient(); const online = useOnline(); const { modal, message } = App.useApp();
  const kind = Form.useWatch(['content', 'rebalance_schedule', 'kind'], form);
  const mutation = useMutation({ mutationFn: async (values: Fields) => {
    const body = mandateRequest(project, values);
    return dataOf(await api.POST('/api/v2/portfolio-mandates', { body, params: { header: intent.current.headers('POST', '/api/v2/portfolio-mandates', body) } }));
  }, onSuccess: async result => {
    intent.current.clear(); setDirty(false); await client.invalidateQueries({ queryKey: ['mandates', project] });
    await message.success(result.replayed ? '已读取原配置回执，没有重复创建。' : `已保存不可变配置 v${result.resource.version}，未启动组合。`); close();
  } });
  useGuard(dirty || mutation.isPending);
  function dismiss() {
    if (mutation.isPending) return;
    if (!dirty) return close();
    modal.confirm({ title: '放弃未保存的组合配置？', content: '已发送的请求不会被撤销。结果未知时应保留原输入重试。', okText: '放弃修改', cancelText: '继续编辑', onOk: close });
  }
  return <Drawer title="新建不可变组合配置" open width={800} onClose={dismiss} maskClosable={!mutation.isPending} closable={!mutation.isPending}>
    <Alert showIcon type="info" title="保存将冻结以下全部字段，不会启动求解或交付。" description="使用已登记的准确引用和当前 Runtime 版本。默认值是可修改的配置意图，不是数据或收益估计。原生不支持的目标仍需后续开发，不能换目标冒充支持。" />
    <ErrorNotice error={mutation.error} />
    <Form form={form} layout="vertical" disabled={!online || mutation.isPending} onValuesChange={() => setDirty(true)} onFinish={values => { if (online && !mutation.isPending) mutation.mutate(values); }} initialValues={{
      content: { objective: 'MIN_RISK', risk_measure: 'VARIANCE', exposure_tolerance: '0.000001',
        constraints: { long_only: true, min_cash_weight: '0', max_cash_weight: '0', min_asset_weight: '0', max_asset_weight: '1', max_gross_exposure: '1', min_net_exposure: '1', max_net_exposure: '1', max_turnover_per_rebalance: '2', group_bounds: [], asset_overrides: [] },
        rebalance_schedule: { kind: 'MANUAL', timezone: 'UTC', max_input_age_seconds: 60, target_ttl_seconds: 300 } },
      parameters: { risk_aversion: '1', max_iterations: 200, solver_tolerance: '0.0000000001', accept_inaccurate: false },
    }}>
      <Card title="原始引用与执行环境">
        <Typography.Paragraph type="secondary">编号来自已有登记记录；服务端会核对项目、政策与执行假设的一致性，不自动选择第一个版本。</Typography.Paragraph>
        <Form.Item name="runtime_id" label="Runtime 编号" rules={uuidRules}><Input /></Form.Item>
        <Form.Item name="expected_runtime_revision" label="Runtime 配置版本" rules={counterRules}><Input inputMode="numeric" /></Form.Item>
        {([['universe_version_id', '投资域版本编号'], ['required_evaluation_policy_id', '评估政策编号'], ['execution_assumptions_id', '执行假设编号']] as const).map(([name, label]) => <Form.Item key={name} name={['content', name]} label={label} rules={uuidRules}><Input /></Form.Item>)}
        <Form.Item name={['content', 'base_currency']} label="基础币种" rules={[required, { validator: async (_, value) => { if (!validateBaseCurrency(value)) throw new Error('请选择有效 ISO 币种代码。'); } }]}><Input maxLength={3} /></Form.Item>
        <Form.Item name={['content', 'capital_assumption']} label="资本假设" rules={decimalRules}><Input inputMode="decimal" /></Form.Item>
        <Form.Item name={['content', 'exposure_tolerance']} label="发布敞口容差" rules={decimalRules}><Input inputMode="decimal" /></Form.Item>
      </Card>
      <Card title="原生模型与目标">
        <Typography.Paragraph>样本协方差 ndarray-stats 0.7.0（ddof=1）；固定预测聚合 ndarray 0.17.1；优化器 Clarabel 0.11.1。需要 portfolio-models/4 镜像能力。</Typography.Paragraph>
        <Form.Item name={['content', 'objective']} label="优化目标" rules={[required]}><Select options={[{ value: 'MIN_RISK', label: '最小风险' }, { value: 'MAX_UTILITY', label: '最大效用' }, { value: 'RISK_BUDGETING', label: '风险预算（当前原生未支持）', disabled: true }]} /></Form.Item>
        <Form.Item name={['content', 'risk_measure']} label="风险度量" rules={[required]}><Select options={[{ value: 'VARIANCE', label: '方差' }, { value: 'CVAR', label: 'CVaR（当前原生未支持）', disabled: true }]} /></Form.Item>
        <Form.Item name={['parameters', 'risk_aversion']} label="风险厌恶系数" rules={decimalRules}><Input inputMode="decimal" /></Form.Item>
        <Form.Item name={['parameters', 'solver_tolerance']} label="求解停止容差" rules={decimalRules}><Input inputMode="decimal" /></Form.Item>
        <Form.Item name={['parameters', 'max_iterations']} label="最大迭代次数" rules={[required, { type: 'integer', min: 1, max: 100000 }]}><InputNumber min={1} max={100000} precision={0} /></Form.Item>
        <Form.Item name={['parameters', 'accept_inaccurate']} label="允许原生非精确成功状态" valuePropName="checked"><Switch /></Form.Item>
      </Card>
      <Card title="完整组合约束">
        <Form.Item name={['content', 'constraints', 'long_only']} label="仅做多" valuePropName="checked"><Switch /></Form.Item>
        {exposures.map(([name, label]) => <Form.Item key={name} name={['content', 'constraints', name]} label={label} rules={decimalRules}><Input inputMode="decimal" /></Form.Item>)}
        <Form.Item name={['content', 'constraints', 'transaction_costs_ref']} label="费用依据产物编号" rules={uuidRules}><Input /></Form.Item>
        <Form.Item name={['content', 'constraints', 'liquidity_ref']} label="流动性产物编号（不用时留空）" rules={[{ pattern: uuidPattern, message: '请输入完整 UUIDv7。' }]}><Input /></Form.Item>
        <Form.Item name={['content', 'constraints', 'max_participation']} label="参与率上限（不用时留空）" rules={optionalDecimal}><Input inputMode="decimal" /></Form.Item>
        <Typography.Paragraph type="secondary">非线性事前风险上限当前不支持，明确保存为 null。空的组/资产覆盖列表表示仅使用上方默认约束。</Typography.Paragraph>
        {(['group_bounds', 'asset_overrides'] as const).map(name => <Form.List key={name} name={['content', 'constraints', name]}>{(fields, { add, remove }) => <>
          {fields.map(field => <Card key={field.key} size="small" title={`${name === 'group_bounds' ? '分组' : '资产覆盖'} ${field.name + 1}`}>
            <Form.Item name={[field.name, name === 'group_bounds' ? 'group_id' : 'instrument_id']} label={name === 'group_bounds' ? '组编号' : '资产标识'} rules={[required, { max: name === 'group_bounds' ? 120 : 200, whitespace: true }]}><Input /></Form.Item>
            <Form.Item name={[field.name, 'min']} label="权重下限" rules={decimalRules}><Input inputMode="decimal" /></Form.Item><Form.Item name={[field.name, 'max']} label="权重上限" rules={decimalRules}><Input inputMode="decimal" /></Form.Item>
            <Button onClick={() => remove(field.name)}>删除此约束</Button>
          </Card>)}
          <Button disabled={fields.length >= (name === 'group_bounds' ? 64 : 256) || mutation.isPending || !online} onClick={() => add()}>{name === 'group_bounds' ? '添加分组约束' : '添加资产覆盖'}</Button>
        </>}</Form.List>)}
      </Card>
      <Card title="调仓意图">
        <Form.Item name={['content', 'rebalance_schedule', 'kind']} label="调仓方式" rules={[required]}><Select options={[{ value: 'MANUAL', label: '手动' }, { value: 'FIXED_INTERVAL', label: '固定间隔' }, { value: 'CALENDAR_SESSION', label: '交易日历' }]} /></Form.Item>
        <Form.Item name={['content', 'rebalance_schedule', 'timezone']} label="IANA 时区" rules={[required, { max: 200, whitespace: true }]}><Input /></Form.Item>
        {kind === 'FIXED_INTERVAL' && <Form.Item name={['content', 'rebalance_schedule', 'interval_seconds']} label="间隔秒数" rules={[required, { type: 'integer', min: 1, max: 4294967295 }]}><InputNumber min={1} max={4294967295} precision={0} /></Form.Item>}
        {kind === 'CALENDAR_SESSION' && <><Form.Item name={['content', 'rebalance_schedule', 'calendar_ref']} label="日历引用" rules={[required, { max: 200, whitespace: true }]}><Input /></Form.Item><Form.Item name={['content', 'rebalance_schedule', 'session_offset_seconds']} label="相对时段偏移秒数" rules={[required, { type: 'integer', min: -2147483648, max: 2147483647 }]}><InputNumber min={-2147483648} max={2147483647} precision={0} /></Form.Item></>}
        <Form.Item name={['content', 'rebalance_schedule', 'max_input_age_seconds']} label="输入最大年龄（秒）" rules={[required, { type: 'integer', min: 1, max: 4294967295 }]}><InputNumber min={1} max={4294967295} precision={0} /></Form.Item>
        <Form.Item name={['content', 'rebalance_schedule', 'target_ttl_seconds']} label="目标有效期（秒）" rules={[required, { type: 'integer', min: 1, max: 4294967295 }]}><InputNumber min={1} max={4294967295} precision={0} /></Form.Item>
      </Card>
      <Space wrap><Button type="primary" htmlType="submit" loading={mutation.isPending}>保存不可变配置</Button><Button onClick={dismiss}>取消</Button></Space>
    </Form>
  </Drawer>;
}
