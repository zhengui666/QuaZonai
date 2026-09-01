import { onlineManager } from '@tanstack/react-query';
import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import { useRegisterSW } from 'virtual:pwa-register/react';
import { useDisplayMode } from '../lib/useDisplayMode';

export interface PwaState {
  isStandalone: boolean;
  canInstall: boolean;
  isOnline: boolean;
  offlineReady: boolean;
  needRefresh: boolean;
  install: () => Promise<boolean>;
  applyUpdate: () => Promise<void>;
}

interface BeforeInstallPromptEvent extends Event {
  prompt: () => Promise<void>;
  userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>;
}

const emptyPwaState: PwaState = {
  isStandalone: false,
  canInstall: false,
  isOnline: true,
  offlineReady: false,
  needRefresh: false,
  install: async () => false,
  applyUpdate: async () => undefined,
};

const PwaContext = createContext<PwaState>(emptyPwaState);

export function usePwa(): PwaState {
  return useContext(PwaContext);
}

export function PwaProvider({ children }: { children: ReactNode }) {
  const { isStandalone } = useDisplayMode();
  const [isOnline, setIsOnline] = useState(() => typeof navigator === 'undefined' || navigator.onLine);
  const [installEvent, setInstallEvent] = useState<BeforeInstallPromptEvent | null>(null);
  const [offlineReady, setOfflineReady] = useState(false);
  const [needRefresh, setNeedRefresh] = useState(false);
  const { updateServiceWorker } = useRegisterSW({
    immediate: true,
    onNeedRefresh: () => setNeedRefresh(true),
    onOfflineReady: () => setOfflineReady(true),
  });

  useEffect(() => {
    const onOnline = () => {
      setIsOnline(true);
      onlineManager.setOnline(true);
    };
    const onOffline = () => {
      setIsOnline(false);
      onlineManager.setOnline(false);
    };
    const onInstallPrompt = (event: Event) => {
      event.preventDefault();
      setInstallEvent(event as BeforeInstallPromptEvent);
    };
    window.addEventListener('online', onOnline);
    window.addEventListener('offline', onOffline);
    window.addEventListener('beforeinstallprompt', onInstallPrompt);
    onlineManager.setOnline(navigator.onLine);
    return () => {
      window.removeEventListener('online', onOnline);
      window.removeEventListener('offline', onOffline);
      window.removeEventListener('beforeinstallprompt', onInstallPrompt);
    };
  }, []);

  const value = useMemo<PwaState>(() => ({
    isStandalone,
    canInstall: !isStandalone && installEvent !== null,
    isOnline,
    offlineReady,
    needRefresh,
    install: async () => {
      if (!installEvent) return false;
      await installEvent.prompt();
      const choice = await installEvent.userChoice;
      setInstallEvent(null);
      return choice.outcome === 'accepted';
    },
    applyUpdate: async () => {
      await updateServiceWorker(true);
    },
  }), [installEvent, isOnline, isStandalone, needRefresh, offlineReady, updateServiceWorker]);

  return <PwaContext.Provider value={value}>{children}</PwaContext.Provider>;
}
