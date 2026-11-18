import { Button, Select } from 'antd';
import { useEffect, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { DatasetRegistryView } from '../../../packages/contracts/src/data-sources.ts';
import { api } from './api';
import { useResource } from './data-hooks';
import { ErrorNotice } from './ui';
export default function DatasetSelection() {
  const { t } = useTranslation('settings'); const cache = useQueryClient();
  const query = useResource<DatasetRegistryView>('/datasets/registry');
  const [selected, setSelected] = useState<string[]>(); const [busy, setBusy] = useState(false); const [error, setError] = useState<unknown>();
  useEffect(() => { if (selected === undefined && query.data) setSelected([...query.data.datasetIds]); }, [query.data, selected]);
  async function save(datasetIds: string[] | null) {
    setBusy(true); setError(undefined);
    try {
      const saved = await api<DatasetRegistryView>('/datasets/selection', { method: 'PUT', body: { datasetIds } });
      setSelected([...saved.datasetIds]); cache.setQueryData(['/datasets/registry'], saved);
      await cache.invalidateQueries({ predicate: query => typeof query.queryKey[0] === 'string' && query.queryKey[0].startsWith('/datasets') });
    } catch (failure) { setError(failure); } finally { setBusy(false); }
  }
  const archived = query.data?.availableDatasetIds.filter(id => !query.data!.datasetIds.includes(id)) ?? [];
  return <section className="settings-group"><header><h2>{t('Current dataset references')}</h2></header><div className="settings-group-body">
    <p className="quiet">{t('Choose the registered catalog references offered for new research. This setting does not merge or delete data, change existing research bindings, or turn legacy files into a native catalog.')}</p>
    <ErrorNotice error={error ?? query.error} retry={() => { void query.refetch(); }} />
    <Select mode="multiple" aria-label={t('Current dataset references')} style={{ width: '100%' }} value={selected ?? []} onChange={setSelected} disabled={busy || !query.data} options={query.data?.availableDatasetIds.map(id => ({ value: id, label: id }))} />
    <div className="settings-save"><Button loading={busy} disabled={!query.data} onClick={() => { void save(selected ?? []); }}>{t('Save current references')}</Button><Button disabled={busy || !query.data} onClick={() => { void save(null); }}>{t('Restore default references')}</Button></div>
    <details><summary>{t('Archived references remain readable')} ({archived.length})</summary><p>{t('These references are not offered for new research. Their original IDs, data, and historical bindings are preserved, not deleted.')}</p>{archived.map(id => <div key={id}><code>{id}</code></div>)}</details>
  </div></section>;
}
