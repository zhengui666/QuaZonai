import { getActiveLocale, getIntlLocale, translateSource } from '../i18n';
import { translateDomainLabel } from '../i18n/domain';
import { translateRuntimeLabel } from '../i18n/runtime';

export function formatDateTime(value?: string | null): string {
  if (!value) return '—';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(getIntlLocale(), { year: 'numeric', month: 'short', day: '2-digit', hour: '2-digit', minute: '2-digit' }).format(date);
}

export function formatNumber(value?: number | string | null, options?: Intl.NumberFormatOptions): string {
  if (value === null || value === undefined || value === '') return '—';
  const locale = getIntlLocale();
  if (typeof value === 'string') {
    const exact = formatPlainDecimalString(value.trim(), locale);
    if (exact !== null) return exact;
  }
  const numeric = Number(value);
  if (Number.isNaN(numeric)) return String(value);
  return new Intl.NumberFormat(locale, options).format(numeric);
}

const capitalNumberFormatOptions = { maximumSignificantDigits: 21 } as const;
const plainDecimalPattern = /^([+-]?)(\d*)(?:\.(\d*))?$/;

type PlainDecimal = {
  sign: -1 | 1;
  integer: string;
  fraction: string;
};

function parsePlainDecimal(value: string): PlainDecimal | null {
  const match = plainDecimalPattern.exec(value);
  if (match === null) return null;
  const rawInteger = match[2] ?? '';
  const rawFraction = match[3] ?? '';
  if (!rawInteger && !rawFraction) return null;
  const integer = rawInteger.replace(/^0+(?=\d)/, '') || '0';
  const fraction = rawFraction.replace(/0+$/, '');
  const isZero = integer === '0' && !fraction;
  return { sign: match[1] === '-' && !isZero ? -1 : 1, integer, fraction };
}

function compareDigits(left: string, right: string): number {
  return left === right ? 0 : left < right ? -1 : 1;
}

export function comparePlainDecimalStrings(left: string, right: string): number | undefined {
  const leftDecimal = parsePlainDecimal(left);
  const rightDecimal = parsePlainDecimal(right);
  if (leftDecimal === null || rightDecimal === null) return undefined;
  if (leftDecimal.sign !== rightDecimal.sign) return leftDecimal.sign - rightDecimal.sign;

  let magnitude = leftDecimal.integer.length - rightDecimal.integer.length;
  if (magnitude === 0) magnitude = compareDigits(leftDecimal.integer, rightDecimal.integer);
  if (magnitude === 0) {
    const fractionLength = Math.max(leftDecimal.fraction.length, rightDecimal.fraction.length);
    magnitude = compareDigits(
      leftDecimal.fraction.padEnd(fractionLength, '0'),
      rightDecimal.fraction.padEnd(fractionLength, '0'),
    );
  }
  return leftDecimal.sign === -1 ? -magnitude : magnitude;
}

export function formatPlainDecimalString(value: string, locale: string): string | null {
  const match = plainDecimalPattern.exec(value);
  if (match === null) return null;
  const sign = match[1] ?? '';
  const integerDigits = match[2] ?? '';
  const fractionalDigits = match[3] ?? '';
  if (!integerDigits && !fractionalDigits) return null;

  const integerFormatter = new Intl.NumberFormat(locale, { maximumFractionDigits: 0 });
  const integerValue = BigInt(integerDigits || '0');
  const formattedInteger = sign === '-' && integerValue === 0n
    ? integerFormatter.formatToParts(-1).map((part) => part.type === 'integer' ? integerFormatter.format(0) : part.value).join('')
    : integerFormatter.format(sign === '-' ? -integerValue : integerValue);
  if (!fractionalDigits) return formattedInteger;

  const decimal = new Intl.NumberFormat(locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 })
    .formatToParts(1.1)
    .find((part) => part.type === 'decimal')?.value ?? '.';
  const localizedDigits = Array.from(
    { length: 10 },
    (_, digit) => new Intl.NumberFormat(locale, { useGrouping: false }).format(digit),
  );
  return formattedInteger + decimal + fractionalDigits.replace(/\d/g, (digit) => localizedDigits[Number(digit)] ?? digit);
}

export function formatCapitalAmount(value?: number | string | null, locale = getIntlLocale()): string {
  if (value === null || value === undefined || value === '') return '—';
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!trimmed) return '—';
    return formatPlainDecimalString(trimmed, locale) ?? value;
  }
  return Number.isFinite(value)
    ? new Intl.NumberFormat(locale, capitalNumberFormatOptions).format(value)
    : String(value);
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

export function localizeSystemInferred(value: string | null | undefined, localizedValue: string): string | null | undefined {
  return value === 'System inferred' ? localizedValue : value;
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
