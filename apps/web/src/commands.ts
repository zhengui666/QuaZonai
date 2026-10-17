import { useRef, useState, useSyncExternalStore } from 'react';
import { ApiError, api } from './api';
import { clearResearchIntent, readResearchIntent } from './request-intent';

type Kind = 'research' | 'portfolios' | 'observations';
interface Envelope { raw: string; persisted: boolean }
const key = (kind: Kind) => `quazonai.command.${kind}.v3`;
interface CommandState { pending?: Envelope; error?: unknown }
const memory = new Map<Kind, CommandState>();
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
const drafts = new Map<string, { value: unknown; persisted: boolean }>();
const exitNotice = (event: BeforeUnloadEvent) => { event.preventDefault(); event.returnValue = ''; };
function updateExitNotice() {
  if (typeof window === 'undefined') return;
  window.removeEventListener('beforeunload', exitNotice);
  if ([...memory.values()].some(item => item.pending && !item.pending.persisted) || [...drafts.values()].some(item => !item.persisted)) window.addEventListener('beforeunload', exitNotice);
}
function read(kind: Kind): CommandState {
  const cached = memory.get(kind); if (cached) return cached;
  let pending: Envelope | undefined;
  try { const raw = sessionStorage.getItem(key(kind)); if (raw && typeof JSON.parse(raw).requestId === 'string') pending = { raw, persisted: true }; }
  catch { /* Session storage and the legacy store can fail independently. */ }
  try { if (!pending && kind === 'research') { const legacy = readResearchIntent(); if (legacy) pending = { raw: legacy.raw, persisted: true }; } }
  catch { /* The original in-memory command is still preferred. */ }
  const state = { pending }; memory.set(kind, state); return state;
}
function publish(kind: Kind, state: CommandState) { memory.set(kind, state); updateExitNotice(); for (const listener of listeners) listener(); }
function clear(kind: Kind, command: Envelope, error?: unknown) {
  if (read(kind).pending !== command) return false;
  try { sessionStorage.removeItem(key(kind)); } catch { /* The service retains the acknowledged identity. */ }
  try { if (kind === 'research') clearResearchIntent(); } catch { /* One failed store must not prevent clearing the other. */ }
  publish(kind, { error }); return true;
}
export function useCommand<T>(kind: Kind, accepted: (result: T) => void) {
  const { pending, error } = useSyncExternalStore(subscribe, () => read(kind));
  const initialRaw = useRef(pending?.raw);
  const previousKey = `command.previous.${kind}`; const [previousRaw, setPreviousRaw] = useState(() => readDraft<string>(previousKey));
  const [busy, setBusy] = useState(false); const inFlight = useRef(false);
  async function submit(input?: object) {
    if (inFlight.current) return;
    let command = read(kind).pending;
    if (!command) {
      if (!input) return;
      const raw = JSON.stringify({ requestId: crypto.randomUUID(), ...input });
      let persisted = false;
      try { sessionStorage.setItem(key(kind), raw); persisted = true; } catch { /* Keep the original command in memory. */ }
      command = { raw, persisted };
    }
    // Each explicit attempt owns its cleanup, without changing the original request bytes.
    command = { ...command }; publish(kind, { pending: command });
    inFlight.current = true; setBusy(true);
    try {
      const result = await api<T>(`/${kind}`, { method: 'POST', rawBody: command.raw });
      if (clear(kind, command)) accepted(result);
    } catch (failure) {
      if (failure instanceof ApiError && failure.status >= 400 && failure.status < 500 && failure.status !== 401 && failure.code !== 'request_id_conflict') clear(kind, command, failure);
      else if (read(kind).pending === command) publish(kind, { pending: command, error: failure });
    } finally { inFlight.current = false; setBusy(false); }
  }
  const canAbandon = !!pending && !busy && error instanceof ApiError && error.code === 'request_id_conflict';
  function abandon() {
    const command = read(kind).pending;
    if (!canAbandon || !command || command !== pending) return;
    saveDraft(previousKey, command.raw); setPreviousRaw(command.raw); clear(kind, command);
  }
  const rawValues = pending?.raw ?? initialRaw.current;
  return { pending, busy, error, submit, abandon, canAbandon, previousRaw, previousPersisted: draftIsPersisted(previousKey), values: rawValues ? JSON.parse(rawValues) as Record<string, unknown> : undefined };
}
export function readDraft<T>(key: string): T | undefined { if (drafts.has(key)) return drafts.get(key)!.value as T; try { const raw = sessionStorage.getItem(`quazonai.draft.${key}`); if (!raw) return; const value = JSON.parse(raw) as T; drafts.set(key, { value, persisted: true }); return value; } catch { return; } }
export function draftIsPersisted(key: string) { return drafts.get(key)?.persisted ?? true; }
export function saveDraft(key: string, value: unknown) { let persisted = false; try { sessionStorage.setItem(`quazonai.draft.${key}`, JSON.stringify(value)); persisted = true; } catch { /* Retain the nonsecret edit in this runtime. */ } drafts.set(key, { value, persisted }); updateExitNotice(); return persisted; }
export function removeDraft(key: string) { drafts.delete(key); updateExitNotice(); try { sessionStorage.removeItem(`quazonai.draft.${key}`); } catch { /* Nothing to remove when storage is unavailable. */ } }
export function decimalUnits(value: string): bigint | undefined {
  if (!/^-?\d+(?:\.\d{1,18})?$/.test(value)) return;
  const negative = value.startsWith('-'); const [whole, fraction = ''] = (negative ? value.slice(1) : value).split('.');
  return (negative ? -1n : 1n) * BigInt(whole! + fraction.padEnd(18, '0'));
}
