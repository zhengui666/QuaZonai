import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { I18nProvider, useI18n, type Locale } from '../i18n';

const countForms = {
  coolingPrograms: {
    zero: 'home.coolingPrograms.zero', one: 'home.coolingPrograms.one', two: 'home.coolingPrograms.two',
    few: 'home.coolingPrograms.few', many: 'home.coolingPrograms.many', other: 'home.coolingPrograms.other',
  },
  blockedPrograms: {
    zero: 'home.blockedPrograms.zero', one: 'home.blockedPrograms.one', two: 'home.blockedPrograms.two',
    few: 'home.blockedPrograms.few', many: 'home.blockedPrograms.many', other: 'home.blockedPrograms.other',
  },
  discoveryMissions: {
    zero: 'home.discoveryMissions.zero', one: 'home.discoveryMissions.one', two: 'home.discoveryMissions.two',
    few: 'home.discoveryMissions.few', many: 'home.discoveryMissions.many', other: 'home.discoveryMissions.other',
  },
  runningEvaluations: {
    zero: 'home.runningEvaluations.zero', one: 'home.runningEvaluations.one', two: 'home.runningEvaluations.two',
    few: 'home.runningEvaluations.few', many: 'home.runningEvaluations.many', other: 'home.runningEvaluations.other',
  },
  observedEvaluationMissions: {
    zero: 'home.observedEvaluationMissions.zero', one: 'home.observedEvaluationMissions.one', two: 'home.observedEvaluationMissions.two',
    few: 'home.observedEvaluationMissions.few', many: 'home.observedEvaluationMissions.many', other: 'home.observedEvaluationMissions.other',
  },
  portfolioPrograms: {
    zero: 'home.portfolioPrograms.zero', one: 'home.portfolioPrograms.one', two: 'home.portfolioPrograms.two',
    few: 'home.portfolioPrograms.few', many: 'home.portfolioPrograms.many', other: 'home.portfolioPrograms.other',
  },
  claimedHandoffs: {
    zero: 'home.claimedHandoffs.zero', one: 'home.claimedHandoffs.one', two: 'home.claimedHandoffs.two',
    few: 'home.claimedHandoffs.few', many: 'home.claimedHandoffs.many', other: 'home.claimedHandoffs.other',
  },
} as const;

type CountFamily = keyof typeof countForms;

function CountProbe({ family, count }: { family: CountFamily; count: number }) {
  const { plural } = useI18n();
  return <div>{plural(countForms[family], count)}</div>;
}

function expectCount(locale: Locale, family: CountFamily, count: number, expected: string) {
  const view = render(<I18nProvider initialLocale={locale}><CountProbe family={family} count={count} /></I18nProvider>);
  expect(screen.getByText(expected)).toBeInTheDocument();
  view.unmount();
}

const localizedNumber = (locale: Locale, value: number) => new Intl.NumberFormat(locale).format(value);

describe('Home KPI count messages', () => {
  it('uses singular and plural English forms for every reachable KPI note', () => {
    const cases: Array<[CountFamily, string, string]> = [
      ['coolingPrograms', '1 cooling program', '2 cooling programs'],
      ['blockedPrograms', '1 blocked program', '2 blocked programs'],
      ['discoveryMissions', '1 alpha discovery mission', '2 alpha discovery missions'],
      ['runningEvaluations', '1 evaluation running', '2 evaluations running'],
      ['observedEvaluationMissions', '1 evaluation mission observed', '2 evaluation missions observed'],
      ['portfolioPrograms', '1 portfolio program', '2 portfolio programs'],
      ['claimedHandoffs', '1 claimed handoff', '2 claimed handoffs'],
    ];
    for (const [family, singular, plural] of cases) {
      expectCount('en', family, 1, singular);
      expectCount('en', family, 2, plural);
    }
  });

  it('uses singular Spanish forms for every reachable KPI note', () => {
    const cases: Array<[CountFamily, string]> = [
      ['coolingPrograms', '1 programa en enfriamiento'],
      ['blockedPrograms', '1 programa bloqueado'],
      ['discoveryMissions', '1 misión de descubrimiento Alpha'],
      ['runningEvaluations', '1 evaluación en ejecución'],
      ['observedEvaluationMissions', 'Se observó 1 misión de evaluación'],
      ['portfolioPrograms', '1 programa de cartera'],
      ['claimedHandoffs', '1 entrega reclamada'],
    ];
    for (const [family, expected] of cases) expectCount('es', family, 1, expected);
  });

  it('covers every Arabic plural category with locale-formatted digits', () => {
    expectCount('ar', 'runningEvaluations', 0, 'لا تقييمات قيد التشغيل');
    expectCount('ar', 'runningEvaluations', 1, 'تقييم واحد قيد التشغيل');
    expectCount('ar', 'runningEvaluations', 2, 'تقييمان قيد التشغيل');
    expectCount('ar', 'runningEvaluations', 3, `${localizedNumber('ar', 3)} تقييمات قيد التشغيل`);
    expectCount('ar', 'runningEvaluations', 11, `${localizedNumber('ar', 11)} تقييمًا قيد التشغيل`);
    expectCount('ar', 'runningEvaluations', 100, `${localizedNumber('ar', 100)} تقييم قيد التشغيل`);
  });
});
