import { Alert, Button, Card, Checkbox, Descriptions, Drawer, Form, Input, Select, Space, Steps, Tag, Timeline, Typography } from 'antd';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useState, useSyncExternalStore } from 'react';
import { api, ApiFailure, dataOf, displayTime, uuidPattern, type Schema } from './api';
import { ResourceSelect } from './resource-select';
import { ErrorNotice, NoData, Pager, QueryPanel, ResourceFacts, useClock, useOnline } from './ui';
import { createExitResultReader, ExitSession, money, nonnegativeDecimal, positiveDecimal, previewCanStart, remainingReduction, withdrawableDisplay, type ExitAction, type ExitPreview, type ExitRequest, type ExitResult, type ExitView } from './capital-exit-state';
import { exitActionAllowed, futureDeadline, resumeFormValues, type FormValues } from './capital-exit-form';

const sessions = new Map<string, ExitSession>();
function sessionFor(project: string, source: string) {
  const key = `quazonai:capital-exit:v1:${project}:${source}`;
  let session = sessions.get(key);
  if (!session) {
    let storage: Storage | undefined;
    try { storage = typeof window === 'undefined' ? undefined : window.sessionStorage; } catch { /* Fail closed on write. */ }
    session = new ExitSession(key, storage, { project, source }); sessions.set(key, session);
  }
  return session;
}
async function send(request: ExitRequest, key: string): Promise<ExitResult> {
  const header = { 'Idempotency-Key': key };
  if (request.kind === 'PREVIEW') return { kind: 'PREVIEW', value: dataOf(await api.POST('/api/v2/projects/{project_id}/capital-exit-previews', {
    body: request.body, params: { path: { project_id: request.project }, header },
  })).resource };
  if (request.kind === 'START') return { kind: 'INTENT', value: dataOf(await api.POST('/api/v2/projects/{project_id}/capital-exits', {
    body: request.body, params: { path: { project_id: request.project }, header },
  })).resource };
  const paths = { PAUSE: '/api/v2/capital-exits/{id}/pause', CANCEL: '/api/v2/capital-exits/{id}/cancel', RESUME: '/api/v2/capital-exits/{id}/resume', RECONCILE_WITHDRAWAL: '/api/v2/capital-exits/{id}/reconcile-withdrawal' } as const;
  return { kind: 'INTENT', value: dataOf(await api.POST(paths[request.body.action], { body: request.body, params: { path: { id: request.id }, header } })).resource };
}
const states: Record<ExitView['state'], string> = {
  REQUESTED: '已请求，等待原生确认', FENCING: '正在禁止新增资金占用', CANCELLING_OPENERS: '正在核对开仓撤单', REDUCING: '正在限价减仓', WAITING_EVIDENCE: '等待原生成交、结算或可用资金证据', PAUSED: '已暂停，资金仍保留', CANCELLING_EXIT: '正在取消后续退出', CANCELLED_RESERVED: '后续退出已取消，资金仍保留', RECONCILING_WITHDRAWAL: '正在核对自行提款', COMPLETED: '原生证据已核对提款', BLOCKED: '已阻止，需处理原因',
};
const required = { required: true, whitespace: true, message: '请填写此项' };
const decimalRule = { validator: (_: unknown, value: string) => positiveDecimal(value ?? '') ? Promise.resolve() : Promise.reject(new Error('请输入精确的正十进制金额或数量')) };
const idRule = { pattern: uuidPattern, message: '请输入原记录 UUID' };
function Evidence({ ids }: { ids: string[] }) {
  return ids.length ? <Space orientation="vertical">{[...new Set(ids)].map(id => <Typography.Text className="break-word" copyable key={id}>{id}</Typography.Text>)}</Space> : <Typography.Text>暂无证据</Typography.Text>;
}
function Environment({ value }: { value: Schema['ForwardEnvironmentV1'] }) { return <Tag color={value === 'LIVE' ? 'orange' : 'blue'}>{value === 'LIVE' ? 'LIVE · 实盘来源' : 'PAPER / SANDBOX · 模拟'}</Tag>; }

