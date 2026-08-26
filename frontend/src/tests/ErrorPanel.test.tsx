import { Theme } from '@radix-ui/themes';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { I18nProvider, useI18n } from '../i18n';
import { ApiError } from '../lib/api/client';

function LocaleChangeButton() {
  const { setLocale } = useI18n();
  return <button type="button" onClick={() => setLocale('ar')}>Switch locale</button>;
}

describe('ErrorPanel', () => {
  it('re-renders client HTTP fallbacks in the selected locale', async () => {
    render(
      <I18nProvider initialLocale="en">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <LocaleChangeButton />
          <ErrorPanel error={new ApiError({ kind: 'http', status: 503 }, 503)} />
        </Theme>
      </I18nProvider>,
    );

    expect(screen.getByText('Request failed with HTTP 503.')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Switch locale' }));
    await waitFor(() => expect(screen.getByText(`فشل الطلب مع HTTP ${new Intl.NumberFormat('ar').format(503)}.`)).toBeInTheDocument());
  });

  it('localizes client network fallbacks', () => {
    render(
      <I18nProvider initialLocale="ar">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <ErrorPanel error={new ApiError({ kind: 'network' }, 0)} />
        </Theme>
      </I18nProvider>,
    );

    expect(screen.getByText('تعذر تحميل البيانات')).toBeInTheDocument();
    expect(screen.getByText('تعذر الوصول إلى الخدمة.')).toBeInTheDocument();
    expect(screen.queryByText('HTTP_ERROR')).not.toBeInTheDocument();
  });

  it('preserves API-authored error text', () => {
    render(
      <I18nProvider initialLocale="ar">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <ErrorPanel error={new ApiError({ kind: 'api', message: 'Origin policy denied.' }, 403, 'ORIGIN_DENIED')} />
        </Theme>
      </I18nProvider>,
    );

    expect(screen.getByText('Origin policy denied.')).toBeInTheDocument();
  });
});
