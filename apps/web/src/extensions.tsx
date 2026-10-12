import { Switch } from 'antd';
import { useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import type { ExtensionView } from '../../../packages/contracts/src/index.ts';
import { api } from './api';
import { display, ErrorNotice, NoData, PageHeading, QueryPanel, RefreshButton, Status, Surface, useResource } from './ui';

export function Extensions() {
  const listing = useResource<ExtensionView[]>('/extensions', 10000);
  return <div className="page"><PageHeading title="扩展" count={listing.data?.length} action={<RefreshButton loading={listing.isFetching} label="刷新扩展" onClick={() => { void listing.refetch(); }} />} />
    <QueryPanel query={listing}>{listing.data?.length ? <div className="extension-grid">{listing.data.map(item => <ExtensionCard key={item.name} value={item} disabled={listing.isError} />)}</div> : <Surface><NoData text="暂无已安装扩展" /></Surface>}</QueryPanel>
  </div>;
}
function ExtensionCard({ value, disabled }: { value: ExtensionView; disabled: boolean }) {
  const client = useQueryClient(); const [busy, setBusy] = useState(false); const [error, setError] = useState<unknown>();
  async function toggle(enabled: boolean) {
    setBusy(true); setError(undefined);
    try {
      const updated = await api<ExtensionView>(`/extensions/${encodeURIComponent(value.name)}`, { method: 'PATCH', body: { enabled } });
      client.setQueryData<ExtensionView[]>(['/extensions'], previous => previous?.map(item => item.name === updated.name ? updated : item));
    } catch (failure) { setError(failure); }
    finally { setBusy(false); }
  }
  return <article className="surface extension-card"><div className="card-top"><h2>{display(value.name)}</h2><Switch checked={value.enabled} loading={busy} disabled={disabled || busy} aria-label={`启用 ${value.name}`} onChange={toggle} /></div>
    <div className="extension-meta"><span>{display(value.kind)}</span><span>{display(value.version)}</span></div><Status value={value.state} /><ErrorNotice error={error || value.error} />
  </article>;
}
