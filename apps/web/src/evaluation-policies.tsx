import { Alert, App, Button, Checkbox, Drawer, Form, Input, InputNumber, Select, Space, Table, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, dataOf, displayTime, Intent, isCounter, isDecimal } from './api';
import type { Schema } from './api';
import { uuidPattern } from './auth';
import { counterRules } from './budget-fields';
import { ErrorNotice, NoData, Pager, QueryPanel, useGuard, useOnline } from './ui';

type Policy = Schema['EvaluationPolicyView'];
type Request = Schema['EvaluationPolicyCreate'];
type Fields = Omit<Request, 'schema_version' | 'project_id'> & { use_portfolio?: boolean; use_study?: boolean; use_manual_study?: boolean };
type RequirementGroup = 'metric_requirements' | 'sealed_metric_requirements' | 'portfolio_metric_requirements' | 'promotion_metric_requirements' | 'degradation_metric_requirements';
const required = { required: true, message: '请填写此项。' };
const textRules = [required, { max: 120, whitespace: true }];
const ids = [required, { pattern: uuidPattern, message: '需要现有记录的 UUIDv7 编号。' }];
const countRules = [required, { validator: async (_: unknown, value: unknown) => { if (typeof value !== 'string' || !isCounter(value)) throw new Error('需要非负 bigint 整数字符串。'); } }];
const optionalCount = [{ validator: async (_: unknown, value: unknown) => { if (value != null && value !== '' && (typeof value !== 'string' || !isCounter(value, true))) throw new Error('需要正整数字符串，或留空。'); } }];
const optionalDecimal = [{ validator: async (_: unknown, value: unknown) => { if (value != null && value !== '' && !isDecimal(value)) throw new Error('需要精确十进制字符串，或留空。'); } }];
const blank = (value: string | null | undefined) => value == null || value === '' ? null : value;

function request(project: string, values: Fields): Request {
  const { use_portfolio, use_study, use_manual_study, ...value } = values;
  const metrics = (items: Schema['MetricRequirementV1'][]) => items.map(item => ({ ...item, schema_version: 1 as const, required: item.required === true, threshold_low: blank(item.threshold_low), threshold_high: blank(item.threshold_high) }));
  const split = value.split_policy;
  return { ...value, schema_version: 1, project_id: project,
    required_capabilities: value.required_capabilities ?? [],
    metric_requirements: metrics(value.metric_requirements), sealed_metric_requirements: metrics(value.sealed_metric_requirements),
    portfolio_metric_requirements: use_portfolio ? metrics(value.portfolio_metric_requirements ?? []) : null,
    portfolio_study_plan: use_portfolio && use_study && value.portfolio_study_plan ? {
      ...value.portfolio_study_plan, schema_version: 1,
      manual_cutoffs: use_manual_study ? value.portfolio_study_plan.manual_cutoffs ?? [] : null,
    } : null,
    split_policy: { ...split, schema_version: 1, interval_validation_required: true,
      step_size: split.kind === 'WALK_FORWARD' ? blank(split.step_size) : null,
      group_count: split.kind === 'CPCV_FIXED_HORIZON' ? split.group_count ?? null : null,
      test_group_count: split.kind === 'CPCV_FIXED_HORIZON' ? split.test_group_count ?? null : null,
      label_horizon_observations: blank(split.label_horizon_observations),
    },
  };
}

export function EvaluationPolicies({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<string>(); const [creating, setCreating] = useState(false); const online = useOnline();
  const query = useQuery({ queryKey: ['evaluation-policies', project, history.at(-1)], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/evaluation-policies', { params: { query: { project_id: project, cursor: history.at(-1), limit: 25 } }, signal }));
    if (page.items.some(item => item.project_id !== project)) throw new Error('政策不属于当前项目。');
    return page;
  } });
  return <Space orientation="vertical" className="full-width" size="middle">
    <Alert showIcon type="info" title="先冻结独立要求，再开展研究" description="新政策不修改历史版本，不自动启动实验或授予资格；方法能力仍须实际准入核查。" />
    <Space wrap><Button type="primary" disabled={!online} onClick={() => setCreating(true)}>新建评估政策</Button><Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新政策</Button></Space>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Policy> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 700 }} onHeaderRow={() => ({ tabIndex: 0 })} locale={{ emptyText: <NoData text="尚无评估政策，不填充默认合格阈值。" /> }} columns={[
        { title: '版本', key: 'version', render: (_, item) => <Button type="link" disabled={query.isError} onClick={() => setSelected(item.id)}>政策 v{item.version}</Button> },
        { title: '研究问题', dataIndex: 'question' }, { title: '选择评估', key: 'kind', render: (_, item) => item.selection_rule.evaluation_kind },
        { title: '组合要求', key: 'portfolio', render: (_, item) => item.portfolio_metric_requirements === null ? '未定义，不能授予组合 PASS' : `${item.portfolio_metric_requirements.length} 项独立要求` },
        { title: '组合研究计划', key: 'study', render: (_, item) => item.portfolio_study_plan === null ? '未定义' : '已冻结原输入与起点' },
        { title: '创建于', dataIndex: 'created_at', render: displayTime },
      ]} />
    </QueryPanel>
    <Pager history={history} next={query.isError ? undefined : query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    {selected && <Detail key={selected} id={selected} project={project} close={() => setSelected(undefined)} />}
    {creating && <Editor project={project} close={() => setCreating(false)} />}
  </Space>;
}

