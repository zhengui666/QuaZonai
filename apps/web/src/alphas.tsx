import { Button, Descriptions, Drawer, Skeleton, Space, Table, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { lazy, Suspense, useState } from 'react';
import { api, dataOf, displayTime } from './api';
import type { Schema } from './api';
import { ResourceSelect } from './resource-select';
import { NoData, Pager, QueryPanel, StateTag } from './ui';
import { isForecastAlphaVersion } from './producer-views';

const EquityCurve = lazy(() => import('./equity-curve'));

type Alpha = Schema['AlphaView'];
type Version = Schema['AlphaVersionEnvelopeV2'];
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
    <Typography.Paragraph>查看原始版本与评估证据。研究操作由外部 Agent 通过 CLI/Skill 执行。</Typography.Paragraph>
    <ResourceSelect label="选择 Alpha 所属项目" value={project} onChange={setProject} queryKey={['alpha-projects']}
      load={async (cursor, signal) => {
        const page = dataOf(await api.GET('/api/v2/projects', { params: { query: { cursor, limit: 50 } }, signal }));
        return { next_cursor: page.next_cursor, items: page.items.map(item => ({ value: item.id, label: `${item.name} · ${item.id}` })) };
      }} />
    {project ? <AlphaList key={project} project={project} /> : <NoData text="请选择项目" />}
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
        locale={{ emptyText: <NoData text="暂无 Alpha" /> }} columns={[
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
          { title: '数据来源', key: 'origin', render: (_, item) => isForecastAlphaVersion(item) ? item.origin ?? '来源未核验' : '此版本未提供来源结论' },
          { title: '信号 / 单位', key: 'unit', render: (_, item) => isForecastAlphaVersion(item) ? `${item.signal_kind} / ${item.forecast_unit}` : item.output_kind },
          { title: 'Horizon', key: 'horizon', render: (_, item) => isForecastAlphaVersion(item) ? `${item.horizon_kind} · ${item.horizon_value ?? '变量区间'}` : '不适用（目标权重）' },
          { title: '校准引用', key: 'calibration', render: (_, item) => isForecastAlphaVersion(item) ? item.calibration_id ?? '未登记校准' : '不适用（目标权重）' },
          { title: '创建于', key: 'created', render: (_, item) => displayTime(item.created_at) },
        ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
  </Space>;
}

export function VersionDetail({ alpha, project, number, expectedId }: { alpha: string; project: string; number: string; expectedId: string }) {
  const [calibration, setCalibration] = useState(false);
  const [qualifications, setQualifications] = useState(false);
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
      ]} />
      {isForecastAlphaVersion(version) ? <>
        <Descriptions column={1} items={[
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
        <Evaluations key={version.id} version={version.id} project={version.project_id} />
        {calibration && version.calibration_id && <CalibrationDetail version={version.id} project={version.project_id} expectedId={version.calibration_id} close={() => setCalibration(false)} />}
        {qualifications && <Qualifications key={version.id} version={version.id} close={() => setQualifications(false)} />}
      </> : <>
        <Descriptions column={1} items={[
          { key: 'output', label: '策略输出', children: version.output_kind },
          { key: 'code', label: '原 CODE', children: version.policy.code_artifact_id },
          { key: 'model', label: '原 MODEL', children: version.policy.model_artifact_id },
          { key: 'parameters', label: '冻结参数', children: version.policy.parameter_artifact_id },
          { key: 'runtime', label: '冻结镜像', children: version.policy.runtime_image_ref },
        ]} />
        <Typography.Paragraph>目标权重策略不提供预测单位、Horizon 或校准资格；已登记版本不代表科学评估通过。</Typography.Paragraph>
        <Typography.Title level={3}>服务器保存的冻结策略</Typography.Title>
        <pre className="break-word" style={{ whiteSpace: 'pre-wrap' }}>{JSON.stringify(version.policy, null, 2)}</pre>
      </>}
    </Space>}
  </QueryPanel>;
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
        locale={{ emptyText: <NoData text="暂无 Validation 评估" /> }} columns={[
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
        {candidate && !query.isError && value.evaluation_kind === 'PORTFOLIO' && <Suspense fallback={<Skeleton active />}><EquityCurve key={value.id} evaluation={value} /></Suspense>}
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
          { key: 'checked', label: '服务器检查时间', children: `${displayTime(value.checked_at)} · ${value.unexpired_at_read ? '当时未过期' : '无有效期'}` },
          { key: 'report', label: '完成报告引用（不下载）', children: value.report_artifact_id },
          { key: 'methods', label: '方法版本报告引用', children: value.method_versions_artifact_id },
        ]} />
        <QueryPanel pending={metrics.isPending} error={metrics.error} stale={!!metrics.data} reload={() => { void metrics.refetch(); }}>
          <Table<Metric> rowKey={item => `${item.evaluation_id}/${item.metric_code}/${item.scope}`} dataSource={metrics.data?.items} pagination={false} scroll={{ x: 1500 }} onHeaderRow={() => ({ tabIndex: 0 })}
            locale={{ emptyText: <NoData text="暂无指标" /> }} columns={[
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
