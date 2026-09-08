import { Alert, Button, Modal, Space, Typography } from 'antd';
import { useIsMutating } from '@tanstack/react-query';
import { useContext, useEffect, useRef, useState } from 'react';
import { GuardContext, useOnline } from './ui';

/** Browser lifecycle glue only; Vite/Workbox owns generation and static caching. */
export function PwaUpdate() {
  const { blocked } = useContext(GuardContext); const mutating = useIsMutating();
  const online = useOnline();
  const [waiting, setWaiting] = useState<ServiceWorker>();
  const [dismissed, setDismissed] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [ready, setReady] = useState(false);
  const [failure, setFailure] = useState(false);
  const consent = useRef(false); const reloaded = useRef(false);
  const protectedWork = useRef(false); protectedWork.current = blocked || mutating > 0;
  useEffect(() => {
    if (!import.meta.env.PROD || !('serviceWorker' in navigator)) return;
    let disposed = false;
    let registration: ServiceWorkerRegistration | undefined;
    const observed = new Set<ServiceWorker>();
    function observe(worker: ServiceWorker | null) {
      if (!worker || observed.has(worker)) return;
      observed.add(worker); worker.addEventListener('statechange', changed);
    }
    function changed() {
      if (!disposed && registration?.waiting && navigator.serviceWorker.controller) setWaiting(registration.waiting);
    }
    function found() { if (registration) observe(registration.installing); }
    let previousController = navigator.serviceWorker.controller;
    function activate() {
      const next = navigator.serviceWorker.controller;
      if (disposed || next === previousController || reloaded.current) return;
      const hadController = previousController !== null;
      previousController = next;
      if (!hadController) return;
      setWaiting(undefined); setInstalling(false);
      // Another tab may activate the new worker, but cannot authorize this tab
      // to reload. Keep a manual reload action instead of a stale waiting worker.
      if (!consent.current || protectedWork.current) { setReady(true); return; }
      reloaded.current = true; window.location.reload();
    }
    function check() {
      if (navigator.onLine && document.visibilityState === 'visible' && registration) {
        void registration.update().catch(() => { if (!disposed) setFailure(true); });
      }
    }
    navigator.serviceWorker.addEventListener('controllerchange', activate);
    document.addEventListener('visibilitychange', check);
    const timer = window.setInterval(check, 60 * 60 * 1000);
    void navigator.serviceWorker.register('/sw.js', { scope: '/', updateViaCache: 'none' }).then(value => {
      if (disposed) return;
      registration = value;
      value.addEventListener('updatefound', found); observe(value.installing); changed();
    }).catch(() => { if (!disposed) setFailure(true); });
    return () => {
      disposed = true; clearInterval(timer);
      navigator.serviceWorker.removeEventListener('controllerchange', activate);
      document.removeEventListener('visibilitychange', check);
      registration?.removeEventListener('updatefound', found);
      observed.forEach(worker => worker.removeEventListener('statechange', changed));
    };
  }, []);
  const unavailable = !online || blocked || mutating > 0 || installing;
  function update() {
    if (unavailable) return;
    if (ready) { reloaded.current = true; window.location.reload(); return; }
    if (!waiting) return;
    consent.current = true; setInstalling(true);
    waiting.postMessage({ type: 'SKIP_WAITING' });
  }
  return <>
    {(waiting || ready) && <Button onClick={() => setDismissed(false)}>有新版本</Button>}
    {failure && <Typography.Text type="secondary">版本检查暂不可用</Typography.Text>}
    <Modal open={!!(waiting || ready) && !dismissed} title="检测到新的前端版本" okText="确认更新" cancelText="稍后" onCancel={() => { if (!installing) setDismissed(true); }} onOk={update}
      okButtonProps={{ 'aria-label': '确认更新', 'aria-busy': installing, disabled: unavailable }} confirmLoading={installing} closable={!installing} maskClosable={!installing}>
      <Space orientation="vertical" className="full-width">
        <Typography.Paragraph>确认后刷新本页，载入已准备好的静态版本。服务器上的运行不会因此停止。</Typography.Paragraph>
        {(blocked || mutating > 0) && <Alert type="warning" showIcon title="请先保存表单或关闭操作确认窗口，再更新。未保存内容不会被自动清除。" />}
        {!online && <Alert type="warning" showIcon title="当前离线，请恢复连接后更新。" />}
        <Typography.Paragraph type="secondary">仅缓存静态文件；业务数据、认证、证据、审批和事件流不缓存，也不在恢复联网时自动补交操作。</Typography.Paragraph>
      </Space>
    </Modal>
  </>;
}
