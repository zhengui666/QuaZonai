import { useRef, useState } from 'react';
import { Button, Modal } from 'antd';
import { useQueryClient } from '@tanstack/react-query';
import type { DeletionReceipt } from '../../../packages/contracts/src/index.ts';
import { api } from './api';
import { currentRoute, navigate } from './router';
import { ErrorNotice } from './ui';

const records = {
  mandate: { collection: 'mandates', page: 'pm', label: '投资决策' },
  research: { collection: 'research', page: 'research', label: '研究' },
  factor: { collection: 'factors', page: 'factors', label: '因子' },
  portfolio: { collection: 'portfolios', page: 'portfolios', label: '组合' },
} as const;

/** DELETE retries keep the exact resource identity; the service retains its receipt. */
export function DeleteRecord({ kind, id, back }: { kind: keyof typeof records; id: string; back?: string }) {
  const cache = useQueryClient(); const record = records[kind];
  const [open, setOpen] = useState(false); const [busy, setBusy] = useState(false); const [error, setError] = useState<unknown>();
  const inFlight = useRef(false); const path = `/${record.collection}/${encodeURIComponent(id)}`;
  async function remove() {
    if (inFlight.current) return;
    inFlight.current = true; setBusy(true); setError(undefined);
    try {
      const receipt = await api<DeletionReceipt>(path, { method: 'DELETE' });
      if (receipt.kind !== kind || receipt.id !== id || !receipt.deletedAt) throw new Error('删除回执身份不一致');
      const deletedPath = (value: unknown) => typeof value === 'string' && (value === path || value.startsWith(`${path}/`) || kind === 'research' && value.startsWith(`/factors/r${id}.`));
      await cache.cancelQueries({ predicate: query => deletedPath(query.queryKey[0]) });
      cache.removeQueries({ predicate: query => deletedPath(query.queryKey[0]) });
      setOpen(false);
      const route = currentRoute();
      if (route.page === record.page && route.id === id) navigate(back ?? `/${record.page}`, true);
      // Research deletion also removes its factor projection and available consumers.
      for (const collection of ['/mandates', '/research', '/factors', '/portfolios']) void cache.invalidateQueries({ queryKey: [collection] });
    } catch (failure) { setError(failure); }
    finally { inFlight.current = false; setBusy(false); }
  }
  return <><Button danger onClick={() => { setError(undefined); setOpen(true); }}>删除{record.label}</Button>
    <Modal title={`删除${record.label}？`} open={open} destroyOnHidden maskClosable={!busy} closable={!busy} keyboard={!busy}
      okText={error ? '重试删除' : '确认删除'} cancelText="取消" confirmLoading={busy} okButtonProps={{ danger: true, disabled: busy }} cancelButtonProps={{ disabled: busy }}
      onOk={() => { void remove(); }} onCancel={() => { if (!busy) setOpen(false); }}>
      <p>删除后将从列表移除，原始试验、证据与回执历史保留。运行中的任务或仍被使用的对象须先处理相关依赖。</p>
      <code>{id}</code><ErrorNotice error={error} />
    </Modal></>;
}
