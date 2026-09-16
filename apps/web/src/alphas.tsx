import { App, Alert, Button, Descriptions, Drawer, Form, Input, InputNumber, Modal, Space, Table, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent, isCounter } from './api';
import type { Schema } from './api';
import { ResourceSelect } from './resource-select';
import { ErrorNotice, NoData, Pager, QueryPanel, StateTag, useGuard, useOnline } from './ui';
import { counterRules } from './budget-fields';
import { RunDetail } from './runs';

type Alpha = Schema['AlphaView'];
type Version = Schema['AlphaVersionView'];
type Evaluation = Schema['EvaluationView'];
type Metric = Schema['MetricValueV1'];
function isAlphaEvaluation(value: Evaluation, project: string, version?: string): value is Evaluation & { subject_alpha_version_id: string } {
  return value.project_id === project && typeof value.subject_alpha_version_id === 'string'
    && value.subject_candidate_id === null && value.evaluation_kind === 'WALK_FORWARD'
    && (version === undefined || value.subject_alpha_version_id === version);
}

export function Alphas() {
  const [project, setProject] = useState<string>();
  return <Space orientation="vertical" size="large" className="full-width">
    <Typography.Title level={1}>Alpha</Typography.Title>
    <Alert type="info" showIcon title="研究登记、科学 PASS 和未过期都不等于可交付资格。"
      description="查看版本和正式 Validation 不启动任务。封存评估需另行明确提交，并使用运行中 Cycle 的冻结政策和预算；不授审批或交付。" />
    <ResourceSelect label="选择 Alpha 所属项目" value={project} onChange={setProject} queryKey={['alpha-projects']}
      load={async (cursor, signal) => {
        const page = dataOf(await api.GET('/api/v2/projects', { params: { query: { cursor, limit: 50 } }, signal }));
        return { next_cursor: page.next_cursor, items: page.items.map(item => ({ value: item.id, label: `${item.name} · ${item.id}` })) };
      }} />
    {project ? <AlphaList key={project} project={project} /> : <NoData text="请选择项目后查看已有 Alpha，不会自动选择或创建研究。" />}
  </Space>;
}

