import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import { en as coreEn, zhCN as coreZh } from './locales/core';
import { en as settingsEn, zhCN as settingsZh } from './locales/settings';
import { en as sharedEn, zhCN as sharedZh } from './locales/shared';

// The persisted /settings preference is the only application language setting.
// Login/setup and older installations without a preference start in English.
void i18n.use(initReactI18next).init({
  lng: 'en', fallbackLng: 'en', supportedLngs: ['en', 'zh-CN'],
  defaultNS: 'core', ns: ['core', 'settings', 'shared'],
  keySeparator: false, nsSeparator: false, initAsync: false,
  interpolation: { escapeValue: false }, react: { useSuspense: false },
  resources: {
    en: { core: coreEn, settings: settingsEn, shared: sharedEn },
    'zh-CN': { core: coreZh, settings: settingsZh, shared: sharedZh },
  },
});
export const locale = () => i18n.resolvedLanguage === 'zh-CN' ? 'zh-CN' : 'en-US';
export const tr = (key: string, options?: Record<string, unknown>) => i18n.t(key, { ns: 'core', ...options }) as string;
export default i18n;
