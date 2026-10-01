import { App } from 'antd';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createRoot } from 'react-dom/client';
import type { Schema } from '../src/api';
import { BriefExecution } from '../src/cycles';
import { GuardProvider } from '../src/ui';

export function mountBriefExecution(brief: Schema['BriefView']) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 60_000 } } });
  client.setQueryData(['frozen-brief', brief.id], { schema_version: 1, brief, execution_context: {
    schema_version: 1, runtime_id: brief.id, runtime_revision: '1', discovery_input_set_id: brief.id,
    validation_input_set_id: brief.id, sealed_input_set_id: brief.id,
  } });
  const host = document.createElement('div'); host.id = 'brief-cycle-fixture'; document.body.append(host);
  createRoot(host, { onUncaughtError: error => { host.dataset.error = String(error); } }).render(
    <App><QueryClientProvider client={client}><GuardProvider>
      <BriefExecution brief={brief} close={() => {}} />
    </GuardProvider></QueryClientProvider></App>,
  );
}
