import { useState } from 'react';
import { Select } from 'antd';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { InterfaceLanguage, SettingsView } from '../../../packages/contracts/src/index.ts';
import type { ModelLoginRequest } from './model-login';
import { href, type Route } from './router';
import { api } from './api';
import { useResource } from './data-hooks';
import { ErrorNotice, Loading } from './ui';
import ModelSettings from './model-settings';
import Extensions from './extensions';
import InstanceSettings from './instance-settings';
import AccountSettings from './account-settings';
import Data from './data';
import DataPlugins from './data-plugins';

function GeneralSettings() {
  const { t, i18n } = useTranslation('core');
  const query = useResource<SettingsView>('/settings');
  const cache = useQueryClient();
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<unknown>();
  const changeLanguage = async (language: InterfaceLanguage) => {
    setSaving(true); setError(undefined);
    try {
      const saved = await api<SettingsView>('/settings', { method: 'PATCH', body: { language } });
      cache.setQueryData(['/settings'], saved);
      await i18n.changeLanguage(saved.language ?? 'en');
    } catch (reason) { setError(reason); }
    finally { setSaving(false); }
  };
  return <section className="panel">
    <h2>{t('General')}</h2>
    <ErrorNotice error={error ?? query.error} />
    {query.isPending ? <Loading /> : query.data && <>
      <label htmlFor="interface-language">{t('Language')}</label>
      <Select id="interface-language" aria-label={t('Language')} value={query.data.language ?? 'en'}
        loading={saving} disabled={saving} onChange={value => void changeLanguage(value)}
        options={[{ value: 'en', label: 'English' }, { value: 'zh-CN', label: '简体中文' }]} />
      <p className="quiet">{t('Choose the interface language. External agents use this preference for their replies.')}</p>
    </>}
  </section>;
}

export default function Settings({ connect, route }: { connect: (value: ModelLoginRequest) => void; route: Route }) {
  const { t } = useTranslation('core');
  const sections = [{ id: 'general', label: 'General' }, { id: 'models', label: 'Models' }, { id: 'data', label: 'Data' }, { id: 'data-plugins', label: 'Data plugins' }, { id: 'extensions', label: 'Extensions' }, { id: 'accounts', label: 'Accounts' }, { id: 'instances', label: 'Connections' }];
  const requested = route.page === 'data' ? 'data' : route.page === 'extensions' ? 'extensions' : route.params.get('section');
  const section = sections.some(item => item.id === requested) ? requested : 'general';
  return <div className="settings-page settings-hub">
    <header className="page-heading"><h1>{t('Settings')}</h1></header>
    <nav className="settings-nav" aria-label={t('Settings sections')}>{sections.map(item => <a key={item.id} href={href(`/settings?section=${item.id}`)} aria-current={section === item.id ? 'page' : undefined}>{t(item.label)}</a>)}</nav>
    {section === 'general' ? <GeneralSettings /> : section === 'models' ? <ModelSettings connect={connect} /> : section === 'data' ? <Data /> : section === 'data-plugins' ? <DataPlugins /> : section === 'extensions' ? <Extensions /> : section === 'accounts' ? <AccountSettings /> : <InstanceSettings />}
  </div>;
}
