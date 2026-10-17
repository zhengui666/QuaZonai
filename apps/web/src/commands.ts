import { useRef, useState } from 'react';
import { ApiError, api } from './api';
import { clearResearchIntent, readResearchIntent } from './request-intent';

type Kind = 'research' | 'portfolios' | 'observations';
interface Envelope { raw: string; persisted: boolean }
const key = (kind: Kind) => `quazonai.command.${kind}.v3`;
const memory = new Map<Kind, Envelope>();
const drafts = new Map<string, { value: unknown; persisted: boolean }>();
const exitNotice = (event: BeforeUnloadEvent) => { event.preventDefault(); event.returnValue = ''; };
function updateExitNotice() {
  if (typeof window === 'undefined') return;
  window.removeEventListener('beforeunload', exitNotice);
  if ([...memory.values(), ...drafts.values()].some(item => !item.persisted)) window.addEventListener('beforeunload', exitNotice);
}
function read(kind: Kind): Envelope | undefined {
  if (memory.has(kind)) return memory.get(kind);
  try {
    const raw = sessionStorage.getItem(key(kind));
    if (raw && typeof JSON.parse(raw).requestId === 'string') { const command = { raw, persisted: true }; memory.set(kind, command); return command; }
  } catch { /* Session storage and the legacy store can fail independently. */ }
  try { if (kind === 'research') { const legacy = readResearchIntent(); if (legacy) { const command = { raw: legacy.raw, persisted: true }; memory.set(kind, command); return command; } } } catch { /* The original in-memory command is still preferred. */ }
}
function clear(kind: Kind, command: Envelope) {
  if (memory.get(kind) !== command) return false;
  memory.delete(kind); updateExitNotice();
  try { sessionStorage.removeItem(key(kind)); } catch { /* The service retains the acknowledged identity. */ }
  try { if (kind === 'research') clearResearchIntent(); } catch { /* One failed store must not prevent clearing the other. */ }
  return true;
}
export function useCommand<T>(kind: Kind, accepted: (result: T) => void) {
  const [pending, setPending] = useState<Envelope | undefined>(() => read(kind));
  const previousKey = `command.previous.${kind}`; const [previousRaw, setPreviousRaw] = useState(() => readDraft<string>(previousKey));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  const original = useRef(pending); const inFlight = useRef(false);
  async function submit(input?: object) {
    if (inFlight.current) return;
    let command = original.current;
    if (!command) {
      if (!input) return;
      const raw = JSON.stringify({ requestId: crypto.randomUUID(), ...input });
      let persisted = false;
      try { sessionStorage.setItem(key(kind), raw); persisted = true; } catch { /* Keep the original command in memory. */ }
      command = { raw, persisted };
    }
    // Each explicit attempt owns its cleanup, without changing the original request bytes.
    command = { ...command }; memory.set(kind, command); updateExitNotice(); original.current = command; setPending(command);
    inFlight.current = true; setBusy(true); setError(undefined);
    try {
      const result = await api<T>(`/${kind}`, { method: 'POST', rawBody: command.raw });
      if (clear(kind, command)) { original.current = undefined; setPending(undefined); accepted(result); }
    } catch (failure) {
      setError(failure);
      if (failure instanceof ApiError && failure.status >= 400 && failure.status < 500 && failure.status !== 401 && failure.code !== 'request_id_conflict') { clear(kind, command); if (original.current === command) { original.current = undefined; setPending(undefined); } }
    } finally { inFlight.current = false; setBusy(false); }
  }
  const canAbandon = !!pending && !busy && error instanceof ApiError && error.code === 'request_id_conflict';
  function abandon() {
    const command = original.current;
    if (!canAbandon || !command || memory.get(kind) !== command) return;
    saveDraft(previousKey, command.raw); setPreviousRaw(command.raw);
    if (clear(kind, command)) { original.current = undefined; setPending(undefined); setError(undefined); }
  }
  return { pending, busy, error, submit, abandon, canAbandon, previousRaw, previousPersisted: draftIsPersisted(previousKey), values: pending ? JSON.parse(pending.raw) as Record<string, unknown> : undefined };
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
