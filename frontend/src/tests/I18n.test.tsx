import { render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { I18nProvider, resolveLocale, translateKey, translateSource, useI18n, type Locale } from '../i18n';

function Probe() {
  const { locale, t } = useI18n();
  return <div>{locale}:{t('nav.dashboard')}</div>;
}

function RowCountProbe({ count }: { count: number }) {
  const { plural } = useI18n();
  return <div>{plural({
    zero: 'table.rows.zero',
    one: 'table.rows.one',
    two: 'table.rows.two',
    few: 'table.rows.few',
    many: 'table.rows.many',
    other: 'table.rows.other',
  }, count)}</div>;
}

function expectRowCount(locale: Locale, count: number, expected: string) {
  const view = render(<I18nProvider initialLocale={locale}><RowCountProbe count={count} /></I18nProvider>);
  expect(screen.getByText(expected)).toBeInTheDocument();
  view.unmount();
}

afterEach(() => {
  localStorage.clear();
  document.documentElement.lang = 'en';
  document.documentElement.dir = 'ltr';
});

describe('i18n', () => {
  it('negotiates common regional locale variants with an English fallback', () => {
    expect(resolveLocale(['zh-HK'])).toBe('zh-TW');
    expect(resolveLocale(['zh-SG'])).toBe('zh-CN');
    expect(resolveLocale(['es-MX'])).toBe('es');
    expect(resolveLocale(['fr-FR'])).toBe('en');
  });

  it('keeps semantic keys and legacy English source strings on the same catalog', () => {
    expect(translateKey('zh-CN', 'nav.dashboard')).toBe('仪表盘');
    expect(translateSource('ja', 'Page not found')).toBe('ページが見つかりません');
  });

  it('selects locale-aware row-count plural forms', () => {
    expectRowCount('en', 1, '1 row');
    expectRowCount('en', 2, '2 rows');
    expectRowCount('es', 1, '1 fila');
    expectRowCount('ar', 0, 'لا صفوف');
    expectRowCount('ar', 1, 'صف واحد');
    expectRowCount('ar', 2, 'صفّان');
    expectRowCount('ar', 3, '3 صفوف');
    expectRowCount('ar', 11, '11 صفًا');
  });

  it('synchronizes Arabic locale and direction to the document', async () => {
    render(<I18nProvider initialLocale="ar"><Probe /></I18nProvider>);
    expect(screen.getByText('ar:لوحة التحكم')).toBeInTheDocument();
    await waitFor(() => {
      expect(document.documentElement.lang).toBe('ar');
      expect(document.documentElement.dir).toBe('rtl');
      expect(localStorage.getItem('qz-locale')).toBe('ar');
    });
  });
});
