import { Direction } from 'radix-ui';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { LocaleDirectionProvider } from '../components/layout/AppShell';
import { KpiStrip } from '../components/ui/KpiStrip';
import { PageHeader } from '../components/ui/PageHeader';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { StateBadge } from '../components/ui/StateBadge';
import { I18nProvider, resolveLocale, translateKey, translateSource, useI18n, type Locale } from '../i18n';
import { translateDomainLabel } from '../i18n/domain';
import { humanizeIdentifier } from '../lib/format';

function Probe() {
  const { locale, t } = useI18n();
  return <div>{locale}:{t('nav.dashboard')}</div>;
}

function LocaleSwitchProbe() {
  const { locale, setLocale } = useI18n();
  return <button onClick={() => setLocale('es')}>{locale}</button>;
}

function RadixDirectionProbe() {
  return <div data-testid="radix-direction">{Direction.useDirection()}</div>;
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
  document.title = 'QuaZonai Research Workbench';
});

describe('i18n', () => {
  it('negotiates common regional locale variants with an English fallback', () => {
    expect(resolveLocale(['zh-HK'])).toBe('zh-TW');
    expect(resolveLocale(['zh-SG'])).toBe('zh-CN');
    expect(resolveLocale(['zh-Hans-HK'])).toBe('zh-CN');
    expect(resolveLocale(['zh-Hant-CN'])).toBe('zh-TW');
    expect(resolveLocale(['es-MX'])).toBe('es');
    expect(resolveLocale(['kok-IN', 'en-US'])).toBe('en');
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

  it('preserves user-authored header text and lets the browser infer its direction', () => {
    render(<I18nProvider initialLocale="ar"><PageHeader title="Dashboard 12 / ES" description="English research rationale: EUR/USD" translateTitle={false} translateDescription={false} /></I18nProvider>);
    const heading = screen.getByRole('heading', { name: 'Dashboard 12 / ES' });
    expect(heading).toBeInTheDocument();
    expect(heading).toHaveAttribute('dir', 'auto');
    expect(screen.getByText('English research rationale: EUR/USD')).toHaveAttribute('dir', 'auto');
  });

  it('propagates the locale direction through the Radix direction context', () => {
    render(
      <I18nProvider initialLocale="ar">
        <LocaleDirectionProvider><RadixDirectionProbe /></LocaleDirectionProvider>
      </I18nProvider>,
    );
    expect(screen.getByTestId('radix-direction')).toHaveTextContent('rtl');
  });

  it('formats numeric KPI values and preserves nonnumeric React nodes', () => {
    render(
      <I18nProvider initialLocale="ar">
        <KpiStrip items={[
          { label: 'Count', value: 1234 },
          { label: 'Custom', value: <strong data-testid="custom-kpi">raw node</strong>, note: 'EUR/USD operator note' },
        ]} />
      </I18nProvider>,
    );
    expect(screen.getByText(formatted('ar', 1234))).toBeInTheDocument();
    expect(screen.getByText(formatted('ar', 1234)).closest('.qz-kpi-value')).toHaveAttribute('dir', 'auto');
    expect(screen.getByTestId('custom-kpi')).toHaveTextContent('raw node');
    expect(screen.getByTestId('custom-kpi').closest('.qz-kpi-value')).toHaveAttribute('dir', 'auto');
    expect(screen.getByText('EUR/USD operator note')).toHaveAttribute('dir', 'auto');
  });

  it('localizes reachable lifecycle, portfolio, runtime-health, overlap, and plugin capability values', () => {
    expect(translateDomainLabel('zh-CN', 'Received')).toBe('已接收');
    expect(translateDomainLabel('ja', 'Installing')).toBe('インストール中');
    expect(translateDomainLabel('ko', 'Validating')).toBe('검증 중');
    expect(translateDomainLabel('es', 'Draining')).toBe('Drenando');
    expect(translateDomainLabel('ar', 'Removing')).toBe('جارٍ الإزالة');
    expect(translateDomainLabel('zh-TW', 'Removed')).toBe('已移除');
    expect(translateDomainLabel('zh-CN', 'Revoked')).toBe('已撤销');
    expect(translateDomainLabel('zh-CN', 'Approval Pending')).toBe('待审批');
    expect(translateDomainLabel('ja', 'Cancelled')).toBe('キャンセル済み');
    expect(translateDomainLabel('ko', 'Degrading')).toBe('열화 중');
    expect(translateDomainLabel('zh-CN', 'Candidate Ready')).toBe('候选就绪');
    expect(translateDomainLabel('zh-CN', 'Primary Alpha')).toBe('主 Alpha');
    expect(translateDomainLabel('ja', 'Diversifier Alpha')).toBe('分散Alpha');
    expect(translateDomainLabel('ko', 'Hedge Alpha')).toBe('헤지 Alpha');
    expect(translateDomainLabel('es', 'Regime Signal')).toBe('Señal de régimen');
    expect(translateDomainLabel('ar', 'Risk Modulator')).toBe('مُعدِّل المخاطر');
    expect(translateDomainLabel('zh-TW', 'Shadow Alpha')).toBe('影子 Alpha');
    expect(translateDomainLabel('zh-CN', 'Historical Import')).toBe('历史导入');
    expect(translateDomainLabel('ja', 'Live Data')).toBe('リアルタイムデータ');
    expect(translateDomainLabel('es', 'Research Tool')).toBe('Herramienta de investigación');
    expect(translateDomainLabel('zh-CN', 'Database')).toBe('数据库');
    expect(translateDomainLabel('zh-TW', 'Worker')).toBe('工作程序');
    expect(translateDomainLabel('ja', 'Agent Worker')).toBe('エージェントワーカー');
    expect(translateDomainLabel('ko', 'Evaluator')).toBe('평가기');
    expect(translateDomainLabel('es', 'Storage')).toBe('Almacenamiento');
    expect(translateDomainLabel('ar', 'Codex')).toBe('خدمة Codex');
    expect(translateDomainLabel('zh-CN', 'Duplicate')).toBe('重复');
    expect(translateDomainLabel('ja', 'Branch')).toBe('分岐');
    expect(translateDomainLabel('ko', 'Related Program')).toBe('관련 프로그램');
    expect(translateDomainLabel('ar', 'New')).toBe('جديد');
  });

  it('keeps arbitrary schema identifiers language-neutral', () => {
    expect(humanizeIdentifier('capacity')).toBe('Capacity');
    expect(humanizeIdentifier('search_adjusted_quality')).toBe('Search Adjusted Quality');
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
      expect(document.title).toBe(translateKey('ar', 'app.documentTitle'));
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
      expect(document.title).toBe(translateKey('es', 'app.documentTitle'));
    });
  });
  it('gives API-authored error text automatic direction', () => {
    render(<I18nProvider initialLocale="ar"><ErrorPanel error={new Error('Request failed: https://example.test/path')} /></I18nProvider>);
    expect(screen.getByText('Request failed: https://example.test/path')).toHaveAttribute('dir', 'auto');
  });


  it('isolates unknown API status labels from RTL chrome', () => {
    render(<I18nProvider initialLocale="ar"><StateBadge state="EXTERNAL_EUR/USD" /></I18nProvider>);
    const label = document.querySelector('.qz-status bdi');
    expect(label).toHaveTextContent('External Eur/usd');
    expect(label).toHaveAttribute('dir', 'auto');
  });

});
