import { Alert, App, Button, Card, Descriptions, Drawer, Space, Table, Tag, Typography, Upload } from 'antd';
import { UploadOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { ResponseValidatorLoadError, validateResponseAsync } from '@quazonai/web/response-contract/lazy';
import { api, ApiFailure, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { caseCounts, measured, observedIdentity } from './agent-evaluation-data';
import type { AgentCase, AgentReport } from './agent-evaluation-data';
import { ErrorNotice, NoData, Pager, QueryPanel, useGuard, useOnline } from './ui';

const reportPath = '/api/v2/artifacts/{id}/agent-evaluation';
const maxBytes = 2 * 1024 * 1024;
type ReportSubmission = { body: Schema['ArtifactCreate']; headers: { 'Idempotency-Key': string } };
const colors = { PASS: 'success', FAIL: 'error', BLOCKED: 'warning', UNRUN: 'default' } as const;
function Outcome({ status }: { status: AgentReport['status'] }) { return <Tag color={colors[status]}>{status}</Tag>; }
function Hash({ value }: { value: string }) { return <Typography.Text className="break-word" copyable>{value}</Typography.Text>; }

export function AgentEvaluations({ projectId }: { projectId: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<string>();
  const [uploading, setUploading] = useState(false);
  const online = useOnline();
  const query = useQuery({ queryKey: ['agent-evaluation-artifacts', projectId, history.at(-1)], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/artifacts', {
    params: { query: { project_id: projectId, cursor: history.at(-1), limit: 25 } }, signal,
  })) });
  return <Space orientation="vertical" size="middle" className="full-width">
    <Alert type="info" showIcon title="Agent 评估报告" description="查看上传的运行器证据。PASS 表示报告内断言通过；上传、合同校验和模型执行成功都不授予独立科学资格。" />
    <Space wrap><Button onClick={() => setUploading(true)} disabled={!online}>上传报告</Button><Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新</Button></Space>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Table<Schema['ArtifactView']> rowKey="id" pagination={false} scroll={{ x: 680 }}
        dataSource={query.data?.items.filter(item => item.kind === 'REPORT')}
        locale={{ emptyText: <NoData text="本页暂无 REPORT 产物；可继续翻页，或上传真实报告" /> }}
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
    {uploading && <ReportUpload projectId={projectId} close={() => setUploading(false)} uploaded={id => { setUploading(false); setHistory([undefined]); setSelected(id); }} />}
  </Space>;
}
function ReportDetail({ id }: { id: string }) {
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
function ReportUpload({ projectId, close, uploaded }: { projectId: string; close: () => void; uploaded: (id: string) => void }) {
  const [file, setFile] = useState<{ name: string; content: string }>();
  const [reading, setReading] = useState(false);
  const [error, setError] = useState<unknown>();
  const [request, setRequest] = useState<ReportSubmission>();
  const sequence = useRef(0); const intent = useRef(new Intent()); const submitting = useRef(false);
  const online = useOnline(); const client = useQueryClient(); const { modal } = App.useApp();
  const mutation = useMutation({ mutationFn: async (submission: ReportSubmission) => {
    const result = dataOf(await api.POST('/api/v2/artifacts', { body: submission.body, params: { header: submission.headers } }));
    if (result.resource.project_id !== projectId || result.resource.kind !== 'REPORT') {
      throw new ApiFailure('HTTP_CONTRACT_ERROR', '报告回执与原项目或种类不匹配；保留原请求后重试');
    }
    return result;
  }, onSuccess: result => { void client.invalidateQueries({ queryKey: ['agent-evaluation-artifacts', projectId] }); uploaded(result.resource.id); },
  onSettled: () => { submitting.current = false; } });
  function submit() {
    if (!file || reading || !online || submitting.current || mutation.isPending) return;
    const body: Schema['ArtifactCreate'] = { schema_version: 1, project_id: projectId, kind: 'REPORT', content: file.content };
    const original = request ?? { body, headers: intent.current.headers('POST', '/api/v2/artifacts', body) };
    // Once any write starts, replacement requires the existing explicit close
    // confirmation. Keep the exact body/key even if a committed receipt is lost
    // or its asynchronous validator cannot load; never automatically replay.
    submitting.current = true; setRequest(original); mutation.mutate(original);
  }
  useGuard(!!file || reading || mutation.isPending || !!request);
  function dismiss() {
    if (submitting.current || mutation.isPending) return;
    const discard = () => { sequence.current++; close(); };
    if (file || reading || request) modal.confirm({ title: request ? '关闭报告上传？' : '放弃未上传的报告？', content: request ? '请求可能已保存。关闭不会撤回已保存的报告；继续编辑可使用同一请求和幂等键重试。' : undefined, okText: '关闭', cancelText: '继续编辑', onOk: discard }); else discard();
  }
  return <Drawer open title="上传 Agent 评估报告" onClose={dismiss} closable={!mutation.isPending} maskClosable={!mutation.isPending} keyboard={!mutation.isPending}>
    <Space orientation="vertical" className="full-width">
      <Alert type="info" showIcon title="不可变报告产物" description="选择符合 AgentEvaluationReportV1 的 JSON（最多 2 MiB）。服务端检查断言、模型身份和分区；上传不会运行模型或授予资格。" />
      <Upload accept=".json,application/json" showUploadList={false} disabled={mutation.isPending || reading || !!request} beforeUpload={async selected => {
        if (submitting.current || request) return Upload.LIST_IGNORE;
        const current = ++sequence.current; setReading(true); setError(undefined); setFile(undefined); mutation.reset();
        try {
          if (selected.size > maxBytes || selected.size === 0) throw new ApiFailure('REPORT_SIZE', '报告必须为 1 byte 至 2 MiB');
          const content = await selected.text();
          if (current !== sequence.current) return Upload.LIST_IGNORE;
          if (new TextEncoder().encode(content).length > maxBytes) throw new ApiFailure('REPORT_SIZE', '报告必须为 1 byte 至 2 MiB');
          const valid = await validateResponseAsync(reportPath, 'GET', 200, JSON.parse(content), 'application/json');
          if (current !== sequence.current) return Upload.LIST_IGNORE;
          if (!valid) throw new ApiFailure('REPORT_CONTRACT', '报告格式不符合原生合同');
          setFile({ name: selected.name, content });
        } catch (failure) {
          if (current === sequence.current) setError(failure instanceof ResponseValidatorLoadError
            ? new ApiFailure('REPORT_VALIDATOR_UNAVAILABLE', '本地报告校验组件加载失败；报告尚未上传。请刷新后重新选择报告。')
            : failure instanceof ApiFailure ? failure : new ApiFailure('REPORT_JSON', '无法读取有效 JSON 报告'));
        }
        finally { if (current === sequence.current) setReading(false); }
        return Upload.LIST_IGNORE;
      }}><Button icon={<UploadOutlined aria-hidden />} loading={reading} aria-busy={reading} disabled={mutation.isPending || reading || !!request}>选择 JSON 报告</Button></Upload>
      {file && <Typography.Text>{file.name} · {new TextEncoder().encode(file.content).length} bytes</Typography.Text>}
      <ErrorNotice error={error ?? mutation.error} />
      {request && mutation.isError && <Alert type="warning" showIcon title="原报告内容与幂等键已锁定；只能原样重试。选择其他报告前须明确关闭本次上传。" />}
      <Space wrap><Button type="primary" loading={mutation.isPending} aria-busy={mutation.isPending} disabled={!file || reading || !online || mutation.isPending} onClick={submit}>{request ? '原样重试上传请求' : '上传报告'}</Button><Button onClick={dismiss} disabled={mutation.isPending}>取消</Button></Space>
    </Space>
  </Drawer>;
}
