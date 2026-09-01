import { act, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { I18nProvider } from '../i18n';
import { OnlineStatusBanner } from './OnlineStatusBanner';
import { PwaProvider, usePwa } from './PwaProvider';
import { PwaUpdateBanner } from './PwaUpdateBanner';

const swMock = vi.hoisted(() => ({
  onNeedRefresh: null as (() => void) | null,
  update: vi.fn(async () => undefined),
}));

vi.mock('virtual:pwa-register/react', async () => {
  return {
    useRegisterSW: (options: { onNeedRefresh?: () => void }) => {
      swMock.onNeedRefresh = options.onNeedRefresh ?? null;
      return { needRefresh: [false, vi.fn()], offlineReady: [false, vi.fn()], updateServiceWorker: swMock.update };
    },
  };
});

function Probe() {
  const { canInstall, isOnline } = usePwa();
  return <div>{canInstall ? 'install-ready' : 'install-unavailable'} · {isOnline ? 'online' : 'offline'}</div>;
}

afterEach(() => {
  swMock.onNeedRefresh = null;
  swMock.update.mockClear();
  vi.unstubAllGlobals();
});

describe('PWA lifecycle', () => {
  it('keeps install capability and credentials in memory only', async () => {
    render(<PwaProvider><Probe /></PwaProvider>);
    expect(screen.getByText(/install-unavailable · online/)).toBeInTheDocument();
    const prompt = vi.fn(async () => undefined);
    const event = new Event('beforeinstallprompt', { cancelable: true }) as Event & { prompt: () => Promise<void>; userChoice: Promise<{ outcome: 'accepted' }> };
    event.prompt = prompt;
    event.userChoice = Promise.resolve({ outcome: 'accepted' });
    act(() => window.dispatchEvent(event));
    expect(await screen.findByText(/install-ready · online/)).toBeInTheDocument();
    expect(document.querySelectorAll('script')).toHaveLength(0);
  });

  it('shows a user-action update prompt and never reloads automatically', async () => {
    render(<I18nProvider initialLocale="en"><PwaProvider><PwaUpdateBanner /></PwaProvider></I18nProvider>);
    expect(screen.queryByText('A new QuaZonai version is available.')).not.toBeInTheDocument();
    await waitFor(() => expect(swMock.onNeedRefresh).not.toBeNull());
    act(() => swMock.onNeedRefresh?.());
    expect(await screen.findByText('A new QuaZonai version is available.')).toBeInTheDocument();
    expect(swMock.update).not.toHaveBeenCalled();
    await act(async () => { await screen.getByRole('button', { name: 'Update' }).click(); });
    expect(swMock.update).toHaveBeenCalledWith(true);
  });

  it('surfaces offline state and online recovery without fabricating domain data', () => {
    render(<I18nProvider initialLocale="en"><PwaProvider><OnlineStatusBanner /></PwaProvider></I18nProvider>);
    act(() => window.dispatchEvent(new Event('offline')));
    expect(screen.getByText(/Connect to the QuaZonai server/)).toBeInTheDocument();
    act(() => window.dispatchEvent(new Event('online')));
    expect(screen.queryByText(/Connect to the QuaZonai server/)).not.toBeInTheDocument();
  });
});
