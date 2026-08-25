import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { I18nProvider, localeOrder, type Locale } from '../i18n';
import { runtimeLabelSources, translateRuntimeLabel } from '../i18n/runtime';
import { humanize } from '../lib/format';

function RuntimeProbe({ value }: { value: string }) {
  return <div>{humanize(value)}</div>;
}

const streamedEventLabels = [
  'Program Created',
  'Idea Contributed',
  'Program Paused',
  'Program Active',
  'Program Archived',
  'Mission Ready',
  'Mission Started',
  'Mission Succeeded',
  'Mission Failed',
  'Mandate Enabled',
  'Mandate Disabled',
  'Approval Expired',
  'Approval Approved',
  'Approval Rejected',
  'Handoff Available',
  'Handoff Revoked',
  'Handoff Claimed',
  'Handoff Accepted',
  'Handoff Downstream Rejected',
  'Forward Evidence Recorded',
  'Handoff Feedback Status',
  'Data Source Registered',
  'Downstream Registered',
  'Downstream Service Token Rotated',
  'Job Leased',
  'Job Failed',
  'Job Succeeded',
  'Plugin Release Received',
  'Plugin Release Activated',
  'Plugin Release Draining',
  'Plugin Release Remove Requested',
  'Plugin Release Failed',
  'Plugin Release Staged',
  'Plugin Bundle Ready',
  'Plugin Release Removed',
  'Credential Set Created',
  'Credential Set Replaced',
  'Runtime Configuration Updated',
] as const;

describe('research runtime presentation labels', () => {
  it('preserves established translations for core research runtime labels', () => {
    const cases: Array<[Locale, string, string]> = [
      ['zh-CN', 'Alpha Discovery', 'Alpha 发现'],
      ['ja', 'Alpha Researcher', 'Alpha研究者'],
      ['ko', 'Program Created', '프로그램 생성됨'],
      ['es', 'Idea Contributed', 'Idea aportada'],
      ['ar', 'Program Paused', 'تم إيقاف البرنامج مؤقتًا'],
      ['zh-TW', 'Program Active', '專案已啟用'],
      ['ja', 'Program Archived', 'プログラムアーカイブ済み'],
      ['ko', 'Mission Ready', '미션 준비됨'],
      ['es', 'Mission Started', 'Misión iniciada'],
      ['ar', 'Mission Succeeded', 'نجحت المهمة'],
      ['zh-CN', 'Mission Failed', '任务失败'],
    ];

    for (const [locale, source, expected] of cases) {
      expect(translateRuntimeLabel(locale, source)).toBe(expected);
    }
  });

  it('catalogs every currently emitted Event kind for every supported locale', () => {
    for (const source of streamedEventLabels) {
      expect(runtimeLabelSources).toContain(source);
      for (const locale of localeOrder) {
        expect(translateRuntimeLabel(locale, source), `${locale}: ${source}`).toBeTruthy();
      }
    }
  });

  it('localizes canonical runtime enums only at the humanize presentation boundary', () => {
    const canonical = 'MISSION_READY';
    const view = render(<I18nProvider initialLocale="ar"><RuntimeProbe value={canonical} /></I18nProvider>);
    expect(screen.getByText('المهمة جاهزة')).toBeInTheDocument();
    expect(canonical).toBe('MISSION_READY');
    view.unmount();
  });
});
