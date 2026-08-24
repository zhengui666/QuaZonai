import { Background, Controls, MarkerType, ReactFlow, type Edge, type Node } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import type { CandidateMember } from '../../lib/api/types';
import { humanize } from '../../lib/format';

export interface RedundancyEdge { source: string; target: string; score?: number; reason?: string; }

export function RedundancyGraph({ members, edges }: { members: CandidateMember[]; edges?: RedundancyEdge[] }) {
  const nodes: Node[] = members.map((member, index) => ({ id: member.alpha_qualification_id, position: { x: (index % 4) * 210, y: Math.floor(index / 4) * 125 }, data: { label: `${member.alpha_name ?? member.alpha_qualification_id.slice(0, 8)} · ${humanize(member.role)}` }, style: { background: 'var(--qz-bg-elevated)', border: '1px solid var(--qz-border-strong)', color: 'var(--qz-text)', borderRadius: 8, width: 175, fontSize: 10 } }));
  const flowEdges: Edge[] = (edges ?? []).map((edge, index) => ({ id: `r-${index}`, source: edge.source, target: edge.target, label: edge.score !== undefined ? edge.score.toFixed(2) : edge.reason, markerEnd: { type: MarkerType.ArrowClosed }, style: { stroke: 'var(--qz-warning)' }, labelStyle: { fill: 'var(--qz-text-faint)', fontSize: 9 } }));
  return <div className="qz-flow"><ReactFlow nodes={nodes} edges={flowEdges} fitView nodesDraggable={false} nodesConnectable={false} minZoom={.4}><Background gap={22} color="var(--qz-border)" /><Controls showInteractive={false} /></ReactFlow></div>;
}
