import { render } from '@testing-library/react';
import type { ReactNode } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RedundancyGraph } from '../components/graphs/RedundancyGraph';
import { MissionGraph } from '../components/graphs/MissionGraph';
import { I18nProvider, translateKey } from '../i18n';
import { AlphaDetailPage } from '../pages/AlphaDetailPage';

const reactFlowSpy = vi.hoisted(() => vi.fn());

vi.mock('@xyflow/react', () => ({
  ReactFlow: (props: unknown) => {
    reactFlowSpy(props);
    return <div data-testid="react-flow" />;
  },
  Background: () => null,
  Controls: () => null,
  MarkerType: { ArrowClosed: 'arrowclosed' },
}));

vi.mock('../lib/api/hooks', () => ({
  useAlpha: () => ({
    isLoading: false,
    error: null,
    data: { id: 'alpha-1', name: 'Alpha EUR/USD', role: 'PRIMARY_ALPHA', state: 'ACTIVE', metrics: {}, lineage: [] },
  }),
}));

function expected() {
  return {
    'controls.ariaLabel': translateKey('ar', 'a11y.flowControls'),
    'controls.zoomIn.ariaLabel': translateKey('ar', 'a11y.flowZoomIn'),
    'controls.zoomOut.ariaLabel': translateKey('ar', 'a11y.flowZoomOut'),
    'controls.fitView.ariaLabel': translateKey('ar', 'a11y.flowFitView'),
  };
}

function expectLocalizedFlow() {
  const props = reactFlowSpy.mock.calls.at(-1)?.[0] as { ariaLabelConfig?: Record<string, string> } | undefined;
  expect(props?.ariaLabelConfig).toMatchObject(expected());
}

function renderArabic(ui: ReactNode) {
  return render(<MemoryRouter><I18nProvider initialLocale="ar">{ui}</I18nProvider></MemoryRouter>);
}

describe('React Flow accessibility labels', () => {
  beforeEach(() => reactFlowSpy.mockClear());

  it('passes localized labels to the redundancy graph', () => {
    renderArabic(<RedundancyGraph members={[]} />);
    expectLocalizedFlow();
  });

  it('passes localized labels to the mission DAG', () => {
    renderArabic(<MissionGraph missions={[]} />);
    expectLocalizedFlow();
  });

  it('passes localized labels to the Alpha lineage graph', () => {
    renderArabic(<AlphaDetailPage />);
    expectLocalizedFlow();
  });
});
