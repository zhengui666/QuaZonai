import { useEffect, useRef, useState } from 'react';
import { Button, Checkbox, Form, Input, Select, Table } from 'antd';
import { useQueryClient } from '@tanstack/react-query';
import type { ObservationView } from '../../../packages/contracts/src/index.ts';
import type { CapitalExitAction, CapitalExitPlanningContext, CapitalExitPolicy, CapitalExitPreview, CapitalExitStatus, CapitalExitView, CapitalExitNativeOrderRef } from '../../../packages/contracts/src/capital-exit.ts';
import { api } from './api';
import { decimalUnits, readDraft, removeDraft, saveDraft, useCommand } from './commands';
import { useResource } from './data-hooks';
import { ErrorNotice, Exact, Facts, MetricLabel, PendingCommand, Status, TableSurface, Timestamp, useConfirm } from './ui';
import { Reason } from './diagnostics';
import { currentRoute, href, navigate } from './router';
import { capitalAction, capitalEvidenceIsCurrent, capitalPreviewInput, capitalPreviewIsCurrent, capitalResumePreviewMatches, suggestedCapitalSlice, type CapitalPlanFields } from './capital-input';

export function AutomaticControls({ value }: { value: ObservationView }) {
  const cache = useQueryClient(); const [saving, setSaving] = useState(false); const [error, setError] = useState<unknown>(); const [form] = Form.useForm();
  useEffect(() => { form.setFieldsValue(value.automaticPolicy ?? { enabled: false, cashBuffer: '0', maximumSnapshotAgeSeconds: 60 }); }, [value.id, value.automaticPolicy?.enabled, value.automaticPolicy?.cashBuffer, value.automaticPolicy?.maximumSnapshotAgeSeconds]);
  const allocation = value.allocation;
  return <section className="account-control-section"><h3>按账户净值自动调整</h3><p className="quiet">仅在原选因子池内按批准权重缩放；预留资金与现金缓冲先扣除。后台由官方 Pi Durable 任务运行，浏览器关闭不影响。权益、预留或容量未知时不交付；原目标过期仅显示投影，不延长有效期。</p>
    <Form form={form} layout="vertical" disabled={saving || value.status === 'STOPPED'} onFinish={(fields: { enabled: boolean; cashBuffer: string; maximumSnapshotAgeSeconds: string | number }) => { setSaving(true); setError(undefined); void api<ObservationView>(`/observations/${encodeURIComponent(value.id)}/automatic`, { method: 'POST', body: { enabled: !!fields.enabled, cashBuffer: fields.cashBuffer, maximumSnapshotAgeSeconds: Number(fields.maximumSnapshotAgeSeconds) } }).then(() => cache.invalidateQueries({ queryKey: ['/observations'] }), setError).finally(() => setSaving(false)); }}>
      <Form.Item name="enabled" valuePropName="checked"><Checkbox>启用自动调整</Checkbox></Form.Item><div className="form-columns"><Form.Item label="显式现金缓冲" name="cashBuffer" rules={[{ required: true }, { validator: (_rule, input: string) => (decimalUnits(input ?? '') ?? -1n) >= 0n ? Promise.resolve() : Promise.reject(new Error('请输入非负精确金额')) }]}><Input inputMode="decimal" suffix={value.currency} /></Form.Item><Form.Item label="账户快照最大年龄（秒）" name="maximumSnapshotAgeSeconds" rules={[{ required: true }]}><Input inputMode="numeric" /></Form.Item></div><Button htmlType="submit" loading={saving}>保存自动调整设置</Button><ErrorNotice error={error} />
    </Form>
    {allocation && <><Facts items={[["最近调整", <Timestamp value={allocation.updatedAt} />], ["状态", ({ PROJECTED: '仅资金投影，未交付', BLOCKED: '已阻止交付', SIZED: '已计算目标', DELIVERED: '目标已接收（不代表成交）', UNCHANGED: '目标未变化', UNCERTAIN: '交付结果待核对', STOPPED: '本地自动调整已停止' } as Record<string, string>)[allocation.status] ?? allocation.status], ["原因", allocation.reason ? <Reason value={allocation.reason} /> : '未提供'], ["账户权益", <Exact value={allocation.equity} unit={value.currency} />], ["账户总预留", <Exact value={allocation.reserved} unit={value.currency} />], ["显式现金缓冲", <Exact value={allocation.cashBuffer} />], ["扣预留后的可用权益", <Exact value={allocation.budget} />], ["有效策略资金", <Exact value={allocation.capital} />], ["条件容量上限", <Exact value={allocation.capacity} />], ["现金 / 费用后剩余", <Exact value={allocation.residualCash} />], ["快照 / 控制修订 / 调整序号", `${allocation.sequence ?? '未知'} / ${allocation.controlRevision ?? '未知'} / ${allocation.epoch}`]]} />
      <Table components={{ table: TableSurface }} rowKey="factorId" size="small" pagination={false} scroll={{ x: 460 }} dataSource={allocation.members} columns={[{ title: '原选因子 / 组合成员', dataIndex: 'factorId' }, { title: <MetricLabel metricKey="strategyCapital" label="本次建议资金" />, render: (_cell, row) => <Exact value={row.capital} unit={value.currency} /> }, { title: <MetricLabel metricKey="scale" />, render: (_cell, row) => <Exact value={row.scale} /> }]} />
      {allocation.receipt && <div className="adjustment-receipt"><h4>最近实际交付目标回执</h4><Facts items={[["原始回执身份", allocation.receipt.id], ["下游状态", allocation.receipt.status], ["源到期时间（未续期）", <Timestamp value={allocation.receipt.expiresAt} />], ["下游身份", allocation.receipt.externalId ?? '未提供']]} /></div>}</>}
  </section>;
}

