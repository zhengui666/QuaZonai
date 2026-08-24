import { Theme } from '@radix-ui/themes';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, type RenderOptions } from '@testing-library/react';
import type { ReactElement, ReactNode } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { I18nProvider } from '../i18n';

export function renderApp(ui: ReactElement, options?: RenderOptions & { route?: string }) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });

  function Wrapper({ children }: { children: ReactNode }) {
    return (
      <I18nProvider initialLocale="en">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="medium" scaling="90%">
          <QueryClientProvider client={client}>
            <MemoryRouter initialEntries={[options?.route ?? '/']}>{children}</MemoryRouter>
          </QueryClientProvider>
        </Theme>
      </I18nProvider>
    );
  }

  return render(ui, { wrapper: Wrapper, ...options });
}

export function jsonResponse(value: unknown, status = 200) {
  return Promise.resolve(
    new Response(JSON.stringify(value), {
      status,
      headers: { 'content-type': 'application/json' },
    }),
  );
}