function AlphaList({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<Alpha>();
  const cursor = history.at(-1);
  const query = useQuery({ queryKey: ['alphas', project, cursor], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/alphas', {
    params: { query: { project_id: project, cursor, limit: 25 } }, signal,
    }));
    if (page.items.some(item => item.project_id !== project)) throw new Error('Alpha 记录不属于当前项目。');
    return page;
  } });
  if (selected) return <Space orientation="vertical" size="middle" className="full-width">
    <Button onClick={() => setSelected(undefined)}>返回 Alpha 列表</Button>
    <Versions key={selected.id} alpha={selected} />
  </Space>;
  return <Space orientation="vertical" size="middle" className="full-width">
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新 Alpha</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Alpha> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 700 }}
        locale={{ emptyText: <NoData text="本项目还没有 Alpha 登记；这不是无有效 Alpha 的科学结论。" /> }} columns={[
          { title: 'Alpha', key: 'name', render: (_, item) => <Button type="link" disabled={query.isError || query.isFetching} onClick={() => setSelected(item)}>{item.name}</Button> },
          { title: '登记状态（非当前资格）', key: 'state', render: (_, item) => <StateTag value={item.lifecycle} /> },
          { title: '活动版本', key: 'version', render: (_, item) => item.active_version ?? '未指定' },
          { title: '更新于', key: 'time', render: (_, item) => displayTime(item.updated_at) },
        ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
  </Space>;
}

function Versions({ alpha }: { alpha: Alpha }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<Version>();
  const cursor = history.at(-1);
  const query = useQuery({ queryKey: ['alpha-versions', alpha.id, cursor], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/alphas/{id}/versions', {
    params: { path: { id: alpha.id }, query: { cursor, limit: 25 } }, signal,
    }));
    if (page.items.some(item => item.alpha_id !== alpha.id || item.project_id !== alpha.project_id)) throw new Error('版本记录不属于当前 Alpha 或项目。');
    return page;
  } });
  if (selected) return <Space orientation="vertical" size="middle" className="full-width">
    <Button onClick={() => setSelected(undefined)}>返回版本列表</Button>
    <VersionDetail key={selected.id} alpha={alpha.id} project={alpha.project_id} number={selected.version} expectedId={selected.id} />
  </Space>;
  return <Space orientation="vertical" size="middle" className="full-width">
    <Typography.Title level={2}>{alpha.name} · 不可变版本</Typography.Title>
    <Typography.Text className="break-word">Alpha {alpha.id} · 列表读取时的活动版本 {alpha.active_version ?? '未指定'}</Typography.Text>
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新版本</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Version> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 850 }}
        locale={{ emptyText: <NoData text="尚无已登记版本。" /> }} columns={[
          { title: '版本', key: 'version', render: (_, item) => <Button type="link" disabled={query.isError || query.isFetching} onClick={() => setSelected(item)}>版本 {item.version}</Button> },
          { title: '数据来源', key: 'origin', render: (_, item) => item.origin ?? '来源未核验' },
          { title: '信号 / 单位', key: 'unit', render: (_, item) => `${item.signal_kind} / ${item.forecast_unit}` },
          { title: 'Horizon', key: 'horizon', render: (_, item) => `${item.horizon_kind} · ${item.horizon_value ?? '变量区间'}` },
          { title: '校准引用', key: 'calibration', render: (_, item) => item.calibration_id ?? '未登记校准' },
          { title: '创建于', key: 'created', render: (_, item) => displayTime(item.created_at) },
        ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
  </Space>;
}

function VersionDetail({ alpha, project, number, expectedId }: { alpha: string; project: string; number: string; expectedId: string }) {
  const [calibration, setCalibration] = useState(false);
  const [qualifications, setQualifications] = useState(false);
  const [evaluate, setEvaluate] = useState(false);
  const query = useQuery({ queryKey: ['alpha-version', alpha, number, project, expectedId], queryFn: async ({ signal }) => {
    const item = dataOf(await api.GET('/api/v2/alphas/{id}/versions/{version}', {
    params: { path: { id: alpha, version: number } }, signal,
    }));
    if (item.id !== expectedId || item.alpha_id !== alpha || item.project_id !== project || item.version !== number) throw new Error('返回的 Alpha 版本与原选择不一致。');
    return item;
  } });
  const version = query.isError ? undefined : query.data;
  return <QueryPanel pending={query.isPending} error={query.error} stale={!!version} reload={() => { void query.refetch(); }}>
    {version && <Space orientation="vertical" size="middle" className="full-width break-word">
      <Typography.Title level={2}>Alpha 版本 {version.version}</Typography.Title>
      <Descriptions column={1} items={[
        { key: 'id', label: '版本编号', children: version.id },
        { key: 'alpha', label: 'Alpha', children: version.alpha_id },
        { key: 'experiment', label: '原实验', children: version.experiment_id },
        { key: 'lineage', label: '根血缘', children: version.root_lineage_id },
        { key: 'code', label: '原 CODE', children: version.code_artifact_id },
        { key: 'model', label: '原 MODEL', children: version.model_artifact_id ?? '未登记模型' },
        { key: 'signal', label: '信号合同 / 单位', children: `${version.signal_contract_version} · ${version.signal_kind} · ${version.forecast_unit}` },
        { key: 'horizon', label: 'Horizon', children: `${version.horizon_kind} · ${version.horizon_value ?? '变量区间'}` },
        { key: 'origin', label: '原 Discovery 数据来源', children: version.origin ?? '来源未核验' },
        { key: 'calibration', label: '校准引用', children: version.calibration_id ?? '未登记校准，不能据此认为已校准' },
        { key: 'runtime', label: '冻结镜像', children: version.runtime_image_ref },
      ]} />
      {version.calibration_id && <Button onClick={() => setCalibration(true)}>查看冻结校准来源</Button>}
      <Button onClick={() => setQualifications(true)}>查看原资格历史</Button>
      <Button disabled={!version.model_artifact_id || (version.signal_kind === 'SCORE' && !version.calibration_id)} onClick={() => setEvaluate(true)}>请求封存评估</Button>
      <Evaluations key={version.id} version={version.id} project={version.project_id} />
      {calibration && version.calibration_id && <CalibrationDetail version={version.id} project={version.project_id} expectedId={version.calibration_id} close={() => setCalibration(false)} />}
      {qualifications && <Qualifications key={version.id} version={version.id} close={() => setQualifications(false)} />}
      {evaluate && <AlphaEvaluate version={version} close={() => setEvaluate(false)} />}
    </Space>}
  </QueryPanel>;
}

function AlphaEvaluate({ version, close }: { version: Version; close: () => void }) {
  type Request = Schema['AlphaEvaluateRequestV1'];
  type Fields = Omit<Request['limits'], 'schema_version' | 'experiments'> & { cycle_id: string };
  const [form] = Form.useForm<Fields>();
  const online = useOnline(); const client = useQueryClient(); const { modal } = App.useApp();
  const intent = useRef(new Intent()); const hadUnknown = useRef(false);
  const [submitted, setSubmitted] = useState<Request>();
  const [receipt, setReceipt] = useState<Schema['RunSnapshotV1']>();
  const [showRun, setShowRun] = useState(false);
  const cycleId: string | undefined = Form.useWatch('cycle_id', form);
  const cycle = useQuery({ queryKey: ['cycle', cycleId], enabled: !!cycleId, staleTime: 0,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/cycles/{id}', { params: { path: { id: cycleId! } }, signal })) });
  const frozen = useQuery({ queryKey: ['frozen-brief', cycle.data?.brief_id], enabled: !!cycle.data && !cycle.isError,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/briefs/{id}/execution-context', { params: { path: { id: cycle.data!.brief_id } }, signal })) });
  const mutation = useMutation({ mutationFn: async (body: Request) => dataOf(await api.POST('/api/v2/alpha-versions/{id}/evaluations', {
    params: { path: { id: version.id }, header: intent.current.headers('POST', `/api/v2/alpha-versions/${version.id}/evaluations`, body) }, body,
  })), onSuccess: async result => {
    setReceipt(result.resource); intent.current.clear();
    await Promise.all([client.invalidateQueries({ queryKey: ['runs'] }), client.invalidateQueries({ queryKey: ['cycles', version.project_id] })]);
  }, onError: error => {
    const rejected = error instanceof ApiFailure && ((!!error.problem && error.status >= 400 && error.status < 500) || error.code === 'OFFLINE');
    if (!rejected) hadUnknown.current = true;
    if (rejected && !hadUnknown.current) setSubmitted(undefined);
  } });
  useGuard(!receipt);
  const ready = cycle.data?.project_id === version.project_id && cycle.data.state === 'RUNNING' && !cycle.isError && !cycle.isFetching
    && frozen.data?.brief.project_id === version.project_id && !frozen.isError && !frozen.isFetching;
  const retry = submitted !== undefined && mutation.isError;
  function submit(value: Fields) {
    if (!online || mutation.isPending || submitted || !ready || !frozen.data) return;
    const context = frozen.data.execution_context;
    const request: Request = { schema_version: 1, cycle_id: value.cycle_id, policy_id: frozen.data.brief.content.evaluation_policy_id,
      input_set_id: context.sealed_input_set_id, runtime_id: context.runtime_id, expected_runtime_revision: context.runtime_revision,
      limits: { schema_version: 1, experiments: 0, cpu_seconds: value.cpu_seconds, wall_seconds: value.wall_seconds, memory_mib: value.memory_mib, output_bytes: value.output_bytes } };
    setSubmitted(request); mutation.mutate(request);
  }
  function dismiss() {
    if (mutation.isPending) return;
    if (retry) modal.confirm({ title: '关闭未确认的封存请求？', content: '关闭不撤销可能已登记的 Run。请先核对运行记录，不要创建另一次封存请求。', okText: '关闭并核对', cancelText: '保留原请求', onOk: close });
    else close();
  }
  if (showRun && receipt) return <RunDetail id={receipt.id} close={() => setShowRun(false)} />;
  return <Modal open title="确认请求封存评估" width={760} maskClosable={false} onCancel={dismiss} closable={!mutation.isPending}
    footer={receipt ? <Button onClick={close}>返回版本</Button> : undefined} cancelText="返回" okText={retry ? '重试同一请求' : '确认请求评估'}
    confirmLoading={mutation.isPending} okButtonProps={{ disabled: !online || (!retry && !ready) }}
    onOk={() => { if (!online || mutation.isPending || receipt) return; if (retry && submitted) mutation.mutate(submitted); else form.submit(); }}>
    <Space orientation="vertical" className="full-width" size="middle">
      <Typography.Paragraph className="break-word">原 Alpha 版本：{version.id}</Typography.Paragraph>
      <ErrorNotice error={mutation.error} />
      {retry && <Alert type="warning" showIcon title="请求结果尚未确认。" description="保留原内容与幂等键；重试不会重新选择 Cycle、政策或资源限额。" />}
      {receipt ? <>
        <Alert type="success" showIcon title="封存评估 Run 已登记。" description="202 不是科学通过、Reviewer 结论或资格；首次读取仍需原 Attempt 的机会预约。" />
        <Typography.Text className="break-word">Run {receipt.id} · {receipt.state}</Typography.Text>
        <Button onClick={() => setShowRun(true)}>查看评估运行</Button>
      </> : <>
        <Alert type="info" showIcon title="复用原模型和校准，不重收原编译试验。" description="新的 Run 仍占 Cycle 资源和封存机会，失败或取消不退已授机会。默认限额只是可修改草稿，不是实测用量。" />
        <Form form={form} layout="vertical" onFinish={submit} disabled={!online || mutation.isPending || submitted !== undefined}
          initialValues={{ cpu_seconds: '10', wall_seconds: 60, memory_mib: 1024, output_bytes: '1048576' }}>
          <Form.Item name="cycle_id" label="承担评估预算的 Cycle" rules={[{ required: true, message: '请选择本项目运行中的 Cycle。' }]}>
            <ResourceSelect label="选择评估 Cycle" queryKey={['alpha-evaluate-cycles', version.project_id]} load={async (cursor, signal) => {
              const page = dataOf(await api.GET('/api/v2/projects/{id}/cycles', { params: { path: { id: version.project_id }, query: { cursor, limit: 50 } }, signal }));
              return { next_cursor: page.next_cursor, items: page.items.filter(item => item.project_id === version.project_id).map(item => ({ value: item.id, label: `Cycle ${item.ordinal} · ${item.state} · ${item.id}`, disabled: item.state !== 'RUNNING' })) };
            }} />
          </Form.Item>
          <ErrorNotice error={cycle.error} /><ErrorNotice error={frozen.error} />
          {frozen.data && <Descriptions column={1} size="small" className="break-word" items={[
            { key: 'policy', label: '冻结政策', children: frozen.data.brief.content.evaluation_policy_id },
            { key: 'input', label: '冻结 Sealed 输入', children: frozen.data.execution_context.sealed_input_set_id },
            { key: 'runtime', label: '冻结 Runtime / 修订', children: `${frozen.data.execution_context.runtime_id} / ${frozen.data.execution_context.runtime_revision}` },
          ]} />}
          <div className="field-grid">
            <Form.Item name="cpu_seconds" label="CPU 秒数上限" rules={counterRules}><Input inputMode="numeric" maxLength={19} /></Form.Item>
            <Form.Item name="wall_seconds" label="墙钟秒数上限" rules={[{ required: true, type: 'integer', min: 1, max: 86400 }]}><InputNumber min={1} max={86400} precision={0} /></Form.Item>
            <Form.Item name="memory_mib" label="内存上限（MiB）" rules={[{ required: true, type: 'integer', min: 1, max: 1048576 }]}><InputNumber min={1} max={1048576} precision={0} /></Form.Item>
            <Form.Item name="output_bytes" label="输出字节上限" rules={[{ required: true }, { validator: (_: unknown, value: unknown) => typeof value === 'string' && isCounter(value, true) && BigInt(value) <= 67108864n ? Promise.resolve() : Promise.reject(new Error('请输入 1 至 67108864 的整数字符串。')) }]}><Input inputMode="numeric" maxLength={19} /></Form.Item>
          </div>
        </Form>
      </>}
    </Space>
  </Modal>;
}

function Qualifications({ version, close }: { version: string; close: () => void }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const cursor = history.at(-1);
  const query = useQuery({ queryKey: ['alpha-qualifications', version, cursor], staleTime: 0,
    queryFn: async ({ signal }) => {
      const page = dataOf(await api.GET('/api/v2/alpha-versions/{id}/qualifications', {
      params: { path: { id: version }, query: { cursor, limit: 25 } }, signal,
      }));
      if (page.items.some(item => item.alpha_version_id !== version)) throw new Error('资格历史不属于原 Alpha 版本。');
      return page;
    } });
  return <Drawer title="原资格历史" open width={900} onClose={close}>
    <Space orientation="vertical" size="middle" className="full-width">
      <Alert type="info" showIcon title="授予时间窗开放不等于当前可用于组合。"
        description="只展示原授予、期限和最早撤销。当前政策、Alpha 生命周期、REAL/PIT 和许可证仍须在组合准入时检查；本页不读取 Sealed 报告或授予交付权限。状态截至服务端观察时间，之后可能变化。" />
      <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新资格历史</Button>
      <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
        <Table<Schema['QualificationView']> rowKey="id" pagination={false} dataSource={query.data?.items ?? []}
          onHeaderRow={() => ({ tabIndex: 0 })}
          locale={{ emptyText: <NoData text="该版本没有资格授予记录。" /> }} scroll={{ x: 850 }} columns={[
            { title: '原资格 / 政策 / 评估', key: 'identity', render: (_, item) => <div className="break-word">{item.id}<br />{item.policy_id}<br />{item.qualifying_evaluation_id}</div> },
            { title: '授予时间窗', key: 'window', render: (_, item) => <>{item.grant_window_open ? '观察时刻开放' : '观察时刻未开放'}<br />{displayTime(item.granted_at)} 至 {displayTime(item.valid_until)}</> },
            { title: '最早撤销（含未来生效）', key: 'revocation', render: (_, item) => item.revocation ? <div className="break-word">{item.revocation.reason_code}<br />{displayTime(item.revocation.effective_at)}<br />{item.revocation.id}<br />{item.revocation.evidence_evaluation_id ?? '无证据评估引用'}</div> : '没有撤销记录' },
            { title: '服务端观察时间', key: 'checked', render: (_, item) => displayTime(item.checked_at) },
          ]} />
        <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
      </QueryPanel>
    </Space>
  </Drawer>;
}

function CalibrationDetail({ version, project, expectedId, close }: { version: string; project: string; expectedId: string; close: () => void }) {
  const [source, setSource] = useState(false);
  const query = useQuery({ queryKey: ['alpha-calibration', version, project, expectedId], queryFn: async ({ signal }) => {
    const value = dataOf(await api.GET('/api/v2/alpha-versions/{id}/calibration', {
    params: { path: { id: version } }, signal,
    }));
    if (value.id !== expectedId || value.alpha_version_id !== version || !isAlphaEvaluation(value.validation, project)) throw new Error('校准来源不属于原版本或项目。');
    return value;
  } });
  const value = query.isError ? undefined : query.data;
  if (source && value) return <EvaluationDetail id={value.validation.id} alpha={{ id: value.validation.subject_alpha_version_id!, project }} close={() => setSource(false)} />;
  return <Drawer title="冻结校准来源" open width={800} onClose={close}>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!value} reload={() => { void query.refetch(); }}>
      {value && <Space orientation="vertical" size="middle" className="full-width break-word">
        <Alert showIcon type="info" title="新版本附加校准，不继承源版本评估或资格。"
          description="原信号仍是 SCORE；只有应用冻结模型才得到预期收益。本页不读取模型字节、系数、标签或训练索引，也不重新拟合或延长原有效期。" />
        <Descriptions column={1} items={[
          { key: 'id', label: '校准编号', children: value.id },
          { key: 'target', label: '附加到版本', children: value.alpha_version_id },
          { key: 'source', label: '源版本（原 Validation 对象）', children: value.validation.subject_alpha_version_id },
          { key: 'method', label: '冻结方法 / 版本', children: `${value.estimator_kind} / ${value.estimator_version}` },
          { key: 'model', label: '冻结 MODEL 引用（不下载）', children: value.model_artifact_id },
          { key: 'input', label: '原 Validation 输入集', children: value.train_input_set_id },
          { key: 'fit', label: '最后训练标签可用时间（向上取整至微秒）', children: value.fit_end_available_at },
          { key: 'horizon', label: '预测 Horizon / 输出单位', children: `${value.horizon_kind} · ${value.horizon_value} · ${value.output_unit}` },
          { key: 'decision', label: '源评估执行 / 证据 / 科学决策', children: `${value.validation.execution_status} / ${value.validation.evidence_status} / ${value.validation.decision}` },
          { key: 'origin', label: '原数据来源', children: value.validation.origin },
          { key: 'valid', label: '源评估原有效期', children: value.validation.valid_until ? `${value.validation.valid_until} · ${value.validation.unexpired_at_read ? '读取时未过期' : '读取时已过期'}` : '未授予有效期' },
        ]} />
        <Button onClick={() => setSource(true)}>查看源版本原评估</Button>
      </Space>}
    </QueryPanel>
  </Drawer>;
}

function Evaluations({ version, project }: { version: string; project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<string>();
  const cursor = history.at(-1);
  const query = useQuery({ queryKey: ['alpha-evaluations', version, project, cursor], queryFn: async ({ signal }) => {
    const page = dataOf(await api.GET('/api/v2/alpha-versions/{id}/evaluations', {
    params: { path: { id: version }, query: { cursor, limit: 25 } }, signal,
    }));
    if (page.items.some(item => !isAlphaEvaluation(item, project, version))) throw new Error('评估不属于原 Alpha 版本。');
    return page;
  } });
  return <Space orientation="vertical" size="middle" className="full-width">
    <Typography.Title level={3}>已发表的正式 Validation</Typography.Title>
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新评估</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Evaluation> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 850 }}
        locale={{ emptyText: <NoData text="还没有可披露的正式 Validation 评估；不包含 Sealed，也不代表验证通过。" /> }} columns={[
          { title: '评估', key: 'id', render: (_, item) => <Button type="link" disabled={query.isError || query.isFetching} onClick={() => setSelected(item.id)}>评估 {item.id.slice(-8)}</Button> },
          { title: '执行状态', key: 'execution', render: (_, item) => <StateTag value={item.execution_status} /> },
          { title: '证据状态', key: 'evidence', render: (_, item) => <StateTag value={item.evidence_status} /> },
          { title: '科学决策（非资格）', key: 'decision', render: (_, item) => <StateTag value={item.decision} /> },
          { title: '来源', dataIndex: 'origin' },
          { title: '原有效期', key: 'validity', render: (_, item) => item.valid_until ? `${displayTime(item.valid_until)} · ${item.unexpired_at_read ? '读取时未过期' : '读取时已过期'}` : '未授予有效期' },
        ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {selected && <EvaluationDetail key={selected} id={selected} alpha={{ id: version, project }} close={() => setSelected(undefined)} />}
  </Space>;
}

export function EvaluationDetail({ id, close, candidate, alpha }: { id: string; close: () => void } & ({ candidate: { id: string; project: string }; alpha?: never } | { alpha: { id: string; project: string }; candidate?: never })) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const cursor = history.at(-1);
  const query = useQuery({ queryKey: ['evaluation', id, candidate, alpha], queryFn: async ({ signal }) => {
    const value = dataOf(await api.GET('/api/v2/evaluations/{id}', { params: { path: { id } }, signal }));
    if (value.id !== id || (candidate && (value.project_id !== candidate.project || value.subject_candidate_id !== candidate.id || value.subject_alpha_version_id !== null || !['FORWARD', 'PORTFOLIO'].includes(value.evaluation_kind)))) throw new Error('服务器返回了其他评估记录。');
    if (alpha && !isAlphaEvaluation(value, alpha.project, alpha.id)) throw new Error('服务器返回了其他 Alpha 的评估。');
    return value;
  } });
  const metrics = useQuery({ queryKey: ['evaluation-metrics', id, cursor], enabled: !!query.data && !query.isError,
    queryFn: async ({ signal }) => {
      const page = dataOf(await api.GET('/api/v2/evaluations/{id}/metrics', { params: { path: { id }, query: { cursor, limit: 25 } }, signal }));
      if (page.items.some(item => item.evaluation_id !== id)) throw new Error('指标不属于当前评估。');
      return page;
    } });
  const value = query.data;
  return <Drawer title={candidate ? '候选研究评估' : '正式 Validation 评估'} open width={1000} onClose={close}>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!value} reload={() => { void query.refetch(); }}>
      {value && <Space orientation="vertical" size="middle" className="full-width break-word">
        <Alert showIcon type="info" title="这是历史科学证据，不是资格或交付批准。" description="缺值和过期不会被补齐；本页不读取报告字节、标签、训练索引或 Sealed 数据。" />
        <Descriptions column={1} items={[
          { key: 'id', label: '评估编号', children: value.id },
          { key: 'kind', label: '评估类型', children: value.evaluation_kind },
          { key: 'status', label: '执行 / 证据 / 科学决策', children: `${value.execution_status} / ${value.evidence_status} / ${value.decision}` },
          { key: 'subject', label: candidate ? '原候选' : 'Alpha 版本', children: candidate ? value.subject_candidate_id : value.subject_alpha_version_id },
          { key: 'policy', label: '冻结政策', children: value.policy_id },
          { key: 'input', label: '原输入集', children: value.input_set_id },
          { key: 'run', label: '原运行', children: value.run_id },
          { key: 'origin', label: '数据来源', children: value.origin },
          { key: 'concluded', label: '原完成时间', children: displayTime(value.concluded_at) },
          { key: 'valid', label: '原有效期', children: value.valid_until ? displayTime(value.valid_until) : '未授予有效期' },
          { key: 'checked', label: '服务器检查时间', children: `${displayTime(value.checked_at)} · ${value.unexpired_at_read ? '当时未过期' : '当时没有未过期有效期'}` },
          { key: 'report', label: '完成报告引用（不下载）', children: value.report_artifact_id },
          { key: 'methods', label: '方法版本报告引用', children: value.method_versions_artifact_id },
        ]} />
        <QueryPanel pending={metrics.isPending} error={metrics.error} stale={!!metrics.data} reload={() => { void metrics.refetch(); }}>
          <Table<Metric> rowKey={item => `${item.evaluation_id}/${item.metric_code}/${item.scope}`} dataSource={metrics.data?.items} pagination={false} scroll={{ x: 1500 }} onHeaderRow={() => ({ tabIndex: 0 })}
            locale={{ emptyText: <NoData text="本评估没有发表指标；不能把缺失解释成0或通过。" /> }} columns={[
              { title: '指标 / Scope', key: 'name', render: (_, item) => `${item.metric_code} / ${item.scope}` },
              { title: '原始数值', key: 'value', render: (_, item) => item.value === null ? `缺值：${item.reason_code ?? item.status}` : String(item.value) },
              { title: '状态', dataIndex: 'status' }, { title: '单位', dataIndex: 'unit' }, { title: '频率', dataIndex: 'frequency' },
              { title: '方法 / 版本', key: 'method', render: (_, item) => `${item.method_id} / ${item.method_version}` },
              { title: '样本数', dataIndex: 'observation_count' },
              { title: '原期间', key: 'period', render: (_, item) => `${displayTime(item.period_start)} — ${displayTime(item.period_end)}` },
              { title: '年化因子', key: 'factor', render: (_, item) => item.annualization_factor === null ? '不适用' : String(item.annualization_factor) },
              { title: '原来源产物', dataIndex: 'source_artifact_id' },
            ]} />
          <Pager history={history} next={metrics.data?.next_cursor} loading={metrics.isFetching} move={setHistory} />
        </QueryPanel>
      </Space>}
    </QueryPanel>
  </Drawer>;
}
