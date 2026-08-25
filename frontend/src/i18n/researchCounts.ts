import type { Locale } from './messages';

type PluralCategory = 'zero' | 'one' | 'two' | 'few' | 'many' | 'other';
type PluralForms = { other: string } & Partial<Record<Exclude<PluralCategory, 'other'>, string>>;
type LocalizedPluralForms = Record<Locale, PluralForms>;

const missionRunning: LocalizedPluralForms = {
  en: { other: '{count} running' },
  'zh-CN': { other: '{count} 运行中' },
  'zh-TW': { other: '{count} 執行中' },
  ja: { other: '{count} 実行中' },
  ko: { other: '{count} 실행 중' },
  es: { one: '{count} en ejecución', other: '{count} en ejecución' },
  ar: {
    zero: 'لا مهام قيد التشغيل',
    one: 'مهمة واحدة قيد التشغيل',
    two: 'مهمتان قيد التشغيل',
    few: '{count} مهام قيد التشغيل',
    many: '{count} مهمة قيد التشغيل',
    other: '{count} مهمة قيد التشغيل',
  },
};

const missionSucceeded: LocalizedPluralForms = {
  en: { other: '{count} succeeded' },
  'zh-CN': { other: '{count} 成功' },
  'zh-TW': { other: '{count} 成功' },
  ja: { other: '{count} 成功' },
  ko: { other: '{count} 성공' },
  es: { one: '{count} completada', other: '{count} completadas' },
  ar: {
    zero: 'لا مهام ناجحة',
    one: 'مهمة واحدة ناجحة',
    two: 'مهمتان ناجحتان',
    few: '{count} مهام ناجحة',
    many: '{count} مهمة ناجحة',
    other: '{count} مهمة ناجحة',
  },
};

const missionFailed: LocalizedPluralForms = {
  en: { other: '{count} failed' },
  'zh-CN': { other: '{count} 失败' },
  'zh-TW': { other: '{count} 失敗' },
  ja: { other: '{count} 失敗' },
  ko: { other: '{count} 실패' },
  es: { one: '{count} fallida', other: '{count} fallidas' },
  ar: {
    zero: 'لا مهام فاشلة',
    one: 'مهمة واحدة فاشلة',
    two: 'مهمتان فاشلتان',
    few: '{count} مهام فاشلة',
    many: '{count} مهمة فاشلة',
    other: '{count} مهمة فاشلة',
  },
};

const structuredEvents: LocalizedPluralForms = {
  en: { one: '{count} structured event', other: '{count} structured events' },
  'zh-CN': { other: '{count} 个结构化事件' },
  'zh-TW': { other: '{count} 個結構化事件' },
  ja: { other: '構造化イベント {count} 件' },
  ko: { other: '구조화 이벤트 {count}건' },
  es: { one: '{count} evento estructurado', other: '{count} eventos estructurados' },
  ar: {
    zero: 'لا أحداث منظمة',
    one: 'حدث منظم واحد',
    two: 'حدثان منظمان',
    few: '{count} أحداث منظمة',
    many: '{count} حدثًا منظمًا',
    other: '{count} حدث منظم',
  },
};

function pluralized(locale: Locale, count: number, forms: LocalizedPluralForms): string {
  const category = new Intl.PluralRules(locale).select(count) as PluralCategory;
  const template = forms[locale][category] ?? forms[locale].other;
  return template.replaceAll('{count}', new Intl.NumberFormat(locale).format(count));
}

export function formatMissionStateSummary(
  locale: Locale,
  counts: { running: number; succeeded: number; failed: number },
): string {
  return [
    pluralized(locale, counts.running, missionRunning),
    pluralized(locale, counts.succeeded, missionSucceeded),
    pluralized(locale, counts.failed, missionFailed),
  ].join(' · ');
}

export function formatStructuredEventCount(locale: Locale, count: number): string {
  return pluralized(locale, count, structuredEvents);
}
