import { Alert, Button, Descriptions, Empty, Skeleton, Space, Tag, Typography } from 'antd';
import { createContext, useCallback, useContext, useEffect, useId, useState, useSyncExternalStore } from 'react';
import type { ReactNode } from 'react';
import { ApiFailure, displayTime } from './api';

export const GuardContext = createContext({
  blocked: false,
  setGuard: (_id: string, _active: boolean) => {},
});
export function GuardProvider({ children }: { children: ReactNode }) {
  const [guards, setGuards] = useState<Set<string>>(new Set());
  const blocked = guards.size > 0;
  useEffect(() => {
    if (!blocked) return;
    const stop = (event: BeforeUnloadEvent) => { event.preventDefault(); event.returnValue = ''; };
    window.addEventListener('beforeunload', stop);
    return () => window.removeEventListener('beforeunload', stop);
  }, [blocked]);
  const setGuard = useCallback((id: string, active: boolean) => setGuards(previous => {
    if (previous.has(id) === active) return previous;
    const next = new Set(previous);
    if (active) next.add(id); else next.delete(id);
    return next;
  }), []);
  return <GuardContext.Provider value={{ blocked, setGuard }}>{children}</GuardContext.Provider>;
}
export function useGuard(active: boolean) {
  const id = useId();
  const { setGuard } = useContext(GuardContext);
  useEffect(() => {
    setGuard(id, active);
    return () => setGuard(id, false);
  }, [id, active, setGuard]);
}
function subscribeOnline(callback: () => void) {
  window.addEventListener('online', callback);
  window.addEventListener('offline', callback);
  return () => { window.removeEventListener('online', callback); window.removeEventListener('offline', callback); };
}
export function useOnline() {
  return useSyncExternalStore(subscribeOnline, () => navigator.onLine, () => false);
}
export function useClock() {
  const [now, setNow] = useState(Date.now());
  useEffect(() => { const timer = window.setInterval(() => setNow(Date.now()), 1000); return () => clearInterval(timer); }, []);
  return now;
}
export function ErrorNotice({ error, retry }: { error: unknown; retry?: () => void }) {
  const now = useClock();
  if (error === null || error === undefined) return null;
  const failure = error instanceof ApiFailure ? error : undefined;
  const wait = failure ? Math.max(0, Math.ceil((failure.retryAt - now) / 1000)) : 0;
  return <Alert type="error" showIcon title={failure?.message ?? '请求未完成，请重试并检查服务状态。'}
    description={<Space orientation="vertical" size="small">
      {failure?.problem && <>
        <Typography.Text>错误：{failure.code} · 请求编号：{failure.problem.request_id}</Typography.Text>
        {failure.problem.current_revision !== undefined && <Typography.Text>服务器当前版本：{failure.problem.current_revision}。请先重载；不会覆盖新版本。</Typography.Text>}
        {failure.problem.field_errors.map(field => <Typography.Text key={`${field.field}:${field.code}`}>{field.field}：{field.message}</Typography.Text>)}
      </>}
      {wait > 0 && <Typography.Text>服务器要求至少再等待 {wait} 秒。</Typography.Text>}
      {retry && <Button disabled={wait > 0} onClick={retry}>重新载入</Button>}
    </Space>} />;
}
export function QueryPanel({ pending, error, stale, reload, children }: {
  pending: boolean; error: unknown; stale?: boolean; reload: () => void; children: ReactNode;
}) {
  if (pending) return <div role="status" aria-label="正在载入"><Skeleton active paragraph={{ rows: 5 }} /></div>;
  if (error && !stale) return <ErrorNotice error={error} retry={reload} />;
  return <Space orientation="vertical" className="full-width" size="middle">
    {error ? <><Alert type="warning" title="以下是上次成功读取的数据，当前无法确认其最新状态。" showIcon /><ErrorNotice error={error} retry={reload} /></> : null}
    {children}
  </Space>;
}
const states: Record<string, string> = {
  DRAFT: '草稿', ACTIVE: '启用', PAUSED: '暂停', ARCHIVED: '归档', FROZEN: '已冻结',
  QUEUED: '排队', DISPATCHING: '派发中', RUNNING: '运行中', RECONCILING: '核对结果',
  CANCEL_REQUESTED: '已请求取消', SUCCEEDED: '运行成功', FAILED: '运行失败', CANCELLED: '已取消',
};
export function StateTag({ value }: { value: string }) {
  return <Tag>{states[value] ?? value}</Tag>;
}
export function Pager({ next, history, loading, move }: {
  next: string | null | undefined; history: (string | undefined)[]; loading: boolean;
  move: (history: (string | undefined)[]) => void;
}) {
  return <Space wrap><Button disabled={loading || history.length <= 1} onClick={() => move(history.slice(0, -1))}>上一页</Button>
    <Typography.Text>第 {history.length} 页（游标分页）</Typography.Text>
    <Button disabled={loading || !next} onClick={() => { if (next) move([...history, next]); }}>下一页</Button></Space>;
}
export function NoData({ text }: { text: string }) {
  return <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={text} />;
}
export function ResourceFacts({ id, revision, updated }: { id: string; revision: string; updated: string }) {
  return <Descriptions size="small" column={1} items={[
    { key: 'id', label: '记录编号', children: <Typography.Text className="break-word" copyable>{id}</Typography.Text> },
    { key: 'revision', label: '版本', children: revision },
    { key: 'updated', label: '最后更新', children: displayTime(updated) },
  ]} />;
}
