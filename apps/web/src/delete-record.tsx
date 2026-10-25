import { useState, useSyncExternalStore } from 'react';
import { Button, Modal } from 'antd';
import { useQueryClient } from '@tanstack/react-query';
import type { DeletionReceipt } from '../../../packages/contracts/src/index.ts';
import { ApiError, api } from './api';
import { readDraft, removeDraft, saveDraft } from './commands';
import { currentRoute, navigate } from './router';
import { DraftNotice, ErrorNotice } from './ui';

const records = {
  mandate: { collection: 'mandates', page: 'pm', label: '投资决策' },
  research: { collection: 'research', page: 'research', label: '研究' },
  factor: { collection: 'factors', page: 'factors', label: '因子' },
  portfolio: { collection: 'portfolios', page: 'portfolios', label: '组合' },
} as const;
type Kind = keyof typeof records;
interface DeletionCommand { kind: Kind; id: string; path: string }
interface PendingDeletion extends DeletionCommand { persisted: boolean; busy: boolean }
const draftKey = 'pending-deletions';
let pending: PendingDeletion[] | undefined;
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
const commandFor = (kind: Kind, id: string): DeletionCommand => ({ kind, id, path: `/${records[kind].collection}/${encodeURIComponent(id)}` });
function snapshot(): PendingDeletion[] {
  if (pending) return pending;
  const saved = readDraft<unknown>(draftKey);
  pending = Array.isArray(saved) ? saved.filter((value): value is DeletionCommand => value && typeof value === 'object'
    && Object.hasOwn(records, value.kind) && typeof value.id === 'string' && value.id.length > 0 && value.path === commandFor(value.kind, value.id).path)
    .map(value => ({ ...value, persisted: true, busy: false })) : [];
  return pending;
}
function publish(next: PendingDeletion[], persist = false) {
  if (persist) {
    if (next.length) {
      const persisted = saveDraft(draftKey, next.map(({ kind, id, path }) => ({ kind, id, path })));
      next = next.map(value => ({ ...value, persisted }));
    } else removeDraft(draftKey);
  }
  pending = next;
  for (const listener of listeners) listener();
}
/** Record the exact DELETE before sending; one shared entry serializes all its entrypoints. */
async function submitDeletion(command: DeletionCommand): Promise<DeletionReceipt | undefined> {
  const current = snapshot().find(value => value.path === command.path);
  if (current?.busy) return;
  if (!current) publish([...snapshot(), { ...command, persisted: false, busy: false }], true);
  publish(snapshot().map(value => value.path === command.path ? { ...value, busy: true } : value));
  try {
    const receipt = await api<DeletionReceipt>(command.path, { method: 'DELETE' });
    if (receipt.kind !== command.kind || receipt.id !== command.id || !receipt.deletedAt) throw new Error('删除回执身份不一致');
    publish(snapshot().filter(value => value.path !== command.path), true);
    return receipt;
  } catch (failure) {
    const definitive = failure instanceof ApiError && failure.status >= 400 && failure.status < 500 && ![401, 404].includes(failure.status);
    publish(definitive ? snapshot().filter(value => value.path !== command.path) : snapshot().map(value => value.path === command.path ? { ...value, busy: false } : value), definitive);
    throw failure;
  }
}
function DeleteDialog({ command, back, close }: { command: DeletionCommand; back?: string; close: () => void }) {
  const cache = useQueryClient(); const registry = useSyncExternalStore(subscribe, snapshot);
  const entry = registry.find(value => value.path === command.path); const busy = entry?.busy ?? false;
  const [error, setError] = useState<unknown>(); const record = records[command.kind];
  async function remove() {
    setError(undefined);
    try {
      const receipt = await submitDeletion(command); if (!receipt) return;
      const deletedPath = (value: unknown) => typeof value === 'string' && (value === command.path || value.startsWith(`${command.path}/`) || command.kind === 'research' && value.startsWith(`/factors/r${command.id}.`));
      await cache.cancelQueries({ predicate: query => deletedPath(query.queryKey[0]) });
      cache.removeQueries({ predicate: query => deletedPath(query.queryKey[0]) });
      close();
      const route = currentRoute();
      if (route.page === record.page && route.id === command.id) navigate(back ?? `/${record.page}`, true);
      for (const collection of ['/mandates', '/research', '/factors', '/portfolios']) void cache.invalidateQueries({ queryKey: [collection] });
    } catch (failure) { setError(failure); }
  }
  return <Modal title={`删除${record.label}？`} open destroyOnHidden maskClosable={!busy} closable={!busy} keyboard={!busy}
    okText={error ? '重试删除' : entry ? '核对原删除' : '确认删除'} cancelText="取消" confirmLoading={busy} okButtonProps={{ danger: true, disabled: busy }} cancelButtonProps={{ disabled: busy }}
    onOk={() => { void remove(); }} onCancel={() => { if (!busy) close(); }}>
    <p>删除后将从列表移除，原始试验、证据与回执历史保留。运行中的任务或仍被使用的对象须先处理相关依赖。</p>
    {entry && <p>此删除尚待核对，将沿用原始身份重试；找到记录不存在不代表已取得删除回执。</p>}
    <code>{command.id}</code><ErrorNotice error={error} />{entry && <DraftNotice saved={entry.persisted} />}
  </Modal>;
}
export function DeleteRecord({ kind, id, back }: { kind: Kind; id: string; back?: string }) {
  const [open, setOpen] = useState(false); const registry = useSyncExternalStore(subscribe, snapshot); const command = commandFor(kind, id);
  return <><Button danger disabled={registry.some(value => value.path === command.path && value.busy)} onClick={() => setOpen(true)}>删除{records[kind].label}</Button>
    {open && <DeleteDialog command={command} back={back} close={() => setOpen(false)} />}</>;
}
/** Always available after authentication, including a deleted object's 404 detail. No automatic writes. */
export function DeletionRecovery() {
  const registry = useSyncExternalStore(subscribe, snapshot); const [selected, setSelected] = useState<DeletionCommand>();
  return <>{registry.length > 0 && <section className="deletion-recovery" aria-label="待核对删除"><h2>待核对删除</h2><p>原删除结果未确认。请按原身份核对，收到匹配回执后才会结束此记录。</p>
    {registry.map(value => <div className="deletion-recovery-row" key={value.path}><span>{records[value.kind].label} <code>{value.id}</code></span><Button disabled={value.busy} loading={value.busy} onClick={() => setSelected(value)}>核对删除{records[value.kind].label}</Button><DraftNotice saved={value.persisted} /></div>)}
  </section>}{selected && <DeleteDialog key={selected.path} command={selected} close={() => setSelected(undefined)} />}</>;
}
