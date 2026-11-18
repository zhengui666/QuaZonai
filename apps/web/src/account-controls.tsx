import { useTranslation } from 'react-i18next';
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
 const { t } = useTranslation('settings');

  const cache = useQueryClient(); const [saving, setSaving] = useState(false); const [error, setError] = useState<unknown>(); const [form] = Form.useForm();
  useEffect(() => { form.setFieldsValue(value.automaticPolicy ?? { enabled: false, cashBuffer: '0', maximumSnapshotAgeSeconds: 60 }); }, [value.id, value.automaticPolicy?.enabled, value.automaticPolicy?.cashBuffer, value.automaticPolicy?.maximumSnapshotAgeSeconds]);
  const allocation = value.allocation;
  return <section className="account-control-section"><h3>{t("Adjust automatically to account equity")}</h3>
    <Form form={form} layout="vertical" disabled={saving || value.status === 'STOPPED'} onFinish={(fields: { enabled: boolean; cashBuffer: string; maximumSnapshotAgeSeconds: string | number }) => { setSaving(true); setError(undefined); void api<ObservationView>(`/observations/${encodeURIComponent(value.id)}/automatic`, { method: 'POST', body: { enabled: !!fields.enabled, cashBuffer: fields.cashBuffer, maximumSnapshotAgeSeconds: Number(fields.maximumSnapshotAgeSeconds) } }).then(() => cache.invalidateQueries({ queryKey: ['/observations'] }), setError).finally(() => setSaving(false)); }}>
      <Form.Item name="enabled" valuePropName="checked"><Checkbox>{t("Enable automatic adjustment")}</Checkbox></Form.Item><div className="form-columns"><Form.Item label={t("Explicit cash buffer")} name="cashBuffer" rules={[{ required: true }, { validator: (_rule, input: string) => (decimalUnits(input ?? '') ?? -1n) >= 0n ? Promise.resolve() : Promise.reject(new Error(t("Enter an exact non-negative amount"))) }]}><Input inputMode="decimal" suffix={value.currency} /></Form.Item><Form.Item label={t("Maximum account snapshot age (seconds)")} name="maximumSnapshotAgeSeconds" rules={[{ required: true }]}><Input inputMode="numeric" /></Form.Item></div><Button htmlType="submit" loading={saving}>{t("Save automatic adjustment settings")}</Button><ErrorNotice error={error} />
    </Form>
    {allocation && <><Facts items={[[t("Latest adjustment"), <Timestamp value={allocation.updatedAt} />], [t("Status"), ({ PROJECTED: t("Capital projection only; not delivered"), BLOCKED: t("Delivery blocked"), SIZED: t("Targets calculated"), DELIVERED: t("Targets received (not proof of fills)"), UNCHANGED: t("Targets unchanged"), UNCERTAIN: t("Delivery outcome needs reconciliation"), STOPPED: t("Local automatic adjustment stopped") } as Record<string, string>)[allocation.status] ?? allocation.status], [t("Reason"), allocation.reason ? <Reason value={allocation.reason} /> : t("Not provided")], [t("Account equity"), <Exact value={allocation.equity} unit={value.currency} />], [t("Total account reservations"), <Exact value={allocation.reserved} unit={value.currency} />], [t("Explicit cash buffer"), <Exact value={allocation.cashBuffer} />], [t("Equity available after reservations"), <Exact value={allocation.budget} />], [t("Effective strategy capital"), <Exact value={allocation.capital} />], [t("Conditional capacity limit"), <Exact value={allocation.capacity} />], [t("Cash remaining after fees"), <Exact value={allocation.residualCash} />], [t("Snapshot / control revision / adjustment sequence"), `${allocation.sequence ?? t("Unknown")} / ${allocation.controlRevision ?? t("Unknown")} / ${allocation.epoch}`]]} />
      <Table components={{ table: TableSurface }} rowKey="factorId" size="small" pagination={false} scroll={{ x: 460 }} dataSource={allocation.members} columns={[{ title: t("Original factor / portfolio member"), dataIndex: 'factorId' }, { title: <MetricLabel metricKey="strategyCapital" label={t("Suggested capital for this adjustment")} />, render: (_cell, row) => <Exact value={row.capital} unit={value.currency} /> }, { title: <MetricLabel metricKey="scale" />, render: (_cell, row) => <Exact value={row.scale} /> }]} />
      {allocation.receipt && <div className="adjustment-receipt"><h4>{t("Latest target delivery receipt")}</h4><Facts items={[[t("Original receipt ID"), allocation.receipt.id], [t("Downstream status"), allocation.receipt.status], [t("Source expiry (not renewed)"), <Timestamp value={allocation.receipt.expiresAt} />], [t("Downstream ID"), allocation.receipt.externalId ?? t("Not provided")]]} /></div>}</>}
  </section>;
}

