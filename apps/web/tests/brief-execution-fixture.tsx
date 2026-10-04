import { App } from 'antd';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createRoot } from 'react-dom/client';
import type { Schema } from '../src/api';
import { BriefDetail } from '../src/briefs';

export function mountBriefExecutionContext(value: Schema['FrozenBriefV1']) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 60_000 } } });
  client.setQueryData(['frozen-brief', value.brief.id], value);
  const host = document.createElement('div'); host.id = 'brief-context-fixture'; document.body.append(host);
  const root = createRoot(host, { onUncaughtError: error => { host.dataset.error = String(error); } });
  const close = () => { root.unmount(); client.clear(); host.remove(); };
  root.render(<App><QueryClientProvider client={client}><BriefDetail brief={value.brief} close={close} /></QueryClientProvider></App>);
}
