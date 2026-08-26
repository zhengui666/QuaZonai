import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import { localeLabels, localeOrder, messages, type Locale, type MessageKey } from './messages';
import { pluralMessages, type PluralMessageKey } from './pluralMessages';

const STORAGE_KEY = 'qz-locale';
const localeIndex: Record<Locale, number> = { en: 0, 'zh-CN': 1, 'zh-TW': 2, ja: 3, ko: 4, es: 5, ar: 6 };

function buildSourceIndex(): Map<string, MessageKey> {
  const groups = new Map<string, MessageKey[]>();
  for (const key of Object.keys(messages) as MessageKey[]) {
    const source = messages[key][0];
    groups.set(source, [...(groups.get(source) ?? []), key]);
  }
  const index = new Map<string, MessageKey>();
  for (const [source, keys] of groups) {
    const first = keys[0];
    if (!first) continue;
    const equivalent = keys.slice(1).every((key) => messages[key].every((value, position) => value === messages[first][position]));
    if (equivalent) index.set(source, first);
  }
  return index;
}

const sourceIndex = buildSourceIndex();

export type TranslationValues = Record<string, string | number | null | undefined>;
type PluralTranslationKey = MessageKey | PluralMessageKey;

function interpolate(locale: Locale, template: string, values?: TranslationValues): string {
  if (!values) return template;
  const numberFormat = new Intl.NumberFormat(locale);
  return template.replace(/\{([A-Za-z0-9_]+)\}/g, (match, name: string) => {
    const value = values[name];
    if (value === null || value === undefined) return match;
    return typeof value === 'number' ? numberFormat.format(value) : String(value);
  });
}

export function translateKey(locale: Locale, key: MessageKey, values?: TranslationValues): string {
  return interpolate(locale, messages[key][localeIndex[locale]], values);
}

function translatePluralKey(locale: Locale, key: PluralTranslationKey, values?: TranslationValues): string {
  const template = key in messages
    ? messages[key as MessageKey][localeIndex[locale]]
    : pluralMessages[key as PluralMessageKey][localeIndex[locale]];
  return interpolate(locale, template, values);
}

export function translateSource(locale: Locale, source: string, values?: TranslationValues): string {
  const key = sourceIndex.get(source);
  return key ? translateKey(locale, key, values) : interpolate(locale, source, values);
}

function normalizeLocale(value: string): string {
  try {
    return Intl.getCanonicalLocales(value)[0] ?? value;
  } catch {
    return value;
  }
}

function matchLocale(value: string): Locale | undefined {
  const canonical = normalizeLocale(value);
  if ((localeOrder as readonly string[]).includes(canonical)) return canonical as Locale;
  const lower = canonical.toLowerCase();
  const primaryLanguage = lower.split('-', 1)[0];
  if (primaryLanguage === 'zh') {
    if (/(?:^|-)hans(?:-|$)/i.test(canonical)) return 'zh-CN';
    if (/(?:^|-)hant(?:-|$)/i.test(canonical)) return 'zh-TW';
    return /(?:^|-)(?:tw|hk|mo)(?:-|$)/i.test(canonical) ? 'zh-TW' : 'zh-CN';
  }
  if (primaryLanguage === 'ja') return 'ja';
  if (primaryLanguage === 'ko') return 'ko';
  if (primaryLanguage === 'es') return 'es';
  if (primaryLanguage === 'ar') return 'ar';
  if (primaryLanguage === 'en') return 'en';
  return undefined;
}

export function resolveLocale(candidates: readonly string[]): Locale {
  for (const candidate of candidates) {
    const locale = matchLocale(candidate);
    if (locale) return locale;
  }
  return 'en';
}

function storedLocale(): Locale | undefined {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    return value ? matchLocale(value) : undefined;
  } catch {
    return undefined;
  }
}

function browserLocale(): Locale {
  if (typeof navigator === 'undefined') return 'en';
  return resolveLocale(navigator.languages?.length ? navigator.languages : [navigator.language]);
}

let activeLocale: Locale = 'en';
export function getActiveLocale(): Locale { return activeLocale; }
export function getIntlLocale(): string { return activeLocale; }

interface I18nContextValue {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: (key: MessageKey, values?: TranslationValues) => string;
  text: (source: string, values?: TranslationValues) => string;
  plural: (forms: Partial<Record<Intl.LDMLPluralRule, PluralTranslationKey>>, count: number, values?: TranslationValues) => string;
}

const I18nContext = createContext<I18nContextValue | null>(null);

export function I18nProvider({ children, initialLocale }: { children: ReactNode; initialLocale?: Locale }) {
  const [locale, setLocaleState] = useState<Locale>(() => initialLocale ?? storedLocale() ?? browserLocale());
  activeLocale = locale;

  const setLocale = useCallback((nextLocale: Locale) => {
    setLocaleState(nextLocale);
    try { localStorage.setItem(STORAGE_KEY, nextLocale); } catch { /* Browser privacy modes may reject storage. */ }
  }, []);

  useEffect(() => {
    activeLocale = locale;
    const descriptor = localeLabels[locale];
    document.documentElement.lang = locale;
    document.documentElement.dir = descriptor.dir;
    document.title = translateKey(locale, 'app.documentTitle');
  }, [locale]);

  const value = useMemo<I18nContextValue>(() => ({
    locale,
    setLocale,
    t: (key, values) => translateKey(locale, key, values),
    text: (source, values) => translateSource(locale, source, values),
    plural: (forms, count, values) => {
      const rule = new Intl.PluralRules(locale).select(count);
      const key = (count === 0 ? forms.zero : undefined) ?? forms[rule] ?? forms.other ?? forms.one;
      if (!key) return new Intl.NumberFormat(locale).format(count);
      return translatePluralKey(locale, key, { count, ...values });
    },
  }), [locale, setLocale]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nContextValue {
  const value = useContext(I18nContext);
  if (!value) throw new Error('useI18n must be used inside I18nProvider');
  return value;
}

export function Translated({ source, values }: { source: string; values?: TranslationValues }) {
  const { text } = useI18n();
  return <>{text(source, values)}</>;
}

export { localeLabels, localeOrder };
export type { Locale, MessageKey };
