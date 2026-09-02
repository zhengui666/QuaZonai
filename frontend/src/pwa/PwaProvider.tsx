import { onlineManager } from '@tanstack/react-query';
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useRegisterSW } from 'virtual:pwa-register/react';
import { useDisplayMode } from '../lib/useDisplayMode';

export const UPDATE_CHECK_INTERVAL_MS = 15 * 60 * 1000;
export const UPDATE_CHECK_MIN_GAP_MS = 60 * 1000;

export type PwaUpdatePhase = 'idle' | 'available' | 'applying' | 'failed';

export interface PwaState {
  isStandalone: boolean;
  canInstall: boolean;
  isOnline: boolean;
  offlineReady: boolean;
  needRefresh: boolean;
  updatePromptOpen: boolean;
  updatePhase: PwaUpdatePhase;
  updateError: string | null;
  install: () => Promise<boolean>;
  checkForUpdate: () => Promise<void>;
  dismissUpdatePrompt: () => void;
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
  updatePromptOpen: false,
  updatePhase: 'idle',
  updateError: null,
  install: async () => false,
  checkForUpdate: async () => undefined,
  dismissUpdatePrompt: () => undefined,
  applyUpdate: async () => undefined,
};

function normalizeError(error: unknown): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === 'string' && error.trim()) return error;
  return 'pwa_update_failed';
}

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
  const [updatePromptOpen, setUpdatePromptOpen] = useState(false);
  const [updatePhase, setUpdatePhase] = useState<PwaUpdatePhase>('idle');
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [registration, setRegistration] = useState<ServiceWorkerRegistration | null>(null);
  const registrationRef = useRef<ServiceWorkerRegistration | null>(null);
  const promptedWaitingWorkerRef = useRef<ServiceWorker | null>(null);
  const updateAvailableRef = useRef(false);
  const lastUpdateCheckAtRef = useRef<number | null>(null);
  const updateCheckPromiseRef = useRef<Promise<void> | null>(null);
  const applyUpdatePromiseRef = useRef<Promise<void> | null>(null);

  const markUpdateAvailable = useCallback((registered?: ServiceWorkerRegistration | null) => {
    const waitingWorker = registered?.waiting ?? registrationRef.current?.waiting ?? null;
    const isNewWaitingWorker = waitingWorker === null
      ? !updateAvailableRef.current
      : promptedWaitingWorkerRef.current !== waitingWorker;
    if (waitingWorker) promptedWaitingWorkerRef.current = waitingWorker;
    updateAvailableRef.current = true;
    setNeedRefresh(true);
    if (!isNewWaitingWorker) return;
    setUpdatePhase('available');
    setUpdateError(null);
    setUpdatePromptOpen(true);
  }, []);

  const clearUpdateState = useCallback(() => {
    promptedWaitingWorkerRef.current = null;
    updateAvailableRef.current = false;
    setNeedRefresh(false);
    setUpdatePromptOpen(false);
    setUpdatePhase('idle');
    setUpdateError(null);
  }, []);

  const { updateServiceWorker } = useRegisterSW({
    immediate: true,
    onRegisteredSW: (_swUrl, nextRegistration) => {
      const currentRegistration = nextRegistration ?? null;
      registrationRef.current = currentRegistration;
      setRegistration(currentRegistration);
      if (currentRegistration?.waiting) markUpdateAvailable(currentRegistration);
    },
    onNeedRefresh: () => markUpdateAvailable(),
    onOfflineReady: () => setOfflineReady(true),
    onRegisterError: () => undefined,
  });

  const checkForUpdate = useCallback(async () => {
    if (!registration || typeof navigator === 'undefined' || !navigator.onLine) return;
    if (typeof document !== 'undefined' && document.visibilityState !== 'visible') return;
    if (registration.waiting) {
      markUpdateAvailable(registration);
      return;
    }
    if (promptedWaitingWorkerRef.current) clearUpdateState();
    if (registration.installing) return;

    const inFlight = updateCheckPromiseRef.current;
    if (inFlight) {
      await inFlight;
      return;
    }

    const now = Date.now();
    const lastCheckAt = lastUpdateCheckAtRef.current;
    if (lastCheckAt !== null && now - lastCheckAt < UPDATE_CHECK_MIN_GAP_MS) return;
    lastUpdateCheckAtRef.current = now;

    const updatePromise = (async () => {
      try {
        await registration.update();
      } catch {
        // A background check is best effort. The next trigger will retry.
      }
    })();
    updateCheckPromiseRef.current = updatePromise;
    try {
      await updatePromise;
    } finally {
      if (updateCheckPromiseRef.current === updatePromise) updateCheckPromiseRef.current = null;
    }
  }, [clearUpdateState, markUpdateAvailable, registration]);

  const dismissUpdatePrompt = useCallback(() => {
    setUpdatePromptOpen(false);
  }, []);

  const applyUpdate = useCallback(async () => {
    const inFlight = applyUpdatePromiseRef.current;
    if (inFlight) return inFlight;

    setUpdatePhase('applying');
    setUpdateError(null);
    const updatePromise = (async () => {
      try {
        await updateServiceWorker(true);
        const waitingWorker = registrationRef.current?.waiting ?? null;
        if (waitingWorker) {
          setUpdatePhase('available');
          setUpdatePromptOpen(true);
        } else {
          clearUpdateState();
        }
      } catch (error) {
        setUpdatePhase('failed');
        setUpdateError(normalizeError(error));
        setUpdatePromptOpen(true);
        throw error;
      }
    })();
    applyUpdatePromiseRef.current = updatePromise;
    try {
      await updatePromise;
    } finally {
      if (applyUpdatePromiseRef.current === updatePromise) applyUpdatePromiseRef.current = null;
    }
  }, [clearUpdateState, updateServiceWorker]);

  useEffect(() => {
    const onOnline = () => {
      setIsOnline(true);
      onlineManager.setOnline(true);
      void checkForUpdate();
    };
    const onOffline = () => {
      setIsOnline(false);
      onlineManager.setOnline(false);
    };
    const onInstallPrompt = (event: Event) => {
      event.preventDefault();
      setInstallEvent(event as BeforeInstallPromptEvent);
    };
    const onControllerChange = () => {
      if (!registrationRef.current?.waiting && promptedWaitingWorkerRef.current) clearUpdateState();
    };
    window.addEventListener('online', onOnline);
    window.addEventListener('offline', onOffline);
    window.addEventListener('beforeinstallprompt', onInstallPrompt);
    navigator.serviceWorker?.addEventListener('controllerchange', onControllerChange);
    onlineManager.setOnline(navigator.onLine);
    return () => {
      window.removeEventListener('online', onOnline);
      window.removeEventListener('offline', onOffline);
      window.removeEventListener('beforeinstallprompt', onInstallPrompt);
      navigator.serviceWorker?.removeEventListener('controllerchange', onControllerChange);
    };
  }, [checkForUpdate, clearUpdateState]);

  useEffect(() => {
    const checkWhenVisible = () => {
      if (document.visibilityState === 'visible') void checkForUpdate();
    };
    const interval = window.setInterval(checkWhenVisible, UPDATE_CHECK_INTERVAL_MS);
    document.addEventListener('visibilitychange', checkWhenVisible);
    return () => {
      window.clearInterval(interval);
      document.removeEventListener('visibilitychange', checkWhenVisible);
    };
  }, [checkForUpdate]);

  const install = useCallback(async () => {
    if (!installEvent) return false;
    await installEvent.prompt();
    const choice = await installEvent.userChoice;
    setInstallEvent(null);
    return choice.outcome === 'accepted';
  }, [installEvent]);

  const value = useMemo<PwaState>(() => ({
    isStandalone,
    canInstall: !isStandalone && installEvent !== null,
    isOnline,
    offlineReady,
    needRefresh,
    updatePromptOpen,
    updatePhase,
    updateError,
    install,
    checkForUpdate,
    dismissUpdatePrompt,
    applyUpdate,
  }), [applyUpdate, checkForUpdate, dismissUpdatePrompt, install, installEvent, isOnline, isStandalone, needRefresh, offlineReady, updateError, updatePhase, updatePromptOpen]);

  return <PwaContext.Provider value={value}>{children}</PwaContext.Provider>;
}
