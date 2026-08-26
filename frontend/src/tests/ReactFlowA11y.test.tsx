import { render } from '@testing-library/react';
import { Children, isValidElement, type ReactNode } from 'react';
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
    data: { id: 'alpha-1', name: 'Alpha EUR/USD', role: 'PRIMARY_ALPHA', state: 'ACTIVE', metrics: {}, lineage: [{ id: 'lineage-1', label: 'EUR/USD carry', relationship: 'RELATED_PROGRAM' }] },
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

function expectBidiPart(value: ReactNode, text: string) {
  expect(isValidElement(value)).toBe(true);
  if (!isValidElement<{ dir?: string; children?: ReactNode }>(value)) return;
  expect(value.type).toBe('bdi');
  expect(value.props.dir).toBe('auto');
  expect(value.props.children).toBe(text);
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

  it('isolates API lineage labels from localized relationships', () => {
    renderArabic(<AlphaDetailPage />);
    const props = reactFlowSpy.mock.calls.at(-1)?.[0] as { nodes?: Array<{ id: string; data: { label: ReactNode } }> } | undefined;
    const lineageNode = props?.nodes?.find((node) => node.id === 'lineage-1');
    expect(lineageNode).toBeDefined();
    if (!lineageNode || !isValidElement<{ children?: ReactNode }>(lineageNode.data.label)) return;

    const parts = Children.toArray(lineageNode.data.label.props.children);
    expect(parts).toHaveLength(3);
    expectBidiPart(parts[0], 'EUR/USD carry');
    expect(parts[1]).toBe(' · ');
    expectBidiPart(parts[2], 'برنامج مرتبط');
  });

  it('isolates arbitrary mission types from localized states', () => {
    renderArabic(<MissionGraph missions={[{ id: 'mission-1', type: 'CUSTOM_EUR_USD', state: 'RUNNING' }]} />);
    const props = reactFlowSpy.mock.calls.at(-1)?.[0] as { nodes?: Array<{ id: string; data: { label: ReactNode } }> } | undefined;
    const missionNode = props?.nodes?.find((node) => node.id === 'mission-1');
    expect(missionNode).toBeDefined();
    if (!missionNode || !isValidElement<{ children?: ReactNode }>(missionNode.data.label)) return;

    const parts = Children.toArray(missionNode.data.label.props.children);
    expect(parts).toHaveLength(3);
    expectBidiPart(parts[0], 'Custom Eur Usd');
    expect(parts[1]).toBe(' · ');
    expectBidiPart(parts[2], 'قيد التشغيل');
  });
});
