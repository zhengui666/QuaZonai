import { Button, Switch } from 'antd';
import { ArrowClockwiseIcon, PuzzlePieceIcon } from '@phosphor-icons/react';
import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import type { ExtensionView } from '../../../packages/contracts/src/index.ts';
import { api } from './api';
import { useResource } from './data-hooks';
import { Empty, ErrorNotice, Loading, Status } from './ui';
export default function Extensions() {
  const cache = useQueryClient(); const query = useResource<ExtensionView[]>('/extensions');
  const [pending, setPending] = useState<Record<string, boolean>>({}); const [errors, setErrors] = useState<Record<string, unknown>>({});
  async function toggle(item: ExtensionView, enabled: boolean) {
    if (pending[item.name]) return;
    setPending(old => ({ ...old, [item.name]: true })); setErrors(old => ({ ...old, [item.name]: undefined }));
    try { const next = await api<ExtensionView>(`/extensions/${encodeURIComponent(item.name)}`, { method: 'PATCH', body: { enabled } }); cache.setQueryData<ExtensionView[]>(['/extensions'], old => old?.map(value => value.name === item.name ? next : value)); }
    catch (error) { setErrors(old => ({ ...old, [item.name]: error })); await query.refetch(); }
    finally { setPending(old => ({ ...old, [item.name]: false })); }
  }
  return <div className="settings-page"><header className="page-heading"><h1>扩展</h1><Button icon={<ArrowClockwiseIcon size={15} />} onClick={() => { void query.refetch(); }}>刷新目录</Button></header><ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />{query.isPending ? <Loading /> : !query.data?.length ? !query.error && <Empty title="没有已发现扩展" /> : <div className="extension-list">{query.data.map(item => <section className="extension-row" key={item.name}><div className="extension-symbol"><PuzzlePieceIcon size={22} /></div><div className="extension-meta"><div className="extension-title"><h2>{item.name}</h2><code>{item.version ?? '未提供'}</code></div><span>{item.kind}</span><ErrorNotice error={errors[item.name]} /></div><div className="extension-control"><Status plain value={item.state} />{item.required ? <span className="required-extension">内置必需</span> : <><Switch aria-label={`启用 ${item.name}`} checked={item.enabled} loading={pending[item.name]} onChange={enabled => { void toggle(item, enabled); }} />{item.state === 'pending_restart' && <span className="required-extension">重启后生效</span>}</>}{item.error && <details className="extension-error"><summary>错误详情</summary><code>{item.error}</code></details>}</div></section>)}</div>}</div>;
}
