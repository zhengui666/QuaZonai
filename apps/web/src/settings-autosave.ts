import { App, type FormInstance } from 'antd';
import { useLayoutEffect, useRef, useSyncExternalStore } from 'react';
import { ApiFailure, Intent } from './api';
import { setSettingsWork } from './settings-work';

type Result = { revision: string; updated_at: string };
type Write<T> = (values: T, revision: string, intent: Intent) => Promise<Result>;
type Snapshot = { error?: unknown; saving: boolean; revision: string; updated_at: string };
const sessions = new Map<string, Autosave<object>>();
const empty: Snapshot = { saving: false, revision: '', updated_at: '' };
const noSubscribe = () => () => {};

function withoutConsumedRefs<T extends object>(values: T, sent: T): T {
  const next = { ...values } as Record<string, unknown>;
  const request = sent as Record<string, unknown>;
  for (const field of ['credential_ref', 'ca_certificate_ref']) {
    if (request[field] && next[field] === request[field]) delete next[field];
  }
  return next as T;
}

class Autosave<T extends object> {
  readonly subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  readonly getSnapshot = () => this.snapshot;
  private listeners = new Set<() => void>();
  private snapshot: Snapshot;
  private readonly intent = new Intent();
  private form?: FormInstance<T>;
  private write?: Write<T>;
  private notice?: () => void;
  private latest: T;
  private saved: string;
  private attempted?: T;
  private rejected?: string;
  private running = false;
  private uncertain = false;
  private enabled = false;
  private timer?: number;

  constructor(readonly key: string, initial: T, revision: string, updated_at: string) {
    this.latest = initial;
    this.saved = JSON.stringify(initial);
    this.snapshot = { saving: false, revision, updated_at };
  }
  private emit() {
    this.snapshot = { ...this.snapshot };
    this.listeners.forEach(listener => listener());
    setSettingsWork(`autosave:${this.key}`, this.running || this.uncertain || JSON.stringify(this.latest) !== this.saved);
  }
  attach(form: FormInstance<T>, write: Write<T>, notice: () => void, initial: T, revision: string, updated_at: string, enabled: boolean) {
    this.form = form; this.write = write; this.notice = notice; this.enabled = enabled;
    if (!this.running && JSON.stringify(this.latest) === this.saved && BigInt(revision) > BigInt(this.snapshot.revision)) {
      this.latest = initial; this.saved = JSON.stringify(initial); this.rejected = undefined;
      this.snapshot = { saving: false, revision, updated_at };
    }
    form.setFieldsValue(this.latest as Partial<T>);
    this.emit();
    if (enabled) this.flush();
  }
  detach(form: FormInstance<T>) {
    if (this.form !== form) return;
    this.latest = form.getFieldsValue(true);
    this.form = undefined;
    this.notice = undefined;
    this.flush(true);
  }
  change(_changed: Partial<T>, values: T) {
    this.latest = values;
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => this.flush(), 450);
    this.emit();
  }
  flush(closing = false) {
    window.clearTimeout(this.timer); this.timer = undefined;
    if (this.form) this.latest = this.form.getFieldsValue(true);
    const serialized = JSON.stringify(this.latest);
    if (!this.enabled || this.running || this.uncertain || serialized === this.saved || serialized === this.rejected) return;
    void this.send(this.latest, false, !closing);
  }
  retry() {
    if (!this.enabled || this.running) return;
    if (this.uncertain && this.attempted) {
      this.uncertain = false;
      void this.send(this.attempted, true);
    } else {
      this.rejected = undefined;
      this.flush();
    }
  }
  private async send(values: T, retry = false, validate = true) {
    if (!this.write || this.running) return;
    this.running = true;
    if (!retry && validate && this.form) {
      const form = this.form;
      try { await form.validateFields({ validateOnly: true }); }
      catch { this.running = false; if (this.form) this.emit(); else this.flush(true); return; }
      if (this.form === form && JSON.stringify(form.getFieldsValue(true)) !== JSON.stringify(values)) {
        this.running = false; this.flush(); return;
      }
    }
    this.attempted = values; this.snapshot = { ...this.snapshot, saving: true, error: undefined }; this.emit();
    try {
      const result = await this.write(values, this.snapshot.revision, this.intent);
      this.intent.clear(); this.uncertain = false; this.attempted = undefined; this.rejected = undefined;
      this.saved = JSON.stringify(withoutConsumedRefs(values, values));
      this.latest = withoutConsumedRefs(this.latest, values);
      for (const field of ['credential_ref', 'ca_certificate_ref']) {
        const sent = (values as Record<string, unknown>)[field];
        if (sent && (this.form?.getFieldsValue(true) as Record<string, unknown> | undefined)?.[field] === sent)
          this.form?.setFieldsValue({ [field]: undefined } as Partial<T>);
      }
      this.snapshot = { saving: true, revision: result.revision, updated_at: result.updated_at };
    } catch (error) {
      const rejected = error instanceof ApiFailure && !!error.problem && error.status >= 400 && error.status < 500;
      this.snapshot = { ...this.snapshot, error };
      if (rejected || (error instanceof ApiFailure && error.code === 'OFFLINE')) {
        this.intent.clear(); this.attempted = undefined; this.uncertain = false;
        if (rejected) this.rejected = JSON.stringify(values);
      } else this.uncertain = true;
      this.notice?.();
    } finally {
      this.running = false; this.snapshot = { ...this.snapshot, saving: false }; this.emit();
      if (!this.uncertain && JSON.stringify(this.latest) !== this.saved && JSON.stringify(this.latest) !== this.rejected) this.flush();
    }
  }
}

function sessionFor<T extends object>(key: string, initial: T, revision: string, updated_at: string) {
  let session = sessions.get(key) as Autosave<T> | undefined;
  if (!session) { session = new Autosave(key, initial, revision, updated_at); sessions.set(key, session as unknown as Autosave<object>); }
  return session;
}

export function useFormAutosave<T extends object>(form: FormInstance<T>, key: string | undefined, initial: T,
  revision: string | undefined, updated_at: string | undefined, enabled: boolean, write: Write<T>) {
  const { message } = App.useApp();
  const current = useRef(write); current.current = write;
  const session = key && revision && updated_at ? sessionFor(key, initial, revision, updated_at) : undefined;
  const snapshot = useSyncExternalStore(session?.subscribe ?? noSubscribe, session?.getSnapshot ?? (() => empty), () => empty);
  useLayoutEffect(() => {
    if (!session || !revision || !updated_at) return;
    session.attach(form, (values, currentRevision, intent) => current.current(values, currentRevision, intent),
      () => { void message.error('设置更新失败，请重试'); }, initial, revision, updated_at, enabled);
    return () => session.detach(form);
  }, [session, form, revision, updated_at, enabled, message]);
  return { change: (changed: Partial<T>, values: T) => session?.change(changed, values), flush: () => session?.flush(true),
    retry: () => session?.retry(), error: snapshot.error, saving: snapshot.saving, revision: snapshot.revision, updated_at: snapshot.updated_at };
}
