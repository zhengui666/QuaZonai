import '@fontsource-variable/geist';
import '@fontsource-variable/geist-mono';
import { Theme } from '@radix-ui/themes';
import '@radix-ui/themes/styles.css';
import './styles/theme.css';
import './styles/production.css';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';
import { App } from './app/App';
import { I18nProvider } from './i18n';
import { OnlineStatusBanner } from './pwa/OnlineStatusBanner';
import { PwaProvider } from './pwa/PwaProvider';
import { PwaUpdateDialog } from './pwa/PwaUpdateDialog';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5_000,
      retry: (count, error) => {
        const status = (error as { status?: number }).status;
        return status && status >= 400 && status < 500 ? false : count < 2;
      },
      refetchOnWindowFocus: false,
    },
    mutations: { retry: 0 },
  },
});

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <I18nProvider>
      <PwaProvider>
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <QueryClientProvider client={queryClient}>
            <PwaUpdateDialog />
            <OnlineStatusBanner />
            <BrowserRouter><App /></BrowserRouter>
          </QueryClientProvider>
        </Theme>
      </PwaProvider>
    </I18nProvider>
  </StrictMode>,
);