function Detail({ id, project, close }: { id: string; project: string; close: () => void }) {
  const query = useQuery({ queryKey: ['evaluation-policy', project, id], queryFn: async ({ signal }) => {
    const value = dataOf(await api.GET('/api/v2/evaluation-policies/{id}', { params: { path: { id } }, signal }));
    if (value.id !== id || value.project_id !== project) throw new Error('服务器返回了其他政策。');
    return value;
  } });
  return <Drawer title="不可变评估政策" open width={850} onClose={close}>
    <Alert showIcon type="info" title="原版本只读，修改要求需新建政策" description="null 保持未定义，不复制其他指标组，不改写原有效期。这里没有 Sealed 数据或报告字节。" />
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      {query.data && <pre tabIndex={0} aria-label="原完整评估政策" className="break-word" style={{ whiteSpace: 'pre-wrap' }}>{JSON.stringify(query.data, null, 2)}</pre>}
    </QueryPanel>
  </Drawer>;
}

export function Requirements({ name, title }: { name: RequirementGroup; title: string }) {
  return <section aria-label={title}>
    <Typography.Title level={3}>{title}</Typography.Title>
    <Typography.Paragraph>GT/GE 只填下端点，LT/LE 只填上端点，BETWEEN 填闭区间两端。至少一项必需指标；方法名称不代表已支持。</Typography.Paragraph>
    <Form.List name={name}>{(fields, { add, remove }) => <>
      {fields.map(field => <Space key={field.key} orientation="vertical" className="full-width">
        <Form.Item name={[field.name, 'metric_code']} label={`${title} ${field.name + 1} 指标代码`} rules={textRules}><Input /></Form.Item>
        <Form.Item name={[field.name, 'scope']} label={`${title} ${field.name + 1} Scope`} rules={textRules}><Input /></Form.Item>
        <Form.Item name={[field.name, 'comparator']} label={`${title} ${field.name + 1} 比较器`} rules={[required]}><Select options={['GT', 'GE', 'LT', 'LE', 'BETWEEN'].map(value => ({ value, label: value }))} /></Form.Item>
        {(['threshold_low', 'threshold_high'] as const).map((key, index) => <Form.Item key={key} name={[field.name, key]} label={`${title} ${field.name + 1} ${index === 0 ? '下' : '上'}端点`} rules={optionalDecimal}><Input inputMode="decimal" /></Form.Item>)}
        <Form.Item name={[field.name, 'minimum_observations']} label={`${title} ${field.name + 1} 最少样本`} rules={countRules}><Input inputMode="numeric" /></Form.Item>
        <Form.Item name={[field.name, 'method_allowlist']} label={`${title} ${field.name + 1} 方法白名单`} rules={[{ required: true, type: 'array', min: 1, max: 64 }]}><Select mode="tags" /></Form.Item>
        <Form.Item name={[field.name, 'required']} valuePropName="checked"><Checkbox>{title} {field.name + 1} 必需指标</Checkbox></Form.Item>
        <Button disabled={fields.length <= 1} onClick={() => remove(field.name)}>删除{title}指标 {field.name + 1}</Button>
      </Space>)}
      <Button disabled={fields.length >= 64} onClick={() => add({ required: true })}>添加{title}指标</Button>
    </>}</Form.List>
  </section>;
}

