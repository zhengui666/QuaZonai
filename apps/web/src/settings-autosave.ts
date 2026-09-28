import { App, type FormInstance } from 'antd';
import { useLayoutEffect, useRef, useSyncExternalStore } from 'react';
import { ApiFailure, Intent } from './api';
import { setSettingsWork } from './settings-work';

type Result = { revision: string; updated_at: string };
type Write<T> = (values: T, revision: string, intent: Intent) => Promise<Result>;
type Reload<T> = () => Promise<Result & { values: T; resource: unknown }>;
type Snapshot = { error?: unknown; saving: boolean; revision: string; updated_at: string; resource?: unknown };
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
  private reload?: Reload<T>;
  private notice?: () => void;
  private latest: T;
  private savedValue: T;
  private saved: string;
  private attempted?: T;
  private rejected?: string;
  private running = false;
  private uncertain = false;
  private conflict = false;
  private reloading = false;
  private offline = false;
  private enabled = false;
  private closing = false;
  private validation?: { serialized: string; result: Promise<boolean> };
  private timer?: number;
  private resumeOnline = () => { this.offline = false; this.flush(); };

  constructor(readonly key: string, initial: T, revision: string, updated_at: string) {
    this.latest = initial;
    this.savedValue = initial;
    this.saved = JSON.stringify(initial);
    this.snapshot = { saving: false, revision, updated_at };
  }
  private emit() {
    this.snapshot = { ...this.snapshot };
    this.listeners.forEach(listener => listener());
    setSettingsWork(`autosave:${this.key}`, this.running || this.reloading || this.uncertain || this.conflict || JSON.stringify(this.latest) !== this.saved);
  }
  attach(form: FormInstance<T>, write: Write<T>, reload: Reload<T>, notice: () => void, initial: T, revision: string, updated_at: string, enabled: boolean) {
    this.closing = false;
    this.form = form; this.write = write; this.reload = reload; this.notice = notice; this.enabled = enabled;
    if (enabled && this.offline) { this.offline = false; window.removeEventListener('online', this.resumeOnline); }
    if (!this.running && JSON.stringify(this.latest) === this.saved && BigInt(revision) > BigInt(this.snapshot.revision)) {
      this.latest = initial; this.savedValue = initial; this.saved = JSON.stringify(initial); this.rejected = undefined;
      this.snapshot = { saving: false, revision, updated_at };
    }
    form.setFieldsValue(this.latest as Partial<T>);
    this.emit();
    if (enabled) this.flush();
  }
  detach(form: FormInstance<T>) {
    if (this.form !== form) return;
    const values = form.getFieldsValue(true);
    this.form = undefined;
    this.notice = undefined;
    if (this.closing) { this.closing = false; window.clearTimeout(this.timer); return; }
    const serialized = JSON.stringify(values);
    const validation = this.validation?.serialized === serialized ? this.validation.result
      : form.validateFields({ validateOnly: true }).then(() => true, () => false);
    void validation.then(valid => {
      if (this.form) return;
      this.latest = valid ? values : this.attempted ?? this.savedValue;
      if (valid) this.flush(true); else this.emit();
    });
  }
  change(_changed: Partial<T>, values: T) {
    this.latest = values;
    this.validation = { serialized: JSON.stringify(values),
      result: this.form?.validateFields({ validateOnly: true }).then(() => true, () => false) ?? Promise.resolve(false) };
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => this.flush(), 450);
    this.emit();
  }
  async close(): Promise<void> {
    window.clearTimeout(this.timer); this.timer = undefined;
    const form = this.form;
    if (!form) return;
    this.closing = true;
    const values = form.getFieldsValue(true);
    try { await form.validateFields({ validateOnly: true }); }
    catch { this.latest = this.attempted ?? this.savedValue; this.emit(); return; }
    if (JSON.stringify(form.getFieldsValue(true)) !== JSON.stringify(values)) return this.close();
    this.latest = values;
    this.flush(true);
  }
  flush(closing = false) {
    window.clearTimeout(this.timer); this.timer = undefined;
    if (this.form) this.latest = this.form.getFieldsValue(true);
    const serialized = JSON.stringify(this.latest);
    if (!this.enabled || this.offline || this.running || this.uncertain || this.conflict || serialized === this.saved || serialized === this.rejected) return;
    void this.send(this.latest, false, !closing);
  }
  retry() {
    if (!this.enabled || this.running) return;
    if (this.conflict) { void this.recoverConflict(); return; }
    if (this.uncertain && this.attempted) {
      this.uncertain = false;
      void this.send(this.attempted, true);
    } else {
      this.rejected = undefined;
      this.flush();
    }
  }
  private async recoverConflict() {
    if (!this.reload || this.reloading) return;
    this.reloading = true; this.snapshot = { ...this.snapshot, saving: true }; this.emit();
    try {
      const current = await this.reload();
      this.savedValue = current.values; this.saved = JSON.stringify(current.values); this.latest = current.values;
      this.attempted = undefined; this.rejected = undefined; this.uncertain = false; this.conflict = false; this.intent.clear();
      this.form?.setFieldsValue(current.values as Partial<T>);
      this.snapshot = { saving: true, revision: current.revision, updated_at: current.updated_at, resource: current.resource,
        error: new ApiFailure('REVISION_CONFLICT', '配置在其他地方已更改，已载入最新版本，请重新编辑') };
    } catch (error) { this.snapshot = { ...this.snapshot, error }; }
    finally { this.reloading = false; if (!this.running) this.snapshot = { ...this.snapshot, saving: false }; this.emit(); }
  }
  private async send(values: T, retry = false, validate = true) {
    if (!this.write || this.running) return;
    this.running = true;
    if (!retry && validate && this.form) {
      const form = this.form;
      try { await form.validateFields({ validateOnly: true }); }
      catch {
        this.running = false;
        if (this.form) this.emit();
        else {
          const validation = this.validation?.serialized === JSON.stringify(this.latest) ? this.validation.result : Promise.resolve(false);
          if (await validation) this.flush(true);
          else { this.latest = this.attempted ?? this.savedValue; this.emit(); }
        }
        return;
      }
      if (this.form === form && JSON.stringify(form.getFieldsValue(true)) !== JSON.stringify(values)) {
        this.running = false; this.flush(); return;
      }
    }
    this.attempted = values; this.snapshot = { ...this.snapshot, saving: true, error: undefined }; this.emit();
    try {
      const result = await this.write(values, this.snapshot.revision, this.intent);
      this.intent.clear(); this.uncertain = false; this.conflict = false; this.attempted = undefined; this.rejected = undefined;
      this.savedValue = withoutConsumedRefs(values, values);
      this.saved = JSON.stringify(this.savedValue);
      this.latest = withoutConsumedRefs(this.latest, values);
      for (const field of ['credential_ref', 'ca_certificate_ref']) {
        const sent = (values as Record<string, unknown>)[field];
        if (sent && (this.form?.getFieldsValue(true) as Record<string, unknown> | undefined)?.[field] === sent)
          this.form?.setFieldsValue({ [field]: undefined } as Partial<T>);
      }
      this.snapshot = { saving: true, revision: result.revision, updated_at: result.updated_at, resource: result };
    } catch (error) {
      const rejected = error instanceof ApiFailure && !!error.problem && error.status >= 400 && error.status < 500;
      this.snapshot = { ...this.snapshot, error };
      if (error instanceof ApiFailure && error.code === 'REVISION_CONFLICT') {
        this.conflict = true; this.rejected = JSON.stringify(values); this.intent.clear(); this.uncertain = false;
        await this.recoverConflict();
      }
      if (rejected || (error instanceof ApiFailure && error.code === 'OFFLINE')) {
        this.intent.clear(); this.attempted = undefined; this.uncertain = false;
        if (error instanceof ApiFailure && error.code === 'OFFLINE' && !this.offline) {
          this.offline = true; window.addEventListener('online', this.resumeOnline, { once: true });
        }
        if (rejected && error.code !== 'REVISION_CONFLICT') this.rejected = JSON.stringify(values);
      } else this.uncertain = true;
      if (this.conflict || !(error instanceof ApiFailure && error.code === 'REVISION_CONFLICT')) this.notice?.();
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
  revision: string | undefined, updated_at: string | undefined, enabled: boolean, write: Write<T>, reload: Reload<T>) {
  const { message } = App.useApp();
  const current = useRef(write); current.current = write;
  const currentReload = useRef(reload); currentReload.current = reload;
  const session = key && revision && updated_at ? sessionFor(key, initial, revision, updated_at) : undefined;
  const snapshot = useSyncExternalStore(session?.subscribe ?? noSubscribe, session?.getSnapshot ?? (() => empty), () => empty);
  useLayoutEffect(() => {
    if (!session || !revision || !updated_at) return;
    session.attach(form, (values, currentRevision, intent) => current.current(values, currentRevision, intent), () => currentReload.current(),
      () => { void message.error('设置更新失败，请重试'); }, initial, revision, updated_at, enabled);
    return () => session.detach(form);
  }, [session, form, revision, updated_at, enabled, message]);
  return { change: (changed: Partial<T>, values: T) => session?.change(changed, values), close: async () => { await session?.close(); },
    retry: () => session?.retry(), error: snapshot.error, saving: snapshot.saving, revision: snapshot.revision,
    updated_at: snapshot.updated_at, resource: snapshot.resource };
}
