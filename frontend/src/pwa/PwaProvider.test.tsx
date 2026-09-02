import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider, useMutation } from '@tanstack/react-query';
import { Theme } from '@radix-ui/themes';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { I18nProvider } from '../i18n';
import { OnlineStatusBanner } from './OnlineStatusBanner';
import { PwaProvider, UPDATE_CHECK_INTERVAL_MS, usePwa } from './PwaProvider';
import { PwaUpdateDialog } from './PwaUpdateDialog';

type RegisterOptions = {
  onNeedRefresh?: () => void;
  onRegisteredSW?: (swUrl: string, registration?: ServiceWorkerRegistration) => void;
  onOfflineReady?: () => void;
  onRegisterError?: (error: Error) => void;
};

const swMock = vi.hoisted(() => ({
  options: null as RegisterOptions | null,
  updateServiceWorker: vi.fn<(reloadPage?: boolean) => Promise<void>>(),
}));

vi.mock('virtual:pwa-register/react', () => ({
  useRegisterSW: (options: RegisterOptions) => {
    swMock.options = options;
    return {
      needRefresh: [false, vi.fn()],
      offlineReady: [false, vi.fn()],
      updateServiceWorker: swMock.updateServiceWorker,
    };
  },
}));

function makeRegistration(options?: {
  waiting?: ServiceWorker | null;
  installing?: ServiceWorker | null;
  update?: () => Promise<void>;
}) {
  return {
    waiting: options?.waiting ?? null,
    installing: options?.installing ?? null,
    update: options?.update ?? vi.fn(async () => undefined),
  } as unknown as ServiceWorkerRegistration;
}

function setOnline(value: boolean) {
  Object.defineProperty(navigator, 'onLine', { configurable: true, value });
}

function setVisibility(value: DocumentVisibilityState) {
  Object.defineProperty(document, 'visibilityState', { configurable: true, value });
}

function register(registration: ServiceWorkerRegistration) {
  act(() => swMock.options?.onRegisteredSW?.('/sw.js', registration));
}

function StateProbe() {
  const { needRefresh, updatePhase, updatePromptOpen, updateError } = usePwa();
  return <output data-testid="pwa-state" data-need-refresh={String(needRefresh)} data-phase={updatePhase} data-prompt-open={String(updatePromptOpen)} data-error={updateError ?? ''} />;
}

function ActionProbe() {
  const { checkForUpdate, needRefresh, applyUpdate } = usePwa();
  return (
    <>
      <button onClick={() => { void checkForUpdate(); }}>Check for update</button>
      {needRefresh ? <button onClick={() => { void applyUpdate().catch(() => undefined); }}>Manual update</button> : null}
    </>
  );
}

function PendingMutationProbe() {
  const mutation = useMutation({ mutationFn: async () => new Promise<void>(() => undefined) });
  return <button onClick={() => mutation.mutate()}>Start mutation</button>;
}

function renderPwa(children: React.ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(
    <I18nProvider initialLocale="en">
      <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
        <QueryClientProvider client={client}>
          <PwaProvider>{children}</PwaProvider>
        </QueryClientProvider>
      </Theme>
    </I18nProvider>,
  );
}

const originalOnline = navigator.onLine;
const originalVisibility = document.visibilityState;

afterEach(() => {
  vi.useRealTimers();
  swMock.options = null;
  swMock.updateServiceWorker.mockReset();
  swMock.updateServiceWorker.mockResolvedValue(undefined);
  setOnline(originalOnline);
  setVisibility(originalVisibility);
  vi.unstubAllGlobals();
});