function Editor({ project, close }: { project: string; close: () => void }) {
  const [form] = Form.useForm<Fields>(); const [dirty, setDirty] = useState(false); const intent = useRef(new Intent());
  const kind = Form.useWatch(['split_policy', 'kind'], form); const portfolio = Form.useWatch('use_portfolio', form);
  const study = Form.useWatch('use_study', form); const manual = Form.useWatch('use_manual_study', form);
  const client = useQueryClient(); const online = useOnline(); const { modal, message } = App.useApp();
  const mutation = useMutation({ mutationFn: async (values: Fields) => {
    const body = request(project, values);
    return dataOf(await api.POST('/api/v2/evaluation-policies', { body, params: { header: intent.current.headers('POST', '/api/v2/evaluation-policies', body) } }));
  }, onSuccess: async result => {
    intent.current.clear(); setDirty(false); await client.invalidateQueries({ queryKey: ['evaluation-policies', project] });
    await message.success(result.replayed ? '已读取原政策回执，未重复创建。' : '已保存不可变政策，未启动研究。'); close();
  } });
  useGuard(dirty || mutation.isPending);
  function dismiss() {
    if (mutation.isPending) return;
    if (!dirty) return close();
    modal.confirm({ title: '放弃未保存的评估政策？', content: '不会撤销已发送请求；结果未知时保留原输入重试。', okText: '放弃修改', cancelText: '继续编辑', onOk: close });
  }
  return <Drawer title="新建不可变评估政策" open width={900} onClose={dismiss} closable={!mutation.isPending} maskClosable={!mutation.isPending}>
    <Alert showIcon type="info" title="三组阈值独立冻结，不从数据推断" description="所有编号必须指向已有同项目来源。保存不是科学 PASS；比较器关系、当前许可和原生能力仍由服务器检查。" />
    <ErrorNotice error={mutation.error} />
    <Form form={form} layout="vertical" disabled={!online || mutation.isPending} onValuesChange={() => setDirty(true)} onFinish={values => mutation.mutate(values)} initialValues={{ metric_requirements: [{ required: true }], sealed_metric_requirements: [{ required: true }], portfolio_metric_requirements: [{ required: true }] }}>
      <Form.Item name="question" label="研究问题" rules={[required, { max: 8000, whitespace: true }]}><Input.TextArea rows={3} /></Form.Item>
      <Form.Item name="comparison_input_set_id" label="冻结比较输入编号" rules={ids}><Input /></Form.Item>
      <Form.Item name="execution_assumptions_id" label="原执行假设编号" rules={ids}><Input /></Form.Item>
      <Typography.Title level={2}>原选择规则</Typography.Title>
      <Form.Item name={['selection', 'evaluation_kind']} label="选择评估类型" rules={[required]}><Select options={['WALK_FORWARD', 'SEALED'].map(value => ({ value, label: value }))} /></Form.Item>
      {([['metric_code', '选择指标代码'], ['metric_scope', '选择指标 Scope'], ['method_id', '选择方法'], ['method_version', '选择方法版本'], ['unit', '选择单位'], ['frequency', '选择频率']] as const).map(([key, label]) => <Form.Item key={key} name={['selection', key]} label={label} rules={textRules}><Input /></Form.Item>)}
      <Form.Item name={['selection', 'direction']} label="选择方向" rules={[required]}><Select options={['MAXIMIZE', 'MINIMIZE'].map(value => ({ value, label: value }))} /></Form.Item>
      <Form.Item name={['selection', 'candidate_count']} label="选择候选数" rules={[required, { type: 'integer', min: 1, max: 65535 }]}><InputNumber min={1} max={65535} precision={0} /></Form.Item>
      <Typography.Title level={2}>原切分政策</Typography.Title>
      <Form.Item name={['split_policy', 'kind']} label="切分方法" rules={[required]}><Select options={['WALK_FORWARD', 'CPCV_FIXED_HORIZON'].map(value => ({ value, label: value }))} /></Form.Item>
      {([['train_size', '训练样本数'], ['test_size', '测试样本数']] as const).map(([key, label]) => <Form.Item key={key} name={['split_policy', key]} label={label} rules={counterRules}><Input inputMode="numeric" /></Form.Item>)}
      {kind === 'WALK_FORWARD' && <Form.Item name={['split_policy', 'step_size']} label="窗口步长" rules={counterRules}><Input inputMode="numeric" /></Form.Item>}
      {kind === 'CPCV_FIXED_HORIZON' && (['group_count', 'test_group_count'] as const).map((key, index) => <Form.Item key={key} name={['split_policy', key]} label={index === 0 ? '总组数' : '测试组数'} rules={[required, { type: 'integer', min: index === 0 ? 2 : 1, max: index === 0 ? 65535 : 65534 }]}><InputNumber min={index === 0 ? 2 : 1} max={index === 0 ? 65535 : 65534} precision={0} /></Form.Item>)}
      {([['purge_observations', '清除观察数'], ['embargo_observations', '隔离观察数']] as const).map(([key, label]) => <Form.Item key={key} name={['split_policy', key]} label={label} rules={countRules}><Input inputMode="numeric" /></Form.Item>)}
      <Form.Item name={['split_policy', 'label_horizon_observations']} label="标签固定跨度（CPCV 必填）" rules={kind === 'CPCV_FIXED_HORIZON' ? counterRules : optionalCount}><Input inputMode="numeric" /></Form.Item>
      <Form.Item name={['split_policy', 'sealed_revision_id']} label="原 Sealed 数据版本编号" rules={ids}><Input /></Form.Item>
      <Typography.Paragraph>始终要求实际区间校验，不在网页读取 Sealed 数据。</Typography.Paragraph>
      <Requirements name="metric_requirements" title="Validation" />
      <Requirements name="sealed_metric_requirements" title="Sealed" />
      <Form.Item name="use_portfolio" valuePropName="checked"><Checkbox disabled={!online || mutation.isPending}>定义独立组合要求</Checkbox></Form.Item>
      {portfolio ? <Requirements name="portfolio_metric_requirements" title="组合" /> : <Alert showIcon type="info" title="组合要求为 null，不能授予组合 PASS" />}
      {portfolio && <>
        <Form.Item name="use_study" valuePropName="checked"><Checkbox disabled={!online || mutation.isPending}>冻结组合研究计划</Checkbox></Form.Item>
        {study && <section aria-label="组合研究计划">
          <Typography.Paragraph>引用已冻结的 PORTFOLIO 输入；结束固定为原数据结束，不在运行后挑选窗口。时间使用带时区的 RFC3339，最多六位小数；保留原文本精度，由服务器校验。</Typography.Paragraph>
          <Form.Item name={['portfolio_study_plan', 'input_set_id']} label="组合研究输入编号" rules={ids}><Input /></Form.Item>
          <Form.Item name={['portfolio_study_plan', 'evaluation_start']} label="组合研究起点" rules={[required]}><Input placeholder="2026-09-14T00:00:00.000001Z" /></Form.Item>
          <Form.Item name="use_manual_study" valuePropName="checked"><Checkbox disabled={!online || mutation.isPending}>冻结手动调仓时点</Checkbox></Form.Item>
          {manual && <Form.List name={['portfolio_study_plan', 'manual_cutoffs']} initialValue={['', '']}>{(fields, { add, remove }) => <>
            {fields.map((field, index) => <Space key={field.key} align="baseline">
              <Form.Item name={field.name} label={`研究时点 ${index + 1}`} rules={[required]}><Input /></Form.Item>
              <Button disabled={fields.length <= 2} onClick={() => remove(field.name)}>删除研究时点 {index + 1}</Button>
            </Space>)}
            <Button disabled={fields.length >= 256} onClick={() => add('')}>添加研究时点</Button>
          </>}</Form.List>}
          <Typography.Paragraph>手动时点必须从原起点开始且严格递增；非手动计划不携带该列表。保存不启动研究或授予 PASS。</Typography.Paragraph>
        </section>}
      </>}
      <Typography.Title level={2}>共同证据限制</Typography.Title>
      {([['minimum_observations', '总体最少样本数'], ['maximum_sealed_uses_per_lineage', '每血缘最多 Sealed 使用次数']] as const).map(([key, label]) => <Form.Item key={key} name={key} label={label} rules={[required, { type: 'integer', min: 1, max: 2147483647 }]}><InputNumber min={1} max={2147483647} precision={0} /></Form.Item>)}
      <Form.Item name="maximum_missing_fraction" label="最大缺失比例（0 至 1）" rules={[required, ...optionalDecimal]}><Input inputMode="decimal" /></Form.Item>
      <Form.Item name="require_real_data" label="是否要求真实数据" rules={[required]}><Select options={[{ value: true, label: '要求 REAL' }, { value: false, label: '允许非 REAL 研究（不授真实资格）' }]} /></Form.Item>
      <Form.Item name="required_capabilities" label="必需能力名称（可留空）" rules={[{ type: 'array', max: 64 }]}><Select mode="tags" /></Form.Item>
      <Form.Item name="validity_seconds" label="证据有效秒数" rules={counterRules}><Input inputMode="numeric" /></Form.Item>
      <Space wrap><Button type="primary" htmlType="submit" loading={mutation.isPending}>保存不可变评估政策</Button><Button onClick={dismiss}>取消</Button></Space>
    </Form>
  </Drawer>;
}
