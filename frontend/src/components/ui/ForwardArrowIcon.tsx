import { ArrowLeftIcon, ArrowRightIcon } from '@phosphor-icons/react';
import { localeLabels, useI18n, type Locale } from '../../i18n';

export function forwardArrowDirection(locale: Locale): 'left' | 'right' {
  return localeLabels[locale].dir === 'rtl' ? 'left' : 'right';
}

export function ForwardArrowIcon({ size = 12 }: { size?: number }) {
  const { locale } = useI18n();
  const Icon = forwardArrowDirection(locale) === 'left' ? ArrowLeftIcon : ArrowRightIcon;
  return <Icon size={size} />;
}
