import type { PluralMessageKey } from './pluralMessages';

type PluralForms = Partial<Record<Intl.LDMLPluralRule, PluralMessageKey>>;

export const runningMissionForms: PluralForms = {
  zero: 'research.runningMissions.zero',
  one: 'research.runningMissions.one',
  two: 'research.runningMissions.two',
  few: 'research.runningMissions.few',
  many: 'research.runningMissions.many',
  other: 'research.runningMissions.other',
};

export const succeededMissionForms: PluralForms = {
  zero: 'research.succeededMissions.zero',
  one: 'research.succeededMissions.one',
  two: 'research.succeededMissions.two',
  few: 'research.succeededMissions.few',
  many: 'research.succeededMissions.many',
  other: 'research.succeededMissions.other',
};

export const failedMissionForms: PluralForms = {
  zero: 'research.failedMissions.zero',
  one: 'research.failedMissions.one',
  two: 'research.failedMissions.two',
  few: 'research.failedMissions.few',
  many: 'research.failedMissions.many',
  other: 'research.failedMissions.other',
};

export const structuredEventForms: PluralForms = {
  zero: 'research.structuredEvents.zero',
  one: 'research.structuredEvents.one',
  two: 'research.structuredEvents.two',
  few: 'research.structuredEvents.few',
  many: 'research.structuredEvents.many',
  other: 'research.structuredEvents.other',
};