export function CapitalFundsSummary({ exit }: { exit: CapitalExitView; now?: number }) {
 const { t } = useTranslation('settings');

  return <><p role="status">{t("Historical amounts (not currently withdrawable funds)")}</p><Facts items={[
    [t("Originally requested amount"), <Exact value={exit.funds.requested_amount.amount} unit={exit.funds.requested_amount.currency} />],
    [t("Retained spot reservation"), <Exact value={exit.funds.reserved_amount.amount} unit={exit.funds.reserved_amount.currency} />],
    [t("Retained external withdrawal staging"), <Exact value={exit.funds.staged_for_withdrawal_amount.amount} unit={exit.funds.staged_for_withdrawal_amount.currency} />],
    [t("Historically reconciled principal"), <Exact value={exit.funds.reconciled_withdrawal_amount.amount} unit={exit.funds.reconciled_withdrawal_amount.currency} />],
    [t("Historical fees"), <Exact value={exit.funds.actual_fees?.amount} unit={exit.funds.actual_fees?.currency} missing={t("Not recorded")} />],
    [t("Original evidence time"), <Timestamp value={exit.funds.evidence_asof} />],
  ]}/><p>{t("New planning, resumption, and reservation release are disabled.")}</p></>;
}
export function CapitalControls({ value }: { value: ObservationView }) {
 const { t } = useTranslation('settings');

  const path=`/observations/${encodeURIComponent(value.id)}/capital-exits`;
  const status=useResource<CapitalExitStatus>(path,{poll:5000});const confirm=useConfirm();
  const command=useCommand<{requestId:string;state:string}>(`capital:${value.id}`,()=>{void status.refetch();},`${path}/commands`);
  return <section className="account-control-section"><h3>{t("Historical capital exits and original orders")}</h3><p>{t("Pausing and cancelling original capital plans is temporarily unavailable.")}</p>

    <Facts items={[[t("Original downstream reservation (not an official balance)"), <Exact value={(status.data??value.capitalExit)?.reserved??null}/>]]}/>
    {((status.data??value.capitalExit)?.unsettledReservations??[]).map(fact=><div key={fact.requestId}><code>{fact.requestId}</code> <Status value={fact.state}/> <Exact value={fact.reserved}/></div>)}
    <ErrorNotice error={status.error}/><PendingCommand command={command}/>
    {(status.data?.exits??value.capitalExit?.exits??[]).map(exit=><article key={exit.id}><h4>{exit.id}</h4><Status value={exit.state}/><CapitalFundsSummary exit={exit}/>
      {(['PAUSE','CANCEL'] as const).map(action=><Button key={action} disabled={command.busy||!capitalAction(exit,action).allowed} onClick={()=>confirm(t("Stop the original capital plan?"),t("Stop and reconcile only the original orders. Keep reservations and create no new orders."),()=>{void command.submit({action,exitId:exit.id,expectedRevision:exit.revision});})}>{action==='PAUSE'?t("Pause original plan"):t("Cancel and retain reservations")}</Button>)}
      <details><summary>{t("Original historical record")}</summary><pre>{JSON.stringify(exit,null,2)}</pre></details></article>)}
  </section>;
}
