import { App, type FormInstance } from 'antd';
import { useLayoutEffect, useRef, useSyncExternalStore } from 'react';
import { ApiFailure, Intent } from './api';
import { setSettingsWork } from './settings-work';

type Result = { revision: string; updated_at: string };
type Write<T> = (values: T, revision: string, intent: Intent) => Promise<Result>;
type Request<T> = { values: T; revision: string; intent: Intent };
type Reload<T> = () => Promise<Result & { values: T; resource: unknown }>;
type Snapshot = { error?: unknown; saving: boolean; uncertain?: boolean; revision: string; updated_at: string; resource?: unknown };
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

function withRegisteredRefs<T extends object>(base: T, values: T): T {
  const next = { ...base } as Record<string, unknown>;
  const current = values as Record<string, unknown>;
  for (const field of ['credential_ref', 'ca_certificate_ref']) {
    if (current[field]) next[field] = current[field];
  }
  if (next.ca_certificate_ref && next.tls_policy === 'SYSTEM_CA') {
    next.tls_policy = 'PINNED_CA';
    next.development_http = false;
  }
  return next as T;
}

class Autosave<T extends object> {
  readonly subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  readonly getSnapshot = () => this.snapshot;
  private listeners = new Set<() => void>();
  private snapshot: Snapshot;
  private form?: FormInstance<T>;
  private write?: Write<T>;
  private reload?: Reload<T>;
  private notice?: () => void;
  private latest: T;
  private savedValue: T;
  private saved: string;
  private attempted?: Request<T>;
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
  private resumeOnline = () => { this.enabled = true; this.offline = false; this.flush(); };

