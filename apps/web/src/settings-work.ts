import { useSyncExternalStore } from 'react';

const active = new Set<string>();
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };

export const settingsWorkActive = () => active.size > 0;

export function setSettingsWork(key: string, pending: boolean) {
  if (active.has(key) === pending) return;
  if (pending) active.add(key); else active.delete(key);
  listeners.forEach(listener => listener());
}

export function useSettingsWork() {
  return useSyncExternalStore(subscribe, settingsWorkActive, () => false);
}

export function useSettingsWorkKey(key: string) {
  return useSyncExternalStore(subscribe, () => active.has(key), () => false);
}
