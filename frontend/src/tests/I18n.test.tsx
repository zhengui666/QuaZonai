import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { PageHeader } from '../components/ui/PageHeader';
import { I18nProvider, resolveLocale, translateKey, translateSource, useI18n, type Locale } from '../i18n';
import { translateDomainLabel } from '../i18n/domain';

function Probe() {
  const { locale, t } = useI18n();
  return <div>{locale}:{t('nav.dashboard')}</div>;
}

function LocaleSwitchProbe() {
  const { locale, setLocale } = useI18n();
  return <button onClick={() => setLocale('es')}>{locale}</button>;
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

function DecisionCountProbe({ count }: { count: number }) {
  const { plural } = useI18n();
  return <div>{plural({
    zero: 'home.decisions.zero',
    one: 'home.decisions.one',
    two: 'home.decisions.two',
    few: 'home.decisions.few',
    many: 'home.decisions.many',
    other: 'home.decisions.other',
  }, count)}</div>;
}

function expectRowCount(locale: Locale, count: number, expected: string) {
  const view = render(<I18nProvider initialLocale={locale}><RowCountProbe count={count} /></I18nProvider>);
  expect(screen.getByText(expected)).toBeInTheDocument();
  view.unmount();
}

function expectDecisionCount(locale: Locale, count: number, expected: string) {
  const view = render(<I18nProvider initialLocale={locale}><DecisionCountProbe count={count} /></I18nProvider>);
  expect(screen.getByText(expected)).toBeInTheDocument();
  view.unmount();
}

const formatted = (locale: Locale, value: number) => new Intl.NumberFormat(locale).format(value);

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

  it('keeps semantic keys and unambiguous legacy English sources on the same catalog', () => {
    expect(translateKey('zh-CN', 'nav.dashboard')).toBe('仪表盘');
    expect(translateSource('ja', 'Page not found')).toBe('ページが見つかりません');
  });

  it('does not collapse divergent semantic keys that share an English source', () => {
    expect(translateKey('zh-CN', 'research.observed')).toBe('观测时间');
    expect(translateKey('zh-CN', 'alpha.observed')).toBe('观测');
    expect(translateSource('zh-CN', 'Observed')).toBe('Observed');
  });

  it('preserves user-authored text that happens to equal a catalog source', () => {
    render(<I18nProvider initialLocale="zh-CN"><PageHeader title="Dashboard" translateTitle={false} /></I18nProvider>);
    expect(screen.getByRole('heading', { name: 'Dashboard' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: '仪表盘' })).not.toBeInTheDocument();
  });

  it('localizes reachable lifecycle and handoff states', () => {
    expect(translateDomainLabel('zh-CN', 'Received')).toBe('已接收');
    expect(translateDomainLabel('ja', 'Installing')).toBe('インストール中');
    expect(translateDomainLabel('ko', 'Validating')).toBe('검증 중');
    expect(translateDomainLabel('es', 'Draining')).toBe('Drenando');
    expect(translateDomainLabel('ar', 'Removing')).toBe('جارٍ الإزالة');
    expect(translateDomainLabel('zh-TW', 'Removed')).toBe('已移除');
    expect(translateDomainLabel('zh-CN', 'Revoked')).toBe('已撤销');
  });

  it('formats numeric interpolation using the active locale', () => {
    expect(translateKey('ar', 'table.page', { page: 1234, pages: 5678 }))
      .toBe(`الصفحة ${formatted('ar', 1234)} / ${formatted('ar', 5678)}`);
    expect(translateKey('es', 'table.perPage', { count: 1234 }))
      .toBe(`${formatted('es', 1234)} / página`);
  });

  it('selects locale-aware row-count plural forms', () => {
    expectRowCount('en', 1, '1 row');
    expectRowCount('en', 2, '2 rows');
    expectRowCount('es', 1, '1 fila');
    expectRowCount('ar', 0, 'لا صفوف');
    expectRowCount('ar', 1, 'صف واحد');
    expectRowCount('ar', 2, 'صفّان');
    expectRowCount('ar', 3, `${formatted('ar', 3)} صفوف`);
    expectRowCount('ar', 11, `${formatted('ar', 11)} صفًا`);
  });

  it('selects all Arabic decision-count plural categories', () => {
    expectDecisionCount('ar', 0, 'لا قرارات');
    expectDecisionCount('ar', 1, 'قرار واحد');
    expectDecisionCount('ar', 2, 'قراران');
    expectDecisionCount('ar', 3, `${formatted('ar', 3)} قرارات`);
    expectDecisionCount('ar', 11, `${formatted('ar', 11)} قرارًا`);
    expectDecisionCount('ar', 100, `${formatted('ar', 100)} قرار`);
  });

  it('synchronizes an inferred/initial locale to the document without persisting it', async () => {
    render(<I18nProvider initialLocale="ar"><Probe /></I18nProvider>);
    expect(screen.getByText('ar:لوحة التحكم')).toBeInTheDocument();
    await waitFor(() => {
      expect(document.documentElement.lang).toBe('ar');
      expect(document.documentElement.dir).toBe('rtl');
      expect(localStorage.getItem('qz-locale')).toBeNull();
    });
  });

  it('persists locale only after an explicit selection', async () => {
    render(<I18nProvider initialLocale="en"><LocaleSwitchProbe /></I18nProvider>);
    expect(localStorage.getItem('qz-locale')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'en' }));
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'es' })).toBeInTheDocument();
      expect(localStorage.getItem('qz-locale')).toBe('es');
      expect(document.documentElement.lang).toBe('es');
    });
  });
});
