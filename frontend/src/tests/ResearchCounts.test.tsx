import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { I18nProvider, useI18n } from '../i18n';
import { failedMissionForms, runningMissionForms, structuredEventForms, succeededMissionForms } from '../i18n/researchPlural';

function MissionSummary({ running, succeeded, failed }: { running: number; succeeded: number; failed: number }) {
  const { plural } = useI18n();
  return <div>{[
    plural(runningMissionForms, running),
    plural(succeededMissionForms, succeeded),
    plural(failedMissionForms, failed),
  ].join(' · ')}</div>;
}

function StructuredEventCount({ count }: { count: number }) {
  const { plural } = useI18n();
  return <div>{plural(structuredEventForms, count)}</div>;
}

describe('research detail count localization', () => {
  it('pluralizes each mission state independently in Spanish', () => {
    const view = render(
      <I18nProvider initialLocale="es">
        <MissionSummary running={2} succeeded={1} failed={1} />
      </I18nProvider>,
    );
    expect(screen.getByText('2 en ejecución · 1 completada · 1 fallida')).toBeInTheDocument();

    view.rerender(
      <I18nProvider initialLocale="es">
        <MissionSummary running={1} succeeded={2} failed={2} />
      </I18nProvider>,
    );
    expect(screen.getByText('1 en ejecución · 2 completadas · 2 fallidas')).toBeInTheDocument();
  });

  it('covers Arabic zero, one, two, few, many, and other event categories', () => {
    const cases: Array<[number, string]> = [
      [0, 'لا أحداث منظمة'],
      [1, 'حدث منظم واحد'],
      [2, 'حدثان منظمان'],
      [3, `${new Intl.NumberFormat('ar').format(3)} أحداث منظمة`],
      [11, `${new Intl.NumberFormat('ar').format(11)} حدثًا منظمًا`],
      [100, `${new Intl.NumberFormat('ar').format(100)} حدث منظم`],
    ];

    for (const [count, expected] of cases) {
      const view = render(
        <I18nProvider initialLocale="ar">
          <StructuredEventCount count={count} />
        </I18nProvider>,
      );
      expect(screen.getByText(expected)).toBeInTheDocument();
      view.unmount();
    }
  });
});
