import { useState } from 'react';
import { Button } from 'antd';
import type { ObservationView } from '../../../packages/contracts/src/index.ts';
import type { NativeAccountStrategyStatus } from '../../../packages/contracts/src/account-strategy.ts';
import { useResource } from './data-hooks';
import { readDraft, saveDraft, useCommand } from './commands';
import { ErrorNotice, Facts, PendingCommand, Status, useConfirm } from './ui';

export interface StrategyReceipt {
  requestId: string; action: 'START' | 'STOP'; status: 'PENDING' | 'UNCERTAIN' | 'COMPLETED' | 'REJECTED'; error: string | null;
  receipt: { persisted?: boolean; result?: Record<string, unknown> | null; persistenceError?: string } | null;
}
export function strategyResultText(value: StrategyReceipt) {
  if (value.receipt?.persisted === false) return value.receipt.result?.state === 'STOPPED'
    ? '原生已确认停止，但远端回执未能持久化；原命令仍标为不确定，不会自动重发。'
    : '远端回执未能持久化，且未确认停止；请核对当前策略状态，不会自动重发。';
  return value.status === 'COMPLETED' ? '原命令已收到回执；不代表订单已成交或资金已退出。'
    : value.status === 'REJECTED' ? '原生明确拒绝本次启动，未启动策略。'
    : '原命令尚未确定；这里只查询同一请求，不自动重发或用当前状态代替回执。';
}
export default function AccountStrategy({ value }: { value: ObservationView }) {
  const path = `/observations/${encodeURIComponent(value.id)}/strategy`;
  const status = useResource<{ strategy: NativeAccountStrategyStatus }>(path, { poll: 3000 });
  const key = `account-strategy:${value.id}`;
  const [last, setLast] = useState(() => readDraft<string>(key));
  const confirm = useConfirm();
  const accepted = (result: StrategyReceipt) => { saveDraft(key, result.requestId); setLast(result.requestId); void status.refetch(); };
  const start = useCommand<StrategyReceipt>(`strategy:${value.id}:START`, accepted, `${path}/commands`);
  const stop = useCommand<StrategyReceipt>(`strategy:${value.id}:STOP`, accepted, `${path}/commands`);
  const requestId = (stop.pending ? stop.values?.requestId as string | undefined : start.pending ? start.values?.requestId as string | undefined : undefined) ?? last;
  const receipt = useResource<StrategyReceipt>(`${path}/commands/${encodeURIComponent(requestId ?? '')}`, { enabled: !!requestId, poll: 2000 });
  const native = status.data?.strategy;
  const supported = native?.config.kind === 'EMA_CROSS';
  const unsettled = !!requestId && (!receipt.data || ['PENDING', 'UNCERTAIN'].includes(receipt.data.status));
  return <section className="result-section"><h3>Nautilus 原生策略</h3>
    <p>账户当前仅接入官方 EMA_CROSS；ORDER_BOOK_IMBALANCE 暂仅可回测，尚未接入账户运行器。策略配置由账户实例提供，本页面不会把历史目标转成实时交易。</p>
    <p>重启默认暂停；再次启动会按官方策略重新预热。停止遵从上游配置，不保证撤销全部订单或清空持仓。资金退出请使用独立资金操作。</p>
    <ErrorNotice error={status.error} retry={() => { void status.refetch(); }} />
    {native && <><Facts items={[["账户模式", value.mode === 'live' ? '实盘：可能产生真实订单' : '模拟盘'], ["策略", native.strategy_id], ["状态", <Status value={native.state} />], ["标的", native.instrument_id], ["原生版本", native.native_version]]} /><details><summary>账户原生策略完整配置</summary><pre>{JSON.stringify(native.config, null, 2)}</pre></details></>}
    <div className="form-actions"><Button type="primary" loading={start.busy} disabled={!supported || unsettled || !!status.error || native?.state === 'RUNNING' || !!start.pending} onClick={() => confirm('启动原生策略？', `${value.mode === 'live' ? '这是实盘，会按已配置策略发送真实订单。' : '这是模拟盘。'}账户：${value.connectionName ?? value.connectionId ?? value.id}；策略：${native?.strategy_id}；标的：${native?.instrument_id}。不是仅启动观测。`, () => { void start.submit({ action: 'START' }); })}>启动{value.mode === 'live' ? '实盘' : '模拟'}策略</Button>
    <Button danger loading={stop.busy} disabled={!!stop.pending} onClick={() => confirm('停止原生策略？', '向此账户发送官方策略 STOP；依上游配置可能撤单或平仓。停止本地观测不等于停止策略。', () => { void stop.submit({ action: 'STOP' }); })}>停止策略</Button>
    <Button onClick={() => { void status.refetch(); if (requestId) void receipt.refetch(); }}>核对状态与原回执</Button></div>
    <PendingCommand command={start} /><PendingCommand command={stop} /><ErrorNotice error={start.error ?? stop.error ?? receipt.error} />
    {receipt.data && <div><h4>最近策略命令回执</h4><Facts items={[["请求", receipt.data.requestId], ["操作", receipt.data.action], ["回执", <Status value={receipt.data.status} />]]} /><p>{strategyResultText(receipt.data)}</p>{receipt.data.error && <code>{receipt.data.error}</code>}</div>}
  </section>;
}
