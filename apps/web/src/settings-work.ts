import { useSyncExternalStore } from 'react';

const active = new Set<string>();
const listeners = new Set<() => void>();

export const settingsWorkActive = () => active.size > 0;

export function setSettingsWork(key: string, pending: boolean) {
  if (active.has(key) === pending) return;
  if (pending) active.add(key); else active.delete(key);
  listeners.forEach(listener => listener());
}

export function useSettingsWork() {
  return useSyncExternalStore(listener => {
    listeners.add(listener);
    return () => { listeners.delete(listener); };
  }, settingsWorkActive, () => false);
}