const exitPhase: Record<string, string> = {
  REQUESTED: '请求已受理', FENCING: '隔离旧目标', CANCELLING_OPENERS: '撤销增加敞口订单',
  WAITING_CASH: '等待策略自然释放', REDUCING: '按界限分批减仓', WAITING_EVIDENCE: '等待原生资金证据',
  READY: '可在外部提取', PAUSING: '暂停中，等待原生订单核对', PAUSED: '已暂停，保留预留', RECOVERING: '恢复原生账户与订单', CANCELLING_EXIT: '正在停止退出订单',
  CANCELLED_RESERVED: '已取消，保留预留', RECONCILING_WITHDRAWAL: '核对外部提取',
  COMPLETED: '外部提取已对账', CLOSED: '计划已关闭', BLOCKED: '需要处理',
};
const preparationLabel: Record<string, string> = {
  WAITING_CASH: '等待原策略释放现货现金', REDUCING: '正在受限分批减仓',
  WAITING_EXTERNAL_TRANSFER: '现货 USDC 已备妥，等待在外部划转至提现路径',
  READY: '平台可提取金额已核实', STALE: '资金证据已过期，等待重新核实', BLOCKED: '资金准备受阻', CLOSED: '资金准备已结束',
};
const actionLabel: Partial<Record<CapitalExitAction, string>> = { PAUSE: '暂停计划', CANCEL: '取消并保留预留', RESUME: '按新预览继续', RELEASE_RESERVATION: '解除剩余退出义务' };
function NativeOrders({ orders, title }: { orders: CapitalExitNativeOrderRef[]; title: string }) {
  return <><h4>{title}</h4><Table components={{ table: TableSurface }} rowKey="native_client_order_id" size="small" pagination={false} scroll={{ x: 460 }} dataSource={orders} columns={[{ title: '原生订单', dataIndex: 'native_client_order_id' }, { title: '策略', dataIndex: 'native_strategy_id' }, { title: '标的', dataIndex: 'native_instrument_id' }]} /></>;
}
export function CapitalFundsSummary({ exit, now = Date.now() }: { exit: CapitalExitView; now?: number }) {
  const current = capitalEvidenceIsCurrent(exit, now);
  return <><p role="status">{exit.preparation_state === 'READY' && !current ? (exit.funds.withdrawability === 'SIMULATED' ? '模拟资金准备就绪，不证明实盘可提取' : '平台可提取证据待重新核实') : preparationLabel[exit.preparation_state] ?? exit.preparation_state}</p><Facts items={[
    ['原请求金额', <Exact value={exit.funds.requested_amount.amount} unit={exit.funds.requested_amount.currency} />],
    ['现货交易预算内预留', <Exact value={exit.funds.reserved_amount.amount} unit={exit.funds.reserved_amount.currency} />],
    ['已外部划转，尚待提取', <Exact value={exit.funds.staged_for_withdrawal_amount.amount} unit={exit.funds.staged_for_withdrawal_amount.currency} />],
    ['现货已备妥现金', <Exact value={exit.funds.released_cash_amount?.amount} unit={exit.funds.requested_amount.currency} missing="尚未核实" />],
    ['本计划已核实的平台可提取额', <Exact value={current ? exit.funds.verified_withdrawable_amount?.amount : null} unit={exit.funds.requested_amount.currency} missing={exit.funds.withdrawability === 'SIMULATED' ? '仅有模拟证据' : '未知，不能用现货余额代替'} />],
    ['已对账外部提取本金', <Exact value={exit.funds.reconciled_withdrawal_amount.amount} unit={exit.funds.reconciled_withdrawal_amount.currency} />],
    ['已取消的退出金额', <Exact value={exit.funds.waived_amount.amount} unit={exit.funds.waived_amount.currency} />],
    ['真实手续费（单列）', <Exact value={exit.funds.actual_fees?.amount} unit={exit.funds.actual_fees?.currency} missing="尚未核实" />],
    ['资金证据时间', <Timestamp value={exit.funds.evidence_asof} />], ['证据有效至', <Timestamp value={exit.funds.evidence_valid_until} />],
    ['剩余资金交易', exit.remaining_trading === 'BUDGET_AWARE' ? '按当前预算继续原策略' : '等待新的有效预算目标'],
  ]} /><p className="quiet">外部划转不计为提现。已对账本金表示源账户现金变动，外部费用及钱包净到账以平台原始记录为准。</p><p className="quiet">经济损失逐笔按原参考金额减去含费净卖出回款计算，负值按零累计。有利价格只抵本笔费用，不抵销过去损失；真实手续费单独展示，不再与经济损失相加。</p></>;
}
function policyFields(policy: CapitalExitPolicy, amount: string): CapitalPlanFields {
  const base: CapitalPlanFields = { amount, policy: policy.kind };
  if (policy.kind === 'CASH_ONLY') return base;
  const local = new Date(policy.deadline);
  base.deadline = new Date(local.getTime() - local.getTimezoneOffset() * 60_000).toISOString().slice(0, 16);
  if (policy.kind === 'BOUNDED_LIMIT') return { ...base, maxCost: policy.max_execution_cost.amount, maxSlice: policy.pacing.maximum_slice_notional, minimumInterval: String(policy.pacing.minimum_interval_seconds), participation: policy.pacing.maximum_participation, legs: policy.legs.map(row => ({ instrumentId: row.instrument_id, quantity: row.maximum_reduction_quantity, minimumPrice: row.minimum_sell_price })) };
  return base;
}
export function CapitalControls({ value }: { value: ObservationView }) {
  const path = `/observations/${encodeURIComponent(value.id)}/capital-exits`;
  const status = useResource<CapitalExitStatus>(path, { poll: 5000 });
  const context = useResource<CapitalExitPlanningContext>(`${path}/planning-context`);
  const current = status.data ?? value.capitalExit;
  const selectedId = currentRoute().params.get('exitId') ?? current?.exits[0]?.id;
  const detail = useResource<CapitalExitView>(`${path}/${encodeURIComponent(selectedId ?? '')}`, { enabled: !!selectedId, poll: 5000 });
  const exit = detail.data ?? current?.exits.find(row => row.id === selectedId);
  const cache = useQueryClient(); const confirm = useConfirm(); const [form] = Form.useForm<CapitalPlanFields>();
  const sliceEdited = useRef(false);
  const [inputError, setInputError] = useState<unknown>(); const [previewing, setPreviewing] = useState(false);
  const [now, setNow] = useState(Date.now());
  const [editingExitId, setEditingExitId] = useState<string | undefined>(() => readDraft<string>(`capital-editing-exit:${value.id}`));
  const [previewForExitId, setPreviewForExitId] = useState<string | undefined>(() => readDraft<string>(`capital-preview-exit:${value.id}`));
  const [preview, setPreview] = useState<CapitalExitPreview | undefined>(() => readDraft<CapitalExitPreview>(`capital-preview:${value.id}`));
  const previewCommand = useCommand<CapitalExitPreview>(`capital-preview:${value.id}`, result => { setPreview(result); setPreviewForExitId(editingExitId); saveDraft(`capital-preview:${value.id}`, result); if (editingExitId) saveDraft(`capital-preview-exit:${value.id}`, editingExitId); else removeDraft(`capital-preview-exit:${value.id}`); void status.refetch(); }, `${path}/preview`);
  const command = useCommand<{ requestId: string; state: string }>(`capital:${value.id}`, () => { void status.refetch(); if (selectedId) void detail.refetch(); void cache.invalidateQueries({ queryKey: ['/observations'] }); }, `${path}/commands`);
  const policy = Form.useWatch('policy', form);
  useEffect(() => { const timer = setInterval(() => setNow(Date.now()), 1000); return () => clearInterval(timer); }, []);
  useEffect(() => {
    const pending = previewCommand.values as unknown as { amount: string; policy: CapitalExitPolicy } | undefined;
    const initial: CapitalPlanFields = pending?.policy ? policyFields(pending.policy, pending.amount) : readDraft<CapitalPlanFields>(`capital-form:${value.id}`) ?? { amount: '', policy: 'NATURAL_RELEASE', minimumInterval: '5', participation: '0.1' };
    sliceEdited.current = !!initial.maxSlice;
    form.setFieldsValue(initial);
  }, [value.id]);
  useEffect(() => {
    if (context.data && !form.getFieldValue('legs')) form.setFieldValue('legs', context.data.instruments.filter(row => (decimalUnits(row.eligible_quantity) ?? 0n) > 0n).map(row => ({ instrumentId: row.instrument_id, quantity: row.eligible_quantity, minimumPrice: row.bid_price ?? '' })));
    if (!sliceEdited.current && !form.getFieldValue('maxSlice')) { const suggestion = suggestedCapitalSlice(form.getFieldValue('amount') ?? '', context.data?.funds.free_cash?.amount, current?.reserved); if (suggestion) form.setFieldValue('maxSlice', suggestion); }
  }, [context.data?.id, current?.reserved]);
  const busy = !!command.pending || command.busy || !!current?.commands?.some(item => item.state === 'PENDING' || item.state === 'UNCERTAIN');
  const canInitiate = current?.availableActions?.includes('START') === true && context.data?.capability === 'SUPPORTED';
  const validPreview = capitalPreviewIsCurrent(preview, current, now);
  const clearPreview = () => { setPreview(undefined); setPreviewForExitId(undefined); removeDraft(`capital-preview:${value.id}`); removeDraft(`capital-preview-exit:${value.id}`); };
  const choosePlan = (id: string) => { const params = new URLSearchParams(currentRoute().params); params.set('section', 'capital'); params.set('exitId', id); navigate(`/observations/${encodeURIComponent(value.id)}?${params}`, true); };
  return <section className="account-control-section capital-exit-section"><div className="section-heading"><h3>退出资金</h3><Button onClick={() => { void status.refetch(); void context.refetch(); if (selectedId) void detail.refetch(); }}>刷新资金状态</Button></div>
    <p className="quiet">QZ 准备现货 USDC 并核对原生资金证据。spot → perp 划转和提现在平台外部完成；浏览器关闭或本地观测停止不取消已受理计划。</p>
    <ErrorNotice error={status.error} retry={() => { void status.refetch(); }} />
    <Facts items={[["观测模式", value.mode === 'live' ? '实盘' : '模拟'], ["现货总权益", <Exact value={value.equity} unit={value.currency} />], ["现货预算内总预留", <Exact value={current?.reserved} unit={value.currency} missing="未知，不能增加交易资金" />], ["当前交易预算", <Exact value={value.allocation?.capital} unit={value.currency} />], ["规划依据时间", <Timestamp value={context.data?.asof} />]]} />
    {current?.reason && <Reason value={current.reason} />}
    <details className="capital-create" open={!current?.exits.length || !!editingExitId}><summary>{editingExitId ? '重新预览现有退出计划' : '准备新的退出资金'}</summary>
      {editingExitId && <Button disabled={busy || previewCommand.busy || !!previewCommand.pending} onClick={() => { setEditingExitId(undefined); removeDraft(`capital-editing-exit:${value.id}`); clearPreview(); }}>改为新建退出计划</Button>}
      <ErrorNotice error={context.error} retry={() => { void context.refetch(); }} />
      {!canInitiate && <p role="status">当前不能发起新计划。已有计划的停止、解除和对账按各自动作能力处理。</p>}
      <PendingCommand command={previewCommand} />
      <Form form={form} layout="vertical" validateTrigger="onBlur" scrollToFirstError={{ focus: true }} disabled={!canInitiate || busy || !!previewCommand.pending || previewCommand.busy || previewing}
        onValuesChange={changed => { if ('maxSlice' in changed) sliceEdited.current = true; if (changed.amount && !sliceEdited.current) { const suggestion = suggestedCapitalSlice(changed.amount, context.data?.funds.free_cash?.amount, current?.reserved); if (suggestion) form.setFieldValue('maxSlice', suggestion); } saveDraft(`capital-form:${value.id}`, form.getFieldsValue()); clearPreview(); }}
        onFinish={async fields => { setInputError(undefined); setPreviewing(true); try { const fresh = await context.refetch(); if (fresh.error || !fresh.data) throw fresh.error ?? new Error('capital_exit_status_unavailable'); const resume = editingExitId ? await api<CapitalExitView>(`${path}/${encodeURIComponent(editingExitId)}`) : undefined; await previewCommand.submit(capitalPreviewInput(fields, fresh.data, Date.now(), resume)); } catch (error) { setInputError(error); } finally { setPreviewing(false); } }}>
        <Form.Item label={editingExitId ? '本计划仍在现货预留的准备金额' : '准备退出金额'} name="amount" rules={[{ required: true }, { validator: (_rule, input: string) => (decimalUnits(input ?? '') ?? 0n) > 0n ? Promise.resolve() : Promise.reject(new Error('请输入精确正金额')) }]}><Input inputMode="decimal" suffix={value.currency} disabled={!!editingExitId} /></Form.Item>
        <Form.Item label="资金释放方式" name="policy" rules={[{ required: true }]}><Select options={[
          { value: 'NATURAL_RELEASE', label: '等待策略自然释放' }, { value: 'BOUNDED_LIMIT', label: '主动受限分批减仓' }, { value: 'CASH_ONLY', label: '仅当前空闲现金' },
        ].map(row => ({ ...row, disabled: !context.data?.supported_policies.includes(row.value as CapitalExitPolicy['kind']) }))} /></Form.Item>
        {policy === 'NATURAL_RELEASE' && <p className="quiet">保留原持仓并持续跟随原策略信号，限制新增买入占用退出预留。到期不足时等待处理，不会自动转为主动卖出。</p>}
        {policy !== 'CASH_ONLY' && <Form.Item label={`最晚时间（${Intl.DateTimeFormat().resolvedOptions().timeZone}）`} name="deadline" rules={[{ required: true }]}><Input type="datetime-local" /></Form.Item>}
        {policy === 'BOUNDED_LIMIT' && <><p className="quiet">从真实持仓生成限价界限。每批受初始现金缺口、价限内买盘和成本约束；最小成交额不满足时需重新预览，不自动增加数量。</p>
          <div className="form-columns"><Form.Item label="累计经济损失上限" name="maxCost" extra="每笔原参考金额减去含费净回款，正损失累计；有利价格只抵本笔费用。" rules={[{ required: true }]}><Input inputMode="decimal" suffix={value.currency} /></Form.Item><Form.Item label="最大单批名义金额（按参考买价计）" name="maxSlice" extra="默认建议为初始净现金缺口的 10%；金额过小不会自动放大。按参考买价限制批量，实际成交价改善时回款可高于该金额。" rules={[{ required: true }]}><Input inputMode="decimal" suffix={value.currency} /></Form.Item></div>
          <div className="form-columns"><Form.Item label="最小批次间隔（秒）" name="minimumInterval" rules={[{ required: true }]}><Input inputMode="numeric" /></Form.Item><Form.Item label="价限内买盘最多参与比例" name="participation" extra="0.1 表示 10%，可以收紧。" rules={[{ required: true }]}><Input inputMode="decimal" /></Form.Item></div>
          <Table components={{ table: TableSurface }} rowKey="instrument_id" size="small" pagination={false} scroll={{ x: 680 }} dataSource={context.data?.instruments} columns={[
            { title: '真实持仓', dataIndex: 'symbol' }, { title: '可减数量', dataIndex: 'eligible_quantity' },
            { title: '原生买价 / 卖价', render: (_cell, row) => <><Exact value={row.bid_price} /> / <Exact value={row.ask_price} /></> },
            { title: '数量 / 价格增量', render: (_cell, row) => <>{row.quantity_increment} / {row.price_increment}</> },
            { title: '报价时间', render: (_cell, row) => <Timestamp value={row.quote_asof} /> },
          ]} />
          <Form.List name="legs">{(fields, actions) => <>{fields.map(field => <div className="bounded-leg" key={field.key}><Form.Item label="减仓持仓" name={[field.name, 'instrumentId']} rules={[{ required: true }]}><Select options={context.data?.instruments.map(row => ({ value: row.instrument_id, label: `${row.symbol} · 可减 ${row.eligible_quantity}`, disabled: (decimalUnits(row.eligible_quantity) ?? 0n) <= 0n }))} onChange={id => { const row = context.data?.instruments.find(item => item.instrument_id === id); if (row) { form.setFieldValue(['legs', field.name, 'quantity'], row.eligible_quantity); form.setFieldValue(['legs', field.name, 'minimumPrice'], row.bid_price ?? ''); } }} /></Form.Item><div className="form-columns"><Form.Item label="最多减仓数量" name={[field.name, 'quantity']} rules={[{ required: true }]}><Input inputMode="decimal" /></Form.Item><Form.Item label="最低卖出价格" name={[field.name, 'minimumPrice']} rules={[{ required: true }]}><Input inputMode="decimal" suffix={value.currency} /></Form.Item></div><Button type="text" onClick={() => actions.remove(field.name)}>移除此持仓</Button></div>)}<Button onClick={() => actions.add()}>添加可减持仓</Button></>}</Form.List>
        </>}
        <Button htmlType="submit" loading={previewing || previewCommand.busy}>预览退出计划</Button>
      </Form>
      {previewCommand.pending && <Button loading={previewCommand.busy} onClick={() => { void previewCommand.submit(); }}>核对原预览</Button>}
      <ErrorNotice error={inputError ?? previewCommand.error} />
    </details>
    {preview && <div className="capital-preview"><h4>原生退出计划预览</h4><Facts items={[
      ['请求金额', <Exact value={preview.funds.requested.amount} unit={preview.funds.requested.currency} />], ['剩余可交易资金', <Exact value={preview.funds.remaining_managed_capital?.amount} unit={value.currency} />],
      [preview.policy.kind === 'BOUNDED_LIMIT' ? '当前批次及已发生经济损失估计' : '预计经济损失', <Exact value={preview.funds.estimated_execution_cost?.amount} unit={value.currency} />], ['既有未实现盈亏（非提取成本）', <Exact value={preview.funds.existing_unrealized_pnl?.amount} unit={value.currency} />],
      ['预览有效至', <Timestamp value={preview.valid_until} />], ['当前预览', validPreview ? '有效' : '已失效，请按当前账户依据重新预览'],
    ]} />{preview.policy.kind === 'BOUNDED_LIMIT' && <p className="quiet">预览仅核验当前可执行批次；后续批次仍按授权数量、最低价、总成本和新行情逐批核验，不保证截止前全部完成。</p>}{preview.reason_codes.map(reason => <Reason key={reason} value={reason} />)}
      <Table components={{ table: TableSurface }} rowKey="instrument_id" size="small" pagination={false} scroll={{ x: 430 }} dataSource={preview.reduction_legs} columns={[{ title: '减仓标的', dataIndex: 'instrument_id' }, { title: <MetricLabel metricKey="maximumReductionQuantity" />, dataIndex: 'maximum_reduction_quantity' }, { title: <MetricLabel metricKey="minimumSellPrice" />, dataIndex: 'minimum_sell_price' }]} />
      <details className="provenance-disclosure"><summary>拟撤销与保留保护单</summary><NativeOrders orders={preview.proposed_cancellations} title="拟撤销增加敞口订单" /><NativeOrders orders={preview.retained_protective_orders} title="保留保护单" /></details>
      {previewForExitId || preview.resume_exit_id ? <p role="status">此预览用于继续原资金计划，请在该计划中选择“按新预览继续”。</p> : <Button disabled={!canInitiate || !validPreview || busy} onClick={() => confirm('开始准备退出资金？', `为 ${preview.funds.requested.amount} ${preview.funds.requested.currency} 建立退出预留，按预览政策准备现金，余资按新预算继续交易。`, () => { void command.submit({ action: 'START', previewId: preview.id }); })}>开始准备退出资金</Button>}
    </div>}
    <PendingCommand command={command} />{command.pending && <Button loading={command.busy} onClick={() => { void command.submit(); }}>核对原资金命令</Button>}<ErrorNotice error={command.error} />
    {current?.commands?.map(item => <div className="quiet" role="status" key={item.requestId}>资金命令 <Status plain value={item.state} label={({ PENDING: '已提交，等待接收', UNCERTAIN: '提交结果待核对', ACCEPTED: '已受理，继续跟踪资金计划', REJECTED: '已拒绝' })[item.state]} />{item.error && <Reason value={item.error} />}<details><summary>原请求身份</summary><code>{item.requestId}</code></details></div>)}
    {!!current?.exits.length && <Form.Item label="查看资金计划"><Select value={selectedId} onChange={choosePlan} options={current.exits.map(row => ({ value: row.id, label: `${row.funds.requested_amount.amount} ${row.funds.requested_amount.currency} · ${exitPhase[row.state] ?? row.state}` }))} /></Form.Item>}
    <ErrorNotice error={detail.error} retry={() => { void detail.refetch(); }} />
    {exit && <article className="capital-intent"><h4>资金计划 · {exit.preparation_state === 'WAITING_EXTERNAL_TRANSFER' ? '现货现金已备妥，等待外部划转' : exit.state === 'READY' && exit.funds.withdrawability === 'SIMULATED' ? '模拟资金准备就绪' : exit.state === 'READY' && !capitalEvidenceIsCurrent(exit, now) ? '等待重新核实平台可提取额' : exitPhase[exit.state] ?? exit.state}</h4><CapitalFundsSummary exit={exit} now={now} />{(decimalUnits(exit.funds.reserved_amount.amount) ?? 0n) === 0n && (decimalUnits(exit.funds.staged_for_withdrawal_amount.amount) ?? 0n) > 0n && <p className="quiet">现货预留已为零，继续核对外部划转与提现，无需重新减仓准备原请求金额。</p>}{exit.reason_codes.map(reason => <Reason key={reason} value={reason} />)}
      <Table components={{ table: TableSurface }} rowKey="instrument_id" size="small" pagination={false} scroll={{ x: 650 }} dataSource={exit.progress} columns={[{ title: '标的', dataIndex: 'instrument_id' }, { title: '授权最多数量', dataIndex: 'authorized_quantity' }, { title: '已成交数量', dataIndex: 'filled_quantity' }, { title: '剩余授权数量', dataIndex: 'remaining_quantity' }, { title: '当前原生订单', render: (_cell, row) => row.active_order_ids.join(' / ') || '无' }, { title: '已发生经济损失', render: (_cell, row) => <Exact value={row.execution_cost?.amount} unit={row.execution_cost?.currency} /> }, { title: '真实手续费（单列）', render: (_cell, row) => <Exact value={row.actual_fees?.amount} unit={row.actual_fees?.currency} missing="尚未核实" /> }]} />
      <div className="capital-actions"><Button disabled={busy || previewCommand.busy || !!previewCommand.pending || (decimalUnits(exit.funds.reserved_amount.amount) ?? 0n) <= 0n} onClick={() => { setEditingExitId(exit.id); saveDraft(`capital-editing-exit:${value.id}`, exit.id); const fields = policyFields(exit.policy, exit.funds.reserved_amount.amount); if (fields.legs) fields.legs = fields.legs.map(row => ({ ...row, quantity: exit.progress.find(progress => progress.instrument_id === row.instrumentId)?.remaining_quantity ?? row.quantity })).filter(row => (decimalUnits(row.quantity) ?? 0n) > 0n); form.setFieldsValue(fields); clearPreview(); }}>按现货剩余预留重新预览</Button>
        {(['PAUSE', 'CANCEL', 'RESUME', 'RELEASE_RESERVATION'] as const).map(action => { const available = capitalAction(exit, action); const resumeReady = action !== 'RESUME' || capitalResumePreviewMatches(exit, preview, current, previewForExitId, now); return <div className="capital-action" key={action}><Button disabled={busy || !available.allowed || !resumeReady} onClick={() => confirm(`${actionLabel[action]}？`, action === 'RELEASE_RESERVATION' ? `解除现货预留 ${exit.funds.reserved_amount.amount} ${value.currency}，并取消外部待提取 ${exit.funds.staged_for_withdrawal_amount.amount} ${value.currency} 的退出义务。外部资金仍在原处，不会增加现货余额。` : '下游按当前计划修订核对原生订单。暂停和取消继续保留资金预留。', () => { void command.submit({ action, exitId: exit.id, expectedRevision: exit.revision, ...(action === 'RESUME' ? { previewId: preview!.id } : {}) }); })}>{actionLabel[action]}</Button>{!available.allowed && available.reason && <Reason value={available.reason} />}{!resumeReady && <p className="quiet">请先按此计划的现货剩余预留重新预览。</p>}</div>; })}
      </div>
      <ExternalEvidence action="RECONCILE_EXTERNAL_TRANSFER" exit={exit} busy={busy} submit={input => { void command.submit(input); }} />
      <ExternalEvidence action="RECONCILE_WITHDRAWAL" exit={exit} busy={busy} submit={input => { void command.submit(input); }} />
      <details className="provenance-disclosure"><summary>最近原生活动与计划身份</summary><Facts items={[["计划身份 / 修订", `${exit.id} / ${exit.revision}`], ["最近更新", <Timestamp value={exit.updated_at} />]]} /><Table components={{ table: TableSurface }} rowKey="id" size="small" pagination={{ pageSize: 10, hideOnSinglePage: true }} dataSource={exit.events} columns={[{ title: '时间', render: (_cell, row) => <Timestamp value={row.at} /> }, { title: '事件', dataIndex: 'message' }, { title: '原始引用', dataIndex: 'reference' }]} /></details>
      <a className="text-link" href={href(`/observations/${encodeURIComponent(value.id)}?section=capital&exitId=${encodeURIComponent(exit.id)}`)}>此计划固定入口</a>
    </article>}
  </section>;
}
function ExternalEvidence({ action, exit, busy, submit }: { action: 'RECONCILE_EXTERNAL_TRANSFER' | 'RECONCILE_WITHDRAWAL'; exit: CapitalExitView; busy: boolean; submit: (input: object) => void }) {
  const confirm = useConfirm(); const available = capitalAction(exit, action); const transfer = action === 'RECONCILE_EXTERNAL_TRANSFER';
  const title = transfer ? '对账已在外部完成的账户间划转' : '对账已在外部完成的提现';
  return <details className="provenance-disclosure"><summary>{title}</summary><p className="quiet">{transfer ? '在 Hyperliquid 平台自行完成 spot 与 perp 间划转后，提交原始流水引用。划转不计为提现。' : '在外部完成提现后提交原始流水；下游核对真实现金变动后才累计已提取本金。'}</p>{!available.allowed && available.reason && <Reason value={available.reason} />}
    <Form layout="vertical" disabled={busy || !available.allowed} scrollToFirstError={{ focus: true }} onFinish={(fields: { amount: string; externalTransferRef: string }) => confirm(`${title}？`, '提交仅请求核对真实原生资金流水，不能凭手工填报改变资金完成状态。', () => submit({ action, exitId: exit.id, expectedRevision: exit.revision, amount: fields.amount, currency: exit.scope.currency, externalTransferRef: fields.externalTransferRef }))}>
      <Form.Item label={transfer ? '外部划转本金' : '源账户已提取本金'} name="amount" rules={[{ required: true }, { validator: (_rule, input: string) => (decimalUnits(input ?? '') ?? 0n) > 0n ? Promise.resolve() : Promise.reject(new Error('请输入精确正金额')) }]}><Input inputMode="decimal" suffix={exit.scope.currency} /></Form.Item>
      <Form.Item label="原始外部资金流水引用" name="externalTransferRef" rules={[{ required: true, whitespace: true }]}><Input /></Form.Item><Button htmlType="submit">提交原生流水对账</Button>
    </Form>
  </details>;
}