describe('PWA lifecycle', () => {
  it('keeps install capability and credentials in memory only', async () => {
    renderPwa(<StateProbe />);
    expect(screen.getByTestId('pwa-state')).toHaveAttribute('data-need-refresh', 'false');
    const prompt = vi.fn(async () => undefined);
    const event = new Event('beforeinstallprompt', { cancelable: true }) as Event & { prompt: () => Promise<void>; userChoice: Promise<{ outcome: 'accepted' }> };
    event.prompt = prompt;
    event.userChoice = Promise.resolve({ outcome: 'accepted' });
    act(() => window.dispatchEvent(event));
    expect(prompt).not.toHaveBeenCalled();
    expect(document.querySelectorAll('script')).toHaveLength(0);
  });

  it('does not show a dialog until a new worker is reported', () => {
    renderPwa(<PwaUpdateDialog />);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(swMock.updateServiceWorker).not.toHaveBeenCalled();
  });

  it('opens a confirmation dialog without reloading automatically', async () => {
    renderPwa(<><PwaUpdateDialog /><StateProbe /><ActionProbe /></>);
    act(() => swMock.options?.onNeedRefresh?.());
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
    expect(screen.getByText('A new frontend is ready. Updating reloads this page; unsaved input may be lost.')).toBeInTheDocument();
    expect(swMock.updateServiceWorker).not.toHaveBeenCalled();
    expect(screen.getByTestId('pwa-state')).toHaveAttribute('data-phase', 'available');
  });

  it('closes on Later while retaining the manual update entry', async () => {
    const user = userEvent.setup();
    renderPwa(<><PwaUpdateDialog /><StateProbe /><ActionProbe /></>);
    act(() => swMock.options?.onNeedRefresh?.());
    await user.click(screen.getByRole('button', { name: 'Later' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.getByTestId('pwa-state')).toHaveAttribute('data-need-refresh', 'true');
    expect(screen.getByRole('button', { name: 'Manual update' })).toBeInTheDocument();
    expect(swMock.updateServiceWorker).not.toHaveBeenCalled();
  });

  it('applies a confirmed update with the reload flag', async () => {
    const user = userEvent.setup();
    renderPwa(<PwaUpdateDialog />);
    act(() => swMock.options?.onNeedRefresh?.());
    await user.click(await screen.findByRole('button', { name: 'Update now' }));
    expect(swMock.updateServiceWorker).toHaveBeenCalledTimes(1);
    expect(swMock.updateServiceWorker).toHaveBeenCalledWith(true);
  });

  it('prevents duplicate apply requests while updating', async () => {
    const user = userEvent.setup();
    let resolveUpdate!: () => void;
    swMock.updateServiceWorker.mockImplementationOnce(() => new Promise<void>((resolve) => { resolveUpdate = resolve; }));
    renderPwa(<PwaUpdateDialog />);
    act(() => swMock.options?.onNeedRefresh?.());
    const button = await screen.findByRole('button', { name: 'Update now' });
    await user.click(button);
    await waitFor(() => expect(screen.getByRole('button', { name: 'Updating…' })).toBeDisabled());
    expect(swMock.updateServiceWorker).toHaveBeenCalledTimes(1);
    resolveUpdate();
  });

  it('keeps the dialog open and offers retry after an apply failure', async () => {
    const user = userEvent.setup();
    swMock.updateServiceWorker.mockRejectedValueOnce(new Error('network down'));
    renderPwa(<><PwaUpdateDialog /><StateProbe /></>);
    act(() => swMock.options?.onNeedRefresh?.());
    await user.click(await screen.findByRole('button', { name: 'Update now' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Update failed. Check your connection and try again.');
    expect(screen.getByRole('button', { name: 'Retry update' })).toBeInTheDocument();
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByTestId('pwa-state')).toHaveAttribute('data-need-refresh', 'true');
    expect(screen.getByTestId('pwa-state')).toHaveAttribute('data-phase', 'failed');
  });

  it('reopens failure feedback after a manual update attempt', async () => {
    const user = userEvent.setup();
    const waitingWorker = {} as ServiceWorker;
    const registration = makeRegistration({ waiting: waitingWorker });
    swMock.updateServiceWorker.mockRejectedValueOnce(new Error('network down'));
    renderPwa(<><PwaUpdateDialog /><ActionProbe /></>);
    register(registration);
    await user.click(await screen.findByRole('button', { name: 'Later' }));
    await user.click(screen.getByRole('button', { name: 'Manual update' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Update failed. Check your connection and try again.');
    expect(screen.getByRole('button', { name: 'Retry update' })).toBeInTheDocument();
  });

  it('preserves applying state when the same waiting worker is re-observed', async () => {
    const user = userEvent.setup();
    const waitingWorker = {} as ServiceWorker;
    const registration = makeRegistration({ waiting: waitingWorker });
    let releaseUpdate!: () => void;
    swMock.updateServiceWorker.mockImplementationOnce(() => new Promise<void>((resolve) => { releaseUpdate = resolve; }));
    renderPwa(<PwaUpdateDialog />);
    register(registration);
    await user.click(await screen.findByRole('button', { name: 'Update now' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Updating…' })).toBeDisabled());
    act(() => swMock.options?.onNeedRefresh?.());
    expect(screen.getByRole('button', { name: 'Updating…' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Later' })).toBeDisabled();
    releaseUpdate();
  });

  it('blocks apply while a domain mutation is active', async () => {
    const user = userEvent.setup();
    renderPwa(<><PendingMutationProbe /><PwaUpdateDialog /></>);
    await user.click(screen.getByRole('button', { name: 'Start mutation' }));
    act(() => swMock.options?.onNeedRefresh?.());
    expect(await screen.findByText('An operation is being submitted. You can update when it finishes.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Update now' })).toBeDisabled();
  });

  it('checks the registered worker once after 15 minutes in the foreground', async () => {
    vi.useFakeTimers();
    setOnline(true);
    setVisibility('visible');
    const update = vi.fn(async () => undefined);
    renderPwa(<ActionProbe />);
    register(makeRegistration({ update }));
    await act(async () => {
      vi.advanceTimersByTime(UPDATE_CHECK_INTERVAL_MS);
      await Promise.resolve();
    });
    expect(update).toHaveBeenCalledTimes(1);
  });

  it('reuses an in-flight worker check across concurrent triggers', async () => {
    setOnline(true);
    setVisibility('visible');
    let release!: () => void;
    const update = vi.fn(() => new Promise<void>((resolve) => { release = resolve; }));
    renderPwa(<ActionProbe />);
    register(makeRegistration({ update }));

    await screen.getByRole('button', { name: 'Check for update' }).click();
    await waitFor(() => expect(update).toHaveBeenCalledTimes(1));
    await act(async () => {
      setVisibility('hidden');
      document.dispatchEvent(new Event('visibilitychange'));
      setVisibility('visible');
      document.dispatchEvent(new Event('visibilitychange'));
      window.dispatchEvent(new Event('online'));
      await Promise.resolve();
    });
    expect(update).toHaveBeenCalledTimes(1);
    release();
    await waitFor(() => expect(update).toHaveBeenCalledTimes(1));
  });

  it('does not poll while hidden and checks immediately on foreground resume', async () => {
    vi.useFakeTimers();
    setOnline(true);
    setVisibility('hidden');
    const update = vi.fn(async () => undefined);
    renderPwa(<ActionProbe />);
    register(makeRegistration({ update }));
    await act(async () => {
      vi.advanceTimersByTime(UPDATE_CHECK_INTERVAL_MS);
      await Promise.resolve();
    });
    expect(update).not.toHaveBeenCalled();
    setVisibility('visible');
    await act(async () => {
      document.dispatchEvent(new Event('visibilitychange'));
      await Promise.resolve();
    });
    expect(update).toHaveBeenCalledTimes(1);
  });

  it('throttles visibility and online recovery checks to 60 seconds', async () => {
    vi.useFakeTimers();
    setOnline(true);
    setVisibility('hidden');
    const update = vi.fn(async () => undefined);
    renderPwa(<ActionProbe />);
    register(makeRegistration({ update }));
    setVisibility('visible');
    await act(async () => {
      document.dispatchEvent(new Event('visibilitychange'));
      await Promise.resolve();
    });
    setVisibility('hidden');
    document.dispatchEvent(new Event('visibilitychange'));
    setVisibility('visible');
    await act(async () => {
      document.dispatchEvent(new Event('visibilitychange'));
      window.dispatchEvent(new Event('online'));
      await Promise.resolve();
    });
    expect(update).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(60_000);
    await act(async () => {
      setVisibility('hidden');
      document.dispatchEvent(new Event('visibilitychange'));
      setVisibility('visible');
      document.dispatchEvent(new Event('visibilitychange'));
      await Promise.resolve();
    });
    expect(update).toHaveBeenCalledTimes(2);
  });

  it('skips checks offline and checks when connectivity returns', async () => {
    vi.useFakeTimers();
    setOnline(false);
    setVisibility('visible');
    const update = vi.fn(async () => undefined);
    renderPwa(<ActionProbe />);
    register(makeRegistration({ update }));
    await act(async () => {
      vi.advanceTimersByTime(UPDATE_CHECK_INTERVAL_MS);
      await Promise.resolve();
    });
    expect(update).not.toHaveBeenCalled();
    setOnline(true);
    await act(async () => {
      window.dispatchEvent(new Event('online'));
      await Promise.resolve();
    });
    expect(update).toHaveBeenCalledTimes(1);
  });

  it('does not redundantly update while a worker is installing or waiting', async () => {
    const update = vi.fn(async () => undefined);
    renderPwa(<><PwaUpdateDialog /><ActionProbe /></>);
    register(makeRegistration({ installing: {} as ServiceWorker }));
    await screen.getByRole('button', { name: 'Check for update' }).click();
    expect(update).not.toHaveBeenCalled();

    const waitingWorker = {} as ServiceWorker;
    register(makeRegistration({ waiting: waitingWorker, update }));
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
    await screen.getByRole('button', { name: 'Later' }).click();
    await screen.getByRole('button', { name: 'Check for update' }).click();
    expect(update).not.toHaveBeenCalled();
  });

  it('clears stale update state when a waiting worker activates without reloading this tab', async () => {
    const user = userEvent.setup();
    const waitingWorker = {} as ServiceWorker;
    const registration = makeRegistration({ waiting: waitingWorker });
    renderPwa(<><PwaUpdateDialog /><StateProbe /><ActionProbe /></>);
    register(registration);
    await user.click(await screen.findByRole('button', { name: 'Later' }));
    Object.defineProperty(registration, 'waiting', { configurable: true, value: null });
    await user.click(screen.getByRole('button', { name: 'Check for update' }));
    await waitFor(() => {
      expect(screen.getByTestId('pwa-state')).toHaveAttribute('data-need-refresh', 'false');
      expect(screen.getByTestId('pwa-state')).toHaveAttribute('data-phase', 'idle');
    });
  });

  it('swallows background update failures so the workbench remains usable', async () => {
    const update = vi.fn(async () => { throw new Error('temporary failure'); });
    renderPwa(<ActionProbe />);
    register(makeRegistration({ update }));
    await screen.getByRole('button', { name: 'Check for update' }).click();
    await waitFor(() => expect(update).toHaveBeenCalledTimes(1));
    expect(screen.getByRole('button', { name: 'Check for update' })).toBeInTheDocument();
  });

  it('keeps offline recovery messaging separate from domain data', () => {
    renderPwa(<OnlineStatusBanner />);
    act(() => window.dispatchEvent(new Event('offline')));
    expect(screen.getByText(/Connect to the QuaZonai server/)).toBeInTheDocument();
    act(() => window.dispatchEvent(new Event('online')));
    expect(screen.queryByText(/Connect to the QuaZonai server/)).not.toBeInTheDocument();
  });
});
