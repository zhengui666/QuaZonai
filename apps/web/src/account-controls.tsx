import { useEffect, useState } from 'react';
import { Button, Checkbox, Form, Input, Table } from 'antd';
import { useQueryClient } from '@tanstack/react-query';
import type { ObservationView } from '../../../packages/contracts/src/index.ts';
import type { CapitalExitStatus, CapitalExitView } from '../../../packages/contracts/src/capital-exit.ts';
import { api } from './api';
import { decimalUnits, useCommand } from './commands';
import { useResource } from './data-hooks';
import { ErrorNotice, Exact, Facts, MetricLabel, PendingCommand, Status, TableSurface, Timestamp, useConfirm } from './ui';
import { Reason } from './diagnostics';
import { capitalAction } from './capital-input';

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

export function CapitalFundsSummary({ exit }: { exit: CapitalExitView; now?: number }) {
  return <><p role="status">历史金额原值，不能据此认定当前可提资金或解除预留。</p><Facts items={[
    ['原请求金额', <Exact value={exit.funds.requested_amount.amount} unit={exit.funds.requested_amount.currency} />],
    ['保留的现货预留', <Exact value={exit.funds.reserved_amount.amount} unit={exit.funds.reserved_amount.currency} />],
    ['保留的外部待提现金额', <Exact value={exit.funds.staged_for_withdrawal_amount.amount} unit={exit.funds.staged_for_withdrawal_amount.currency} />],
    ['历史已对账本金', <Exact value={exit.funds.reconciled_withdrawal_amount.amount} unit={exit.funds.reconciled_withdrawal_amount.currency} />],
    ['历史手续费', <Exact value={exit.funds.actual_fees?.amount} unit={exit.funds.actual_fees?.currency} missing="未记录" />],
    ['原证据时间', <Timestamp value={exit.funds.evidence_asof} />],
  ]}/><p>新规划、恢复执行和释放预留入口已退役。停止原订单不会自动解除资金义务。</p></>;
}
export function CapitalControls({ value }: { value: ObservationView }) {
  const path=`/observations/${encodeURIComponent(value.id)}/capital-exits`;
  const status=useResource<CapitalExitStatus>(path,{poll:5000});const confirm=useConfirm();
  const command=useCommand<{requestId:string;state:string}>(`capital:${value.id}`,()=>{void status.refetch();},`${path}/commands`);
  return <section className="account-control-section"><h3>历史资金退出与原订单</h3><p>自研资金规划、费用估算、切片和恢复执行已退役。以下金额为保留记录，不是新核定可提资金；未决预留不会自动释放。</p>
    <ErrorNotice error={status.error}/><PendingCommand command={command}/>
    {(status.data?.exits??value.capitalExit?.exits??[]).map(exit=><article key={exit.id}><h4>{exit.id}</h4><Status value={exit.state}/><CapitalFundsSummary exit={exit}/>
      {(['PAUSE','CANCEL'] as const).map(action=><Button key={action} disabled={command.busy||!capitalAction(exit,action).allowed} onClick={()=>confirm('停止原资金计划？','只停止并核对原订单，保留资金预留，不创建新订单。',()=>{void command.submit({action,exitId:exit.id,expectedRevision:exit.revision});})}>{action==='PAUSE'?'暂停原计划':'取消并保留预留'}</Button>)}
      <details><summary>原始历史记录</summary><pre>{JSON.stringify(exit,null,2)}</pre></details></article>)}
  </section>;
}
