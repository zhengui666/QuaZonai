import { useSyncExternalStore } from 'react';

const active = new Set<string>();
const versions = new Map<string, number>();
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };

export const settingsWorkActive = () => active.size > 0;

export function setSettingsWork(key: string, pending: boolean) {
  if (active.has(key) === pending) return;
  if (pending) active.add(key); else active.delete(key);
  versions.set(key, (versions.get(key) ?? 0) + 1);
  listeners.forEach(listener => listener());
}

export function useSettingsWork() {
  return useSyncExternalStore(subscribe, settingsWorkActive, () => false);
}

export function useSettingsWorkKey(key: string) {
  return useSyncExternalStore(subscribe, () => active.has(key), () => false);
}

export function useSettingsWorkPrefix(prefix: string) {
  return useSyncExternalStore(subscribe, () => [...active].some(key => key.startsWith(prefix)), () => false);
}

export function useSettingsWorkVersion(key: string) {
  return useSyncExternalStore(subscribe, () => versions.get(key) ?? 0, () => 0);
}
