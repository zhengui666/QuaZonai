import { describe, expect, it } from 'vitest';
import { formatMissionStateSummary, formatStructuredEventCount } from '../i18n/researchCounts';

describe('research detail count localization', () => {
  it('pluralizes each mission state independently in Spanish', () => {
    expect(formatMissionStateSummary('es', { running: 2, succeeded: 1, failed: 1 }))
      .toBe('2 en ejecución · 1 completada · 1 fallida');
    expect(formatMissionStateSummary('es', { running: 1, succeeded: 2, failed: 2 }))
      .toBe('1 en ejecución · 2 completadas · 2 fallidas');
  });

  it('covers Arabic zero, one, two, few, many, and other categories', () => {
    expect(formatStructuredEventCount('ar', 0)).toBe('لا أحداث منظمة');
    expect(formatStructuredEventCount('ar', 1)).toBe('حدث منظم واحد');
    expect(formatStructuredEventCount('ar', 2)).toBe('حدثان منظمان');
    expect(formatStructuredEventCount('ar', 3)).toContain(new Intl.NumberFormat('ar').format(3));
    expect(formatStructuredEventCount('ar', 11)).toContain(new Intl.NumberFormat('ar').format(11));
    expect(formatStructuredEventCount('ar', 100)).toContain(new Intl.NumberFormat('ar').format(100));
  });
});
