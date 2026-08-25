import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { I18nProvider, type Locale } from '../i18n';
import { translateRuntimeLabel } from '../i18n/runtime';
import { humanize } from '../lib/format';

function RuntimeProbe({ value }: { value: string }) {
  return <div>{humanize(value)}</div>;
}

describe('research runtime presentation labels', () => {
  it('catalogs every currently reachable program, mission, type, and role label', () => {
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

  it('localizes canonical runtime enums only at the humanize presentation boundary', () => {
    const canonical = 'MISSION_READY';
    const view = render(<I18nProvider initialLocale="ar"><RuntimeProbe value={canonical} /></I18nProvider>);
    expect(screen.getByText('المهمة جاهزة')).toBeInTheDocument();
    expect(canonical).toBe('MISSION_READY');
    view.unmount();
  });
});
