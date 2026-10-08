// Browser component fixture only. Transport is deliberately isolated from the
// native schema/API acceptance suite; no native release or withdrawal is proved.
import { App, Button, ConfigProvider } from 'antd';
import { createRoot } from 'react-dom/client';
import { useState } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { api, ApiFailure } from '../../src/api';
import { CapitalExitDrawer } from '../../src/capital-exits';
import { id, source } from '../../src/capital-exit-fixtures';
import '../../src/styles.css';
async function transport(method: string, path: string, options: any) {
  for (const [key, value] of Object.entries(options?.params?.path ?? {})) path = path.replace(`{${key}}`, String(value));
  const response = await fetch(`/fixture-api${path}`, { method, headers: { 'Content-Type': 'application/json', ...options?.params?.header }, body: method === 'POST' ? JSON.stringify(options.body) : undefined });
  const value = await response.json();
  if (!response.ok) throw new ApiFailure(value.code, value.detail, response.status, value);
  return { data: value };
}
api.GET = ((path: string, options: any) => transport('GET', path, options)) as typeof api.GET;
api.POST = ((path: string, options: any) => transport('POST', path, options)) as typeof api.POST;
const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
function Fixture() {
  const [open, setOpen] = useState(true);
  return <><Button onClick={() => setOpen(true)}>重新打开账户</Button>{open && <CapitalExitDrawer project={id} source={source} close={() => setOpen(false)} />}</>;
}
createRoot(document.getElementById('root')!).render(<ConfigProvider><App><QueryClientProvider client={client}><Fixture /></QueryClientProvider></App></ConfigProvider>);
