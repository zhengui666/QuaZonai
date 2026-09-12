import { Alert, Button, Descriptions, Drawer, Space, Table, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api, dataOf, displayTime } from './api';
import type { Schema } from './api';
import { ResourceSelect } from './resource-select';
import { NoData, Pager, QueryPanel, StateTag } from './ui';

type Alpha = Schema['AlphaView'];
type Version = Schema['AlphaVersionView'];
type Evaluation = Schema['EvaluationView'];
type Metric = Schema['MetricValueV1'];

export function Alphas() {
  const [project, setProject] = useState<string>();
  return <Space orientation="vertical" size="large" className="full-width">
    <Typography.Title level={1}>Alpha</Typography.Title>
    <Alert type="info" showIcon title="研究登记、科学 PASS 和未过期都不等于可交付资格。"
      description="只查看原版本和正式 Validation 记录；不会启动研究、校准、Sealed 读取、审批或交付。" />
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
  const query = useQuery({ queryKey: ['alphas', project, cursor], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/alphas', {
    params: { query: { project_id: project, cursor, limit: 25 } }, signal,
  })) });
  if (selected) return <Space orientation="vertical" size="middle" className="full-width">
    <Button onClick={() => setSelected(undefined)}>返回 Alpha 列表</Button>
    <Versions key={selected.id} alpha={selected} />
  </Space>;
  return <Space orientation="vertical" size="middle" className="full-width">
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新 Alpha</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Alpha> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 700 }}
        locale={{ emptyText: <NoData text="本项目还没有 Alpha 登记；这不是无有效 Alpha 的科学结论。" /> }} columns={[
          { title: 'Alpha', key: 'name', render: (_, item) => <Button type="link" onClick={() => setSelected(item)}>{item.name}</Button> },
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
  const [selected, setSelected] = useState<string>();
  const cursor = history.at(-1);
  const query = useQuery({ queryKey: ['alpha-versions', alpha.id, cursor], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/alphas/{id}/versions', {
    params: { path: { id: alpha.id }, query: { cursor, limit: 25 } }, signal,
  })) });
  if (selected) return <Space orientation="vertical" size="middle" className="full-width">
    <Button onClick={() => setSelected(undefined)}>返回版本列表</Button>
    <VersionDetail key={`${alpha.id}/${selected}`} alpha={alpha.id} number={selected} />
  </Space>;
  return <Space orientation="vertical" size="middle" className="full-width">
    <Typography.Title level={2}>{alpha.name} · 不可变版本</Typography.Title>
    <Typography.Text className="break-word">Alpha {alpha.id} · 列表读取时的活动版本 {alpha.active_version ?? '未指定'}</Typography.Text>
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新版本</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Version> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 850 }}
        locale={{ emptyText: <NoData text="尚无已登记版本。" /> }} columns={[
          { title: '版本', key: 'version', render: (_, item) => <Button type="link" onClick={() => setSelected(item.version)}>版本 {item.version}</Button> },
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

function VersionDetail({ alpha, number }: { alpha: string; number: string }) {
  const query = useQuery({ queryKey: ['alpha-version', alpha, number], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/alphas/{id}/versions/{version}', {
    params: { path: { id: alpha, version: number } }, signal,
  })) });
  const version = query.data;
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
      <Evaluations key={version.id} version={version.id} />
    </Space>}
  </QueryPanel>;
}

function Evaluations({ version }: { version: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<string>();
  const cursor = history.at(-1);
  const query = useQuery({ queryKey: ['alpha-evaluations', version, cursor], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/alpha-versions/{id}/evaluations', {
    params: { path: { id: version }, query: { cursor, limit: 25 } }, signal,
  })) });
  return <Space orientation="vertical" size="middle" className="full-width">
    <Typography.Title level={3}>已发表的正式 Validation</Typography.Title>
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新评估</Button>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Evaluation> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 850 }}
        locale={{ emptyText: <NoData text="还没有可披露的正式 Validation 评估；不包含 Sealed，也不代表验证通过。" /> }} columns={[
          { title: '评估', key: 'id', render: (_, item) => <Button type="link" onClick={() => setSelected(item.id)}>评估 {item.id.slice(-8)}</Button> },
          { title: '执行状态', key: 'execution', render: (_, item) => <StateTag value={item.execution_status} /> },
          { title: '证据状态', key: 'evidence', render: (_, item) => <StateTag value={item.evidence_status} /> },
          { title: '科学决策（非资格）', key: 'decision', render: (_, item) => <StateTag value={item.decision} /> },
          { title: '来源', dataIndex: 'origin' },
          { title: '原有效期', key: 'validity', render: (_, item) => item.valid_until ? `${displayTime(item.valid_until)} · ${item.unexpired_at_read ? '读取时未过期' : '读取时已过期'}` : '未授予有效期' },
        ]} />
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {selected && <EvaluationDetail key={selected} id={selected} close={() => setSelected(undefined)} />}
  </Space>;
}

function EvaluationDetail({ id, close }: { id: string; close: () => void }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const cursor = history.at(-1);
  const query = useQuery({ queryKey: ['evaluation', id], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/evaluations/{id}', { params: { path: { id } }, signal })) });
  const metrics = useQuery({ queryKey: ['evaluation-metrics', id, cursor], enabled: !!query.data && !query.isError,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/evaluations/{id}/metrics', { params: { path: { id }, query: { cursor, limit: 25 } }, signal })) });
  const value = query.data;
  return <Drawer title="正式 Validation 评估" open width={1000} onClose={close}>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!value} reload={() => { void query.refetch(); }}>
      {value && <Space orientation="vertical" size="middle" className="full-width break-word">
        <Alert showIcon type="info" title="这是历史科学证据，不是资格或交付批准。" description="缺值和过期不会被补齐；本页不读取报告字节、标签、训练索引或 Sealed 数据。" />
        <Descriptions column={1} items={[
          { key: 'id', label: '评估编号', children: value.id },
          { key: 'status', label: '执行 / 证据 / 科学决策', children: `${value.execution_status} / ${value.evidence_status} / ${value.decision}` },
          { key: 'alpha', label: 'Alpha 版本', children: value.subject_alpha_version_id },
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
