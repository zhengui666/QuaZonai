import { App } from 'antd';
import { useRef } from 'react';

export function closeDecision(pending: boolean, dirty: boolean, failed: boolean, retainedRequest = false) {
  return pending ? 'wait' : retainedRequest ? 'close' : dirty || failed ? 'confirm' : 'close';
}

/** Detach only when the caller retains the exact request outside this editor. Closing never cancels a write. */
export function useDialogClose({ pending, failed, dirty, close, retainedRequest = false }: {
  pending: boolean; failed: boolean; dirty: () => boolean; close: () => void; retainedRequest?: boolean;
}) {
  const { modal } = App.useApp();
  const confirming = useRef(false);
  return () => {
    if (confirming.current) return;
    const decision = closeDecision(pending, dirty(), failed, retainedRequest);
    if (decision === 'wait') return;
    if (decision === 'close') { close(); return; }
    confirming.current = true;
    modal.confirm({
      title: failed ? '离开当前提交窗口？' : '放弃未保存的更改？',
      content: failed
        ? '关闭不会撤销已发送的请求。若提交结果未知，请继续编辑，并在当前窗口保持原内容重试；重新打开会丢失当前重试标识。'
        : '关闭后将丢失当前输入，已发送的请求不会因此撤销。',
      okText: '确认离开', cancelText: '继续编辑',
      onOk: close,
      afterClose: () => { confirming.current = false; },
    });
  };
}
