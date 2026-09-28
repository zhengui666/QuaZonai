import { App, type FormInstance } from 'antd';
import { useEffect, useRef, useState } from 'react';

export function useFormAutosave<T extends object>(form: FormInstance<T>, initial: T, enabled: boolean, save: (values: T) => Promise<T | void>) {
  const { message } = App.useApp();
  const [error, setError] = useState<unknown>();
  const [saving, setSaving] = useState(false);
  const saved = useRef(JSON.stringify(initial));
  const latest = useRef(initial);
  const attempted = useRef<T | undefined>(undefined);
  const running = useRef(false);
  const failed = useRef(false);
  const timer = useRef<number | undefined>(undefined);
  const saveRef = useRef(save);
  const enabledRef = useRef(enabled);
  saveRef.current = save;
  enabledRef.current = enabled;

  async function send(values: T, retry = false) {
    if (running.current || !enabledRef.current || (failed.current && !retry)) return;
    running.current = true;
    if (!retry) {
      try { await form.validateFields({ validateOnly: true }); }
      catch { running.current = false; return; }
      if (JSON.stringify(form.getFieldsValue(true)) !== JSON.stringify(values)) {
        running.current = false; void flush(); return;
      }
    }
    setSaving(true); setError(undefined); attempted.current = values;
    try {
      const canonical = await saveRef.current(values);
      saved.current = JSON.stringify(canonical ?? values); failed.current = false; attempted.current = undefined;
    } catch (failure) {
      failed.current = true; setError(failure); void message.error('设置更新失败，请重试');
    } finally {
      running.current = false; setSaving(false);
      latest.current = form.getFieldsValue(true);
      if (!failed.current && JSON.stringify(latest.current) !== saved.current) void flush();
    }
  }
  async function flush() {
    window.clearTimeout(timer.current);
    latest.current = form.getFieldsValue(true);
    if (JSON.stringify(latest.current) !== saved.current) void send(latest.current);
  }
  function change(_changed: Partial<T>, values: T) {
    latest.current = values;
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => { void flush(); }, 450);
  }
  function retry() {
    if (!enabledRef.current || !attempted.current || running.current) return;
    failed.current = false;
    void send(attempted.current, true);
  }
  useEffect(() => { if (enabled) void flush(); }, [enabled]);
  return { change, flush, retry, error, saving };
}
