// Loaded only by the existing local Vite test server, never the production entry.
import { StrictMode, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { App, Button, ConfigProvider, Space, theme } from 'antd';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import * as echarts from 'echarts/core';
import EquityCurve from '../src/equity-curve';
import type { Schema } from '../src/api';
import 'antd/dist/reset.css';
import '../src/styles.css';

const id = (tail: number) => `01990000-0000-7000-8000-${tail.toString().padStart(12, '0')}`;
const evaluation: Schema['EvaluationView'] = {
  id: id(91), project_id: id(1), subject_alpha_version_id: null, subject_candidate_id: id(81),
  input_set_id: id(92), policy_id: id(93), run_id: id(94), evaluation_kind: 'PORTFOLIO',
  execution_status: 'SUCCEEDED', evidence_status: 'VALID', decision: 'REJECT',
  report_artifact_id: id(95), method_versions_artifact_id: id(95), origin: 'FIXTURE',
  concluded_at: '2026-01-03T00:00:00Z', valid_until: '2026-01-04T00:00:00Z', checked_at: '2026-09-20T00:00:00Z', unexpired_at_read: false,
};
const client = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } } });
function chart() {
  const element = document.querySelector<HTMLElement>('.echarts-for-react');
  return element ? echarts.getInstanceByDom(element) : undefined;
}
const inspection = {
  zoom(start: number, end: number) { chart()?.dispatchAction({ type: 'dataZoom', start, end }); },
  inspect() {
    const instance = chart();
    const option = instance?.getOption();
    const zoom = Array.isArray(option?.dataZoom) ? option.dataZoom[0] : undefined;
    const series = Array.isArray(option?.series) ? option.series[0] : undefined;
    return { id: instance?.id ?? null, width: instance?.getWidth() ?? null,
      background: typeof option?.backgroundColor === 'string' ? option.backgroundColor : null,
      start: typeof zoom?.start === 'number' ? zoom.start : null,
      end: typeof zoom?.end === 'number' ? zoom.end : null,
      points: Array.isArray(series?.data) ? series.data.length : 0,
    };
  },
};
declare global { interface Window { equityHarness: typeof inspection } }
window.equityHarness = inspection;

function Harness() {
  const [dark, setDark] = useState(false);
  const [visible, setVisible] = useState(true);
  const [mounted, setMounted] = useState(true);
  const [narrow, setNarrow] = useState(false);
  return <ConfigProvider theme={{ algorithm: dark ? theme.darkAlgorithm : theme.defaultAlgorithm, token: { motion: false } }}>
    <App><QueryClientProvider client={client}>
      <Space wrap>
        <Button onClick={() => setDark(value => !value)}>切换测试主题</Button>
        <Button onClick={() => setVisible(value => !value)}>切换测试可见性</Button>
        <Button onClick={() => setMounted(value => !value)}>切换测试挂载</Button>
        <Button onClick={() => setNarrow(value => !value)}>切换测试宽度</Button>
      </Space>
      <div style={{ display: visible ? 'block' : 'none', width: narrow ? 320 : 900, maxWidth: '100%' }}>
        {mounted && <EquityCurve evaluation={evaluation} />}
      </div>
    </QueryClientProvider></App>
  </ConfigProvider>;
}
createRoot(document.getElementById('root')!).render(<StrictMode><Harness /></StrictMode>);
