import { getActiveLocale, getIntlLocale, translateSource } from '../i18n';
import { translateDomainLabel } from '../i18n/domain';
import { translateRuntimeLabel } from '../i18n/runtime';

export function formatDateTime(value?: string | null): string {
  if (!value) return '—';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(getIntlLocale(), { year: 'numeric', month: 'short', day: '2-digit', hour: '2-digit', minute: '2-digit' }).format(date);
}

export function formatNumber(value?: number | string | null): string {
  if (value === null || value === undefined || value === '') return '—';
  const numeric = Number(value);
  if (Number.isNaN(numeric)) return String(value);
  return new Intl.NumberFormat(getIntlLocale()).format(numeric);
}

export function formatCompactNumber(value?: number | string | null): string {
  if (value === null || value === undefined || value === '') return '—';
  const numeric = Number(value);
  if (Number.isNaN(numeric)) return String(value);
  return new Intl.NumberFormat(getIntlLocale(), { notation: 'compact', maximumFractionDigits: 2 }).format(numeric);
}

export function formatPercent(value?: number | string | null, decimals = 1): string {
  if (value === null || value === undefined || value === '') return '—';
  const numeric = Number(value);
  if (Number.isNaN(numeric)) return String(value);
  return new Intl.NumberFormat(getIntlLocale(), { style: 'percent', minimumFractionDigits: decimals, maximumFractionDigits: decimals }).format(numeric);
}

export function humanizeIdentifier(value?: string | null): string {
  if (!value) return '—';
  return value.replaceAll('_', ' ').toLowerCase().replace(/(^|\s)\S/g, (character) => character.toUpperCase());
}

export function humanize(value?: string | null): string {
  if (!value) return '—';
  const source = humanizeIdentifier(value);
  const locale = getActiveLocale();
  return translateRuntimeLabel(locale, source) ?? translateDomainLabel(locale, source) ?? translateSource(locale, source);
}

export function readMetric(metrics: Record<string, unknown> | undefined, keys: string[]): number | string | null | undefined {
  for (const key of keys) {
    if (metrics && key in metrics) {
      const value = metrics[key];
      if (typeof value === 'number' || typeof value === 'string' || value === null) return value;
    }
  }
  return undefined;
}
