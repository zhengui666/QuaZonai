import { render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { I18nProvider, resolveLocale, translateKey, translateSource, useI18n } from '../i18n';

function Probe() {
  const { locale, t } = useI18n();
  return <div>{locale}:{t('nav.dashboard')}</div>;
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