  constructor(readonly key: string, initial: T, revision: string, updated_at: string) {
    this.latest = initial;
    this.savedValue = initial;
    this.saved = JSON.stringify(initial);
    this.snapshot = { saving: false, revision, updated_at };
  }
  private emit() {
    this.snapshot = { ...this.snapshot, uncertain: this.uncertain };
    this.listeners.forEach(listener => listener());
    setSettingsWork(`autosave:${this.key}`, this.running || this.reloading || this.uncertain || this.conflict || JSON.stringify(this.latest) !== this.saved);
  }
  attach(form: FormInstance<T>, write: Write<T>, reload: Reload<T>, notice: () => void, initial: T, revision: string, updated_at: string, enabled: boolean) {
    this.closing = false;
    this.form = form; this.write = write; this.reload = reload; this.notice = notice; this.enabled = enabled;
    if (enabled && this.offline) { this.offline = false; window.removeEventListener('online', this.resumeOnline); }
    if (!enabled && !this.offline) { this.offline = true; window.addEventListener('online', this.resumeOnline, { once: true }); }
    if (!this.running && !this.uncertain && JSON.stringify(this.latest) === this.saved && BigInt(revision) > BigInt(this.snapshot.revision)) {
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
    if (this.closing) { this.closing = false; window.clearTimeout(this.timer); this.flush(true); return; }
    const serialized = JSON.stringify(values);
    const validation = this.validation?.serialized === serialized ? this.validation.result
      : form.validateFields({ validateOnly: true }).then(() => true, () => false);
    void validation.then(valid => {
      if (this.form) return;
      this.latest = valid ? values : withRegisteredRefs(this.attempted?.values ?? this.savedValue, values);
      this.emit();
      this.flush(true);
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
    catch { this.latest = withRegisteredRefs(this.attempted?.values ?? this.savedValue, values); this.emit(); return; }
    if (JSON.stringify(form.getFieldsValue(true)) !== JSON.stringify(values)) return this.close();
    this.latest = values;
    this.flush(true);
  }
  flush(closing = false) {
    window.clearTimeout(this.timer); this.timer = undefined;
    if (this.form) {
      const values = this.form.getFieldsValue(true);
      if (JSON.stringify(values) !== JSON.stringify(this.latest)) { this.latest = values; this.emit(); }
    }
    const serialized = JSON.stringify(this.latest);
    if (serialized === this.saved && !this.running && !this.uncertain && !this.conflict && (this.rejected || this.snapshot.error)) {
      this.rejected = undefined; this.snapshot = { ...this.snapshot, error: undefined }; this.emit();
    }
    if (!this.enabled || this.offline || this.running || this.uncertain || this.conflict || serialized === this.saved || serialized === this.rejected) return;
    void this.send(this.latest, false, !closing);
  }
  retry() {
    if (!this.enabled || this.running) return;
    if (this.conflict) { void this.recoverConflict(); return; }
    if (this.uncertain && this.attempted) {
      void this.send(this.attempted.values, true);
    } else {
      this.rejected = undefined;
      this.flush();
    }
  }
  private async recoverConflict() {
    if (!this.reload || this.reloading || this.uncertain) return;
    this.reloading = true; this.snapshot = { ...this.snapshot, saving: true }; this.emit();
    try {
      const current = await this.reload();
      const rebased = { ...current.values } as Record<string, unknown>;
      const previous = this.savedValue as Record<string, unknown>;
      for (const [field, value] of Object.entries(this.latest)) {
        if (JSON.stringify(value) !== JSON.stringify(previous[field])) rebased[field] = value;
      }
      this.savedValue = current.values; this.saved = JSON.stringify(current.values); this.latest = rebased as T;
      const reconciled = JSON.stringify(this.latest) === this.saved;
      this.attempted = undefined; this.rejected = reconciled ? undefined : JSON.stringify(this.latest);
      this.uncertain = false; this.conflict = false;
      this.form?.setFieldsValue(this.latest as Partial<T>);
      this.snapshot = { saving: true, revision: current.revision, updated_at: current.updated_at, resource: current.resource,
        error: reconciled ? undefined : new ApiFailure('REVISION_CONFLICT', '配置在其他地方已更改，已保留本次编辑，请检查后重试') };
    } catch (error) { this.snapshot = { ...this.snapshot, error }; }
    finally { this.reloading = false; if (!this.running) this.snapshot = { ...this.snapshot, saving: false }; this.emit(); }
  }
  private async send(values: T, retry = false, validate = true) {
    if (!this.write || this.running || (retry && !this.attempted)) return;
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
          else { this.latest = withRegisteredRefs(this.attempted?.values ?? this.savedValue, this.latest); this.emit(); this.flush(true); }
        }
        return;
      }
      if (this.form === form && JSON.stringify(form.getFieldsValue(true)) !== JSON.stringify(values)) {
        this.running = false; this.flush(); return;
      }
    }
    // Keep the full request separate from edits and newly attached resource revisions.
    const request = retry ? this.attempted! : { values: structuredClone(values), revision: this.snapshot.revision, intent: new Intent() };
    values = request.values;
    this.attempted = request; this.snapshot = { ...this.snapshot, saving: true, error: undefined }; this.emit();
    try {
      const result = await this.write(values, request.revision, request.intent);
      request.intent.clear(); this.uncertain = false; this.conflict = false; this.attempted = undefined; this.rejected = undefined;
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
      const offline = error instanceof ApiFailure && error.code === 'OFFLINE';
      if (offline && !this.offline) {
        this.offline = true; window.addEventListener('online', this.resumeOnline, { once: true });
      }
      // A failed retry cannot resolve an earlier unknown outcome, even a definite 4xx.
      if (!this.uncertain) {
        if (error instanceof ApiFailure && error.code === 'REVISION_CONFLICT') {
          this.conflict = true; this.rejected = JSON.stringify(values); request.intent.clear();
          await this.recoverConflict();
        }
        if (rejected || offline) {
          request.intent.clear(); this.attempted = undefined;
          if (rejected && error.code !== 'REVISION_CONFLICT') this.rejected = JSON.stringify(values);
        } else this.uncertain = true;
      }
      if (this.uncertain || this.conflict || !(error instanceof ApiFailure && error.code === 'REVISION_CONFLICT')) this.notice?.();
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
    retry: () => session?.retry(), error: snapshot.error, saving: snapshot.saving, uncertain: !!snapshot.uncertain, revision: snapshot.revision,
    updated_at: snapshot.updated_at, resource: snapshot.resource };
}