export function CapitalExitAccounts() {
  const [project, setProject] = useState<string>();
  return <Space orientation="vertical" className="full-width" size="large">
    <Typography.Title level={2}>账户与资金退出</Typography.Title>
    <Typography.Paragraph>账户资金由原生所有者管理。部分退出会保留指定资金，必要时按批准约束减仓；QZ 不替你提款，也不承诺无损或完成时间。</Typography.Paragraph>
    <ResourceSelect label="选择账户所属项目" value={project} onChange={setProject} queryKey={['exit-projects']} load={async (cursor, signal) => {
      const page = dataOf(await api.GET('/api/v2/projects', { params: { query: { cursor, limit: 50 } }, signal }));
      return { items: page.items.map(item => ({ value: item.id, label: item.name })), next_cursor: page.next_cursor };
    }} />
    {project && <ProjectAccounts key={project} project={project} />}
  </Space>;
}
function ProjectAccounts({ project }: { project: string }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [exitHistory, setExitHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<{ source: string; intent?: string }>();
  const online = useOnline(); const cursor = history.at(-1); const exitCursor = exitHistory.at(-1);
  const sources = useQuery({ queryKey: ['capital-exit-sources', project, cursor], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{project_id}/account-sources', { params: { path: { project_id: project }, query: { cursor, limit: 25 } }, signal })) });
  const exits = useQuery({ queryKey: ['capital-exits', project, exitCursor], refetchInterval: online ? 10000 : false, queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{project_id}/capital-exits', { params: { path: { project_id: project }, query: { cursor: exitCursor, limit: 25 } }, signal })) });
  return <>
    <QueryPanel pending={sources.isPending} error={sources.error} stale={!!sources.data} reload={() => { void sources.refetch(); }}>
      {sources.data?.items.map(source => <Card key={source.id} title={<Space wrap><span>{source.binding.native_account_id}</span><Environment value={source.binding.environment} /></Space>}>
        <Descriptions column={1} items={[
          { key: 'source', label: '原生账户来源', children: <Typography.Text copyable>{source.id}</Typography.Text> },
          { key: 'connection', label: '连接状态', children: source.connection },
          { key: 'evidence', label: '当前账户快照', children: source.latest_snapshot_id ?? '暂无证据' },
          { key: 'capability', label: '退出能力', children: '尚未核验；在预览中检查当前所有者与原生证据' },
        ]} />
        <AccountEvidenceClock project={project} source={source.id} />
        <Button disabled={sources.isError} onClick={() => setSelected({ source: source.id })}>部分退出资金</Button>
      </Card>)}
      {sources.data?.items.length === 0 && <NoData text="暂无已绑定原生账户，不可创建替代账户退出" />}
      <Pager history={history} next={sources.data?.next_cursor} loading={sources.isFetching} move={setHistory} />
    </QueryPanel>
    <Typography.Title level={3}>已有退出记录</Typography.Title>
    <Button disabled={!online || exits.isFetching} onClick={() => { void exits.refetch(); }}>刷新原退出记录</Button>
    <QueryPanel pending={exits.isPending} error={exits.error} stale={!!exits.data} reload={() => { void exits.refetch(); }}>
      {exits.data?.items.map(exit => <Card key={exit.id} size="small"><Space orientation="vertical" className="full-width">
        <Environment value={exit.environment} /><Typography.Text>{states[exit.state]} · 申请 {money(exit.funds.requested_amount)} · 保留 {money(exit.funds.reserved_amount)}</Typography.Text>
        <Button onClick={() => setSelected({ source: exit.account_source_id, intent: exit.id })}>查看退出 {exit.id}</Button>
      </Space></Card>)}
      {exits.data?.items.length === 0 && <NoData text="暂无退出记录；空列表不代表任何未确认请求已取消" />}
      <Pager history={exitHistory} next={exits.data?.next_cursor} loading={exits.isFetching} move={setExitHistory} />
    </QueryPanel>
    {selected && <CapitalExitDrawer key={`${project}:${selected.source}:${selected.intent ?? ''}`} project={project} source={selected.source} initialIntent={selected.intent} close={() => setSelected(undefined)} />}
  </>;
}

function AccountEvidenceClock({ project, source }: { project: string; source: string }) {
  const query = useQuery({ queryKey: ['capital-exit-current', project, source], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{project_id}/account-sources/{source_id}/current', { params: { path: { project_id: project, source_id: source } }, signal })) });
  return <Typography.Paragraph type="secondary">原生快照时间（纳秒）：{query.isError ? '读取失败，暂无当前证据' : query.data?.latest_snapshot?.observation.snapshot?.ts_event ?? '暂无证据'}</Typography.Paragraph>;
}

const splitIds = (value: string) => value.split(/[\s,]+/).filter(Boolean);
export function CapitalExitDrawer({ project, source, initialIntent, close }: { project: string; source: string; initialIntent?: string; close: () => void }) {
  const session = sessionFor(project, source); const work = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const [intentId, setIntentId] = useState(initialIntent ?? work.lastIntent);
  const [readResult] = useState(() => createExitResultReader(initialIntent, work.result));
  const [preview, setPreview] = useState<ExitPreview>(); const [approved, setApproved] = useState(false);
  const [form] = Form.useForm<FormValues>(); const policy = Form.useWatch('policy', form); const scope = Form.useWatch('scope', form);
  const deadline = Form.useWatch('deadline', form);
  const [action, setAction] = useState<'PAUSE' | 'CANCEL' | 'RESUME' | 'RECONCILE_WITHDRAWAL'>();
  const [reported, setReported] = useState(''); const [transfer, setTransfer] = useState(''); const [actionApproved, setActionApproved] = useState(false);
  const online = useOnline(); const now = useClock(); const client = useQueryClient(); const busy = work.pending || work.unknown;
  const current = useQuery({ queryKey: ['capital-exit-current', project, source], refetchInterval: online ? 10000 : false, queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{project_id}/account-sources/{source_id}/current', { params: { path: { project_id: project, source_id: source } }, signal })) });
  const detail = useQuery({ queryKey: ['capital-exit', intentId], enabled: !!intentId, refetchInterval: online ? 5000 : false, queryFn: async ({ signal }) => { const value = dataOf(await api.GET('/api/v2/capital-exits/{id}', { params: { path: { id: intentId! } }, signal })); if (value.project_id !== project || value.account_source_id !== source) throw new ApiFailure('HTTP_CONTRACT_ERROR', '退出记录与当前账户不匹配'); return value; } });
  const exit = detail.data; const observation = current.data?.latest_snapshot;
  const resumeValues = exit ? resumeFormValues(exit) : undefined;
  const actionAllowed = !!exit && !!action && exitActionAllowed(exit.state, action);
  useEffect(() => {
    const result = readResult(work.result);
    if (result?.kind === 'PREVIEW') { setPreview(result.value); setApproved(false); setActionApproved(false); }
    if (result?.kind === 'INTENT') { setIntentId(result.value.id); setAction(undefined); setActionApproved(false); setApproved(false); setPreview(undefined);
      client.setQueryData(['capital-exit', result.value.id], result.value); void client.invalidateQueries({ queryKey: ['capital-exits', project] }); void client.invalidateQueries({ queryKey: ['capital-exit', result.value.id] }); }
  }, [work.result, readResult, client, project]);
  useEffect(() => {
    if (work.error instanceof ApiFailure && work.error.status === 409) {
      setPreview(undefined); setApproved(false); setActionApproved(false);
      void client.invalidateQueries({ queryKey: ['capital-exit-current', project, source] });
      if (intentId) void client.invalidateQueries({ queryKey: ['capital-exit', intentId] });
    }
  }, [work.error, client, project, source, intentId]);
  const canStart = !!preview && previewCanStart(preview, now, observation?.id) && !current.isError && !current.isFetching
    && (preview.policy.kind === 'CASH_ONLY' || futureDeadline(preview.policy.deadline, now));
  async function previewRequest(values: FormValues) {
    if (!observation || busy || !online) return;
    if (values.policy === 'BOUNDED_LIMIT' && !futureDeadline(values.deadline, Date.now())) {
      form.setFields([{ name: 'deadline', errors: ['截止时间已过期或无效，请手动修改后重新预览'] }]); return;
    }
    const moneyScope = { amount: values.amount, currency: values.currency };
    const body: Schema['CapitalExitPreviewRequestV1'] = { schema_version: 1, account_source_id: source, expected_source_observation_id: observation.id,
      scope: values.scope === 'AMOUNT' ? { kind: 'AMOUNT', ...moneyScope } : { kind: 'PORTFOLIO_SCOPE', ...moneyScope, stream_ids: splitIds(values.streamIds), release_ids: splitIds(values.releaseIds) },
      policy: values.policy === 'CASH_ONLY' ? { kind: 'CASH_ONLY' } : { kind: 'BOUNDED_LIMIT', deadline: values.deadline,
        legs: [{ instrument_id: values.instrument, maximum_reduction_quantity: values.quantity, minimum_sell_price: values.price }],
        max_execution_cost: { amount: values.cost, currency: values.currency, reference_evidence_id: values.costEvidence } },
    };
    setPreview(undefined); setApproved(false); setActionApproved(false);
    await session.submit({ kind: 'PREVIEW', project, source, body }, send);
  }
  function start() {
    if (!preview || !canStart || !approved || busy || !online || intentId) return;
    if (preview.policy.kind === 'BOUNDED_LIMIT' && !futureDeadline(preview.policy.deadline, Date.now())) return;
    void session.submit({ kind: 'START', project, source, body: { schema_version: 1, preview_id: preview.id,
      expected_account_control_revision: preview.expected_account_control_revision!, acknowledged_plan_artifact_id: preview.plan_artifact_id, expected_source_observation_id: preview.original_observation_id } }, send);
  }
  function submitAction() {
    if (!exit || !action || !actionAllowed || busy || !online || !actionApproved || detail.isError || detail.isFetching) return;
    const common = { schema_version: 1 as const, expected_revision: exit.revision };
    let body: ExitAction;
    if (action === 'RESUME') { if (!preview || !canStart || !approved || !resumeValues
      || (preview.policy.kind === 'BOUNDED_LIMIT' && !futureDeadline(preview.policy.deadline, Date.now()))) return; body = { ...common, action, preview_id: preview.id }; }
    else if (action === 'RECONCILE_WITHDRAWAL') { if (!positiveDecimal(reported)) return; body = { ...common, action, user_reported_amount: reported, currency: exit.funds.requested_amount.currency, external_transfer_ref: transfer || null }; }
    else body = { ...common, action };
    void session.submit({ kind: 'ACTION', project, source, id: exit.id, body }, send);
  }
  return <Drawer open size="large" title="部分退出资金" onClose={close} extra={<Button onClick={close}>关闭</Button>}>
    <Space orientation="vertical" size="large" className="full-width">
      <Alert type="info" showIcon title="关闭、返回或离开页面不会取消退出" description="已成交减仓不会撤销；暂停或取消后续退出均保留资金，不会自动买回或重新投入。请保留原操作标识。" />
      <QueryPanel pending={current.isPending} error={current.error} stale={!!current.data} reload={() => { void current.refetch(); }}>
        {current.data && <><Environment value={current.data.source.binding.environment} /><Typography.Text>账户：{current.data.source.binding.native_account_id}</Typography.Text>
          <Descriptions column={1} items={[
            { key: 'source', label: '账户来源', children: source }, { key: 'evidence', label: '原生快照证据', children: observation?.id ?? '暂无证据' },
            { key: 'clock', label: '原生事件时间（纳秒）', children: observation?.observation.snapshot?.ts_event ?? '暂无证据' },
            { key: 'received', label: '快照接收时间（非原生时间）', children: observation ? displayTime(observation.received_at) : '暂无证据' },
            { key: 'valuation', label: '估值状态', children: current.data.valuation },
          ]} /></>}
      </QueryPanel>
      {work.unknown && <Alert type="warning" showIcon title="原操作结果未知；不要创建新请求" description={<Space orientation="vertical">
        {work.operation && <><Typography.Text copyable>原操作标识：{work.operation.key}</Typography.Text><Typography.Text>操作：{work.operation.request.kind === 'ACTION' ? work.operation.request.body.action : work.operation.request.kind}</Typography.Text>
          <details><summary>原请求（仅按原标识核对）</summary><Typography.Paragraph className="break-word">{JSON.stringify(work.operation.request.body)}</Typography.Paragraph></details>
          <Button disabled={!online || work.pending} onClick={() => { void session.submit(undefined, send); }}>按原请求核对结果</Button></>}
        <Button disabled={!online || detail.isFetching} onClick={() => { void detail.refetch(); void client.invalidateQueries({ queryKey: ['capital-exits', project] }); }}>只读刷新已有退出记录</Button>
      </Space>} />}
      <ErrorNotice error={work.error} />
      {intentId && <QueryPanel pending={detail.isPending} error={detail.error} stale={!!exit} reload={() => { void detail.refetch(); }}>
        {exit && <><CapitalExitFacts exit={exit} now={now} readCurrent={!detail.isError && !detail.isFetching} />
          <Space wrap>
            <Button disabled={!online || detail.isFetching} onClick={() => { void detail.refetch(); void current.refetch(); }}>刷新可提证据</Button>
            {exit.state === 'COMPLETED' && <Button disabled={busy || !online || detail.isError} onClick={() => { if (!session.forgetCompletedView()) return; setIntentId(undefined); setPreview(undefined); setApproved(false); form.resetFields(); }}>新建资金退出</Button>}
            {exit.state !== 'COMPLETED' && <>
              <Button disabled={busy || !online || detail.isError || !exitActionAllowed(exit.state, 'PAUSE')} onClick={() => { setAction('PAUSE'); setActionApproved(false); }}>暂停退出</Button>
              <Button disabled={busy || !online || detail.isError} onClick={() => { setAction('CANCEL'); setActionApproved(false); }}>取消后续退出</Button>
              <Button disabled={busy || !online || detail.isError || !exitActionAllowed(exit.state, 'RESUME') || !resumeValues} onClick={() => { if (!resumeValues || !exitActionAllowed(exit.state, 'RESUME')) return; setAction('RESUME'); setActionApproved(false); setPreview(undefined); setApproved(false); form.setFieldsValue(resumeValues); }}>继续退出</Button>
              <Button disabled={busy || !online || detail.isError || exit.environment !== 'LIVE'} onClick={() => { setAction('RECONCILE_WITHDRAWAL'); setActionApproved(false); }}>我已自行提款，核对余额</Button>
            </>}
          </Space>
          {!resumeValues && <Alert type="warning" showIcon title="当前仅支持单腿限价减仓，无法继续此退出" description="原记录的减仓腿不会被截断或替换；请核对原始计划。" />}
          <Alert type="info" showIcon title="手动提款前请刷新，并核对交易所当前提款页面" description="刷新仅读取当前原生证据，不会制造新证明。资金保留不是钱包锁；其他交易或转账可能立即使证据失效。模拟资金不能真实提款。" />
        </>}
      </QueryPanel>}
      {(!intentId || action === 'RESUME') && <>
        <Steps size="small" current={preview ? 2 : 0} items={[{ title: '选择退出范围' }, { title: '预览影响' }, { title: '确认退出约束' }]} />
        {action === 'RESUME' && <Alert type="info" title="为同一退出刷新剩余范围与约束；继续退出不会恢复买入" />}
        <Form form={form} layout="vertical" initialValues={{ scope: 'AMOUNT', policy: 'CASH_ONLY' }} disabled={busy || !online} onValuesChange={() => { setPreview(undefined); setApproved(false); setActionApproved(false); }} onFinish={previewRequest}>
          <Form.Item name="scope" label="退出范围" rules={[required]}><Select options={[{ value: 'AMOUNT', label: '指定资金金额' }, { value: 'PORTFOLIO_SCOPE', label: '指定已有组合目标流' }]} /></Form.Item>
          {scope === 'PORTFOLIO_SCOPE' && <><Alert showIcon type="info" title="多个 Alpha 共用账户净持仓，不是各自可提款的独立资金" />
            <Form.Item name="streamIds" label="已有目标流 ID（以逗号或换行分隔）" rules={[required, { validator: (_, value: string) => splitIds(value ?? '').every(id => uuidPattern.test(id)) ? Promise.resolve() : Promise.reject(new Error('请使用原目标流 UUID')) }]}><Input.TextArea rows={2} /></Form.Item>
            <Form.Item name="releaseIds" label="已有发布 ID（以逗号或换行分隔）" rules={[required, { validator: (_, value: string) => splitIds(value ?? '').every(id => uuidPattern.test(id)) ? Promise.resolve() : Promise.reject(new Error('请使用原发布 UUID')) }]}><Input.TextArea rows={2} /></Form.Item></>}
          <Form.Item name="amount" label="申请退出金额" rules={[required, decimalRule]}><Input inputMode="decimal" /></Form.Item>
          <Form.Item name="currency" label="账户抵押币种" rules={[required]}><Input maxLength={32} /></Form.Item>
          <Form.Item name="policy" label="执行约束" rules={[required]}><Select options={[{ value: 'CASH_ONLY', label: '仅使用有证据的闲置现金' }, { value: 'BOUNDED_LIMIT', label: '允许明确约束的限价减仓' }]} /></Form.Item>
          {policy === 'BOUNDED_LIMIT' && <>
            <Alert type="info" showIcon title="当前仅支持单腿限价减仓" />
            <Form.Item name="instrument" label="原生交易标的 ID" rules={[required]}><Input /></Form.Item>
            <Form.Item name="quantity" label="最大累计减仓数量" rules={[required, decimalRule]}><Input inputMode="decimal" /></Form.Item>
            <Form.Item name="price" label="最低卖出价格" rules={[required, decimalRule]}><Input inputMode="decimal" /></Form.Item>
            <Form.Item name="deadline" label="截止时间（RFC 3339，含时区）" rules={[required, { validator: (_, value: string) => futureDeadline(value ?? '', Date.now()) ? Promise.resolve() : Promise.reject(new Error('请输入带时区的未来时间')) }]}><Input placeholder="2026-10-07T18:00:00Z" /></Form.Item>
            {deadline && Date.parse(deadline) <= now && <Alert type="warning" showIcon title="截止时间已过期，请手动修改后重新预览" description="原截止时间原样保留，不会自动延长或放宽执行约束。" />}
            <Form.Item name="cost" label="最大增量执行成本（同币种）" rules={[required, { validator: (_, value: string) => nonnegativeDecimal(value ?? '') ? Promise.resolve() : Promise.reject(new Error('请输入非负精确十进制值')) }]}><Input inputMode="decimal" /></Form.Item>
            <Form.Item name="costEvidence" label="原始价格 / 成本参考证据 ID" rules={[required, idRule]}><Input /></Form.Item>
            <Alert type="warning" showIcon title="到期只会暂停并保留资金，不自动市价卖出" description="成本上限约束订单准入，不保证没有损失；原有投资盈亏另列。不会自行放宽价格、数量或成本上限。" />
          </>}
          <Button htmlType="submit" loading={work.pending} disabled={!online || busy || !observation || current.isError || current.isFetching}>预览影响</Button>
        </Form>
        {preview && <><CapitalExitPreviewFacts preview={preview} now={now} />
          {!canStart && <Alert type="warning" showIcon title="当前预览不可执行" description="请检查能力或阻止原因；若证据、报价或版本改变，请刷新账户证据后重新预览。" />}
          <Checkbox checked={approved} disabled={!canStart || busy} onChange={event => setApproved(event.target.checked)}>我已核对账户、环境、金额、范围及所有限价 / 数量 / 成本约束，并批准此原始计划；保护订单保留，资金不再投入</Checkbox>
          {!intentId && <Button type="primary" disabled={!canStart || !approved || busy || !online} loading={work.pending} onClick={start}>开始退出</Button>}
        </>}
      </>}
      {action && exit && <Card title={action === 'PAUSE' ? '确认暂停退出' : action === 'CANCEL' ? '确认取消后续退出' : action === 'RESUME' ? '确认继续退出' : '核对自行提款'}>
        <Space orientation="vertical" className="full-width">
          <Typography.Paragraph>{action === 'RECONCILE_WITHDRAWAL' ? '这只提交你已自行完成的提款报告，需原生转账证据匹配；不会发起任何提款。' : action === 'RESUME' ? '只继续剩余减仓，不恢复新增买入；必须批准新的有效预览。' : '停止后续退出工作并核对未完成订单。已成交交易保留，资金仍禁止再投入，不会买回。'}</Typography.Paragraph>
          {action === 'RECONCILE_WITHDRAWAL' && <><Input aria-label="已自行提款金额" value={reported} onChange={event => { setReported(event.target.value); setActionApproved(false); }} inputMode="decimal" /><Typography.Text>{exit.funds.requested_amount.currency}</Typography.Text><Input aria-label="原转账参考（可选）" value={transfer} onChange={event => { setTransfer(event.target.value); setActionApproved(false); }} /></>}
          <Checkbox checked={actionApproved} disabled={busy} onChange={event => setActionApproved(event.target.checked)}>确认此操作及上述后果</Checkbox>
          <Space wrap><Button onClick={() => { setAction(undefined); setActionApproved(false); }}>返回详情</Button><Button disabled={!actionAllowed || !actionApproved || busy || !online || detail.isError || detail.isFetching || (action === 'RESUME' && (!approved || !canStart || !resumeValues)) || (action === 'RECONCILE_WITHDRAWAL' && !positiveDecimal(reported))} loading={work.pending} onClick={submitAction}>提交此操作</Button></Space>
        </Space>
      </Card>}
    </Space>
  </Drawer>;
}

export function CapitalExitPreviewFacts({ preview, now }: { preview: ExitPreview; now: number }) {
  const f = preview.funds;
  return <Card title="原始退出影响预览"><Space orientation="vertical" className="full-width">
    <Environment value={preview.environment} /><Tag>{preview.capability} · {Date.parse(preview.valid_until) <= now ? '预览已过期' : '预览尚在有效期内'}</Tag>
    <Descriptions column={1} items={[
      { key: 'requested', label: '申请退出', children: money(f.requested) }, { key: 'idle', label: '已核验闲置现金', children: money(f.verified_idle_cash) },
      { key: 'remainder', label: '仍需减仓释放（申请减闲置现金）', children: remainingReduction(f.requested, f.verified_idle_cash) }, { key: 'release', label: '预计释放（不保证）', children: money(f.estimated_release) }, { key: 'total', label: '原生总现金', children: money(f.native_total_cash) },
      { key: 'free', label: '原生空闲现金（不等于可提）', children: money(f.native_free_cash) }, { key: 'locked', label: '原生锁定现金', children: money(f.native_locked_cash) },
      { key: 'equity', label: '原生权益', children: money(f.native_equity) }, { key: 'before', label: '退出前管理资金', children: money(f.managed_capital_before) },
      { key: 'after', label: '退出后管理资金', children: money(f.remaining_managed_capital) }, { key: 'cost', label: '预计增量执行成本', children: money(f.estimated_execution_cost) },
      { key: 'pnl', label: '已有未实现盈亏（非本次成本）', children: money(f.existing_unrealized_pnl) }, { key: 'risk', label: '剩余组合风险证据', children: preview.remaining_risk_evidence_id ?? '暂无证据；可能降低分散度，无法确认剩余组合风险' },
      { key: 'expiry', label: '计划有效至', children: displayTime(preview.valid_until) }, { key: 'scope', label: '精确退出范围', children: <Typography.Text className="break-word">{JSON.stringify(preview.scope)}</Typography.Text> },
      { key: 'policy', label: '精确执行约束', children: <Typography.Text className="break-word">{JSON.stringify(preview.policy)}</Typography.Text> },
    ]} />
    {preview.reason_codes.length > 0 && <Alert type="warning" showIcon title="能力 / 证据说明" description={preview.reason_codes.join(' · ')} />}
    <Typography.Text>拟取消开仓订单：{preview.proposed_cancellations.length ? preview.proposed_cancellations.map(order => `${order.native_client_order_id} (${order.native_instrument_id})`).join('、') : '计划未列出'}</Typography.Text>
    <Typography.Text>保留保护订单：{preview.retained_protective_orders.length ? preview.retained_protective_orders.map(order => `${order.native_client_order_id} (${order.native_instrument_id})`).join('、') : '计划未列出；不代表账户没有保护需求'}</Typography.Text>
    <Typography.Text>计划减仓：{preview.reduction_legs.length ? preview.reduction_legs.map(leg => `${leg.instrument_id}，最多 ${leg.maximum_reduction_quantity}，最低价格 ${leg.minimum_sell_price}`).join('；') : '计划未列出减仓'}</Typography.Text>
    <Typography.Text>原生观测：{preview.original_observation_id}</Typography.Text>
    <Typography.Link href={`/api/v2/artifacts/${preview.plan_artifact_id}/content`} target="_blank" rel="noreferrer">原始不可变计划证据</Typography.Link>
    <Evidence ids={preview.evidence_refs} />
  </Space></Card>;
}
export function CapitalExitFacts({ exit, now, readCurrent = true }: { exit: ExitView; now: number; readCurrent?: boolean }) {
  const available = withdrawableDisplay(exit, now, readCurrent);
  return <Card title={states[exit.state]}><Space orientation="vertical" className="full-width">
    <Environment value={exit.environment} /><ResourceFacts id={exit.id} revision={exit.revision} updated={exit.updated_at} />
    <Descriptions column={{ xs: 1, sm: 2 }} items={[
      { key: 'requested', label: '申请退出', children: money(exit.funds.requested_amount) }, { key: 'reserved', label: '禁止再投入', children: money(exit.funds.reserved_amount) },
      { key: 'released', label: '已释放现金', children: money(exit.funds.released_cash_amount) }, { key: 'available', label: available.label, children: available.value },
      { key: 'unreleased', label: '尚未释放', children: money(exit.funds.unreleased_amount) }, { key: 'reconciled', label: exit.environment === 'LIVE' ? '已匹配提款' : '模拟核对金额', children: money(exit.funds.reconciled_withdrawal_amount) },
      { key: 'asof', label: '资金证据原始时间', children: displayTime(exit.funds.evidence_asof) }, { key: 'until', label: '资金证据有效至', children: displayTime(exit.funds.evidence_valid_until) },
      { key: 'remaining', label: '剩余交易', children: exit.remaining_trading === 'FENCED_PENDING_TARGET' ? '等待合格兼容目标，新增买入仍受限制；保护性风险管理保留' : '通过当前所有者的资金预算检查继续' },
    ]} />
    {exit.reason_codes.length > 0 && <Alert type="warning" showIcon title="待处理原因" description={exit.reason_codes.join(' · ')} />}
    <Timeline items={[{ content: `${displayTime(exit.created_at)} · 原退出申请已保存` }, { content: `${displayTime(exit.updated_at)} · 最近记录阶段：${states[exit.last_phase]}；当前状态：${states[exit.state]}` }]} />
    <Typography.Link href={`/api/v2/artifacts/${exit.plan_artifact_id}/content`} target="_blank" rel="noreferrer">原始计划证据</Typography.Link><Evidence ids={exit.evidence_refs} />
  </Space></Card>;
}
