import { Background, Controls, MarkerType, ReactFlow, type Edge, type Node } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { useI18n } from '../../i18n';
import type { ResearchMission } from '../../lib/api/types';
import { humanize } from '../../lib/format';
import { GraphViewport, type GraphDataItem } from './GraphViewport';
import { useReactFlowAriaLabelConfig } from './reactFlowA11y';

export function MissionGraph({ missions }: { missions: ResearchMission[] }) {
  const { t } = useI18n();
  const ariaLabelConfig = useReactFlowAriaLabelConfig();
  const nodes: Node[] = missions.map((mission, index) => ({
    id: mission.id,
    position: { x: (index % 4) * 220, y: Math.floor(index / 4) * 120 },
    data: { label: <><bdi dir="auto">{humanize(mission.type)}</bdi>{' · '}<bdi dir="auto">{humanize(mission.state)}</bdi></> },
    style: { background: 'var(--qz-bg-elevated)', border: `1px solid ${mission.state === 'RUNNING' ? 'var(--qz-accent)' : 'var(--qz-border-strong)'}`, color: 'var(--qz-text)', borderRadius: 8, fontSize: 11, width: 185 },
  }));
  const edges: Edge[] = missions.flatMap((mission) => (mission.dependencies ?? []).map((source) => ({
    id: `${source}-${mission.id}`,
    source,
    target: mission.id,
    markerEnd: { type: MarkerType.ArrowClosed },
    style: { stroke: 'var(--qz-border-strong)' },
  })));
  const items: GraphDataItem[] = missions.map((mission) => ({
    id: mission.id,
    label: <bdi dir="ltr">{mission.id.slice(0, 12)}</bdi>,
    details: [
      [t('graph.type'), <bdi dir="auto">{humanize(mission.type)}</bdi>],
      [t('graph.state'), <bdi dir="auto">{humanize(mission.state)}</bdi>],
      [t('graph.dependency'), mission.dependencies?.length ? <bdi dir="ltr">{mission.dependencies.join(', ')}</bdi> : '—'],
      [t('graph.branch'), <bdi dir="ltr">{mission.branch_id?.slice(0, 12) ?? '—'}</bdi>],
    ],
  }));
  return <GraphViewport ariaLabel="Mission DAG" items={items}><div className="qz-flow"><ReactFlow ariaLabelConfig={ariaLabelConfig} nodes={nodes} edges={edges} fitView nodesDraggable={false} nodesConnectable={false} elementsSelectable={false} minZoom={.4} maxZoom={1.5}><Background gap={22} color="var(--qz-border)" /><Controls showInteractive={false} /></ReactFlow></div></GraphViewport>;
}
