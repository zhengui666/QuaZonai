import { Background, Controls, MarkerType, ReactFlow, type Edge, type Node } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { useI18n } from '../../i18n';
import type { CandidateMember } from '../../lib/api/types';
import { humanize } from '../../lib/format';

export interface RedundancyEdge { source: string; target: string; score?: number; reason?: string; }

export function RedundancyGraph({ members, edges }: { members: CandidateMember[]; edges?: RedundancyEdge[] }) {
  const { locale } = useI18n();
  const scoreFormatter = new Intl.NumberFormat(locale, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  const nodes: Node[] = members.map((member, index) => ({ id: member.alpha_qualification_id, position: { x: (index % 4) * 210, y: Math.floor(index / 4) * 125 }, data: { label: <bdi dir="auto">{`${member.alpha_name ?? member.alpha_qualification_id.slice(0, 8)} · ${humanize(member.role)}`}</bdi> }, style: { background: 'var(--qz-bg-elevated)', border: '1px solid var(--qz-border-strong)', color: 'var(--qz-text)', borderRadius: 8, width: 175, fontSize: 10 } }));
  const flowEdges: Edge[] = (edges ?? []).map((edge, index) => {
    const label = edge.score !== undefined ? scoreFormatter.format(edge.score) : edge.reason;
    return { id: `r-${index}`, source: edge.source, target: edge.target, label: label ? <bdi dir="auto">{label}</bdi> : undefined, markerEnd: { type: MarkerType.ArrowClosed }, style: { stroke: 'var(--qz-warning)' }, labelStyle: { fill: 'var(--qz-text-faint)', fontSize: 9 } };
  });
  return <div className="qz-flow"><ReactFlow nodes={nodes} edges={flowEdges} fitView nodesDraggable={false} nodesConnectable={false} minZoom={.4}><Background gap={22} color="var(--qz-border)" /><Controls showInteractive={false} /></ReactFlow></div>;
}
