import { Alert, Button, Card, Descriptions, Drawer, Space, Table, Tag, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api, dataOf, displayTime } from './api';
import type { Schema } from './api';
import { caseCounts, measured, observedIdentity } from './agent-evaluation-data';
import type { AgentCase, AgentReport } from './agent-evaluation-data';
import { NoData, Pager, QueryPanel } from './ui';

const reportPath = '/api/v2/artifacts/{id}/agent-evaluation';
const colors = { PASS: 'success', FAIL: 'error', BLOCKED: 'warning', UNRUN: 'default' } as const;
function Outcome({ status }: { status: AgentReport['status'] }) { return <Tag color={colors[status]}>{status}</Tag>; }
function Hash({ value }: { value: string }) { return <Typography.Text className="break-word" copyable>{value}</Typography.Text>; }

export function AgentEvaluations({ projectId }: { projectId: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<string>();
  const query = useQuery({ queryKey: ['agent-evaluation-artifacts', projectId, history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/artifacts', {
    params: { query: { project_id: projectId, cursor: history.at(-1), limit: 25 } }, signal,
  })) });
  return <Space orientation="vertical" size="middle" className="full-width">
    <Alert type="info" showIcon title="Agent 评估报告" description="查看上传的运行器证据。PASS 表示报告内断言通过；上传、合同校验和模型执行成功都不授予独立科学资格。" />
    <Space wrap><Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新</Button></Space>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['ArtifactView']> rowKey="id" pagination={false} scroll={{ x: 680 }}
        dataSource={query.data?.items.filter(item => item.kind === 'REPORT')}
        locale={{ emptyText: <NoData text="本页暂无 REPORT 产物；可继续翻页，报告由外部 Agent 通过 CLI/Skill 登记" /> }}
        columns={[
          { title: '报告产物', key: 'id', render: (_, row) => <Button type="link" className="table-title" onClick={() => setSelected(row.id)}>{row.id}</Button> },
          { title: '上传于', key: 'created', render: (_, row) => displayTime(row.created_at) },
          { title: '原始大小', key: 'size', render: (_, row) => `${row.byte_count} bytes` },
        ]} />
      <Typography.Text type="secondary">按项目产物分页，仅显示本页 REPORT。普通报告可保留，但不支持 Agent 评估视图。</Typography.Text>
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    <Drawer open={!!selected} size="large" title="Agent 评估详情" onClose={() => setSelected(undefined)} destroyOnHidden>
      {selected && <ReportDetail key={selected} id={selected} />}
    </Drawer>
  </Space>;
}
export function ReportDetail({ id }: { id: string }) {
  const query = useQuery({ queryKey: ['agent-evaluation', id], queryFn: async ({ signal }) => dataOf(await api.GET(reportPath, { params: { path: { id } }, signal })) });
  return <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
    {query.data && <ReportView report={query.data} />}
  </QueryPanel>;
}
export function ReportView({ report }: { report: AgentReport }) {
  const counts = caseCounts(report.cases);
  return <Space orientation="vertical" size="middle" className="full-width">
    <Alert showIcon type={report.mode === 'LIVE' ? 'info' : 'warning'}
      title={report.mode === 'LIVE' ? '运行器声明：LIVE' : 'PROTOCOL_ONLY：协议测试，不是实际模型评估'}
      description="报告由上传者提供；此页面不证明执行独立性、数据保密性或科学资格。未知用量与费用不会补零。" />
    <Space wrap><Outcome status={report.status} />{Object.entries(counts).map(([status, count]) => <Tag key={status}>{status}: {count}</Tag>)}</Space>
    <Descriptions bordered size="small" column={1} items={[
      { key: 'runner', label: '运行器', children: `${report.runner.name} / ${report.runner.version}` },
      { key: 'subject', label: '评估对象', children: `${report.subject.name} / ${report.subject.version}` },
      { key: 'request', label: '请求模型 / 推理强度', children: `${report.requested.model} / ${report.requested.reasoning_effort}` },
      { key: 'date', label: '报告记录时间', children: displayTime(report.recorded_at) },
      { key: 'source', label: '源修订', children: <Hash value={report.source_revision} /> },
      { key: 'source-hash', label: '源 SHA-256', children: <Hash value={report.source_sha256} /> },
      { key: 'suite', label: '套件', children: report.suite_id },
      { key: 'suite-hash', label: '套件 SHA-256', children: <Hash value={report.suite_sha256} /> },
    ]} />
    {([['TUNING', report.tuning], ['HELD_OUT', report.held_out]] as const).map(([split, data]) => {
      return <Card key={split} title={`${split}: ${data.id}`} size="small"><Space orientation="vertical" className="full-width"><Hash value={data.sha256} /><Typography.Text className="break-word">案例：{data.case_ids.join(', ')}</Typography.Text></Space></Card>;
    })}
    <Table<AgentCase> rowKey="id" dataSource={report.cases} pagination={{ pageSize: 10 }} scroll={{ x: 620 }}
      columns={[
        { title: '案例 / 分区', key: 'case', render: (_, item) => <Space orientation="vertical"><Typography.Text>{item.id}</Typography.Text><Tag>{item.split}</Tag></Space> },
        { title: '结果', key: 'status', render: (_, item) => <Outcome status={item.status} /> },
        { title: '实际模型 / 推理强度', key: 'observed', render: (_, item) => observedIdentity(item) },
        { title: '原因', dataIndex: 'reason', key: 'reason' },
      ]}
      expandable={{ expandedRowRender: item => <Space orientation="vertical" className="full-width">
        <Descriptions size="small" column={1} items={[
          { key: 'invocation', label: '原生执行编号', children: item.observed?.invocation_id ?? '未知（未观察到）' },
          { key: 'input', label: '输入用量', children: measured(item.measurements.input_tokens, 'tokens') },
          { key: 'output', label: '输出用量', children: measured(item.measurements.output_tokens, 'tokens') },
          { key: 'elapsed', label: '耗时', children: measured(item.measurements.elapsed_ms, 'ms') },
          { key: 'tools', label: '工具调用', children: measured(item.measurements.tool_calls, 'calls') },
          { key: 'cost', label: '实际费用', children: measured(item.measurements.cost?.amount, item.measurements.cost?.currency ?? '') },
          { key: 'hash', label: '场景 SHA-256', children: <Hash value={item.scenario_sha256} /> },
          { key: 'required', label: '必需断言', children: item.required_assertions.join(', ') },
        ]} />
        {item.assertions.length === 0 ? <Typography.Text>尚无断言证据</Typography.Text> : item.assertions.map(assertion => <Space key={assertion.id} orientation="vertical"><Typography.Text>{assertion.id}: {assertion.passed ? 'PASS' : 'FAIL'}</Typography.Text><Hash value={assertion.evidence_sha256} /></Space>)}
      </Space> }} />
  </Space>;
}
